// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free outbound L2CAP-to-HCI ACL fragmentation.

use bt_hci::{AclPacket, BroadcastFlag, PacketBoundary};

use crate::{BasicFrame, Connection, ParseError};

/// A transport-level reason why an in-flight PDU can no longer be completed.
///
/// This is caller evidence, not a recovery action. In particular, elapsed time
/// is not a credit source and is intentionally not represented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalFailure {
    Disconnected,
    TransportLost,
    IntegrityLost,
    CreditsUnreconciled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragmentError {
    InvalidPdu(ParseError),
    InvalidAclDataMaximum,
    PduStorageTooSmall,
    PduTooLarge,
    ForeignConnection,
    InvalidEpoch,
    Busy,
    Idle,
    NoPreparedFragment,
    CannotCancelPartial,
    Failed(TerminalFailure),
    BufferTooSmall,
}

/// Metadata for the complete ACL frame most recently prepared in caller
/// storage. `packet_len` includes the four-octet HCI ACL header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedFragment {
    pub packet_len: usize,
    pub pdu_offset: usize,
    pub acl_payload_len: usize,
    pub boundary: PacketBoundary,
    pub final_fragment: bool,
}

/// Result of confirming that one complete prepared ACL frame was accepted by
/// the single-owner transport. Acceptance does not imply controller completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acceptance {
    More {
        accepted_pdu_bytes: usize,
        accepted_fragments: u32,
    },
    Complete {
        pdu_len: usize,
        fragments: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancellationReason {
    /// No fragment of this PDU was accepted by the transport.
    BeforeFirstAccepted,
    Terminal(TerminalFailure),
    /// The caller established a strictly newer connection context.
    EpochChange {
        next: Connection,
    },
}

/// Durable accounting returned whenever an unfinished PDU is abandoned.
/// Accepted byte counts cover the L2CAP PDU, excluding HCI ACL headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancellation {
    pub connection: Connection,
    pub pdu_len: usize,
    pub accepted_pdu_bytes: usize,
    pub accepted_fragments: u32,
    pub reason: CancellationReason,
}

/// Fragments one complete, basic-header-bearing L2CAP PDU at a time.
///
/// The input slice contains the four-octet L2CAP Basic header followed by its
/// exact declared payload. `start` validates and copies it atomically into the
/// caller-provided storage, so the input borrow ends when `start` returns. The
/// storage remains exclusively borrowed by this cursor. There is no heap
/// allocation.
///
/// `prepare` can be repeated and emits byte-identical data until
/// `confirm_accepted` advances the cursor. The caller must call confirmation
/// only after its single-owner transport accepted the entire prepared ACL frame.
/// Once any fragment is accepted, ordinary cancellation is forbidden: the PDU
/// remains in flight until completion, a typed terminal failure, or a strictly
/// newer connection epoch.
pub struct Fragmenter<'storage> {
    connection: Connection,
    acl_data_max: u16,
    storage: &'storage mut [u8],
    pdu_len: Option<usize>,
    accepted_pdu_bytes: usize,
    accepted_fragments: u32,
    prepared_payload_len: Option<usize>,
    failure: Option<TerminalFailure>,
}

impl<'storage> Fragmenter<'storage> {
    /// `acl_data_max` is the controller's proven maximum HCI ACL data length,
    /// excluding the four-octet HCI ACL header. Zero and values that do not fit
    /// the HCI length field are rejected.
    pub fn new(
        connection: Connection,
        acl_data_max: usize,
        storage: &'storage mut [u8],
    ) -> Result<Self, FragmentError> {
        if acl_data_max == 0 || acl_data_max > usize::from(u16::MAX) {
            return Err(FragmentError::InvalidAclDataMaximum);
        }
        if storage.len() < 4 {
            return Err(FragmentError::PduStorageTooSmall);
        }
        Ok(Self {
            connection,
            acl_data_max: acl_data_max as u16,
            storage,
            pdu_len: None,
            accepted_pdu_bytes: 0,
            accepted_fragments: 0,
            prepared_payload_len: None,
            failure: None,
        })
    }

    pub const fn connection(&self) -> Connection {
        self.connection
    }

    pub const fn acl_data_max(&self) -> u16 {
        self.acl_data_max
    }

    pub const fn is_busy(&self) -> bool {
        self.pdu_len.is_some()
    }

    pub const fn accepted_pdu_bytes(&self) -> usize {
        self.accepted_pdu_bytes
    }

    pub const fn accepted_fragments(&self) -> u32 {
        self.accepted_fragments
    }

    pub fn pdu_len(&self) -> Option<usize> {
        self.pdu_len
    }

    /// Immutable access to the caller-owned storage, including unused bytes.
    /// This is useful for diagnostics and for verifying atomic rejected starts.
    pub fn pdu_storage(&self) -> &[u8] {
        self.storage
    }

    pub const fn terminal_failure(&self) -> Option<TerminalFailure> {
        self.failure
    }

    /// Start one exact, header-bearing L2CAP Basic PDU. Every validation occurs
    /// before the copy or state changes; a busy instance never replaces its PDU.
    pub fn start(&mut self, connection: Connection, pdu: &[u8]) -> Result<(), FragmentError> {
        self.check_connection(connection)?;
        if self.is_busy() {
            return Err(FragmentError::Busy);
        }
        BasicFrame::parse(pdu).map_err(FragmentError::InvalidPdu)?;
        if pdu.len() > self.storage.len() {
            return Err(FragmentError::PduTooLarge);
        }

        self.storage[..pdu.len()].copy_from_slice(pdu);
        self.pdu_len = Some(pdu.len());
        self.accepted_pdu_bytes = 0;
        self.accepted_fragments = 0;
        self.prepared_payload_len = None;
        self.failure = None;
        Ok(())
    }

