// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketType {
    Single,
    Start,
    Continue,
    End,
}

impl PacketType {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Single,
            1 => Self::Start,
            2 => Self::Continue,
            _ => Self::End,
        }
    }

    const fn bits(self) -> u8 {
        match self {
            Self::Single => 0,
            Self::Start => 1,
            Self::Continue => 2,
            Self::End => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageType {
    Command,
    GeneralReject,
    Accept,
    Reject,
}

impl MessageType {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Command,
            1 => Self::GeneralReject,
            2 => Self::Accept,
            _ => Self::Reject,
        }
    }

    const fn bits(self) -> u8 {
        match self {
            Self::Command => 0,
            Self::GeneralReject => 1,
            Self::Accept => 2,
            Self::Reject => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalId {
    Discover = 0x01,
    GetCapabilities = 0x02,
    SetConfiguration = 0x03,
    GetConfiguration = 0x04,
    Reconfigure = 0x05,
    Open = 0x06,
    Start = 0x07,
    Close = 0x08,
    Suspend = 0x09,
    Abort = 0x0a,
    SecurityControl = 0x0b,
    GetAllCapabilities = 0x0c,
    DelayReport = 0x0d,
}

impl SignalId {
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0x01 => Some(Self::Discover),
            0x02 => Some(Self::GetCapabilities),
            0x03 => Some(Self::SetConfiguration),
            0x04 => Some(Self::GetConfiguration),
            0x05 => Some(Self::Reconfigure),
            0x06 => Some(Self::Open),
            0x07 => Some(Self::Start),
            0x08 => Some(Self::Close),
            0x09 => Some(Self::Suspend),
            0x0a => Some(Self::Abort),
            0x0b => Some(Self::SecurityControl),
            0x0c => Some(Self::GetAllCapabilities),
            0x0d => Some(Self::DelayReport),
            _ => None,
        }
    }

    pub const fn raw(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
    InvalidHeader,
    InvalidPacketCount,
    ReservedSignal,
    InvalidEndpoint,
    InvalidEndpointList,
    InvalidMediaType,
}

/// A typed AVDTP signaling header. `signal_id` remains raw so unknown or
/// reserved identifiers are not silently normalized before validation. A
/// General Reject has no Signal Identifier and is correlated by transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    transaction_label: u8,
    packet_type: PacketType,
    message_type: MessageType,
    signal_id: Option<u8>,
    packet_count: Option<u8>,
}

impl Header {
    pub fn single(
        transaction_label: u8,
        message_type: MessageType,
        signal_id: SignalId,
    ) -> Result<Self, ParseError> {
        if transaction_label > 0x0f || message_type == MessageType::GeneralReject {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            transaction_label,
            packet_type: PacketType::Single,
            message_type,
            signal_id: Some(signal_id.raw()),
            packet_count: None,
        })
    }

    /// A General Reject carries only the first signaling-header octet and does
    /// not contain a Signal Identifier.
    pub fn general_reject(transaction_label: u8) -> Result<Self, ParseError> {
        if transaction_label > 0x0f {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            transaction_label,
            packet_type: PacketType::Single,
            message_type: MessageType::GeneralReject,
            signal_id: None,
            packet_count: None,
        })
    }

    pub fn start(
        transaction_label: u8,
        message_type: MessageType,
        signal_id: SignalId,
        packet_count: u8,
    ) -> Result<Self, ParseError> {
        if transaction_label > 0x0f
            || message_type == MessageType::GeneralReject
            || packet_count < 2
        {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            transaction_label,
            packet_type: PacketType::Start,
            message_type,
            signal_id: Some(signal_id.raw()),
            packet_count: Some(packet_count),
        })
    }

    pub fn continuation(
        transaction_label: u8,
        packet_type: PacketType,
        message_type: MessageType,
    ) -> Result<Self, ParseError> {
        if transaction_label > 0x0f
            || !matches!(packet_type, PacketType::Continue | PacketType::End)
            || message_type == MessageType::GeneralReject
        {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            transaction_label,
            packet_type,
            message_type,
            signal_id: None,
            packet_count: None,
        })
    }

    pub const fn transaction_label(self) -> u8 {
        self.transaction_label
    }

    pub const fn packet_type(self) -> PacketType {
        self.packet_type
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

    pub const fn packet_count(self) -> Option<u8> {
        self.packet_count
    }

    pub const fn encoded_len(self) -> usize {
        match self.packet_type {
            PacketType::Single if matches!(self.message_type, MessageType::GeneralReject) => 1,
            PacketType::Single => 2,
            PacketType::Start => 3,
            PacketType::Continue | PacketType::End => 1,
        }
    }

    /// Encodes only after checking the complete destination size.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, ParseError> {
        let length = self.encoded_len();
        if output.len() < length {
            return Err(ParseError::Truncated);
        }
        let first = (self.transaction_label << 4)
            | (self.packet_type.bits() << 2)
            | self.message_type.bits();
        match self.packet_type {
            PacketType::Single => {
                if self.message_type == MessageType::GeneralReject {
                    if self.signal_id.is_some() || self.packet_count.is_some() {
                        return Err(ParseError::InvalidHeader);
                    }
                    output[0] = first;
                } else {
                    let signal = self.signal_id.ok_or(ParseError::InvalidHeader)?;
                    if signal == 0 || signal > 0x3f || self.packet_count.is_some() {
                        return Err(ParseError::InvalidHeader);
                    }
                    output[0] = first;
                    output[1] = signal;
                }
            }
            PacketType::Start => {
                let signal = self.signal_id.ok_or(ParseError::InvalidHeader)?;
                let count = self.packet_count.ok_or(ParseError::InvalidHeader)?;
                if signal == 0 || signal > 0x3f || count == 0 {
                    return Err(ParseError::InvalidHeader);
                }
                output[0] = first;
                output[1] = count;
                output[2] = signal;
            }
            PacketType::Continue | PacketType::End => {
                if self.signal_id.is_some() || self.packet_count.is_some() {
                    return Err(ParseError::InvalidHeader);
                }
                output[0] = first;
            }
        }
        Ok(length)
    }

    fn parse(bytes: &[u8]) -> Result<(Self, usize), ParseError> {
        let first = *bytes.first().ok_or(ParseError::Truncated)?;
        let transaction_label = first >> 4;
        let packet_type = PacketType::from_bits((first >> 2) & 0x03);
        let message_type = MessageType::from_bits(first & 0x03);
        if message_type == MessageType::GeneralReject {
            if packet_type != PacketType::Single {
                return Err(ParseError::InvalidHeader);
            }
            return Ok((
                Self {
                    transaction_label,
                    packet_type,
                    message_type,
                    signal_id: None,
                    packet_count: None,
                },
                1,
            ));
        }
        match packet_type {
            PacketType::Single => {
                let signal = *bytes.get(1).ok_or(ParseError::Truncated)?;
                if signal & 0xc0 != 0 {
                    return Err(ParseError::InvalidHeader);
                }
                let raw = signal & 0x3f;
                if raw == 0 {
                    return Err(ParseError::ReservedSignal);
                }
                Ok((
                    Self {
                        transaction_label,
                        packet_type,
                        message_type,
                        signal_id: Some(raw),
                        packet_count: None,
                    },
                    2,
                ))
            }
            PacketType::Start => {
                let packet_count = *bytes.get(1).ok_or(ParseError::Truncated)?;
                let signal = *bytes.get(2).ok_or(ParseError::Truncated)?;
                if packet_count < 2 {
                    return Err(ParseError::InvalidPacketCount);
                }
                if signal & 0xc0 != 0 {
                    return Err(ParseError::InvalidHeader);
                }
                let raw = signal & 0x3f;
                if raw == 0 {
                    return Err(ParseError::ReservedSignal);
                }
                Ok((
                    Self {
                        transaction_label,
                        packet_type,
                        message_type,
                        signal_id: Some(raw),
                        packet_count: Some(packet_count),
                    },
                    3,
                ))
            }
            PacketType::Continue | PacketType::End => Ok((
                Self {
                    transaction_label,
                    packet_type,
                    message_type,
                    signal_id: None,
                    packet_count: None,
                },
                1,
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message<'a> {
    header: Header,
    payload: &'a [u8],
}

impl<'a> Message<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let (header, header_len) = Header::parse(bytes)?;
        if header.message_type() == MessageType::GeneralReject && bytes.len() != header_len {
            return Err(ParseError::InvalidHeader);
        }
        Ok(Self {
            header,
            payload: &bytes[header_len..],
        })
    }

    pub const fn header(self) -> Header {
        self.header
    }

    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }

    /// Encodes a complete packet without modifying output on failure.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, ParseError> {
        let header_len = self.header.encoded_len();
        let total = header_len
            .checked_add(self.payload.len())
            .ok_or(ParseError::InvalidHeader)?;
        if output.len() < total {
            return Err(ParseError::Truncated);
        }
        self.header.encode(&mut output[..header_len])?;
        output[header_len..total].copy_from_slice(self.payload);
        Ok(total)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamEndpointId(u8);

impl StreamEndpointId {
    pub const fn new(value: u8) -> Result<Self, ParseError> {
        if value >= 1 && value <= 0x3e {
            Ok(Self(value))
        } else {
            Err(ParseError::InvalidEndpoint)
        }
    }

    pub const fn value(self) -> u8 {
        self.0
    }

    pub const fn wire(self) -> u8 {
        self.0 << 2
    }

    pub fn parse_wire(wire: u8) -> Result<Self, ParseError> {
        if wire & 0x03 != 0 {
            return Err(ParseError::InvalidEndpoint);
        }
        Self::new(wire >> 2)
    }

    fn parse_discovery_wire(wire: u8) -> Result<Self, ParseError> {
        // Bit 1 is In Use in Discover responses; bit 0 remains RFA.
        if wire & 0x01 != 0 {
            return Err(ParseError::InvalidEndpoint);
        }
        Self::new(wire >> 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Audio,
    Video,
    Multimedia,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointType {
    Source,
    Sink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamEndpoint {
    pub id: StreamEndpointId,
    pub in_use: bool,
    pub media_type: MediaType,
    pub endpoint_type: EndpointType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamEndpoints<'a> {
    bytes: &'a [u8],
}

impl<'a> StreamEndpoints<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
            return Err(ParseError::InvalidEndpointList);
        }
        for endpoint in (Self { bytes }).iter() {
            endpoint?;
        }
        Ok(Self { bytes })
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / 2
    }

    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }

    pub fn iter(self) -> StreamEndpointIter<'a> {
        StreamEndpointIter {
            remaining: self.bytes,
        }
    }

    pub fn first_available_audio_sink(self) -> Option<StreamEndpoint> {
        self.iter().flatten().find(|endpoint| {
            !endpoint.in_use
                && endpoint.media_type == MediaType::Audio
                && endpoint.endpoint_type == EndpointType::Sink
        })
    }
}

pub struct StreamEndpointIter<'a> {
    remaining: &'a [u8],
}

impl Iterator for StreamEndpointIter<'_> {
    type Item = Result<StreamEndpoint, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let first = self.remaining[0];
        let second = self.remaining[1];
        self.remaining = &self.remaining[2..];
        let id = match StreamEndpointId::parse_discovery_wire(first) {
            Ok(id) => id,
            Err(error) => return Some(Err(error)),
        };
        if second & 0x07 != 0 {
            return Some(Err(ParseError::InvalidEndpointList));
        }
        let media_type = match second >> 4 {
            0 => MediaType::Audio,
            1 => MediaType::Video,
            2 => MediaType::Multimedia,
            _ => return Some(Err(ParseError::InvalidMediaType)),
        };
        Some(Ok(StreamEndpoint {
            id,
            in_use: first & 0x02 != 0,
            media_type,
            endpoint_type: if second & 0x08 == 0 {
                EndpointType::Source
            } else {
                EndpointType::Sink
            },
        }))
    }
}
