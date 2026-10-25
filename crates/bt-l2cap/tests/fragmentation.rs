// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic packets only. No controller capacity, credits or hardware claims.

use bt_hci::{ConnectionHandle, PacketBoundary, ParseError as HciParseError};
use bt_l2cap::{
    Acceptance, Cancellation, CancellationReason, Connection, FragmentError, Fragmenter,
    ParseError, TerminalFailure,
};

fn connection(handle: u16, epoch: u64) -> Connection {
    Connection {
        handle: ConnectionHandle::new(handle).unwrap(),
        epoch,
    }
}

const PDU: [u8; 11] = [7, 0, 0x40, 0, 1, 2, 3, 4, 5, 6, 7];

#[test]
fn exact_bytes_use_non_flushable_start_then_continuations() {
    let own = connection(0x0234, 9);
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, 5, &mut storage).unwrap();
    fragments.start(own, &PDU).unwrap();
    let mut output = [0xee; 10];

    let first = fragments.prepare(own, &mut output).unwrap();
    assert_eq!(first.packet_len, 9);
    assert_eq!(first.pdu_offset, 0);
    assert_eq!(first.acl_payload_len, 5);
    assert_eq!(first.boundary, PacketBoundary::StartNonFlushable);
    assert!(!first.final_fragment);
    assert_eq!(output, [0x34, 0x02, 5, 0, 7, 0, 0x40, 0, 1, 0xee]);
    assert_eq!(
        fragments.confirm_accepted(own),
        Ok(Acceptance::More {
            accepted_pdu_bytes: 5,
            accepted_fragments: 1,
        })
    );
    assert_eq!(
        fragments.confirm_accepted(own),
        Err(FragmentError::NoPreparedFragment)
    );
    assert_eq!(fragments.accepted_pdu_bytes(), 5);
    assert_eq!(fragments.accepted_fragments(), 1);

    let second = fragments.prepare(own, &mut output).unwrap();
    assert_eq!(second.boundary, PacketBoundary::Continuation);
    assert_eq!(second.pdu_offset, 5);
    assert_eq!(second.acl_payload_len, 5);
    assert!(!second.final_fragment);
    assert_eq!(output, [0x34, 0x12, 5, 0, 2, 3, 4, 5, 6, 0xee]);
    assert_eq!(
        fragments.confirm_accepted(own),
        Ok(Acceptance::More {
            accepted_pdu_bytes: 10,
            accepted_fragments: 2,
        })
    );

    let third = fragments.prepare(own, &mut output).unwrap();
    assert_eq!(third.boundary, PacketBoundary::Continuation);
    assert_eq!(third.acl_payload_len, 1);
    assert!(third.final_fragment);
    assert_eq!(&output[..5], &[0x34, 0x12, 1, 0, 7]);
    assert_eq!(
        fragments.confirm_accepted(own),
        Ok(Acceptance::Complete {
            pdu_len: PDU.len(),
            fragments: 3,
        })
    );
    assert!(!fragments.is_busy());
}

#[test]
fn exact_boundary_is_one_packet_and_one_octet_over_adds_one_continuation() {
    let own = connection(1, 1);
    let exact = [4, 0, 0x40, 0, 1, 2, 3, 4];
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, exact.len(), &mut storage).unwrap();
    fragments.start(own, &exact).unwrap();
    let mut output = [0xee; 12];
    assert!(fragments.prepare(own, &mut output).unwrap().final_fragment);
    assert_eq!(&output, &[1, 0, 8, 0, 4, 0, 0x40, 0, 1, 2, 3, 4]);
    assert_eq!(
        fragments.confirm_accepted(own),
        Ok(Acceptance::Complete {
            pdu_len: 8,
            fragments: 1,
        })
    );

    let over = [5, 0, 0x40, 0, 1, 2, 3, 4, 5];
    fragments.start(own, &over).unwrap();
    assert!(!fragments.prepare(own, &mut output).unwrap().final_fragment);
    fragments.confirm_accepted(own).unwrap();
    let last = fragments.prepare(own, &mut output).unwrap();
    assert_eq!(last.acl_payload_len, 1);
    assert_eq!(last.boundary, PacketBoundary::Continuation);
    assert!(last.final_fragment);
    assert_eq!(&output[..5], &[1, 0x10, 1, 0, 5]);
}

