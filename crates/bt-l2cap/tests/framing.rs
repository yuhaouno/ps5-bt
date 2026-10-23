// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic packets only. No device, link, MTU or hardware claims.
use bt_core::Error;
use bt_hci::{AclPacket, BroadcastFlag, ConnectionHandle, PacketBoundary};
use bt_l2cap::{BasicFrame, Connection, ParseError, Reassembler, ReassemblyError};

fn connection(handle: u16, epoch: u64) -> Connection {
    Connection {
        handle: ConnectionHandle::new(handle).unwrap(),
        epoch,
    }
}

fn acl(connection: Connection, boundary: PacketBoundary, payload: &[u8]) -> AclPacket<'_> {
    AclPacket::new(
        connection.handle,
        boundary,
        BroadcastFlag::PointToPoint,
        payload,
    )
    .unwrap()
}

#[test]
fn basic_header_round_trip_byte_order_and_capacity() {
    let frame = BasicFrame::new(0x1234, &[0xaa, 0xbb, 0xcc]).unwrap();
    let mut bytes = [0xee; 8];
    assert_eq!(frame.encode(&mut bytes), Ok(7));
    assert_eq!(bytes, [3, 0, 0x34, 0x12, 0xaa, 0xbb, 0xcc, 0xee]);
    assert_eq!(BasicFrame::parse(&bytes[..7]), Ok(frame));
    assert_eq!(frame.channel_id(), 0x1234);
    assert_eq!(frame.payload(), [0xaa, 0xbb, 0xcc]);
    let mut short = [0xee; 6];
    assert_eq!(frame.encode(&mut short), Err(Error::BufferTooSmall));
    assert_eq!(short, [0xee; 6]);
    assert_eq!(BasicFrame::new(0, &[]), Err(Error::Invalid));
    assert_eq!(
        BasicFrame::parse(&[0, 0, 0, 0]),
        Err(ParseError::NullChannel)
    );
    assert!(
        BasicFrame::parse(&[0, 0, 1, 0])
            .unwrap()
            .payload()
            .is_empty()
    );
}

#[test]
fn all_basic_lengths_and_maximum_frame_truncations() {
    let payload = [0x7a; 65536];
    assert_eq!(BasicFrame::new(1, &payload), Err(Error::Invalid));
    let frame = BasicFrame::new(0x0040, &payload[..65535]).unwrap();
    let mut bytes = [0; 65540];
    assert_eq!(frame.encode(&mut bytes), Ok(65539));
    assert_eq!(&bytes[..4], &[0xff, 0xff, 0x40, 0]);
    for actual in 0..65539 {
        assert_eq!(BasicFrame::parse(&bytes[..actual]), Err(ParseError::Length));
    }
    assert_eq!(BasicFrame::parse(&bytes[..65539]), Ok(frame));
    assert_eq!(BasicFrame::parse(&bytes), Err(ParseError::Length));
    for advertised in 0..=u16::MAX {
        bytes[..2].copy_from_slice(&advertised.to_le_bytes());
        assert_eq!(BasicFrame::parse(&bytes[..32]).is_ok(), advertised == 28);
    }
}

