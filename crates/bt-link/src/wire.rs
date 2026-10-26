// SPDX-License-Identifier: GPL-3.0-or-later
use bt_hci::{ConnectionHandle, Event as HciEvent};
use core::fmt;

/// Explicitly selected BD_ADDR in HCI wire order (least significant octet first).
/// Debug is deliberately redacted; use bytes only for protected local management.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Peer([u8; 6]);

impl Peer {
    pub const fn from_hci_bytes(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }
    pub const fn hci_bytes(self) -> [u8; 6] {
        self.0
    }
}

impl fmt::Debug for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Peer(REDACTED)")
    }
}

/// Owned local bond key. No Debug, Clone, Copy or implicit serialization.
/// Clearing on drop is best effort: this safe Rust type cannot guarantee compiler
/// zeroization, erase controller/storage copies, or prevent caller-created copies.
pub struct SecretKey([u8; 16]);

impl SecretKey {
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    /// Explicit access for a protected bond store or command encoder. Never log it.
    pub fn expose(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Framing,
    UnsupportedEvent(u8),
    InvalidField,
    InvalidHandle,
}

/// Exactly decoded relevant HCI events. This enum deliberately has no Debug:
/// LinkKeyNotification contains a secret. Unknown events belong to the dispatcher.
pub enum Event {
    CommandStatus {
        status: u8,
        credits: u8,
        opcode: u16,
    },
    CommandComplete {
        credits: u8,
        opcode: u16,
        status: u8,
        result: CompletionResult,
    },
    ConnectionComplete {
        status: u8,
        handle: Option<ConnectionHandle>,
        peer: Peer,
        encrypted: bool,
    },
    Disconnected {
        status: u8,
        handle: ConnectionHandle,
        reason: u8,
    },
    AuthenticationComplete {
        status: u8,
        handle: ConnectionHandle,
    },
    EncryptionChange {
        status: u8,
        handle: ConnectionHandle,
        enabled: u8,
        key_size: Option<u8>,
    },
    LinkKeyRequest(Peer),
    LinkKeyNotification {
        peer: Peer,
        key: SecretKey,
        key_type: u8,
    },
    IoCapabilityRequest(Peer),
    IoCapabilityResponse {
        peer: Peer,
        capability: u8,
        oob: u8,
        authentication: u8,
    },
    UserConfirmation {
        peer: Peer,
        value: u32,
    },
    SimplePairingComplete {
        peer: Peer,
        status: u8,
    },
    UnsupportedPairing {
        peer: Peer,
        event_code: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionResult {
    /// Unknown-command alternative for commands that normally use Command Status.
    UnsupportedCommand,
    Peer(Peer),
    EncryptionKeySize {
        handle: ConnectionHandle,
        size: Option<u8>,
    },
}

fn peer(p: &[u8]) -> Peer {
    let mut address = [0; 6];
    address.copy_from_slice(p);
    Peer(address)
}
fn handle(p: &[u8]) -> Result<ConnectionHandle, WireError> {
    ConnectionHandle::new(u16::from_le_bytes([p[0], p[1]])).map_err(|_| WireError::InvalidHandle)
}
fn exact(p: &[u8], n: usize) -> Result<(), WireError> {
    if p.len() == n {
        Ok(())
    } else {
        Err(WireError::Framing)
    }
}

impl Event {
    /// One complete event without H4; truncations and trailing bytes are rejected.
    pub fn parse(packet: &[u8]) -> Result<Self, WireError> {
        let e = HciEvent::parse(packet).map_err(|_| WireError::Framing)?;
        let p = e.parameters;
        Ok(match e.code {
            0x0f => {
                exact(p, 4)?;
                Self::CommandStatus {
                    status: p[0],
                    credits: p[1],
                    opcode: u16::from_le_bytes([p[2], p[3]]),
                }
            }
            0x0e => {
                if p.len() < 4 {
                    return Err(WireError::Framing);
                }
                let opcode = u16::from_le_bytes([p[1], p[2]]);
                let result = match opcode {
                    0x0408 | 0x040b | 0x040c | 0x040e | 0x042b | 0x042c | 0x042d | 0x042f
                    | 0x0433 | 0x0434 => {
                        exact(p, 10)?;
                        // Core Vol 4 E 4.5: BD_ADDR immediately following Status
                        // remains mandatory and valid even on a failed command.
                        CompletionResult::Peer(peer(&p[4..10]))
                    }
                    0x1408 => {
                        // Core Vol 4 E 4.5: the Handle after Status is mandatory;
                        // the trailing key size may be absent/undefined on failure.
                        if p[3] == 0 {
                            exact(p, 7)?;
                        } else if !(6..=7).contains(&p.len()) {
                            return Err(WireError::Framing);
                        }
                        CompletionResult::EncryptionKeySize {
                            handle: handle(&p[4..6])?,
                            size: if p[3] == 0 { Some(p[6]) } else { None },
                        }
                    }
                    0x0405 | 0x0406 | 0x0411 | 0x0413 if p[3] == 1 => {
                        exact(p, 4)?;
                        CompletionResult::UnsupportedCommand
                    }
                    _ => return Err(WireError::UnsupportedEvent(e.code)),
                };
                Self::CommandComplete {
                    credits: p[0],
                    opcode,
                    status: p[3],
                    result,
                }
            }
            0x03 => {
                exact(p, 11)?;
                if p[0] == 0 && (p[9] != 1 || p[10] > 1) {
                    return Err(WireError::InvalidField);
                }
                Self::ConnectionComplete {
                    status: p[0],
                    handle: if p[0] == 0 {
                        Some(handle(&p[1..3])?)
                    } else {
                        None
                    },
                    peer: peer(&p[3..9]),
                    encrypted: p[0] == 0 && p[10] == 1,
                }
            }
            0x05 => {
                exact(p, 4)?;
                Self::Disconnected {
                    status: p[0],
                    handle: handle(&p[1..3])?,
                    reason: p[3],
                }
            }
            0x06 => {
                exact(p, 3)?;
                Self::AuthenticationComplete {
                    status: p[0],
                    handle: handle(&p[1..3])?,
                }
            }
            0x08 | 0x59 => {
                exact(p, if e.code == 0x08 { 4 } else { 5 })?;
                if p[0] == 0 && p[3] > 2 {
                    return Err(WireError::InvalidField);
                }
                let size = if e.code == 0x59 { Some(p[4]) } else { None };
                if p[0] == 0 && p[3] != 0 && size.is_some_and(|s| !(1..=16).contains(&s)) {
                    return Err(WireError::InvalidField);
                }
                Self::EncryptionChange {
                    status: p[0],
                    handle: handle(&p[1..3])?,
                    enabled: p[3],
                    key_size: size,
                }
            }
            0x16 | 0x17 | 0x31 | 0x34 | 0x35 => {
                exact(p, 6)?;
                match e.code {
                    0x17 => Self::LinkKeyRequest(peer(p)),
                    0x31 => Self::IoCapabilityRequest(peer(p)),
                    _ => Self::UnsupportedPairing {
                        peer: peer(p),
                        event_code: e.code,
                    },
                }
            }
            0x18 => {
                exact(p, 23)?;
                let mut key = [0; 16];
                key.copy_from_slice(&p[6..22]);
                Self::LinkKeyNotification {
                    peer: peer(&p[..6]),
                    key: SecretKey::new(key),
                    key_type: p[22],
                }
            }
            0x32 => {
                exact(p, 9)?;
                if p[6] > 3 || p[7] > 1 || p[8] > 5 {
                    return Err(WireError::InvalidField);
                }
                Self::IoCapabilityResponse {
                    peer: peer(&p[..6]),
                    capability: p[6],
                    oob: p[7],
                    authentication: p[8],
                }
            }
            0x33 => {
                exact(p, 10)?;
                let value = u32::from_le_bytes([p[6], p[7], p[8], p[9]]);
                if value > 999_999 {
                    return Err(WireError::InvalidField);
                }
                Self::UserConfirmation {
                    peer: peer(&p[..6]),
                    value,
                }
            }
            0x36 => {
                exact(p, 7)?;
                Self::SimplePairingComplete {
                    status: p[0],
                    peer: peer(&p[1..]),
                }
            }
            _ => return Err(WireError::UnsupportedEvent(e.code)),
        })
    }
}