#[test]
fn one_octet_acl_maximum_handles_every_pdu_octet() {
    let own = connection(0x0eff, 2);
    let zero_payload = [0, 0, 1, 0];
    let mut storage = [0; 4];
    let mut fragments = Fragmenter::new(own, 1, &mut storage).unwrap();
    fragments.start(own, &zero_payload).unwrap();
    let mut output = [0xee; 6];
    for (index, expected) in zero_payload.into_iter().enumerate() {
        let prepared = fragments.prepare(own, &mut output).unwrap();
        assert_eq!(prepared.pdu_offset, index);
        assert_eq!(prepared.acl_payload_len, 1);
        assert_eq!(prepared.final_fragment, index == 3);
        assert_eq!(
            prepared.boundary,
            if index == 0 {
                PacketBoundary::StartNonFlushable
            } else {
                PacketBoundary::Continuation
            }
        );
        let expected_flags = if index == 0 {
            [0xff, 0x0e]
        } else {
            [0xff, 0x1e]
        };
        assert_eq!(&output[..2], &expected_flags);
        assert_eq!(&output[2..5], &[1, 0, expected]);
        let accepted = fragments.confirm_accepted(own).unwrap();
        assert_eq!(matches!(accepted, Acceptance::Complete { .. }), index == 3);
    }
}

#[test]
fn short_output_and_repeated_prepare_do_not_advance_or_change_bytes() {
    let own = connection(2, 3);
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, 6, &mut storage).unwrap();
    fragments.start(own, &PDU).unwrap();
    assert_eq!(
        fragments.confirm_accepted(own),
        Err(FragmentError::NoPreparedFragment)
    );

    let mut short = [0xaa; 9];
    assert_eq!(
        fragments.prepare(own, &mut short),
        Err(FragmentError::BufferTooSmall)
    );
    assert_eq!(short, [0xaa; 9]);
    assert_eq!(fragments.accepted_pdu_bytes(), 0);
    assert_eq!(fragments.accepted_fragments(), 0);

    let mut first = [0; 10];
    let prepared = fragments.prepare(own, &mut first).unwrap();
    let mut retry = [0; 10];
    assert_eq!(fragments.prepare(own, &mut retry), Ok(prepared));
    assert_eq!(retry, first);
    assert_eq!(fragments.accepted_pdu_bytes(), 0);
    assert_eq!(fragments.accepted_fragments(), 0);

    let mut retry_short = [0xbb; 9];
    assert_eq!(
        fragments.prepare(own, &mut retry_short),
        Err(FragmentError::BufferTooSmall)
    );
    assert_eq!(retry_short, [0xbb; 9]);
    assert_eq!(fragments.prepare(own, &mut retry), Ok(prepared));
}

#[test]
fn busy_and_invalid_start_leave_the_original_pdu_in_flight() {
    let own = connection(3, 4);
    let replacement = [1, 0, 0x41, 0, 9];
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, 8, &mut storage).unwrap();
    fragments.start(own, &PDU).unwrap();
    assert_eq!(fragments.start(own, &replacement), Err(FragmentError::Busy));
    assert_eq!(fragments.pdu_len(), Some(PDU.len()));
    let cancelled = fragments.cancel(own).unwrap();
    assert_eq!(cancelled.pdu_len, PDU.len());

    let invalid = [2, 0, 0x40, 0, 1];
    assert_eq!(
        fragments.start(own, &invalid),
        Err(FragmentError::InvalidPdu(ParseError::Length))
    );
    assert!(!fragments.is_busy());
}

