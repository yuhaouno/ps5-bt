// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{Message, MessageType, PacketType, ParseError, SignalId};

/// The caller must bind this identity to one ordered signaling L2CAP channel.
/// Reusing a channel identifier after reconnect requires a newer epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalingContext {
    pub epoch: u64,
    pub channel_id: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReassemblyLimits {
    /// Includes Start and End; at least two, at most the wire NOSP limit (255).
    pub max_packets: u8,
    /// Maximum complete AVDTP packet (one L2CAP SDU), including its header.
    pub max_packet_bytes: usize,
    /// Fixed duration from Start, never extended by a Continue packet.
    pub timeout_us: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReassemblyError {
    InvalidLimits,
    InvalidContext,
    ContextMismatch,
    EpochNotAdvanced,
    ClockRegressed,
    DeadlineOverflow,
    Timeout,
    ResetRequired,
    Malformed(ParseError),
    PacketTooLarge,
    BufferTooSmall,
    PacketLimit,
    InProgress,
    NoStart,
    TransactionMismatch,
    MessageTypeMismatch,
    UnexpectedPacketCount,
    InvalidPacketLength,
}

/// A complete message, independent of the wire's fragmentation headers.
/// Only parsing or successful bounded reassembly can construct this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompleteMessage<'a> {
    transaction_label: u8,
    message_type: MessageType,
    signal_id: Option<u8>,
    payload: &'a [u8],
}

impl<'a> CompleteMessage<'a> {
    pub const fn transaction_label(self) -> u8 {
        self.transaction_label
    }

    pub const fn message_type(self) -> MessageType {
        self.message_type
    }

    pub const fn raw_signal_id(self) -> Option<u8> {
        self.signal_id
    }

    pub const fn signal_id(self) -> Option<SignalId> {
        match self.signal_id {
            Some(raw) => SignalId::from_raw(raw),
            None => None,
        }
    }

    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }

    pub(crate) fn single(message: Message<'a>) -> Self {
        debug_assert_eq!(message.header().packet_type(), PacketType::Single);
        Self {
            transaction_label: message.header().transaction_label(),
            message_type: message.header().message_type(),
            signal_id: message.header().raw_signal_id(),
            payload: message.payload(),
        }
    }
}

/// Observable partial-message accounting, also returned by cancel/reset/expire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialMessage {
    pub context: SignalingContext,
    pub transaction_label: u8,
    pub message_type: MessageType,
    pub signal_id: u8,
    pub declared_packets: u8,
    pub received_packets: u8,
    pub payload_bytes: usize,
    pub start_packet_bytes: usize,
    pub deadline_us: u64,
}

/// AVDTP 1.3 sections 8.3, 8.4 and 8.20.4. One message at a time, with
/// caller-owned payload storage and caller-supplied monotonic microseconds.
/// Rejected packets change neither partial state nor buffer contents. Timeout
/// is reported until the caller explicitly expires/cancels/resets the message.
pub struct SignalingReassembler<'buf> {
    context: SignalingContext,
    limits: ReassemblyLimits,
    storage: &'buf mut [u8],
    partial: Option<PartialMessage>,
    transaction_generation: Option<u64>,
    requires_reset: bool,
    last_now_us: Option<u64>,
}

impl<'buf> SignalingReassembler<'buf> {
    pub fn new(
        context: SignalingContext,
        storage: &'buf mut [u8],
        limits: ReassemblyLimits,
    ) -> Result<Self, ReassemblyError> {
        Self::validate_context(context)?;
        if limits.max_packets < 2
            || !(3..=u16::MAX as usize).contains(&limits.max_packet_bytes)
            || limits.timeout_us == 0
        {
            return Err(ReassemblyError::InvalidLimits);
        }
        Ok(Self {
            context,
            limits,
            storage,
            partial: None,
            transaction_generation: None,
            requires_reset: false,
            last_now_us: None,
        })
    }

    pub const fn context(&self) -> SignalingContext {
        self.context
    }

    pub const fn partial(&self) -> Option<PartialMessage> {
        self.partial
    }

