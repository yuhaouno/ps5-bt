// SPDX-License-Identifier: GPL-3.0-or-later
//! An allocation-free ACL packet reservation ledger for one proven owner.
//!
//! This does not establish controller ownership, discover capacity or dispatch
//! native traffic. The external single owner supplies a real reservation and
//! labels ordered events with the original connection epoch. HCI has no epoch;
//! this code cannot detect an old event falsely labelled with a new context.

use bt_core::{BluetoothTransport, Error};

use crate::{AclPacket, BroadcastFlag, CompletedPackets, ConnectionHandle, ParseError};

/// Dispatcher metadata, never inferred from an HCI packet's handle alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditContext {
    pub handle: ConnectionHandle,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditFailureCause {
    Transport(Error),
    MalformedCompletion(ParseError),
    ForeignContext,
    ForeignCompletionHandle,
    DuplicateCompletionHandle,
    OverCompletion,
    CounterExhausted,
    IntegrityLost,
}

/// First terminal cause. Accounting remains frozen for external reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditFailure {
    pub cause: CreditFailureCause,
    /// A failed transport send may nevertheless have accepted the current ACL
    /// frame. It was not counted as accepted, and must never be blindly retried.
    /// False does not imply that outstanding packets were transmitted/delivered.
    pub send_outcome_uncertain: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditError {
    InvalidAclDataMaximum,
    InvalidAcl(ParseError),
    ForeignContext,
    ForeignPacketHandle,
    AclDataTooLarge,
    BroadcastUnsupported,
    NoCredits,
    /// Busy guarantees rejection of the entire ACL frame. The caller retains
    /// its PDU/cursor and decides whether/how to retry it.
    Rejected(Error),
    Failed(CreditFailure),
}

/// Observable packet accounting. `accepted - completed == outstanding` and
/// `available + outstanding == initial` remain true, including after failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditSnapshot {
    pub context: CreditContext,
    pub initial: u16,
    pub available: u16,
    pub outstanding: u16,
    pub accepted: u64,
    pub completed: u64,
    pub failure: Option<CreditFailure>,
}

/// One packet reservation for one owned handle/epoch, with no reset or cloning.
///
/// The external dispatcher must serialize all sends and completion delivery for
/// the reservation. A controller-wide Read Buffer Size value, USB completion or
/// elapsed time is not an owned reservation. There is no clock/refill/retry API.
/// This ledger neither holds nor cancels PDUs: the caller must retain an
/// unfinished PDU on backpressure, and record terminal cancellation accounting
/// instead of silently discarding partially accepted data.
///
/// A new connection needs an explicitly created new owner after external
/// teardown and stale-event quarantine. Dropping this value does not disconnect
/// anything, release a reservation, complete packets or authorize handle reuse.
/// This portable model does not establish PS5 routing or hardware support.
#[derive(Debug)]
pub struct AclCreditOwner {
    state: CreditSnapshot,
    acl_data_max: u16,
}

impl AclCreditOwner {
    /// Assert an externally established reservation, excluding native/foreign
    /// owners' packets, and a proven ACL data maximum excluding the HCI header.
    /// The caller must not duplicate the same reservation across owner values.
    /// Zero reserved packets is allowed and supplies real backpressure forever
    /// until the owner is replaced through a new externally established context.
    pub fn new(
        context: CreditContext,
        reserved_packets: u16,
        acl_data_max: usize,
    ) -> Result<Self, CreditError> {
        if acl_data_max == 0 || acl_data_max > usize::from(u16::MAX) {
            return Err(CreditError::InvalidAclDataMaximum);
        }
        Ok(Self {
            state: CreditSnapshot {
                context,
                initial: reserved_packets,
                available: reserved_packets,
                outstanding: 0,
                accepted: 0,
                completed: 0,
                failure: None,
            },
            acl_data_max: acl_data_max as u16,
        })
    }

    pub const fn snapshot(&self) -> CreditSnapshot {
        self.state
    }

    pub const fn acl_data_max(&self) -> u16 {
        self.acl_data_max
    }

    /// Validate and submit exactly one complete, point-to-point ACL frame.
    /// Only `send_acl` is invoked, once, after all local validation and credit
    /// checks. Success consumes one packet credit and means accepted, not sent.
    ///
    /// Only Busy guarantees no acceptance in the existing transport contract.
    /// Other backend errors are sticky failures with uncertain send outcome;
    /// no further send or completion can alter credits.
    /// Every local validation/backpressure/rejection leaves accounting intact.
    pub fn send_acl<T: BluetoothTransport + ?Sized>(
        &mut self,
        context: CreditContext,
        packet: &[u8],
        transport: &mut T,
    ) -> Result<(), CreditError> {
        self.check_healthy()?;
        if context != self.state.context {
            return Err(CreditError::ForeignContext);
        }
        let acl = AclPacket::parse(packet).map_err(CreditError::InvalidAcl)?;
        if acl.handle() != self.state.context.handle {
            return Err(CreditError::ForeignPacketHandle);
        }
        if acl.broadcast() != BroadcastFlag::PointToPoint {
            return Err(CreditError::BroadcastUnsupported);
        }
        if acl.payload().len() > usize::from(self.acl_data_max) {
            return Err(CreditError::AclDataTooLarge);
        }
        if self.state.available == 0 {
            return Err(CreditError::NoCredits);
        }
        let next_accepted = match self.state.accepted.checked_add(1) {
            Some(value) => value,
            None => return Err(self.fail(CreditFailureCause::CounterExhausted, false)),
        };
        match transport.send_acl(packet) {
            Ok(()) => {
                self.state.available -= 1;
                self.state.outstanding += 1;
                self.state.accepted = next_accepted;
                Ok(())
            }
            Err(Error::Busy) => Err(CreditError::Rejected(Error::Busy)),
            Err(error) => Err(self.fail(CreditFailureCause::Transport(error), true)),
        }
    }