#[test]
fn start_copy_is_atomic_and_source_storage_is_immediately_reusable() {
    let own = connection(3, 5);
    let mut pdu_storage = [0xee; 8];
    let mut source = [4, 0, 0x40, 0, 1, 2, 3, 4];
    let mut fragments = Fragmenter::new(own, 8, &mut pdu_storage).unwrap();

    fragments.start(own, &source).unwrap();
    source.copy_from_slice(&[4, 0, 0x41, 0, 9, 8, 7, 6]);
    let mut output = [0; 12];
    fragments.prepare(own, &mut output).unwrap();
    assert_eq!(&output[4..], &[4, 0, 0x40, 0, 1, 2, 3, 4]);
    fragments.confirm_accepted(own).unwrap();

    fragments.start(own, &source).unwrap();
    source.fill(0xaa);
    fragments.prepare(own, &mut output).unwrap();
    assert_eq!(&output[4..], &[4, 0, 0x41, 0, 9, 8, 7, 6]);
    fragments.confirm_accepted(own).unwrap();
    assert!(!fragments.is_busy());
}

#[test]
fn invalid_or_oversized_start_preserves_state_and_pdu_storage() {
    let own = connection(3, 6);
    let mut storage = [0xee; 7];
    let mut fragments = Fragmenter::new(own, 4, &mut storage).unwrap();
    let original = [0xee; 7];

    let malformed = [2, 0, 0x40, 0, 1];
    assert_eq!(
        fragments.start(own, &malformed),
        Err(FragmentError::InvalidPdu(ParseError::Length))
    );
    assert_eq!(fragments.pdu_storage(), original);
    assert!(!fragments.is_busy());

    let valid_but_large = [4, 0, 0x40, 0, 1, 2, 3, 4];
    assert_eq!(
        fragments.start(own, &valid_but_large),
        Err(FragmentError::PduTooLarge)
    );
    assert_eq!(fragments.pdu_storage(), original);
    assert!(!fragments.is_busy());

    let fits = [3, 0, 0x40, 0, 1, 2, 3];
    fragments.start(own, &fits).unwrap();
    assert_eq!(fragments.pdu_storage(), fits);
}

#[test]
fn stale_epoch_and_foreign_handle_never_change_state_or_output() {
    let own = connection(4, 8);
    let stale = connection(4, 7);
    let future = connection(4, 9);
    let foreign = connection(5, 8);
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, 5, &mut storage).unwrap();
    fragments.start(own, &PDU).unwrap();
    let mut output = [0xcc; 9];
    for context in [stale, future] {
        assert_eq!(
            fragments.prepare(context, &mut output),
            Err(FragmentError::InvalidEpoch)
        );
        assert_eq!(output, [0xcc; 9]);
    }
    assert_eq!(
        fragments.prepare(foreign, &mut output),
        Err(FragmentError::ForeignConnection)
    );
    assert_eq!(output, [0xcc; 9]);
    assert_eq!(fragments.accepted_pdu_bytes(), 0);
}

#[test]
fn length_handle_and_acl_maximum_overflow_are_rejected() {
    let own = connection(0, 0);
    assert!(matches!(
        ConnectionHandle::new(0x0f00),
        Err(HciParseError::InvalidHandle)
    ));
    let mut storage = [0x44; 65_539];
    assert!(matches!(
        Fragmenter::new(own, 0, &mut storage),
        Err(FragmentError::InvalidAclDataMaximum)
    ));
    assert!(matches!(
        Fragmenter::new(own, usize::from(u16::MAX) + 1, &mut storage),
        Err(FragmentError::InvalidAclDataMaximum)
    ));
    let mut too_short = [0x33; 3];
    assert!(matches!(
        Fragmenter::new(own, 1, &mut too_short),
        Err(FragmentError::PduStorageTooSmall)
    ));

    let mut oversized = [0x55; 65_540];
    oversized[..4].copy_from_slice(&[0xff, 0xff, 0x40, 0]);
    let mut fragments = Fragmenter::new(own, usize::from(u16::MAX), &mut storage).unwrap();
    assert_eq!(
        fragments.start(own, &oversized),
        Err(FragmentError::InvalidPdu(ParseError::Length))
    );
    assert!(!fragments.is_busy());
}