    pub const fn requires_reset(&self) -> bool {
        self.requires_reset
    }

    pub fn push<'a>(
        &'a mut self,
        context: SignalingContext,
        now_us: u64,
        packet: &[u8],
    ) -> Result<Option<CompleteMessage<'a>>, ReassemblyError> {
        let message = self.preflight(context, now_us, packet)?;
        self.push_parsed(now_us, packet.len(), message, None)
    }

    pub(crate) fn preflight<'a>(
        &self,
        context: SignalingContext,
        now_us: u64,
        packet: &'a [u8],
    ) -> Result<Message<'a>, ReassemblyError> {
        self.check_context(context)?;
        if self.requires_reset {
            return Err(ReassemblyError::ResetRequired);
        }
        self.check_clock(now_us)?;
        if self.partial.is_some_and(|part| now_us >= part.deadline_us) {
            return Err(ReassemblyError::Timeout);
        }
        if packet.len() > self.limits.max_packet_bytes {
            return Err(ReassemblyError::PacketTooLarge);
        }
        let message = Message::parse(packet).map_err(ReassemblyError::Malformed)?;
        if message.header().raw_signal_id().is_some() && message.header().signal_id().is_none() {
            return Err(ReassemblyError::Malformed(ParseError::ReservedSignal));
        }
        Ok(message)
    }

    pub(crate) fn push_parsed<'a>(
        &'a mut self,
        now_us: u64,
        packet_bytes: usize,
        message: Message<'_>,
        transaction_generation: Option<u64>,
    ) -> Result<Option<CompleteMessage<'a>>, ReassemblyError> {
        let header = message.header();
        let payload = message.payload();
        match header.packet_type() {
            PacketType::Single => {
                if self.partial.is_some() {
                    return Err(ReassemblyError::InProgress);
                }
                if payload.len() > self.storage.len() {
                    return Err(ReassemblyError::BufferTooSmall);
                }
                self.storage[..payload.len()].copy_from_slice(payload);
                self.last_now_us = Some(now_us);
                Ok(Some(CompleteMessage {
                    transaction_label: header.transaction_label(),
                    message_type: header.message_type(),
                    signal_id: header.raw_signal_id(),
                    payload: &self.storage[..payload.len()],
                }))
            }
            PacketType::Start => {
                if self.partial.is_some() {
                    return Err(ReassemblyError::InProgress);
                }
                let count = header
                    .packet_count()
                    .ok_or(ReassemblyError::Malformed(ParseError::InvalidPacketCount))?;
                if count > self.limits.max_packets {
                    return Err(ReassemblyError::PacketLimit);
                }
                let deadline_us = now_us
                    .checked_add(self.limits.timeout_us)
                    .ok_or(ReassemblyError::DeadlineOverflow)?;
                // All Continue SDUs must be as long as Start. Reject a declared
                // sequence whose mandatory prefix already exceeds storage;
                // End may be shorter, so do not require its maximum to fit.
                let minimum = (usize::from(count) - 2)
                    .checked_mul(packet_bytes - 1)
                    .and_then(|bytes| bytes.checked_add(payload.len()))
                    .ok_or(ReassemblyError::BufferTooSmall)?;
                if minimum > self.storage.len() {
                    return Err(ReassemblyError::BufferTooSmall);
                }
                let part = PartialMessage {
                    context: self.context,
                    transaction_label: header.transaction_label(),
                    message_type: header.message_type(),
                    signal_id: header
                        .raw_signal_id()
                        .ok_or(ReassemblyError::Malformed(ParseError::ReservedSignal))?,
                    declared_packets: count,
                    received_packets: 1,
                    payload_bytes: payload.len(),
                    start_packet_bytes: packet_bytes,
                    deadline_us,
                };
                self.storage[..payload.len()].copy_from_slice(payload);
                self.partial = Some(part);
                self.transaction_generation = transaction_generation;
                self.last_now_us = Some(now_us);
                Ok(None)
            }
            PacketType::Continue | PacketType::End => {
                let mut part = self.partial.ok_or(ReassemblyError::NoStart)?;
                if self.transaction_generation != transaction_generation {
                    return Err(ReassemblyError::TransactionMismatch);
                }
                if header.transaction_label() != part.transaction_label {
                    return Err(ReassemblyError::TransactionMismatch);
                }
                if header.message_type() != part.message_type {
                    return Err(ReassemblyError::MessageTypeMismatch);
                }
                let next_count = u16::from(part.received_packets) + 1;
                let is_end = header.packet_type() == PacketType::End;
                if (is_end && next_count != u16::from(part.declared_packets))
                    || (!is_end && next_count >= u16::from(part.declared_packets))
                {
                    return Err(ReassemblyError::UnexpectedPacketCount);
                }
                if (!is_end && packet_bytes != part.start_packet_bytes)
                    || (is_end && packet_bytes > part.start_packet_bytes)
                {
                    return Err(ReassemblyError::InvalidPacketLength);
                }
                let total = part
                    .payload_bytes
                    .checked_add(payload.len())
                    .filter(|total| *total <= self.storage.len())
                    .ok_or(ReassemblyError::BufferTooSmall)?;
                self.storage[part.payload_bytes..total].copy_from_slice(payload);
                self.last_now_us = Some(now_us);
                if is_end {
                    self.partial = None;
                    self.transaction_generation = None;
                    Ok(Some(CompleteMessage {
                        transaction_label: part.transaction_label,
                        message_type: part.message_type,
                        signal_id: Some(part.signal_id),
                        payload: &self.storage[..total],
                    }))
                } else {
                    part.received_packets += 1;
                    part.payload_bytes = total;
                    self.partial = Some(part);
                    Ok(None)
                }
            }
        }
    }

    /// Clears a partial response only; it does not cancel the Initiator's
    /// pending command. Cancelling an active message quarantines this receiver
    /// until reset to a newer epoch; also invalidate the Initiator's session.
    pub fn cancel(
        &mut self,
        context: SignalingContext,
    ) -> Result<Option<PartialMessage>, ReassemblyError> {
        self.check_context(context)?;
        self.transaction_generation = None;
        let cancelled = self.partial.take();
        self.requires_reset |= cancelled.is_some();
        Ok(cancelled)
    }

    /// Returns and clears an expired message at the exclusive deadline.
    pub fn expire(
        &mut self,
        context: SignalingContext,
        now_us: u64,
    ) -> Result<Option<PartialMessage>, ReassemblyError> {
        self.check_context(context)?;
        self.check_clock(now_us)?;
        self.last_now_us = Some(now_us);
        if self.partial.is_some_and(|part| now_us >= part.deadline_us) {
            self.transaction_generation = None;
            self.requires_reset = true;
            Ok(self.partial.take())
        } else {
            Ok(None)
        }
    }

    /// Rebinding requires a strictly newer epoch even if the CID is unchanged.
    /// The returned accounting makes any abandoned partial response observable.
    pub fn reset(
        &mut self,
        context: SignalingContext,
    ) -> Result<Option<PartialMessage>, ReassemblyError> {
        Self::validate_context(context)?;
        if context.epoch <= self.context.epoch {
            return Err(ReassemblyError::EpochNotAdvanced);
        }
        let cancelled = self.partial.take();
        self.transaction_generation = None;
        self.requires_reset = false;
        self.context = context;
        self.last_now_us = None;
        Ok(cancelled)
    }

    pub(crate) fn check_context(&self, context: SignalingContext) -> Result<(), ReassemblyError> {
        if context != self.context {
            return Err(ReassemblyError::ContextMismatch);
        }
        Ok(())
    }

    fn validate_context(context: SignalingContext) -> Result<(), ReassemblyError> {
        if context.channel_id < 0x0040 {
            return Err(ReassemblyError::InvalidContext);
        }
        Ok(())
    }

    fn check_clock(&self, now_us: u64) -> Result<(), ReassemblyError> {
        if self.last_now_us.is_some_and(|previous| now_us < previous) {
            return Err(ReassemblyError::ClockRegressed);
        }
        Ok(())
    }
}