    /// Encode the current fragment as one complete point-to-point HCI ACL frame
    /// without advancing the PDU cursor. Failure leaves output and state intact.
    pub fn prepare(
        &mut self,
        connection: Connection,
        output: &mut [u8],
    ) -> Result<PreparedFragment, FragmentError> {
        self.check_connection(connection)?;
        let pdu_len = self.pdu_len.ok_or(FragmentError::Idle)?;
        if let Some(failure) = self.failure {
            return Err(FragmentError::Failed(failure));
        }

        let offset = self.accepted_pdu_bytes;
        let payload_len = self
            .prepared_payload_len
            .unwrap_or_else(|| (pdu_len - offset).min(usize::from(self.acl_data_max)));
        let packet_len = payload_len + 4;
        if output.len() < packet_len {
            return Err(FragmentError::BufferTooSmall);
        }
        let boundary = if offset == 0 {
            PacketBoundary::StartNonFlushable
        } else {
            PacketBoundary::Continuation
        };
        let packet = AclPacket::new(
            self.connection.handle,
            boundary,
            BroadcastFlag::PointToPoint,
            &self.storage[offset..offset + payload_len],
        )
        .map_err(|_| FragmentError::InvalidAclDataMaximum)?;
        packet
            .encode(output)
            .map_err(|_| FragmentError::BufferTooSmall)?;
        self.prepared_payload_len = Some(payload_len);
        Ok(PreparedFragment {
            packet_len,
            pdu_offset: offset,
            acl_payload_len: payload_len,
            boundary,
            final_fragment: offset + payload_len == pdu_len,
        })
    }

    /// Advance only after the caller confirms that the complete frame returned
    /// by `prepare` was accepted. Calling this before preparation is rejected.
    pub fn confirm_accepted(
        &mut self,
        connection: Connection,
    ) -> Result<Acceptance, FragmentError> {
        self.check_connection(connection)?;
        let pdu_len = self.pdu_len.ok_or(FragmentError::Idle)?;
        if let Some(failure) = self.failure {
            return Err(FragmentError::Failed(failure));
        }
        let payload_len = self
            .prepared_payload_len
            .ok_or(FragmentError::NoPreparedFragment)?;
        let next_offset = self.accepted_pdu_bytes + payload_len;
        let next_fragments = self.accepted_fragments + 1;

        if next_offset == pdu_len {
            self.clear_pdu();
            Ok(Acceptance::Complete {
                pdu_len,
                fragments: next_fragments,
            })
        } else {
            self.accepted_pdu_bytes = next_offset;
            self.accepted_fragments = next_fragments;
            self.prepared_payload_len = None;
            Ok(Acceptance::More {
                accepted_pdu_bytes: next_offset,
                accepted_fragments: next_fragments,
            })
        }
    }

    /// Record a terminal transport condition. The copied PDU remains observable
    /// until `cancel` (or `replace_connection`) returns its accounting.
    pub fn mark_terminal_failure(
        &mut self,
        connection: Connection,
        failure: TerminalFailure,
    ) -> Result<(), FragmentError> {
        self.check_connection(connection)?;
        if self.pdu_len.is_none() {
            return Err(FragmentError::Idle);
        }
        if let Some(previous) = self.failure {
            return Err(FragmentError::Failed(previous));
        }
        self.failure = Some(failure);
        Ok(())
    }

    /// Cancel before the first accepted fragment, or after a recorded terminal
    /// failure. A partially accepted healthy PDU cannot be dropped.
    pub fn cancel(&mut self, connection: Connection) -> Result<Cancellation, FragmentError> {
        self.check_connection(connection)?;
        let pdu_len = self.pdu_len.ok_or(FragmentError::Idle)?;
        let reason = match self.failure {
            Some(failure) => CancellationReason::Terminal(failure),
            None if self.accepted_fragments == 0 => CancellationReason::BeforeFirstAccepted,
            None => return Err(FragmentError::CannotCancelPartial),
        };
        let cancellation = self.cancellation(pdu_len, reason);
        self.clear_pdu();
        Ok(cancellation)
    }

    /// Install a strictly newer connection epoch. Any unfinished old-epoch PDU
    /// is cancelled with explicit accounting, even after partial acceptance.
    /// The caller remains responsible for transport teardown/quarantine before
    /// assigning the newer epoch; this method cannot prove that external fact.
    pub fn replace_connection(
        &mut self,
        next: Connection,
    ) -> Result<Option<Cancellation>, FragmentError> {
        if next.epoch <= self.connection.epoch {
            return Err(FragmentError::InvalidEpoch);
        }
        let cancellation = self
            .pdu_len
            .map(|pdu_len| self.cancellation(pdu_len, CancellationReason::EpochChange { next }));
        self.clear_pdu();
        self.connection = next;
        Ok(cancellation)
    }

    fn check_connection(&self, connection: Connection) -> Result<(), FragmentError> {
        if connection.epoch != self.connection.epoch {
            return Err(FragmentError::InvalidEpoch);
        }
        if connection.handle != self.connection.handle {
            return Err(FragmentError::ForeignConnection);
        }
        Ok(())
    }

    fn cancellation(&self, pdu_len: usize, reason: CancellationReason) -> Cancellation {
        Cancellation {
            connection: self.connection,
            pdu_len,
            accepted_pdu_bytes: self.accepted_pdu_bytes,
            accepted_fragments: self.accepted_fragments,
            reason,
        }
    }

    fn clear_pdu(&mut self) {
        self.pdu_len = None;
        self.accepted_pdu_bytes = 0;
        self.accepted_fragments = 0;
        self.prepared_payload_len = None;
        self.failure = None;
    }
}
