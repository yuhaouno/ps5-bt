// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::{BluetoothTransport, Error, Received};
use bt_hci::flow::{
    AclCreditOwner, CreditContext, CreditError, CreditFailure, CreditFailureCause, CreditSnapshot,
};
use bt_hci::{AclPacket, BroadcastFlag, ConnectionHandle, PacketBoundary, ParseError};

fn context(epoch: u64) -> CreditContext {
    CreditContext {
        handle: ConnectionHandle::new(0x0042).unwrap(),
        epoch,
    }
}

fn owner(packets: u16, data_max: usize) -> AclCreditOwner {
    AclCreditOwner::new(context(1), packets, data_max).unwrap()
}

fn frame(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; payload.len() + 4];
    AclPacket::new(
        context(1).handle,
        PacketBoundary::StartNonFlushable,
        BroadcastFlag::PointToPoint,
        payload,
    )
    .unwrap()
    .encode(&mut bytes)
    .unwrap();
    bytes
}

fn completion(entries: &[(u16, u16)]) -> Vec<u8> {
    assert!(entries.len() <= 63);
    let mut bytes = vec![0x13, (1 + entries.len() * 4) as u8, entries.len() as u8];
    for (handle, count) in entries {
        bytes.extend_from_slice(&handle.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
    }
    bytes
}

#[derive(Default)]
struct Transport {
    calls: usize,
    error: Option<Error>,
    last: Vec<u8>,
}

impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("credit owner must never send a command");
    }

    fn send_acl(&mut self, packet: &[u8]) -> Result<(), Error> {
        self.calls += 1;
        self.last.clear();
        self.last.extend_from_slice(packet);
        self.error.map_or(Ok(()), Err)
    }

    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("credit owner must never receive");
    }
}

fn invariant(snapshot: CreditSnapshot) {
    assert_eq!(
        u32::from(snapshot.available) + u32::from(snapshot.outstanding),
        u32::from(snapshot.initial)
    );
    assert_eq!(
        snapshot.accepted - snapshot.completed,
        u64::from(snapshot.outstanding)
    );
}

fn same_accounting(before: CreditSnapshot, after: CreditSnapshot) {
    assert_eq!(
        CreditSnapshot {
            failure: before.failure,
            ..after
        },
        before
    );
    invariant(after);
}

#[test]
fn explicit_reservation_and_acl_data_bounds() {
    for invalid in [0, usize::from(u16::MAX) + 1, usize::MAX] {
        assert!(matches!(
            AclCreditOwner::new(context(1), 3, invalid),
            Err(CreditError::InvalidAclDataMaximum)
        ));
    }
    let empty = AclCreditOwner::new(context(0), 0, 1).unwrap();
    assert_eq!(empty.acl_data_max(), 1);
    assert_eq!(empty.snapshot().context.epoch, 0);
    assert_eq!(empty.snapshot().initial, 0);
    invariant(empty.snapshot());
    let maximum = AclCreditOwner::new(context(u64::MAX), u16::MAX, 65535).unwrap();
    assert_eq!(maximum.acl_data_max(), u16::MAX);
    assert_eq!(maximum.snapshot().available, u16::MAX);
    invariant(maximum.snapshot());
}

#[test]
fn real_framing_acceptance_and_completion_are_distinct() {
    let mut credits = owner(2, 8);
    let mut transport = Transport::default();
    let packet = frame(&[3, 0, 0x40, 0, 1, 2, 3]);
    assert_eq!(packet, [0x42, 0, 7, 0, 3, 0, 0x40, 0, 1, 2, 3]);
    for outstanding in 1..=2 {
        credits
            .send_acl(context(1), &packet, &mut transport)
            .unwrap();
        let s = credits.snapshot();
        assert_eq!(s.accepted, u64::from(outstanding));
        assert_eq!(s.outstanding, outstanding);
        assert_eq!(s.available, 2 - outstanding);
        assert_eq!(s.completed, 0);
        invariant(s);
    }
    assert_eq!(transport.last, packet);
    let event = completion(&[(0x42, 1)]);
    assert_eq!(event, [0x13, 5, 1, 0x42, 0, 1, 0]);
    assert_eq!(credits.complete(context(1), &event), Ok(1));
    let s = credits.snapshot();
    assert_eq!((s.accepted, s.completed), (2, 1));
    assert_eq!((s.available, s.outstanding), (1, 1));
    credits
        .send_acl(context(1), &packet, &mut transport)
        .unwrap();
    assert_eq!(credits.snapshot().accepted, 3);
    assert_eq!(
        credits.complete(context(1), &completion(&[(0x42, 2)])),
        Ok(2)
    );
    assert_eq!(
        (credits.snapshot().available, credits.snapshot().outstanding),
        (2, 0)
    );
    invariant(credits.snapshot());
}

