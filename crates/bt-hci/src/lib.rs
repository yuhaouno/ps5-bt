// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal diagnostic decoder. See docs/PROTOCOL.md for specification references.
#![no_std]
#![forbid(unsafe_code)]

/// HCI Read Local Version Information (no H4 indicator).
pub const READ_LOCAL_VERSION: [u8; 3] = [0x01, 0x10, 0x00];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Length,
    UnexpectedEvent,
    UnexpectedOpcode,
    ControllerStatus(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event<'a> {
    pub code: u8,
    pub parameters: &'a [u8],
}

impl<'a> Event<'a> {
    /// Parse one event. Trailing bytes, including a second packet, are rejected.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 2 || bytes.len() != usize::from(bytes[1]) + 2 {
            return Err(ParseError::Length);
        }
        Ok(Self {
            code: bytes[0],
            parameters: &bytes[2..],
        })
    }
}

/// Raw reported values. A version or manufacturer ID is not a quality rating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalVersion {
    pub command_credits: u8,
    pub hci_version: u8,
    pub hci_revision: u16,
    pub lmp_version: u8,
    pub manufacturer: u16,
    pub lmp_subversion: u16,
}

impl LocalVersion {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let event = Event::parse(bytes)?;
        if event.code != 0x0e {
            return Err(ParseError::UnexpectedEvent);
        }
        let p = event.parameters;
        if p.len() < 4 {
            return Err(ParseError::Length);
        }
        if p[1..3] != READ_LOCAL_VERSION[..2] {
            return Err(ParseError::UnexpectedOpcode);
        }
        if p[3] != 0 {
            return Err(ParseError::ControllerStatus(p[3]));
        }
        if p.len() != 12 {
            return Err(ParseError::Length);
        }
        Ok(Self {
            command_credits: p[0],
            hci_version: p[4],
            hci_revision: u16::from_le_bytes([p[5], p[6]]),
            lmp_version: p[7],
            manufacturer: u16::from_le_bytes([p[8], p[9]]),
            lmp_subversion: u16::from_le_bytes([p[10], p[11]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic values, not a real controller or hardware compatibility claim.
    const VERSION: &[u8] = include_bytes!("../../../tests/fixtures/hci/local-version.bin");

    #[test]
    fn decodes_little_endian_fields() {
        assert_eq!(
            LocalVersion::parse(VERSION),
            Ok(LocalVersion {
                command_credits: 1,
                hci_version: 9,
                hci_revision: 0x1234,
                lmp_version: 9,
                manufacturer: 0xabcd,
                lmp_subversion: 0x5678,
            })
        );
    }

    #[test]
    fn rejects_every_truncation_and_trailing_data() {
        for len in 0..VERSION.len() {
            assert!(LocalVersion::parse(&VERSION[..len]).is_err());
        }
        let mut trailing = [0; 15];
        trailing[..14].copy_from_slice(VERSION);
        assert_eq!(LocalVersion::parse(&trailing), Err(ParseError::Length));
    }

    #[test]
    fn distinguishes_foreign_events_opcodes_and_controller_failure() {
        let mut packet: [u8; 14] = VERSION.try_into().unwrap();
        packet[0] = 0xff;
        assert_eq!(
            LocalVersion::parse(&packet),
            Err(ParseError::UnexpectedEvent)
        );
        packet[0] = 0x0e;
        packet[3] = 2;
        assert_eq!(
            LocalVersion::parse(&packet),
            Err(ParseError::UnexpectedOpcode)
        );
        assert_eq!(
            LocalVersion::parse(&[0x0e, 4, 1, 1, 0x10, 0x0c]),
            Err(ParseError::ControllerStatus(0x0c))
        );
    }

    #[test]
    fn all_declared_event_lengths_are_bounded() {
        let mut packet = [0_u8; 257];
        for len in 0..=255_u8 {
            packet[1] = len;
            for actual in 0..=257 {
                let parsed = Event::parse(&packet[..actual]);
                assert_eq!(parsed.is_ok(), actual == usize::from(len) + 2);
                let _ = LocalVersion::parse(&packet[..actual]);
            }
        }
    }
}