#[test]
fn all_two_and_three_fragment_boundaries_include_split_headers_and_empty_fragments() {
    let connection = connection(0x0234, 5);
    let bytes = [8, 0, 0x40, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    let expected = BasicFrame::parse(&bytes).unwrap();
    for first in 0..bytes.len() {
        for second in first..=bytes.len() {
            let mut storage = [0xee; 12];
            let mut r = Reassembler::new(connection, &mut storage).unwrap();
            assert_eq!(
                r.push(
                    connection,
                    acl(connection, PacketBoundary::StartFlushable, &bytes[..first])
                ),
                Ok(None)
            );
            assert!(r.in_progress());
            assert_eq!(r.pending_len(), first);
            assert_eq!(r.expected_len(), if first >= 4 { Some(12) } else { None });
            let middle = r
                .push(
                    connection,
                    acl(
                        connection,
                        PacketBoundary::Continuation,
                        &bytes[first..second],
                    ),
                )
                .unwrap();
            if second == bytes.len() {
                assert_eq!(middle, Some(expected));
            } else {
                assert_eq!(middle, None);
                assert_eq!(
                    r.push(
                        connection,
                        acl(connection, PacketBoundary::Continuation, &bytes[second..])
                    ),
                    Ok(Some(expected))
                );
            }
            assert!(!r.in_progress());
            assert_eq!(r.pending_len(), 0);
            assert_eq!(r.expected_len(), None);
            assert_eq!(storage, bytes);
        }
    }
}

#[test]
fn complete_and_zero_payload_pdus_can_follow_without_retained_state() {
    let connection = connection(0, 0);
    let mut storage = [0; 8];
    let mut r = Reassembler::new(connection, &mut storage).unwrap();
    for bytes in [
        &[0, 0, 1, 0][..],
        &[4, 0, 0x40, 0, 1, 2, 3, 4][..],
        &[0, 0, 1, 0][..],
    ] {
        assert_eq!(
            r.push(
                connection,
                acl(connection, PacketBoundary::StartFlushable, bytes)
            ),
            Ok(Some(BasicFrame::parse(bytes).unwrap()))
        );
    }
    assert_eq!(
        r.push(
            connection,
            acl(connection, PacketBoundary::Continuation, &[])
        ),
        Err(ReassemblyError::MissingStart)
    );
}

#[test]
fn missing_start_foreign_context_bad_flags_and_overshoots_never_replace_pending_data() {
    let own = connection(0x0123, 5);
    let foreign = connection(0x0456, 5);
    let stale = connection(0x0123, 4);
    let future = connection(0x0123, 6);
    let bytes = [4, 0, 0x40, 0, 1, 2, 3, 4];
    let mut storage = [0xee; 16];
    let mut r = Reassembler::new(own, &mut storage).unwrap();
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::Continuation, &[])),
        Err(ReassemblyError::MissingStart)
    );
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::StartFlushable, &bytes[..5])),
        Ok(None)
    );
    let pending = (r.pending_len(), r.expected_len(), r.connection());
    for context in [foreign, stale, future] {
        assert_eq!(
            r.push(context, acl(own, PacketBoundary::Continuation, &[9])),
            Err(ReassemblyError::ForeignConnection)
        );
    }
    assert_eq!(
        r.push(own, acl(foreign, PacketBoundary::Continuation, &[9])),
        Err(ReassemblyError::ForeignConnection)
    );
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::StartNonFlushable, &[9])),
        Err(ReassemblyError::InvalidFlags)
    );
    let broadcast = AclPacket::new(
        own.handle,
        PacketBoundary::Continuation,
        BroadcastFlag::ActivePeripheral,
        &[9],
    )
    .unwrap();
    assert_eq!(r.push(own, broadcast), Err(ReassemblyError::InvalidFlags));
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::StartFlushable, &[9; 8])),
        Err(ReassemblyError::UnexpectedStart)
    );
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::Continuation, &[9; 4])),
        Err(ReassemblyError::Length)
    );
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::Continuation, &[9; 12])),
        Err(ReassemblyError::BufferTooSmall)
    );
    assert_eq!((r.pending_len(), r.expected_len(), r.connection()), pending);
    assert!(r.in_progress());
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::Continuation, &bytes[5..])),
        Ok(Some(BasicFrame::parse(&bytes).unwrap()))
    );
    assert_eq!(&storage[..8], bytes);
    assert_eq!(&storage[8..], &[0xee; 8]);
}

#[test]
fn each_connection_has_independent_storage_when_acl_handles_interleave() {
    let a = connection(1, 5);
    let b = connection(2, 7);
    let mut a_storage = [0; 8];
    let mut b_storage = [0; 8];
    let mut a_r = Reassembler::new(a, &mut a_storage).unwrap();
    let mut b_r = Reassembler::new(b, &mut b_storage).unwrap();
    let a_bytes = [2, 0, 0x40, 0, 0xaa, 0xab];
    let b_bytes = [2, 0, 0x41, 0, 0xba, 0xbb];
    assert_eq!(
        a_r.push(a, acl(a, PacketBoundary::StartFlushable, &a_bytes[..3])),
        Ok(None)
    );
    assert_eq!(
        b_r.push(b, acl(b, PacketBoundary::StartFlushable, &b_bytes[..5])),
        Ok(None)
    );
    assert_eq!(
        a_r.push(a, acl(b, PacketBoundary::Continuation, &b_bytes[5..])),
        Err(ReassemblyError::ForeignConnection)
    );
    assert_eq!(
        a_r.push(a, acl(a, PacketBoundary::Continuation, &a_bytes[3..])),
        Ok(Some(BasicFrame::parse(&a_bytes).unwrap()))
    );
    assert_eq!(
        b_r.push(b, acl(b, PacketBoundary::Continuation, &b_bytes[5..])),
        Ok(Some(BasicFrame::parse(&b_bytes).unwrap()))
    );
}

