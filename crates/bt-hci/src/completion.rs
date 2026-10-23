// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ConnectionHandle, Event, Opcode, ParseError};

/// Raw command completion. Credits are observations, not an owned reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandComplete<'a> {
    pub command_credits: u8,
    pub opcode: Opcode,
    pub return_parameters: &'a [u8],
}

impl<'a> CommandComplete<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let event = Event::parse(bytes)?;
        if event.code != 0x0e {
            return Err(ParseError::UnexpectedEvent);
        }
        let p = event.parameters;
        if p.len() < 3 {
            return Err(ParseError::Length);
        }
        Ok(Self {
            command_credits: p[0],
            opcode: Opcode::from_raw(u16::from_le_bytes([p[1], p[2]])),
            return_parameters: &p[3..],
        })
    }

    /// These information commands have a status byte followed by fixed data.
    fn success(bytes: &'a [u8], opcode: Opcode, length: usize) -> Result<Self, ParseError> {
        let completion = Self::parse(bytes)?;
        if completion.opcode != opcode {
            return Err(ParseError::UnexpectedOpcode);
        }
        let status = *completion
            .return_parameters
            .first()
            .ok_or(ParseError::Length)?;
        // Failed commands may return only Status; do not interpret undefined data.
        if status != 0 {
            return Err(ParseError::ControllerStatus(status));
        }
        if completion.return_parameters.len() != length {
            return Err(ParseError::Length);
        }
        Ok(completion)
    }
}

/// Raw status, including failures; not a parse error or a command dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandStatus {
    pub status: u8,
    pub command_credits: u8,
    pub opcode: Opcode,
}

impl CommandStatus {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let event = Event::parse(bytes)?;
        if event.code != 0x0f {
            return Err(ParseError::UnexpectedEvent);
        }
        let p = event.parameters;
        if p.len() != 4 {
            return Err(ParseError::Length);
        }
        Ok(Self {
            status: p[0],
            command_credits: p[1],
            opcode: Opcode::from_raw(u16::from_le_bytes([p[2], p[3]])),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalSupportedCommands {
    pub command_credits: u8,
    /// Raw specification bit order. No command support inferred from HCI version.
    pub commands: [u8; 64],
}

impl LocalSupportedCommands {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let c = CommandComplete::success(bytes, Opcode::READ_LOCAL_SUPPORTED_COMMANDS, 65)?;
        let mut commands = [0; 64];
        commands.copy_from_slice(&c.return_parameters[1..]);
        Ok(Self {
            command_credits: c.command_credits,
            commands,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalSupportedFeatures {
    pub command_credits: u8,
    pub features: [u8; 8],
}

impl LocalSupportedFeatures {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let c = CommandComplete::success(bytes, Opcode::READ_LOCAL_SUPPORTED_FEATURES, 9)?;
        let mut features = [0; 8];
        features.copy_from_slice(&c.return_parameters[1..]);
        Ok(Self {
            command_credits: c.command_credits,
            features,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalExtendedFeatures {
    pub command_credits: u8,
    pub page: u8,
    pub max_page: u8,
    pub features: [u8; 8],
}

impl LocalExtendedFeatures {
    /// Caller must match the returned page against the outstanding request.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let c = CommandComplete::success(bytes, Opcode::READ_LOCAL_EXTENDED_FEATURES, 11)?;
        let p = c.return_parameters;
        let mut features = [0; 8];
        features.copy_from_slice(&p[3..]);
        Ok(Self {
            command_credits: c.command_credits,
            page: p[1],
            max_page: p[2],
            features,
        })
    }
}

/// Reported controller-wide capacities, never an ACL budget for this client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferSize {
    pub command_credits: u8,
    pub acl_packet_length: u16,
    pub synchronous_packet_length: u8,
    pub total_acl_packets: u16,
    pub total_synchronous_packets: u16,
}

impl BufferSize {
    /// Preserves raw observations (including unexpected zeros) for diagnostics.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let c = CommandComplete::success(bytes, Opcode::READ_BUFFER_SIZE, 8)?;
        let p = c.return_parameters;
        Ok(Self {
            command_credits: c.command_credits,
            acl_packet_length: u16::from_le_bytes([p[1], p[2]]),
            synchronous_packet_length: p[3],
            total_acl_packets: u16::from_le_bytes([p[4], p[5]]),
            total_synchronous_packets: u16::from_le_bytes([p[6], p[7]]),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub handle: ConnectionHandle,
    pub packets: u16,
}

/// Validates every entry before exposing any, retaining foreign handles verbatim.
/// Does not aggregate, replenish credits or infer epochs/ownership from a handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletedPackets<'a> {
    entries: &'a [u8],
}

impl<'a> CompletedPackets<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let event = Event::parse(bytes)?;
        if event.code != 0x13 {
            return Err(ParseError::UnexpectedEvent);
        }
        let p = event.parameters;
        let count = usize::from(*p.first().ok_or(ParseError::Length)?);
        if p.len() != 1 + count * 4 {
            return Err(ParseError::Length);
        }
        for entry in p[1..].chunks_exact(4) {
            ConnectionHandle::new(u16::from_le_bytes([entry[0], entry[1]]))?;
        }
        Ok(Self { entries: &p[1..] })
    }

    pub fn len(self) -> usize {
        self.entries.len() / 4
    }

    pub fn is_empty(self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = Completion> + 'a {
        self.entries.chunks_exact(4).map(|entry| Completion {
            // Validated in parse; private storage is immutable thereafter.
            handle: ConnectionHandle(u16::from_le_bytes([entry[0], entry[1]])),
            packets: u16::from_le_bytes([entry[2], entry[3]]),
        })
    }
}
