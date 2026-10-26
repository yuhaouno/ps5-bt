// SPDX-License-Identifier: GPL-3.0-or-later
use bt_hci::Event as HciEvent;
use bt_link::Peer;

pub const NAME_CAPACITY: usize = 64;

/// Untrusted peer-provided text for a protected UI. No Debug: names identify users.
/// Render with text escaping, never as HTML or a log-format string.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceName {
    bytes: [u8; NAME_CAPACITY],
    len: u8,
    source_complete: bool,
    truncated: bool,
}
impl DeviceName {
    pub fn as_str(&self) -> &str {
        // Only the validated UTF-8 constructor can populate private storage.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated name prefix")
    }
    pub const fn source_complete(&self) -> bool {
        self.source_complete
    }
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
    fn from_bytes(bytes: &[u8], complete: bool) -> Result<Self, WireError> {
        let text = core::str::from_utf8(bytes).map_err(|_| WireError::InvalidName)?;
        // Deliberately narrower UI candidate: do not retain control characters.
        if text.chars().any(char::is_control) {
            return Err(WireError::InvalidName);
        }
        let mut len = bytes.len().min(NAME_CAPACITY);
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        let mut name = Self {
            bytes: [0; NAME_CAPACITY],
            len: len as u8,
            source_complete: complete,
            truncated: len < bytes.len(),
        };
        name.bytes[..len].copy_from_slice(&bytes[..len]);
        Ok(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Framing,
    ForeignEvent(u8),
    ForeignOpcode(u16),
    InvalidCount,
    InvalidPageScanRepetition,
    InvalidRssi,
    InvalidEir,
    InvalidName,
    DuplicateName,
}

/// One protected observation. No Debug or retained EIR/OOB data.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub peer: Peer,
    pub page_scan_repetition: u8,
    pub class_of_device: [u8; 3],
    /// Bits 0..14 of the observed clock offset; not a timing/ownership claim.
    pub clock_offset: u16,
    pub rssi_dbm: Option<i8>,
    pub name: Option<DeviceName>,
}

/// A borrowed, fully validated event; iteration cannot expose a malformed suffix.
/// No Debug because result payloads identify devices and may contain raw EIR data.
pub enum Event<'a> {
    CommandStatus {
        opcode: u16,
        status: u8,
        command_credits: u8,
    },
    CommandComplete {
        opcode: u16,
        status: u8,
        command_credits: u8,
    },
    InquiryComplete {
        status: u8,
    },
    Results(Results<'a>),
}
#[derive(Clone, Copy)]
pub struct Results<'a> {
    kind: u8,
    entries: &'a [u8],
    count: u8,
}
impl Results<'_> {
    pub const fn len(&self) -> usize {
        self.count as usize
    }
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Observation> + '_ {
        let stride = if self.kind == 0x2f { 254 } else { 14 };
        self.entries.chunks_exact(stride).map(|bytes| {
            // Every entry was validated before constructing this private view.
            observation(self.kind, bytes).expect("validated immutable result entry")
        })
    }
}
fn exact(p: &[u8], len: usize) -> Result<(), WireError> {
    if p.len() == len {
        Ok(())
    } else {
        Err(WireError::Framing)
    }
}
fn name_from_eir(eir: &[u8]) -> Result<Option<DeviceName>, WireError> {
    let mut offset = 0;
    let mut name = None;
    while offset < eir.len() {
        let length = usize::from(eir[offset]);
        offset += 1;
        if length == 0 {
            if eir[offset..].iter().any(|byte| *byte != 0) {
                return Err(WireError::InvalidEir);
            }
            break;
        }
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= eir.len())
            .ok_or(WireError::InvalidEir)?;
        let kind = eir[offset];
        if kind == 8 || kind == 9 {
            if name.is_some() {
                return Err(WireError::DuplicateName);
            }
            name = Some(DeviceName::from_bytes(&eir[offset + 1..end], kind == 9)?);
        }
        // Unknown data types, including OOB/key material, are never retained.
        offset = end;
    }
    Ok(name)
}
fn observation(kind: u8, p: &[u8]) -> Result<Observation, WireError> {
    let mut address = [0; 6];
    address.copy_from_slice(&p[..6]);
    if p[6] > 2 {
        return Err(WireError::InvalidPageScanRepetition);
    }
    let class_at = if kind == 2 { 9 } else { 8 };
    let mut class = [0; 3];
    class.copy_from_slice(&p[class_at..class_at + 3]);
    let clock_at = class_at + 3;
    // Reserved fields/bits are ignored, including historical paging-mode fields.
    let clock_offset = u16::from_le_bytes([p[clock_at], p[clock_at + 1]]) & 0x7fff;
    let rssi_dbm = if kind == 2 {
        None
    } else {
        let rssi = p[13] as i8;
        if !(-127..=20).contains(&rssi) {
            return Err(WireError::InvalidRssi);
        }
        Some(rssi)
    };
    Ok(Observation {
        peer: Peer::from_hci_bytes(address),
        page_scan_repetition: p[6],
        class_of_device: class,
        clock_offset,
        rssi_dbm,
        name: if kind == 0x2f {
            name_from_eir(&p[14..])?
        } else {
            None
        },
    })
}
impl<'a> Event<'a> {
    pub fn parse(packet: &'a [u8]) -> Result<Self, WireError> {
        let e = HciEvent::parse(packet).map_err(|_| WireError::Framing)?;
        let p = e.parameters;
        Ok(match e.code {
            0x0f => {
                exact(p, 4)?;
                let opcode = u16::from_le_bytes([p[2], p[3]]);
                if !matches!(opcode, 0x0401 | 0x0402) {
                    return Err(WireError::ForeignOpcode(opcode));
                }
                Self::CommandStatus {
                    status: p[0],
                    command_credits: p[1],
                    opcode,
                }
            }
            0x0e => {
                if p.len() < 3 {
                    return Err(WireError::Framing);
                }
                let opcode = u16::from_le_bytes([p[1], p[2]]);
                if !matches!(opcode, 0x0401 | 0x0402) {
                    return Err(WireError::ForeignOpcode(opcode));
                }
                exact(p, 4)?;
                Self::CommandComplete {
                    status: p[3],
                    command_credits: p[0],
                    opcode,
                }
            }
            1 => {
                exact(p, 1)?;
                Self::InquiryComplete { status: p[0] }
            }
            2 | 0x22 | 0x2f => {
                let count = *p.first().ok_or(WireError::Framing)?;
                if count == 0 || (e.code == 0x2f && count != 1) {
                    return Err(WireError::InvalidCount);
                }
                let stride = if e.code == 0x2f { 254 } else { 14 };
                exact(p, 1 + usize::from(count) * stride)?;
                let results = Results {
                    kind: e.code,
                    entries: &p[1..],
                    count,
                };
                for bytes in results.entries.chunks_exact(stride) {
                    observation(e.code, bytes)?;
                }
                Self::Results(results)
            }
            _ => return Err(WireError::ForeignEvent(e.code)),
        })
    }
}
