// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::Error;

use crate::ParseError;

/// A syntactically valid HCI handle; this does not prove connection ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionHandle(pub(crate) u16);

impl ConnectionHandle {
    pub fn new(raw: u16) -> Result<Self, ParseError> {
        if raw > 0x0eff {
            return Err(ParseError::InvalidHandle);
        }
        Ok(Self(raw))
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Core 6.0 values. Value 3 is previously used and is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketBoundary {
    /// Host-to-controller only, except loopback; not allowed for APB-U.
    StartNonFlushable = 0,
    Continuation = 1,
    /// Controller-to-host starts use this value, including LE-U.
    StartFlushable = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BroadcastFlag {
    PointToPoint = 0,
    /// APB-U, subject to link role and direction restrictions.
    ActivePeripheral = 1,
}

/// A structurally valid complete ACL packet, without H4.
///
/// Link type, role, direction, controller capacity and ownership remain caller
/// policy. In particular, parsing accepts PB=0 for host-to-controller traces;
/// normal controller-to-host traffic must not use PB=0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AclPacket<'a> {
    handle: ConnectionHandle,
    boundary: PacketBoundary,
    broadcast: BroadcastFlag,
    payload: &'a [u8],
}

impl<'a> AclPacket<'a> {
    pub fn new(
        handle: ConnectionHandle,
        boundary: PacketBoundary,
        broadcast: BroadcastFlag,
        payload: &'a [u8],
    ) -> Result<Self, Error> {
        if payload.len() > usize::from(u16::MAX)
            || (broadcast == BroadcastFlag::ActivePeripheral
                && boundary == PacketBoundary::StartNonFlushable)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            handle,
            boundary,
            broadcast,
            payload,
        })
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 4
            || bytes.len() != usize::from(u16::from_le_bytes([bytes[2], bytes[3]])) + 4
        {
            return Err(ParseError::Length);
        }
        let flags = u16::from_le_bytes([bytes[0], bytes[1]]);
        let handle = ConnectionHandle::new(flags & 0x0fff)?;
        let boundary = match (flags >> 12) & 3 {
            0 => PacketBoundary::StartNonFlushable,
            1 => PacketBoundary::Continuation,
            2 => PacketBoundary::StartFlushable,
            _ => return Err(ParseError::InvalidFlags),
        };
        let broadcast = match flags >> 14 {
            0 => BroadcastFlag::PointToPoint,
            1 => BroadcastFlag::ActivePeripheral,
            _ => return Err(ParseError::InvalidFlags),
        };
        Self::new(handle, boundary, broadcast, &bytes[4..]).map_err(|_| ParseError::InvalidFlags)
    }

    pub const fn handle(self) -> ConnectionHandle {
        self.handle
    }

    pub const fn boundary(self) -> PacketBoundary {
        self.boundary
    }

    pub const fn broadcast(self) -> BroadcastFlag {
        self.broadcast
    }

    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }

    /// Writes exactly one ACL header and payload; failure leaves output intact.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, Error> {
        let length = self.payload.len() + 4;
        if output.len() < length {
            return Err(Error::BufferTooSmall);
        }
        let flags =
            self.handle.raw() | ((self.boundary as u16) << 12) | ((self.broadcast as u16) << 14);
        output[..2].copy_from_slice(&flags.to_le_bytes());
        output[2..4].copy_from_slice(&(self.payload.len() as u16).to_le_bytes());
        output[4..length].copy_from_slice(self.payload);
        Ok(length)
    }
}
