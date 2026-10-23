// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::Error;

use crate::ParseError;

/// Raw HCI opcode. Unknown values are preserved, including opcode zero in events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opcode(u16);

impl Opcode {
    pub const READ_LOCAL_VERSION: Self = Self(0x1001);
    pub const READ_LOCAL_SUPPORTED_COMMANDS: Self = Self(0x1002);
    pub const READ_LOCAL_SUPPORTED_FEATURES: Self = Self(0x1003);
    pub const READ_LOCAL_EXTENDED_FEATURES: Self = Self(0x1004);
    pub const READ_BUFFER_SIZE: Self = Self(0x1005);

    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// Checks field widths only; does not assert that a command is implemented.
    pub fn from_fields(ogf: u8, ocf: u16) -> Result<Self, Error> {
        if ogf > 0x3f || ocf > 0x03ff {
            return Err(Error::Invalid);
        }
        Ok(Self((u16::from(ogf) << 10) | ocf))
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn ogf(self) -> u8 {
        (self.0 >> 10) as u8
    }

    pub const fn ocf(self) -> u16 {
        self.0 & 0x03ff
    }
}

/// One complete command, without H4. This type grants no permission or credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command<'a> {
    opcode: Opcode,
    parameters: &'a [u8],
}

impl<'a> Command<'a> {
    pub fn new(opcode: Opcode, parameters: &'a [u8]) -> Result<Self, Error> {
        if parameters.len() > usize::from(u8::MAX) {
            return Err(Error::Invalid);
        }
        Ok(Self { opcode, parameters })
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 3 || bytes.len() != usize::from(bytes[2]) + 3 {
            return Err(ParseError::Length);
        }
        Ok(Self {
            opcode: Opcode::from_raw(u16::from_le_bytes([bytes[0], bytes[1]])),
            parameters: &bytes[3..],
        })
    }

    pub const fn opcode(self) -> Opcode {
        self.opcode
    }

    pub const fn parameters(self) -> &'a [u8] {
        self.parameters
    }

    /// Writes exactly one header-bearing command. Rejection leaves output intact.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, Error> {
        let length = self.parameters.len() + 3;
        if output.len() < length {
            return Err(Error::BufferTooSmall);
        }
        output[..2].copy_from_slice(&self.opcode.raw().to_le_bytes());
        output[2] = self.parameters.len() as u8;
        output[3..length].copy_from_slice(self.parameters);
        Ok(length)
    }
}