    /// Validate the entire exact Number Of Completed Packets event, then
    /// atomically reclaim owned packets. Even a foreign zero-count entry fails:
    /// the dispatcher must supply an exclusively owned event/context, not let
    /// this ledger silently filter a shared event and discard native traffic.
    ///
    /// A raw event has no epoch or sequence identifier. The dispatcher must
    /// deliver completions exactly once with their original context; a replay
    /// whose count fits current outstanding packets cannot be detected here.
    /// Context must be assigned before a handle is reused. Malformed frames,
    /// foreign context/handles, duplicate entries and over-completion record a
    /// sticky failure without reclaiming any packet, including earlier entries.
    pub fn complete(&mut self, context: CreditContext, event: &[u8]) -> Result<u16, CreditError> {
        self.check_healthy()?;
        if context != self.state.context {
            return Err(self.fail(CreditFailureCause::ForeignContext, false));
        }
        let completed = match CompletedPackets::parse(event) {
            Ok(value) => value,
            Err(error) => {
                return Err(self.fail(CreditFailureCause::MalformedCompletion(error), false));
            }
        };
        let mut seen = false;
        let mut reclaimed = 0;
        for entry in completed.iter() {
            if entry.handle != self.state.context.handle {
                return Err(self.fail(CreditFailureCause::ForeignCompletionHandle, false));
            }
            if seen {
                return Err(self.fail(CreditFailureCause::DuplicateCompletionHandle, false));
            }
            seen = true;
            if entry.packets > self.state.outstanding {
                return Err(self.fail(CreditFailureCause::OverCompletion, false));
            }
            reclaimed = entry.packets;
        }
        let next_completed = match self.state.completed.checked_add(u64::from(reclaimed)) {
            Some(value) => value,
            None => return Err(self.fail(CreditFailureCause::CounterExhausted, false)),
        };
        // Reclaimed <= outstanding <= initial and no mutation occurred above.
        self.state.available += reclaimed;
        self.state.outstanding -= reclaimed;
        self.state.completed = next_completed;
        Ok(reclaimed)
    }

    /// Record external evidence of event/queue loss or unreconciled ownership.
    /// Does not clear outstanding packets, synthesize completions or overwrite
    /// the first failure. Retained accounting must be quarantined/reconciled by
    /// the caller before it establishes any new reservation.
    pub fn mark_integrity_lost(&mut self) -> CreditFailure {
        *self.state.failure.get_or_insert(CreditFailure {
            cause: CreditFailureCause::IntegrityLost,
            send_outcome_uncertain: false,
        })
    }

    fn check_healthy(&self) -> Result<(), CreditError> {
        match self.state.failure {
            Some(failure) => Err(CreditError::Failed(failure)),
            None => Ok(()),
        }
    }

    fn fail(&mut self, cause: CreditFailureCause, send_outcome_uncertain: bool) -> CreditError {
        let failure = *self.state.failure.get_or_insert(CreditFailure {
            cause,
            send_outcome_uncertain,
        });
        CreditError::Failed(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::Received;

    struct MustNotCall;

    impl BluetoothTransport for MustNotCall {
        fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
            panic!("unexpected command");
        }

        fn send_acl(&mut self, _: &[u8]) -> Result<(), Error> {
            panic!("counter exhaustion must be checked before acceptance");
        }

        fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
            panic!("unexpected receive");
        }
    }

    #[test]
    fn cumulative_counter_exhaustion_fails_before_transport_acceptance() {
        let context = CreditContext {
            handle: ConnectionHandle::new(0x42).unwrap(),
            epoch: 1,
        };
        let mut owner = AclCreditOwner::new(context, 1, 1).unwrap();
        // Reachable after u64::MAX accepted/completed packet pairs. Seeding
        // private counters avoids a test loop and preserves both invariants.
        owner.state.accepted = u64::MAX;
        owner.state.completed = u64::MAX;
        let before = owner.snapshot();
        let failure = CreditFailure {
            cause: CreditFailureCause::CounterExhausted,
            send_outcome_uncertain: false,
        };
        assert_eq!(
            owner.send_acl(context, &[0x42, 0, 0, 0], &mut MustNotCall),
            Err(CreditError::Failed(failure))
        );
        assert_eq!(
            owner.snapshot(),
            CreditSnapshot {
                failure: Some(failure),
                ..before
            }
        );
        assert_eq!(owner.mark_integrity_lost(), failure);
    }
}