#[test]
fn reset_requires_new_epoch_and_old_context_cannot_complete_reused_handle() {
    let old = connection(1, 9);
    let new = connection(1, 10);
    let mut storage = [0xee; 8];
    let mut r = Reassembler::new(old, &mut storage).unwrap();
    assert_eq!(
        r.push(
            old,
            acl(old, PacketBoundary::StartFlushable, &[4, 0, 0x40, 0, 1])
        ),
        Ok(None)
    );
    for rejected in [old, connection(2, 9), connection(1, 8)] {
        assert_eq!(r.reset(rejected), Err(ReassemblyError::InvalidEpoch));
        assert_eq!(r.pending_len(), 5);
        assert_eq!(r.connection(), old);
    }
    assert_eq!(r.reset(new), Ok(()));
    assert!(!r.in_progress());
    assert_eq!(r.pending_len(), 0);
    assert_eq!(r.expected_len(), None);
    assert_eq!(
        r.push(old, acl(old, PacketBoundary::Continuation, &[2, 3, 4])),
        Err(ReassemblyError::ForeignConnection)
    );
    assert_eq!(
        r.push(new, acl(new, PacketBoundary::Continuation, &[2, 3, 4])),
        Err(ReassemblyError::MissingStart)
    );
    let bytes = [1, 0, 0x40, 0, 9];
    assert_eq!(
        r.push(new, acl(new, PacketBoundary::StartFlushable, &bytes)),
        Ok(Some(BasicFrame::parse(&bytes).unwrap()))
    );
    assert_eq!(r.reset(connection(2, 11)), Ok(()));
    assert_eq!(r.reset(connection(2, u64::MAX)), Ok(()));
    assert_eq!(
        r.reset(connection(2, 0)),
        Err(ReassemblyError::InvalidEpoch)
    );
}

#[test]
fn capacity_and_invalid_header_checks_happen_before_writing() {
    let own = connection(1, 2);
    let mut too_small = [0; 3];
    assert!(matches!(
        Reassembler::new(own, &mut too_small),
        Err(Error::BufferTooSmall)
    ));
    for invalid in [
        &[0xff, 0xff, 0x40, 0][..],
        &[1, 0, 0, 0][..],
        &[0, 0, 1, 0, 9][..],
        &[1; 17][..],
    ] {
        let mut storage = [0xee; 16];
        {
            let mut r = Reassembler::new(own, &mut storage).unwrap();
            assert!(
                r.push(own, acl(own, PacketBoundary::StartFlushable, invalid))
                    .is_err()
            );
            assert!(!r.in_progress());
            assert_eq!(r.pending_len(), 0);
            assert_eq!(r.expected_len(), None);
        }
        assert_eq!(storage, [0xee; 16]);
    }
    // A split header may reveal a capacity/null-CID failure on continuation.
    for (prefix, suffix, error) in [
        (
            &[0xff][..],
            &[0xff, 0x40, 0][..],
            ReassemblyError::BufferTooSmall,
        ),
        (&[0, 0, 0][..], &[0][..], ReassemblyError::NullChannel),
    ] {
        let mut storage = [0xee; 16];
        {
            let mut r = Reassembler::new(own, &mut storage).unwrap();
            assert_eq!(
                r.push(own, acl(own, PacketBoundary::StartFlushable, prefix)),
                Ok(None)
            );
            assert_eq!(
                r.push(own, acl(own, PacketBoundary::Continuation, suffix)),
                Err(error)
            );
            assert_eq!(r.pending_len(), prefix.len());
            assert_eq!(r.expected_len(), None);
            assert!(r.in_progress());
        }
        assert_eq!(&storage[..prefix.len()], prefix);
        assert!(storage[prefix.len()..].iter().all(|&byte| byte == 0xee));
    }
}

#[test]
fn maximum_pdu_recombines_across_maximum_acl_payload_boundary() {
    let own = connection(0x0eff, 1);
    let mut bytes = [0x7a; 65539];
    bytes[..4].copy_from_slice(&[0xff, 0xff, 0x40, 0]);
    let mut storage = [0xee; 65539];
    let mut r = Reassembler::new(own, &mut storage).unwrap();
    assert_eq!(
        r.push(
            own,
            acl(own, PacketBoundary::StartFlushable, &bytes[..65535])
        ),
        Ok(None)
    );
    assert_eq!(r.pending_len(), 65535);
    assert_eq!(r.expected_len(), Some(65539));
    assert_eq!(
        r.push(own, acl(own, PacketBoundary::Continuation, &bytes[65535..])),
        Ok(Some(BasicFrame::parse(&bytes).unwrap()))
    );
    assert!(!r.in_progress());
    assert_eq!(storage, bytes);
}
