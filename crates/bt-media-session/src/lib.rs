// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded orchestration for one already-encoded SBC media packet.
//!
//! This crate owns no clock, encoder, HCI credits or transport. It binds the
//! existing RTP/A2DP packetizer, L2CAP Basic framing and acceptance-confirmed
//! ACL fragmenter without allocating. A prepared ACL frame is retried verbatim
//! until the caller reports whole-frame acceptance.
#![no_std]
#![forbid(unsafe_code)]

mod transmit;
pub use transmit::{TransmitError, TransmitOutcome};

use bt_a2dp::{
    MAX_SBC_FRAMES_PER_PACKET, PACKET_OVERHEAD_BYTES, PacketError, PacketReport, SbcRtpPacketizer,
};
use bt_core::Error as CoreError;
use bt_l2cap::{
    Acceptance, BasicFrame, Cancellation, Connection, FragmentError, Fragmenter, PreparedFragment,
    TerminalFailure,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Stopped,
    Failed(TerminalFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidState,
    Busy,
    Overflow,
    Packet(PacketError),
    L2cap(CoreError),
    Fragment(FragmentError),
}

impl From<PacketError> for Error {
    fn from(value: PacketError) -> Self {
        Self::Packet(value)
    }
}

impl From<CoreError> for Error {
    fn from(value: CoreError) -> Self {
        Self::L2cap(value)
    }
}

impl From<FragmentError> for Error {
    fn from(value: FragmentError) -> Self {
        Self::Fragment(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedPacket {
    pub rtp: PacketReport,
    pub l2cap_pdu_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendProgress {
    More {
        accepted_pdu_bytes: usize,
        accepted_fragments: u32,
    },
    Complete {
        packet: QueuedPacket,
        fragments: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DroppedPacket {
    pub packet: QueuedPacket,
    pub cancellation: Cancellation,
}

/// One media-channel send cursor for one explicit connection epoch.
///
/// `queue_sbc_frames` commits RTP sequence/sample timestamp state after it has
/// preflighted every downstream capacity. The complete L2CAP PDU is then copied
/// into the fragmenter's caller-owned storage. Once any ACL fragment is
/// accepted, ordinary stop/cancel cannot discard the partial PDU. Only a typed
/// terminal failure or a strictly newer connection epoch can terminate it.
pub struct MediaSession<'storage> {
    state: SessionState,
    media_channel_id: u16,
    packetizer: SbcRtpPacketizer,
    fragmenter: Fragmenter<'storage>,
    queued: Option<QueuedPacket>,
}

impl<'storage> MediaSession<'storage> {
    pub fn new(
        packetizer: SbcRtpPacketizer,
        connection: Connection,
        media_channel_id: u16,
        acl_data_max: usize,
        pdu_storage: &'storage mut [u8],
    ) -> Result<Self, Error> {
        BasicFrame::new(media_channel_id, &[])?;
        let fragmenter = Fragmenter::new(connection, acl_data_max, pdu_storage)?;
        Ok(Self {
            state: SessionState::Active,
            media_channel_id,
            packetizer,
            fragmenter,
            queued: None,
        })
    }

    pub const fn state(&self) -> SessionState {
        self.state
    }

    pub const fn connection(&self) -> Connection {
        self.fragmenter.connection()
    }

    pub const fn media_channel_id(&self) -> u16 {
        self.media_channel_id
    }

    pub const fn queued(&self) -> Option<QueuedPacket> {
        self.queued
    }

    pub const fn next_sequence(&self) -> u16 {
        self.packetizer.next_sequence()
    }

    /// RTP timestamps count PCM sample frames, not bytes or channel samples.
    pub const fn next_sample_timestamp(&self) -> u32 {
        self.packetizer.next_timestamp()
    }

    pub const fn accepted_pdu_bytes(&self) -> usize {
        self.fragmenter.accepted_pdu_bytes()
    }

    pub const fn accepted_fragments(&self) -> u32 {
        self.fragmenter.accepted_fragments()
    }

    /// The retained PDU storage, including unused bytes, for diagnostics and
    /// for checking that rejected queue attempts were atomic.
    pub fn pdu_storage(&self) -> &[u8] {
        self.fragmenter.pdu_storage()
    }

    pub fn mark_next_packet(&mut self) -> Result<(), Error> {
        self.require_active()?;
        self.packetizer.mark_next_packet();
        Ok(())
    }

    /// Build one complete RTP/A2DP packet and one complete L2CAP Basic PDU.
    ///
    /// `rtp_scratch` and `l2cap_scratch` are caller-owned temporary buffers.
    /// Every downstream capacity and connection check occurs before the RTP
    /// packetizer advances. Errors leave both buffers and all session state
    /// unchanged. The fragmenter retains its own PDU copy after this returns.
    pub fn queue_sbc_frames(
        &mut self,
        connection: Connection,
        frames: &[&[u8]],
        rtp_scratch: &mut [u8],
        l2cap_scratch: &mut [u8],
    ) -> Result<QueuedPacket, Error> {
        self.require_active()?;
        self.check_connection(connection)?;
        if self.queued.is_some() || self.fragmenter.is_busy() {
            return Err(Error::Busy);
        }
        if frames.is_empty() || frames.len() > MAX_SBC_FRAMES_PER_PACKET {
            return Err(PacketError::InvalidFrameCount.into());
        }
        let payload_bytes = self
            .packetizer
            .frame_bytes()
            .checked_mul(frames.len())
            .ok_or(Error::Overflow)?;
        let rtp_bytes = PACKET_OVERHEAD_BYTES
            .checked_add(payload_bytes)
            .ok_or(Error::Overflow)?;
        if rtp_bytes > self.packetizer.mtu() {
            return Err(PacketError::MtuExceeded.into());
        }
        if rtp_scratch.len() < rtp_bytes {
            return Err(PacketError::BufferTooSmall.into());
        }
        let pdu_bytes = rtp_bytes.checked_add(4).ok_or(Error::Overflow)?;
        if l2cap_scratch.len() < pdu_bytes {
            return Err(CoreError::BufferTooSmall.into());
        }
        if self.fragmenter.pdu_storage().len() < pdu_bytes {
            return Err(FragmentError::PduTooLarge.into());
        }

        let rtp = self.packetizer.packetize(frames, rtp_scratch)?;
        let basic = BasicFrame::new(self.media_channel_id, &rtp_scratch[..rtp.bytes])?;
        let encoded = basic.encode(l2cap_scratch)?;
        debug_assert_eq!(encoded, pdu_bytes);
        self.fragmenter
            .start(connection, &l2cap_scratch[..encoded])?;
        let queued = QueuedPacket {
            rtp,
            l2cap_pdu_bytes: encoded,
        };
        self.queued = Some(queued);
        Ok(queued)
    }

    /// Prepare one whole HCI ACL frame. Repeated calls before confirmation are
    /// byte-identical and do not advance accepted-byte accounting.
    pub fn prepare_acl(
        &mut self,
        connection: Connection,
        output: &mut [u8],
    ) -> Result<PreparedFragment, Error> {
        self.require_active()?;
        self.fragmenter
            .prepare(connection, output)
            .map_err(Into::into)
    }

    /// Advance only after the single-owner transport accepted the entire frame
    /// most recently returned by `prepare_acl`. This is not a controller-credit
    /// completion and this crate never invents or replenishes credits.
    pub fn confirm_acl_accepted(&mut self, connection: Connection) -> Result<SendProgress, Error> {
        self.require_active()?;
        match self.fragmenter.confirm_accepted(connection)? {
            Acceptance::More {
                accepted_pdu_bytes,
                accepted_fragments,
            } => Ok(SendProgress::More {
                accepted_pdu_bytes,
                accepted_fragments,
            }),
            Acceptance::Complete { fragments, .. } => {
                let packet = self.queued.take().ok_or(Error::InvalidState)?;
                Ok(SendProgress::Complete { packet, fragments })
            }
        }
    }

    /// Stop only when idle or before the first fragment was accepted. A
    /// partially accepted healthy PDU remains in flight and returns
    /// `CannotCancelPartial` without changing state.
    pub fn stop(&mut self, connection: Connection) -> Result<Option<DroppedPacket>, Error> {
        self.require_active()?;
        self.check_connection(connection)?;
        let dropped = if self.fragmenter.is_busy() {
            let cancellation = self.fragmenter.cancel(connection)?;
            let packet = self.queued.take().ok_or(Error::InvalidState)?;
            Some(DroppedPacket {
                packet,
                cancellation,
            })
        } else {
            None
        };
        self.state = SessionState::Stopped;
        Ok(dropped)
    }

    /// Terminate on an externally established transport failure. This is the
    /// only same-epoch path that may abandon a partially accepted PDU.
    pub fn fail_transport(
        &mut self,
        connection: Connection,
        failure: TerminalFailure,
    ) -> Result<Option<DroppedPacket>, Error> {
        self.require_active()?;
        self.check_connection(connection)?;
        let dropped = if self.fragmenter.is_busy() {
            self.fragmenter.mark_terminal_failure(connection, failure)?;
            let cancellation = self.fragmenter.cancel(connection)?;
            let packet = self.queued.take().ok_or(Error::InvalidState)?;
            Some(DroppedPacket {
                packet,
                cancellation,
            })
        } else {
            None
        };
        self.state = SessionState::Failed(failure);
        Ok(dropped)
    }

    /// Invalidate this session while installing a strictly newer lower-layer
    /// connection context. The caller must create a fresh media session before
    /// sending again; RTP state is intentionally not reused across reconnect.
    pub fn invalidate_epoch(&mut self, next: Connection) -> Result<Option<DroppedPacket>, Error> {
        self.require_active()?;
        let cancellation = self.fragmenter.replace_connection(next)?;
        let dropped = match cancellation {
            Some(cancellation) => {
                let packet = self.queued.take().ok_or(Error::InvalidState)?;
                Some(DroppedPacket {
                    packet,
                    cancellation,
                })
            }
            None => None,
        };
        self.state = SessionState::Stopped;
        Ok(dropped)
    }

    fn require_active(&self) -> Result<(), Error> {
        if self.state == SessionState::Active {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    fn check_connection(&self, connection: Connection) -> Result<(), Error> {
        let current = self.fragmenter.connection();
        if connection.epoch != current.epoch {
            return Err(FragmentError::InvalidEpoch.into());
        }
        if connection.handle != current.handle {
            return Err(FragmentError::ForeignConnection.into());
        }
        Ok(())
    }
}
