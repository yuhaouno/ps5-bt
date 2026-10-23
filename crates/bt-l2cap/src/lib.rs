// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded basic framing, not L2CAP channels/signaling. See the crate README.
#![no_std]
#![forbid(unsafe_code)]

use bt_core::Error;
use bt_hci::{AclPacket, BroadcastFlag, ConnectionHandle, PacketBoundary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Length,
    NullChannel,
}

/// One complete basic-header-bearing PDU. CID validity beyond nonzero, channel
/// ownership, mode and negotiated MTU must be checked by the future channel layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasicFrame<'a> {
    channel_id: u16,
    payload: &'a [u8],
}

impl<'a> BasicFrame<'a> {
    pub fn new(channel_id: u16, payload: &'a [u8]) -> Result<Self, Error> {
        if channel_id == 0 || payload.len() > usize::from(u16::MAX) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            channel_id,
            payload,
        })
    }

    /// Exactly one frame. No trailing padding or concatenated PDUs are accepted.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 4
            || bytes.len() != usize::from(u16::from_le_bytes([bytes[0], bytes[1]])) + 4
        {
            return Err(ParseError::Length);
        }
        let channel_id = u16::from_le_bytes([bytes[2], bytes[3]]);
        if channel_id == 0 {
            return Err(ParseError::NullChannel);
        }
        Ok(Self {
            channel_id,
            payload: &bytes[4..],
        })
    }

    pub const fn channel_id(self) -> u16 {
        self.channel_id
    }

    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }

    /// Failure leaves output unchanged. The return length includes the header.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, Error> {
        let length = self.payload.len() + 4;
        if output.len() < length {
            return Err(Error::BufferTooSmall);
        }
        output[..2].copy_from_slice(&(self.payload.len() as u16).to_le_bytes());
        output[2..4].copy_from_slice(&self.channel_id.to_le_bytes());
        output[4..length].copy_from_slice(self.payload);
        Ok(length)
    }
}

/// Caller-assigned receive context, not information carried on the HCI wire.
/// The dispatcher must attach it before handle reuse, preserving ordered events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection {
    pub handle: ConnectionHandle,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReassemblyError {
    ForeignConnection,
    InvalidEpoch,
    InvalidFlags,
    MissingStart,
    UnexpectedStart,
    Length,
    NullChannel,
    BufferTooSmall,
}

/// One received, point-to-point PDU at a time for an explicit handle and epoch.
///
/// Storage includes the basic header. Headers may span ACL fragments; neither
/// heap allocation nor an assumed controller packet size is required. One
/// instance is required per concurrently reassembled connection. All errors
/// leave state and storage unchanged. Caller policy must surface integrity
/// failures; this type never retries, discards an unfinished PDU, or reopens.
pub struct Reassembler<'storage> {
    connection: Connection,
    storage: &'storage mut [u8],
    received: usize,
    expected: Option<usize>,
    active: bool,
}

impl<'storage> Reassembler<'storage> {
    pub fn new(connection: Connection, storage: &'storage mut [u8]) -> Result<Self, Error> {
        if storage.len() < 4 {
            return Err(Error::BufferTooSmall);
        }
        Ok(Self {
            connection,
            storage,
            received: 0,
            expected: None,
            active: false,
        })
    }

    pub const fn connection(&self) -> Connection {
        self.connection
    }

    pub const fn in_progress(&self) -> bool {
        self.active
    }

    /// Number of buffered octets, including any partial basic header.
    pub const fn pending_len(&self) -> usize {
        self.received
    }

    /// Full PDU length including header, once all four header octets arrived.
    pub const fn expected_len(&self) -> Option<usize> {
        self.expected
    }

    /// Abandon the previous context only after caller-managed disconnect/loss
    /// handling. Epochs must increase across reset, even if the handle changes;
    /// wrapping or reusing an epoch requires destroying this instance instead.
    /// Reset does not erase caller storage or permit stale packets to be relabeled.
    pub fn reset(&mut self, connection: Connection) -> Result<(), ReassemblyError> {
        if connection.epoch <= self.connection.epoch {
            return Err(ReassemblyError::InvalidEpoch);
        }
        self.connection = connection;
        self.received = 0;
        self.expected = None;
        self.active = false;
        Ok(())
    }

    /// Receive only ordinary controller-to-host point-to-point ACL fragments:
    /// PB=2 starts a PDU, PB=1 continues it. PB=0 and broadcasts are rejected.
    /// A completed frame borrows this instance until consumed by the caller.
    /// Timers, MTUs and traffic routing belong to the future connection layer.
    pub fn push(
        &mut self,
        connection: Connection,
        packet: AclPacket<'_>,
    ) -> Result<Option<BasicFrame<'_>>, ReassemblyError> {
        if connection != self.connection || packet.handle() != self.connection.handle {
            return Err(ReassemblyError::ForeignConnection);
        }
        if packet.broadcast() != BroadcastFlag::PointToPoint {
            return Err(ReassemblyError::InvalidFlags);
        }
        let offset = match packet.boundary() {
            PacketBoundary::StartFlushable if self.active => {
                return Err(ReassemblyError::UnexpectedStart);
            }
            PacketBoundary::StartFlushable => 0,
            PacketBoundary::Continuation if !self.active => {
                return Err(ReassemblyError::MissingStart);
            }
            PacketBoundary::Continuation => self.received,
            PacketBoundary::StartNonFlushable => return Err(ReassemblyError::InvalidFlags),
        };
        let fragment = packet.payload();
        let total = offset
            .checked_add(fragment.len())
            .ok_or(ReassemblyError::BufferTooSmall)?;
        if total > self.storage.len() {
            return Err(ReassemblyError::BufferTooSmall);
        }

        // Reconstruct at most four header octets locally before changing state.
        // This preflight also covers a header split at any ACL boundary.
        let mut header = [0; 4];
        let prefix = offset.min(4);
        header[..prefix].copy_from_slice(&self.storage[..prefix]);
        let added = fragment.len().min(4 - prefix);
        header[prefix..prefix + added].copy_from_slice(&fragment[..added]);
        let expected = if total >= 4 {
            let expected = usize::from(u16::from_le_bytes([header[0], header[1]])) + 4;
            if expected > self.storage.len() {
                return Err(ReassemblyError::BufferTooSmall);
            }
            if total > expected {
                return Err(ReassemblyError::Length);
            }
            if u16::from_le_bytes([header[2], header[3]]) == 0 {
                return Err(ReassemblyError::NullChannel);
            }
            Some(expected)
        } else {
            None
        };

        self.storage[offset..total].copy_from_slice(fragment);
        if expected == Some(total) {
            self.received = 0;
            self.expected = None;
            self.active = false;
            Ok(Some(BasicFrame {
                channel_id: u16::from_le_bytes([header[2], header[3]]),
                payload: &self.storage[4..total],
            }))
        } else {
            self.received = total;
            self.expected = expected;
            self.active = true;
            Ok(None)
        }
    }
}