#[test]
fn cancellation_requires_no_accepted_fragment_or_terminal_failure() {
    let own = connection(6, 10);
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(own, 5, &mut storage).unwrap();
    fragments.start(own, &PDU).unwrap();
    let before = fragments.cancel(own).unwrap();
    assert_eq!(
        before,
        Cancellation {
            connection: own,
            pdu_len: PDU.len(),
            accepted_pdu_bytes: 0,
            accepted_fragments: 0,
            reason: CancellationReason::BeforeFirstAccepted,
        }
    );

    fragments.start(own, &PDU).unwrap();
    let mut output = [0; 9];
    fragments.prepare(own, &mut output).unwrap();
    fragments.confirm_accepted(own).unwrap();
    assert_eq!(
        fragments.cancel(own),
        Err(FragmentError::CannotCancelPartial)
    );
    fragments
        .mark_terminal_failure(own, TerminalFailure::CreditsUnreconciled)
        .unwrap();
    assert_eq!(
        fragments.prepare(own, &mut output),
        Err(FragmentError::Failed(TerminalFailure::CreditsUnreconciled))
    );
    let failed = fragments.cancel(own).unwrap();
    assert_eq!(failed.accepted_pdu_bytes, 5);
    assert_eq!(failed.accepted_fragments, 1);
    assert_eq!(
        failed.reason,
        CancellationReason::Terminal(TerminalFailure::CreditsUnreconciled)
    );
    assert!(!fragments.is_busy());
}

#[test]
fn newer_epoch_reports_partial_cancellation_and_rejects_epoch_reuse() {
    let old = connection(7, 20);
    let next = connection(7, 21);
    let mut storage = [0; 16];
    let mut fragments = Fragmenter::new(old, 5, &mut storage).unwrap();
    fragments.start(old, &PDU).unwrap();
    let mut output = [0; 9];
    fragments.prepare(old, &mut output).unwrap();
    fragments.confirm_accepted(old).unwrap();

    for invalid in [old, connection(8, 19)] {
        assert_eq!(
            fragments.replace_connection(invalid),
            Err(FragmentError::InvalidEpoch)
        );
        assert_eq!(fragments.connection(), old);
        assert!(fragments.is_busy());
    }
    let cancelled = fragments.replace_connection(next).unwrap().unwrap();
    assert_eq!(cancelled.connection, old);
    assert_eq!(cancelled.accepted_pdu_bytes, 5);
    assert_eq!(cancelled.accepted_fragments, 1);
    assert_eq!(cancelled.reason, CancellationReason::EpochChange { next });
    assert_eq!(fragments.connection(), next);
    assert!(!fragments.is_busy());
    assert_eq!(
        fragments.prepare(old, &mut output),
        Err(FragmentError::InvalidEpoch)
    );
}

#[test]
fn maximum_l2cap_pdu_fragments_without_allocation_or_length_loss() {
    let own = connection(0x0eff, u64::MAX);
    let mut pdu = [0xa5; 65_539];
    pdu[..4].copy_from_slice(&[0xff, 0xff, 0x40, 0]);
    let mut storage = [0; 65_539];
    let mut fragments = Fragmenter::new(own, usize::from(u16::MAX), &mut storage).unwrap();
    fragments.start(own, &pdu).unwrap();
    let mut first = [0; 65_539];
    let first_meta = fragments.prepare(own, &mut first).unwrap();
    assert_eq!(first_meta.acl_payload_len, 65_535);
    assert!(!first_meta.final_fragment);
    assert_eq!(&first[..4], &[0xff, 0x0e, 0xff, 0xff]);
    fragments.confirm_accepted(own).unwrap();

    let mut final_packet = [0; 8];
    let final_meta = fragments.prepare(own, &mut final_packet).unwrap();
    assert_eq!(final_meta.acl_payload_len, 4);
    assert!(final_meta.final_fragment);
    assert_eq!(&final_packet[..4], &[0xff, 0x1e, 4, 0]);
    assert_eq!(
        fragments.confirm_accepted(own),
        Ok(Acceptance::Complete {
            pdu_len: 65_539,
            fragments: 2,
        })
    );
}