#[test]
fn busy_retry_preserves_credit_and_packet() {
    let mut credits = owner(1, 8);
    let before = credits.snapshot();
    let mut transport = Transport {
        error: Some(Error::Busy),
        ..Transport::default()
    };
    let packet = frame(&[1, 2, 3]);
    for expected_calls in 1..=3 {
        assert_eq!(
            credits.send_acl(context(1), &packet, &mut transport),
            Err(CreditError::Rejected(Error::Busy))
        );
        assert_eq!(credits.snapshot(), before);
        assert_eq!(transport.calls, expected_calls);
        assert_eq!(transport.last, packet);
    }
    transport.error = None;
    assert_eq!(
        credits.send_acl(context(1), &packet, &mut transport),
        Ok(())
    );
    assert_eq!(credits.snapshot().available, 0);
    assert_eq!(credits.snapshot().accepted, 1);
}

#[test]
fn zero_credit_never_submits_without_a_completion() {
    for initial in [0, 1] {
        let mut credits = owner(initial, 4);
        let mut transport = Transport::default();
        let packet = frame(&[0, 0, 0x40, 0]);
        if initial != 0 {
            credits
                .send_acl(context(1), &packet, &mut transport)
                .unwrap();
        }
        let before = credits.snapshot();
        let before_calls = transport.calls;
        for _ in 0..4 {
            assert_eq!(
                credits.send_acl(context(1), &packet, &mut transport),
                Err(CreditError::NoCredits)
            );
            assert_eq!(credits.snapshot(), before);
            assert_eq!(transport.calls, before_calls);
        }
    }
}

