// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{DataElement, Limits, MAX_DEPTH, ParseError, ServiceSearchAttributeResponse, Uuid};

const SERVICE_SEARCH_ATTRIBUTE_REQUEST: u8 = 0x06;
const AUDIO_SINK: u16 = 0x110b;
const ADVANCED_AUDIO_DISTRIBUTION: u16 = 0x110d;
const L2CAP: u16 = 0x0100;
const AVDTP: u16 = 0x0019;
const AVDTP_PSM: u16 = 0x0019;

const SERVICE_CLASS_ID_LIST: u16 = 0x0001;
const PROTOCOL_DESCRIPTOR_LIST: u16 = 0x0004;
const BLUETOOTH_PROFILE_DESCRIPTOR_LIST: u16 = 0x0009;
const SUPPORTED_FEATURES: u16 = 0x0311;

const REQUEST_FIXED_PARAMETERS: usize = 22;
const MAX_REQUEST_BYTES: usize = 5 + REQUEST_FIXED_PARAMETERS + 16;

/// Fixed implementation ceiling for response chunks and remembered continuation
/// tokens. A lower per-session limit is required from the caller.
pub const MAX_DISCOVERY_CHUNKS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoveryLimits {
    /// Value placed in MaximumAttributeByteCount and enforced per response.
    pub max_chunk_bytes: u16,
    /// Maximum concatenated AttributeLists bytes in caller-owned storage.
    pub max_total_bytes: usize,
    /// Maximum data-element nodes in the completed response.
    pub max_nodes: usize,
    /// Maximum data-element nesting depth in the completed response.
    pub max_depth: usize,
    /// Maximum number of accepted response PDUs, including the final PDU.
    pub max_chunks: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SinkService {
    pub avdtp_version: u16,
    pub profile_version: u16,
    pub supported_features: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryProgress {
    ContinuationRequired {
        accumulated_bytes: usize,
        accepted_chunks: usize,
    },
    Complete(SinkService),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryState {
    Idle,
    AwaitingResponse,
    ContinuationReady,
    Complete,
    Cancelled,
    TimedOut,
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryError {
    InvalidLimits,
    OutputTooSmall,
    ScratchTooSmall,
    Busy,
    NotReady,
    EpochMismatch,
    EpochNotAdvanced,
    TransactionReuse,
    DeadlineExpired,
    ChunkLimit,
    TotalByteLimit,
    EmptyContinuationChunk,
    ContinuationLoop,
    Parse(ParseError),
    InvalidAttributeList,
    MultipleRecords,
    DuplicateAttribute(u16),
    UnexpectedAttribute(u16),
    WrongServiceClass,
    WrongProtocolDescriptorList,
    WrongProfileDescriptorList,
    InvalidSupportedFeatures,
}

impl From<ParseError> for DiscoveryError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// One bounded ServiceSearchAttribute exchange for an A2DP Audio Sink record.
///
/// The client owns no heap memory and borrows the complete accumulation buffer
/// from its caller. Time and transaction allocation remain caller policy. The
/// caller must invoke `poll_timeout`; no timer or retry is hidden here.
pub struct A2dpSinkDiscovery<'a> {
    limits: DiscoveryLimits,
    storage: &'a mut [u8],
    len: usize,
    epoch: u64,
    epoch_known: bool,
    transaction_id: u16,
    deadline: u64,
    chunks: usize,
    continuation: [u8; 16],
    continuation_len: usize,
    seen_continuations: [[u8; 16]; MAX_DISCOVERY_CHUNKS],
    seen_lengths: [u8; MAX_DISCOVERY_CHUNKS],
    seen_count: usize,
    state: DiscoveryState,
}

impl<'a> A2dpSinkDiscovery<'a> {
    pub fn new(storage: &'a mut [u8], limits: DiscoveryLimits) -> Result<Self, DiscoveryError> {
        if limits.max_chunk_bytes < 7
            || limits.max_total_bytes == 0
            || limits.max_total_bytes > storage.len()
            || limits.max_nodes == 0
            || limits.max_depth == 0
            || limits.max_depth > MAX_DEPTH
            || limits.max_chunks == 0
            || limits.max_chunks > MAX_DISCOVERY_CHUNKS
        {
            return Err(DiscoveryError::InvalidLimits);
        }
        Ok(Self {
            limits,
            storage,
            len: 0,
            epoch: 0,
            epoch_known: false,
            transaction_id: 0,
            deadline: 0,
            chunks: 0,
            continuation: [0; 16],
            continuation_len: 0,
            seen_continuations: [[0; 16]; MAX_DISCOVERY_CHUNKS],
            seen_lengths: [0; MAX_DISCOVERY_CHUNKS],
            seen_count: 0,
            state: DiscoveryState::Idle,
        })
    }

    pub const fn state(&self) -> DiscoveryState {
        self.state
    }

    pub const fn epoch(&self) -> Option<u64> {
        if self.epoch_known {
            Some(self.epoch)
        } else {
            None
        }
    }

    pub fn accumulated(&self) -> &[u8] {
        &self.storage[..self.len]
    }

    /// Begin a whole-exchange deadline and encode its first request atomically.
    pub fn start(
        &mut self,
        epoch: u64,
        transaction_id: u16,
        deadline: u64,
        output: &mut [u8],
    ) -> Result<usize, DiscoveryError> {
        if matches!(
            self.state,
            DiscoveryState::AwaitingResponse | DiscoveryState::ContinuationReady
        ) {
            return Err(DiscoveryError::Busy);
        }
        if self.state == DiscoveryState::Disconnected && self.epoch_known && epoch == self.epoch {
            return Err(DiscoveryError::EpochNotAdvanced);
        }
        if self.epoch_known && epoch < self.epoch {
            return Err(DiscoveryError::EpochNotAdvanced);
        }
        let written = encode_request(transaction_id, self.limits.max_chunk_bytes, &[], output)?;
        self.epoch = epoch;
        self.epoch_known = true;
        self.transaction_id = transaction_id;
        self.deadline = deadline;
        self.len = 0;
        self.chunks = 0;
        self.continuation_len = 0;
        self.seen_count = 0;
        self.state = DiscoveryState::AwaitingResponse;
        Ok(written)
    }

    /// Encode exactly one caller-triggered continuation request. The deadline
    /// is not extended, and an output-capacity error leaves state unchanged.
    pub fn continue_request(
        &mut self,
        epoch: u64,
        transaction_id: u16,
        output: &mut [u8],
    ) -> Result<usize, DiscoveryError> {
        self.check_epoch(epoch)?;
        if self.state != DiscoveryState::ContinuationReady {
            return Err(DiscoveryError::NotReady);
        }
        if transaction_id == self.transaction_id {
            return Err(DiscoveryError::TransactionReuse);
        }
        let written = encode_request(
            transaction_id,
            self.limits.max_chunk_bytes,
            &self.continuation[..self.continuation_len],
            output,
        )?;
        self.transaction_id = transaction_id;
        self.state = DiscoveryState::AwaitingResponse;
        Ok(written)
    }

    /// Accept one correlated response. `scratch` is used only for final,
    /// all-or-nothing semantic validation and must hold the completed bytes.
    pub fn accept_response(
        &mut self,
        epoch: u64,
        now: u64,
        bytes: &[u8],
        scratch: &mut [u8],
    ) -> Result<DiscoveryProgress, DiscoveryError> {
        self.check_epoch(epoch)?;
        if self.state != DiscoveryState::AwaitingResponse {
            return Err(DiscoveryError::NotReady);
        }
        if now >= self.deadline {
            self.expire();
            return Err(DiscoveryError::DeadlineExpired);
        }
        let response = ServiceSearchAttributeResponse::parse(
            bytes,
            self.transaction_id,
            self.limits.max_chunk_bytes,
        )?;
        let next_chunks = self
            .chunks
            .checked_add(1)
            .ok_or(DiscoveryError::ChunkLimit)?;
        if next_chunks > self.limits.max_chunks {
            return Err(DiscoveryError::ChunkLimit);
        }
        let next_len = self
            .len
            .checked_add(response.attribute_lists.len())
            .ok_or(DiscoveryError::TotalByteLimit)?;
        if next_len > self.limits.max_total_bytes || next_len > self.storage.len() {
            return Err(DiscoveryError::TotalByteLimit);
        }

        let token = response.continuation.bytes();
        if !token.is_empty() {
            if response.attribute_lists.is_empty() {
                return Err(DiscoveryError::EmptyContinuationChunk);
            }
            if next_chunks == self.limits.max_chunks {
                return Err(DiscoveryError::ChunkLimit);
            }
            if self.has_seen(token) {
                return Err(DiscoveryError::ContinuationLoop);
            }
            self.storage[self.len..next_len].copy_from_slice(response.attribute_lists);
            self.len = next_len;
            self.chunks = next_chunks;
            self.continuation[..token.len()].copy_from_slice(token);
            self.continuation_len = token.len();
            self.seen_continuations[self.seen_count][..token.len()].copy_from_slice(token);
            self.seen_lengths[self.seen_count] = token.len() as u8;
            self.seen_count += 1;
            self.state = DiscoveryState::ContinuationReady;
            return Ok(DiscoveryProgress::ContinuationRequired {
                accumulated_bytes: self.len,
                accepted_chunks: self.chunks,
            });
        }

        if scratch.len() < next_len {
            return Err(DiscoveryError::ScratchTooSmall);
        }
        scratch[..self.len].copy_from_slice(&self.storage[..self.len]);
        scratch[self.len..next_len].copy_from_slice(response.attribute_lists);
        let service = parse_sink_service(
            &scratch[..next_len],
            Limits {
                max_depth: self.limits.max_depth,
                max_elements: self.limits.max_nodes,
                max_bytes: self.limits.max_total_bytes,
            },
        )?;
        self.storage[self.len..next_len].copy_from_slice(response.attribute_lists);
        self.len = next_len;
        self.chunks = next_chunks;
        self.continuation_len = 0;
        self.state = DiscoveryState::Complete;
        Ok(DiscoveryProgress::Complete(service))
    }

    /// Expire the active exchange at the caller's clock boundary. Returns true
    /// exactly when this call performs the transition.
    pub fn poll_timeout(&mut self, epoch: u64, now: u64) -> Result<bool, DiscoveryError> {
        self.check_epoch(epoch)?;
        if matches!(
            self.state,
            DiscoveryState::AwaitingResponse | DiscoveryState::ContinuationReady
        ) && now >= self.deadline
        {
            self.expire();
            return Ok(true);
        }
        Ok(false)
    }

    pub fn cancel(&mut self, epoch: u64) -> Result<(), DiscoveryError> {
        self.check_epoch(epoch)?;
        self.clear_exchange();
        self.state = DiscoveryState::Cancelled;
        Ok(())
    }

    pub fn disconnect(&mut self, epoch: u64) -> Result<(), DiscoveryError> {
        self.check_epoch(epoch)?;
        self.clear_exchange();
        self.state = DiscoveryState::Disconnected;
        Ok(())
    }

    /// Attach the reusable client to a strictly newer connection epoch.
    pub fn reset_epoch(&mut self, new_epoch: u64) -> Result<(), DiscoveryError> {
        if self.epoch_known && new_epoch <= self.epoch {
            return Err(DiscoveryError::EpochNotAdvanced);
        }
        self.clear_exchange();
        self.epoch = new_epoch;
        self.epoch_known = true;
        self.state = DiscoveryState::Idle;
        Ok(())
    }

    fn check_epoch(&self, epoch: u64) -> Result<(), DiscoveryError> {
        if !self.epoch_known || self.epoch != epoch {
            Err(DiscoveryError::EpochMismatch)
        } else {
            Ok(())
        }
    }

    fn has_seen(&self, token: &[u8]) -> bool {
        (0..self.seen_count).any(|index| {
            usize::from(self.seen_lengths[index]) == token.len()
                && self.seen_continuations[index][..token.len()] == *token
        })
    }

    fn expire(&mut self) {
        self.clear_exchange();
        self.state = DiscoveryState::TimedOut;
    }

    fn clear_exchange(&mut self) {
        self.len = 0;
        self.chunks = 0;
        self.continuation_len = 0;
        self.seen_count = 0;
    }
}

fn encode_request(
    transaction_id: u16,
    maximum_attribute_bytes: u16,
    continuation: &[u8],
    output: &mut [u8],
) -> Result<usize, DiscoveryError> {
    if continuation.len() > 16 {
        return Err(DiscoveryError::InvalidLimits);
    }
    let length = 5 + REQUEST_FIXED_PARAMETERS + continuation.len();
    if length > MAX_REQUEST_BYTES || output.len() < length {
        return Err(DiscoveryError::OutputTooSmall);
    }
    let parameter_length = (REQUEST_FIXED_PARAMETERS + continuation.len()) as u16;
    let mut request = [0_u8; MAX_REQUEST_BYTES];
    request[0] = SERVICE_SEARCH_ATTRIBUTE_REQUEST;
    request[1..3].copy_from_slice(&transaction_id.to_be_bytes());
    request[3..5].copy_from_slice(&parameter_length.to_be_bytes());
    request[5..10].copy_from_slice(&[0x35, 0x03, 0x19, 0x11, 0x0b]);
    request[10..12].copy_from_slice(&maximum_attribute_bytes.to_be_bytes());
    request[12..26].copy_from_slice(&[
        0x35, 0x0c, 0x09, 0x00, 0x01, 0x09, 0x00, 0x04, 0x09, 0x00, 0x09, 0x09, 0x03, 0x11,
    ]);
    request[26] = continuation.len() as u8;
    request[27..length].copy_from_slice(continuation);
    output[..length].copy_from_slice(&request[..length]);
    Ok(length)
}

fn parse_sink_service(bytes: &[u8], limits: Limits) -> Result<SinkService, DiscoveryError> {
    let DataElement::Sequence(mut records) = DataElement::parse(bytes, limits)? else {
        return Err(DiscoveryError::InvalidAttributeList);
    };
    let record = records
        .next()
        .transpose()?
        .ok_or(DiscoveryError::InvalidAttributeList)?;
    if records.next().transpose()?.is_some() {
        return Err(DiscoveryError::MultipleRecords);
    }
    let DataElement::Sequence(mut attributes) = record else {
        return Err(DiscoveryError::InvalidAttributeList);
    };
    let mut service_class = None;
    let mut protocol = None;
    let mut profile = None;
    let mut supported_features = None;
    while let Some((id, id_encoded)) = attributes.next_encoded().transpose()? {
        let id = exact_u16(id, id_encoded, DiscoveryError::InvalidAttributeList)?;
        let (value, value_encoded) = attributes
            .next_encoded()
            .transpose()?
            .ok_or(DiscoveryError::InvalidAttributeList)?;
        match id {
            SERVICE_CLASS_ID_LIST => set_once(&mut service_class, (value, value_encoded), id)?,
            PROTOCOL_DESCRIPTOR_LIST => set_once(&mut protocol, (value, value_encoded), id)?,
            BLUETOOTH_PROFILE_DESCRIPTOR_LIST => {
                set_once(&mut profile, (value, value_encoded), id)?
            }
            SUPPORTED_FEATURES => set_once(&mut supported_features, (value, value_encoded), id)?,
            _ => return Err(DiscoveryError::UnexpectedAttribute(id)),
        }
    }
    validate_service_class(service_class.ok_or(DiscoveryError::WrongServiceClass)?.0)?;
    let avdtp_version = validate_protocol(
        protocol
            .ok_or(DiscoveryError::WrongProtocolDescriptorList)?
            .0,
    )?;
    let profile_version =
        validate_profile(profile.ok_or(DiscoveryError::WrongProfileDescriptorList)?.0)?;
    let supported_features = match supported_features {
        Some((value, encoded)) => Some(exact_u16(
            value,
            encoded,
            DiscoveryError::InvalidSupportedFeatures,
        )?),
        None => None,
    };
    Ok(SinkService {
        avdtp_version,
        profile_version,
        supported_features,
    })
}

fn set_once<'a>(
    slot: &mut Option<(DataElement<'a>, &'a [u8])>,
    value: (DataElement<'a>, &'a [u8]),
    id: u16,
) -> Result<(), DiscoveryError> {
    if slot.replace(value).is_some() {
        Err(DiscoveryError::DuplicateAttribute(id))
    } else {
        Ok(())
    }
}

fn validate_service_class(value: DataElement<'_>) -> Result<(), DiscoveryError> {
    let DataElement::Sequence(classes) = value else {
        return Err(DiscoveryError::WrongServiceClass);
    };
    let mut found = false;
    for class in classes {
        match class? {
            DataElement::Uuid(Uuid::U16(AUDIO_SINK)) if !found => found = true,
            DataElement::Uuid(_) => {}
            _ => return Err(DiscoveryError::WrongServiceClass),
        }
    }
    if found {
        Ok(())
    } else {
        Err(DiscoveryError::WrongServiceClass)
    }
}

fn validate_protocol(value: DataElement<'_>) -> Result<u16, DiscoveryError> {
    let DataElement::Sequence(mut descriptors) = value else {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    };
    let l2cap = descriptors
        .next()
        .transpose()?
        .ok_or(DiscoveryError::WrongProtocolDescriptorList)?;
    let avdtp = descriptors
        .next()
        .transpose()?
        .ok_or(DiscoveryError::WrongProtocolDescriptorList)?;
    if descriptors.next().transpose()?.is_some() {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    let DataElement::Sequence(mut l2cap) = l2cap else {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    };
    if l2cap.next().transpose()? != Some(DataElement::Uuid(Uuid::U16(L2CAP))) {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    let (psm, psm_encoded) = l2cap
        .next_encoded()
        .transpose()?
        .ok_or(DiscoveryError::WrongProtocolDescriptorList)?;
    if exact_u16(
        psm,
        psm_encoded,
        DiscoveryError::WrongProtocolDescriptorList,
    )? != AVDTP_PSM
        || l2cap.next().transpose()?.is_some()
    {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    let DataElement::Sequence(mut avdtp) = avdtp else {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    };
    if avdtp.next().transpose()? != Some(DataElement::Uuid(Uuid::U16(AVDTP))) {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    let (version, version_encoded) = avdtp
        .next_encoded()
        .transpose()?
        .ok_or(DiscoveryError::WrongProtocolDescriptorList)?;
    if avdtp.next().transpose()?.is_some() {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    let version = exact_u16(
        version,
        version_encoded,
        DiscoveryError::WrongProtocolDescriptorList,
    )?;
    if version == 0 {
        return Err(DiscoveryError::WrongProtocolDescriptorList);
    }
    Ok(version)
}

fn validate_profile(value: DataElement<'_>) -> Result<u16, DiscoveryError> {
    let DataElement::Sequence(mut descriptors) = value else {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    };
    let descriptor = descriptors
        .next()
        .transpose()?
        .ok_or(DiscoveryError::WrongProfileDescriptorList)?;
    if descriptors.next().transpose()?.is_some() {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    }
    let DataElement::Sequence(mut descriptor) = descriptor else {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    };
    if descriptor.next().transpose()?
        != Some(DataElement::Uuid(Uuid::U16(ADVANCED_AUDIO_DISTRIBUTION)))
    {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    }
    let (version, version_encoded) = descriptor
        .next_encoded()
        .transpose()?
        .ok_or(DiscoveryError::WrongProfileDescriptorList)?;
    if descriptor.next().transpose()?.is_some() {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    }
    let version = exact_u16(
        version,
        version_encoded,
        DiscoveryError::WrongProfileDescriptorList,
    )?;
    if version == 0 {
        return Err(DiscoveryError::WrongProfileDescriptorList);
    }
    Ok(version)
}

fn exact_u16(
    value: DataElement<'_>,
    encoded: &[u8],
    error: DiscoveryError,
) -> Result<u16, DiscoveryError> {
    let DataElement::Unsigned(value) = value else {
        return Err(error);
    };
    if encoded.len() != 3 || encoded[0] != 0x09 {
        return Err(error);
    }
    u16::try_from(value).map_err(|_| error)
}