#[test]
fn every_local_rejection_is_atomic_and_does_not_call_transport() {
    let valid = frame(&[1, 2, 3, 4]);
    let mut packets = Vec::new();
    for len in 0..valid.len() {
        packets.push((
            valid[..len].to_vec(),
            CreditError::InvalidAcl(ParseError::Length),
        ));
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut foreign = valid.clone();
    foreign[0] = 0x43;
    let mut bad_handle = valid.clone();
    bad_handle[0] = 0xff;
    bad_handle[1] = 0x0f;
    let mut bad_boundary = valid.clone();
    bad_boundary[1] = 0x30;
    let mut broadcast = valid.clone();
    broadcast[1] = 0x60; // Legal PB=2/BC=1, unsupported by this owner.
    packets.extend([
        (trailing, CreditError::InvalidAcl(ParseError::Length)),
        (foreign, CreditError::ForeignPacketHandle),
        (
            bad_handle,
            CreditError::InvalidAcl(ParseError::InvalidHandle),
        ),
        (
            bad_boundary,
            CreditError::InvalidAcl(ParseError::InvalidFlags),
        ),
        (broadcast, CreditError::BroadcastUnsupported),
        (frame(&[0; 5]), CreditError::AclDataTooLarge),
    ]);
    for (packet, expected) in packets {
        let mut credits = owner(1, 4);
        let before = credits.snapshot();
        let mut transport = Transport::default();
        assert_eq!(
            credits.send_acl(context(1), &packet, &mut transport),
            Err(expected)
        );
        assert_eq!(credits.snapshot(), before);
        assert_eq!(transport.calls, 0);
    }
    let mut credits = owner(1, 4);
    let before = credits.snapshot();
    let mut transport = Transport::default();
    for other in [
        context(0),
        context(2),
        CreditContext {
            handle: ConnectionHandle::new(0x43).unwrap(),
            ..context(1)
        },
    ] {
        assert_eq!(
            credits.send_acl(other, &valid, &mut transport),
            Err(CreditError::ForeignContext)
        );
        assert_eq!(credits.snapshot(), before);
        assert_eq!(transport.calls, 0);
    }
}

#[test]
fn terminal_transport_errors_are_sticky_and_never_retried_or_credited() {
    for error in [
        Error::Invalid,
        Error::BufferTooSmall,
        Error::Unsupported,
        Error::Io,
        Error::IntegrityLost,
        Error::Disconnected,
        Error::Timeout,
        Error::AbiMismatch,
        Error::Backend(i32::MIN),
        Error::Backend(i32::MAX),
    ] {
        let mut credits = owner(3, 8);
        let mut transport = Transport::default();
        let packet = frame(&[1]);
        credits
            .send_acl(context(1), &packet, &mut transport)
            .unwrap();
        let before = credits.snapshot();
        transport.error = Some(error);
        let failure = CreditFailure {
            cause: CreditFailureCause::Transport(error),
            send_outcome_uncertain: true,
        };
        assert_eq!(
            credits.send_acl(context(1), &packet, &mut transport),
            Err(CreditError::Failed(failure))
        );
        let failed = credits.snapshot();
        same_accounting(before, failed);
        assert_eq!(failed.failure, Some(failure));
        transport.error = None;
        assert_eq!(
            credits.send_acl(context(1), &packet, &mut transport),
            Err(CreditError::Failed(failure))
        );
        assert_eq!(
            credits.complete(context(1), &completion(&[(0x42, 1)])),
            Err(CreditError::Failed(failure))
        );
        assert_eq!(credits.mark_integrity_lost(), failure);
        assert_eq!(credits.snapshot(), failed);
        assert_eq!(transport.calls, 2);
    }
}

#[test]
fn explicit_loss_freezes_outstanding_credit_without_synthesizing_completions() {
    let mut credits = owner(2, 4);
    let mut transport = Transport::default();
    credits
        .send_acl(context(1), &frame(&[1]), &mut transport)
        .unwrap();
    let before = credits.snapshot();
    let failure = credits.mark_integrity_lost();
    assert_eq!(failure.cause, CreditFailureCause::IntegrityLost);
    assert!(!failure.send_outcome_uncertain);
    same_accounting(before, credits.snapshot());
    assert_eq!(
        credits.complete(context(1), &completion(&[(0x42, 1)])),
        Err(CreditError::Failed(failure))
    );
    assert_eq!(
        credits.send_acl(context(1), &frame(&[1]), &mut transport),
        Err(CreditError::Failed(failure))
    );
    assert_eq!(credits.mark_integrity_lost(), failure);
    assert_eq!(transport.calls, 1);
}

#[test]
fn completion_validation_is_whole_event_atomic_including_late_bad_entries() {
    let cases = [
        (
            completion(&[(0x42, 1), (0x43, 0)]),
            CreditFailureCause::ForeignCompletionHandle,
        ),
        (
            completion(&[(0x43, 0), (0x42, 1)]),
            CreditFailureCause::ForeignCompletionHandle,
        ),
        (
            completion(&[(0x42, 1), (0x42, 0)]),
            CreditFailureCause::DuplicateCompletionHandle,
        ),
        (
            completion(&[(0x42, 0), (0x42, 0)]),
            CreditFailureCause::DuplicateCompletionHandle,
        ),
        (completion(&[(0x42, 3)]), CreditFailureCause::OverCompletion),
        (
            completion(&[(0x42, 1), (0x0fff, 0)]),
            CreditFailureCause::MalformedCompletion(ParseError::InvalidHandle),
        ),
    ];
    for (event, cause) in cases {
        let mut credits = owner(3, 4);
        let mut transport = Transport::default();
        for _ in 0..2 {
            credits
                .send_acl(context(1), &frame(&[1]), &mut transport)
                .unwrap();
        }
        let before = credits.snapshot();
        let failure = CreditFailure {
            cause,
            send_outcome_uncertain: false,
        };
        assert_eq!(
            credits.complete(context(1), &event),
            Err(CreditError::Failed(failure))
        );
        same_accounting(before, credits.snapshot());
        let failed = credits.snapshot();
        assert_eq!(
            credits.complete(context(1), &completion(&[(0x42, 2)])),
            Err(CreditError::Failed(failure))
        );
        assert_eq!(credits.snapshot(), failed);
    }
}

#[test]
fn all_event_truncations_trailing_bytes_and_wrong_events_fail_without_reclaim() {
    let valid = completion(&[(0x42, 1)]);
    let mut malformed: Vec<Vec<u8>> = (0..valid.len()).map(|n| valid[..n].to_vec()).collect();
    malformed.push([valid.as_slice(), &[0]].concat());
    malformed.push(vec![0x0e, 0]);
    malformed.push(vec![0x13, 0]);
    malformed.push(vec![0x13, 1, 1]);
    for event in malformed {
        let mut credits = owner(1, 4);
        let mut transport = Transport::default();
        credits
            .send_acl(context(1), &frame(&[1]), &mut transport)
            .unwrap();
        let before = credits.snapshot();
        assert!(matches!(
            credits.complete(context(1), &event),
            Err(CreditError::Failed(CreditFailure {
                cause: CreditFailureCause::MalformedCompletion(_),
                ..
            }))
        ));
        same_accounting(before, credits.snapshot());
    }
}

#[test]
fn foreign_epoch_handle_and_reused_handle_never_reclaim_current_reservation() {
    for other in [
        context(0),
        context(2),
        CreditContext {
            handle: ConnectionHandle::new(0x43).unwrap(),
            ..context(1)
        },
    ] {
        let mut credits = owner(1, 4);
        let mut transport = Transport::default();
        credits
            .send_acl(context(1), &frame(&[1]), &mut transport)
            .unwrap();
        let before = credits.snapshot();
        assert!(matches!(
            credits.complete(other, &completion(&[(0x42, 1)])),
            Err(CreditError::Failed(CreditFailure {
                cause: CreditFailureCause::ForeignContext,
                ..
            }))
        ));
        same_accounting(before, credits.snapshot());
    }
    // The old event retains epoch 1; it must not be relabelled as epoch 2.
    let mut reused = AclCreditOwner::new(context(2), 1, 4).unwrap();
    let before = reused.snapshot();
    assert!(matches!(
        reused.complete(context(1), &completion(&[(0x42, 0)])),
        Err(CreditError::Failed(CreditFailure {
            cause: CreditFailureCause::ForeignContext,
            ..
        }))
    ));
    same_accounting(before, reused.snapshot());
}

#[test]
fn empty_and_zero_completions_are_noops_but_replayed_nonzero_counts_fail() {
    let mut credits = owner(1, 4);
    let before = credits.snapshot();
    assert_eq!(credits.complete(context(1), &[0x13, 1, 0]), Ok(0));
    assert_eq!(
        credits.complete(context(1), &completion(&[(0x42, 0)])),
        Ok(0)
    );
    assert_eq!(credits.snapshot(), before);
    let mut transport = Transport::default();
    credits
        .send_acl(context(1), &frame(&[1]), &mut transport)
        .unwrap();
    let event = completion(&[(0x42, 1)]);
    assert_eq!(credits.complete(context(1), &event), Ok(1));
    let before = credits.snapshot();
    assert!(matches!(
        credits.complete(context(1), &event),
        Err(CreditError::Failed(CreditFailure {
            cause: CreditFailureCause::OverCompletion,
            ..
        }))
    ));
    same_accounting(before, credits.snapshot());
}

#[test]
fn maximum_reservation_packet_size_and_completion_count_do_not_overflow() {
    let mut credits = owner(u16::MAX, 65535);
    let mut transport = Transport::default();
    let maximum = frame(&vec![0x55; 65535]);
    credits
        .send_acl(context(1), &maximum, &mut transport)
        .unwrap();
    assert_eq!(transport.last.len(), 65539);
    let tiny = frame(&[]);
    for _ in 1..u16::MAX {
        credits.send_acl(context(1), &tiny, &mut transport).unwrap();
    }
    let s = credits.snapshot();
    assert_eq!((s.available, s.outstanding), (0, u16::MAX));
    assert_eq!(s.accepted, u64::from(u16::MAX));
    assert_eq!(
        credits.complete(context(1), &completion(&[(0x42, u16::MAX)])),
        Ok(u16::MAX)
    );
    let s = credits.snapshot();
    assert_eq!((s.available, s.outstanding), (u16::MAX, 0));
    assert_eq!(s.completed, u64::from(u16::MAX));
    invariant(s);
}

#[test]
fn longest_completion_frame_is_rejected_without_partial_reclaim() {
    let mut credits = owner(1, 4);
    let mut transport = Transport::default();
    credits
        .send_acl(context(1), &frame(&[1]), &mut transport)
        .unwrap();
    let before = credits.snapshot();
    let mut entries = [(0x42, 0); 63];
    entries[0].1 = 1;
    assert_eq!(completion(&entries).len(), 255);
    assert!(matches!(
        credits.complete(context(1), &completion(&entries)),
        Err(CreditError::Failed(CreditFailure {
            cause: CreditFailureCause::DuplicateCompletionHandle,
            ..
        }))
    ));
    same_accounting(before, credits.snapshot());
}
