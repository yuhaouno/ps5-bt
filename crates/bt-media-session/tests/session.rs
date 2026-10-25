// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic media frames only. No clock, controller credits or hardware claim.

use bt_a2dp::{PacketError, SbcRtpPacketizer};
use bt_core::{
    Error as CoreError, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use bt_hci::{ConnectionHandle, PacketBoundary};
use bt_l2cap::{CancellationReason, Connection, FragmentError, TerminalFailure};
use bt_media_session::{Error, MediaSession, SendProgress, SessionState};

fn parameters() -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap()
}

fn connection(handle: u16, epoch: u64) -> Connection {
    Connection {
        handle: ConnectionHandle::new(handle).unwrap(),
        epoch,
    }
}

fn packetizer(mtu: u16) -> SbcRtpPacketizer {
    SbcRtpPacketizer::new(parameters(), mtu, 96, 0x1122_3344, 7, 1000).unwrap()
}

fn frame(fill: u8) -> [u8; 77] {
    let mut bytes = [fill; 77];
    bytes[0] = 0x9c;
    bytes
}

#[test]
fn one_frame_crosses_rtp_l2cap_and_three_confirmed_acl_fragments() {
    let own = connection(0x234, 5);
    let encoded = frame(0x44);
    let mut pdu_storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x0041, 40, &mut pdu_storage).unwrap();
    session.mark_next_packet().unwrap();
    let mut rtp = [0xa5; 90];
    let mut pdu = [0xa5; 94];
    let queued = session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();

    assert_eq!((queued.rtp.bytes, queued.l2cap_pdu_bytes), (90, 94));
    assert_eq!((queued.rtp.sequence, queued.rtp.timestamp), (7, 1000));
    assert!(queued.rtp.marker);
    assert_eq!(&pdu[..4], &[90, 0, 0x41, 0]);
    assert_eq!(&pdu[4..17], &rtp[..13]);
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (8, 1128)
    );

    let mut acl = [0xee; 44];
    let first = session.prepare_acl(own, &mut acl).unwrap();
    assert_eq!(first.boundary, PacketBoundary::StartNonFlushable);
    assert_eq!((first.acl_payload_len, first.pdu_offset), (40, 0));
    assert_eq!(&acl[4..44], &pdu[..40]);
    assert_eq!(
        session.confirm_acl_accepted(own),
        Ok(SendProgress::More {
            accepted_pdu_bytes: 40,
            accepted_fragments: 1,
        })
    );

    let second = session.prepare_acl(own, &mut acl).unwrap();
    assert_eq!(second.boundary, PacketBoundary::Continuation);
    assert_eq!((second.acl_payload_len, second.pdu_offset), (40, 40));
    session.confirm_acl_accepted(own).unwrap();

    let third = session.prepare_acl(own, &mut acl).unwrap();
    assert_eq!((third.acl_payload_len, third.pdu_offset), (14, 80));
    assert!(third.final_fragment);
    assert_eq!(
        session.confirm_acl_accepted(own),
        Ok(SendProgress::Complete {
            packet: queued,
            fragments: 3,
        })
    );
    assert_eq!(session.queued(), None);
    assert_eq!(session.accepted_pdu_bytes(), 0);
}

#[test]
fn rejected_transport_attempt_reprepares_identical_bytes_without_advancing() {
    let own = connection(1, 9);
    let encoded = frame(0x22);
    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0u8; 90];
    let mut pdu = [0u8; 94];
    session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();
    assert_eq!(
        session.confirm_acl_accepted(own),
        Err(Error::Fragment(FragmentError::NoPreparedFragment))
    );

    let mut first = [0u8; 52];
    let prepared = session.prepare_acl(own, &mut first).unwrap();
    let mut retry = [0u8; 52];
    assert_eq!(session.prepare_acl(own, &mut retry), Ok(prepared));
    assert_eq!(retry, first);
    assert_eq!(
        (session.accepted_pdu_bytes(), session.accepted_fragments()),
        (0, 0)
    );

    let mut short = [0xa5; 51];
    assert_eq!(
        session.prepare_acl(own, &mut short),
        Err(Error::Fragment(FragmentError::BufferTooSmall))
    );
    assert_eq!(short, [0xa5; 51]);
    assert_eq!(session.prepare_acl(own, &mut retry), Ok(prepared));
}

#[test]
fn busy_queue_does_not_replace_the_retained_pdu() {
    let own = connection(10, 1);
    let first = frame(0x11);
    let second = frame(0x22);
    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0u8; 90];
    let mut pdu = [0u8; 94];
    let queued = session
        .queue_sbc_frames(own, &[&first], &mut rtp, &mut pdu)
        .unwrap();
    let retained: [u8; 94] = session.pdu_storage().try_into().unwrap();
    let mut other_rtp = [0xa5; 90];
    let mut other_pdu = [0xa5; 94];

    assert_eq!(
        session.queue_sbc_frames(own, &[&second], &mut other_rtp, &mut other_pdu),
        Err(Error::Busy)
    );
    assert_eq!(session.queued(), Some(queued));
    assert_eq!(session.pdu_storage(), retained);
    assert_eq!(other_rtp, [0xa5; 90]);
    assert_eq!(other_pdu, [0xa5; 94]);
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (8, 1128)
    );
}

#[test]
fn stale_or_foreign_connection_is_rejected_before_packet_state_changes() {
    let own = connection(2, 10);
    let stale = connection(2, 9);
    let foreign = connection(3, 10);
    let encoded = frame(0x33);
    let mut storage = [0xee; 94];
    let original_storage = storage;
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 60, &mut storage).unwrap();
    let mut rtp = [0xa5; 90];
    let mut pdu = [0xa5; 94];

    assert_eq!(
        session.queue_sbc_frames(stale, &[&encoded], &mut rtp, &mut pdu),
        Err(Error::Fragment(FragmentError::InvalidEpoch))
    );
    assert_eq!(
        session.queue_sbc_frames(foreign, &[&encoded], &mut rtp, &mut pdu),
        Err(Error::Fragment(FragmentError::ForeignConnection))
    );
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (7, 1000)
    );
    assert_eq!(rtp, [0xa5; 90]);
    assert_eq!(pdu, [0xa5; 94]);
    assert_eq!(session.queued(), None);
    assert_eq!(session.pdu_storage(), original_storage);
}

#[test]
fn stop_before_first_accept_is_counted_but_partial_healthy_pdu_cannot_be_dropped() {
    let own = connection(4, 1);
    let encoded = frame(0x55);
    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0u8; 90];
    let mut pdu = [0u8; 94];
    let queued = session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();
    let dropped = session.stop(own).unwrap().unwrap();
    assert_eq!(dropped.packet, queued);
    assert_eq!(
        dropped.cancellation.reason,
        CancellationReason::BeforeFirstAccepted
    );
    assert_eq!(session.state(), SessionState::Stopped);

    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();
    let mut acl = [0u8; 52];
    session.prepare_acl(own, &mut acl).unwrap();
    session.confirm_acl_accepted(own).unwrap();
    assert_eq!(
        session.stop(own),
        Err(Error::Fragment(FragmentError::CannotCancelPartial))
    );
    assert_eq!(session.state(), SessionState::Active);
    assert!(session.queued().is_some());
    assert_eq!(
        (session.accepted_pdu_bytes(), session.accepted_fragments()),
        (48, 1)
    );
}

#[test]
fn terminal_failure_can_cancel_partial_pdu_with_durable_accounting() {
    let own = connection(5, 1);
    let encoded = frame(0x66);
    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0u8; 90];
    let mut pdu = [0u8; 94];
    session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();
    let mut acl = [0u8; 52];
    session.prepare_acl(own, &mut acl).unwrap();
    session.confirm_acl_accepted(own).unwrap();

    let dropped = session
        .fail_transport(own, TerminalFailure::TransportLost)
        .unwrap()
        .unwrap();
    assert_eq!(dropped.cancellation.accepted_pdu_bytes, 48);
    assert_eq!(dropped.cancellation.accepted_fragments, 1);
    assert_eq!(
        dropped.cancellation.reason,
        CancellationReason::Terminal(TerminalFailure::TransportLost)
    );
    assert_eq!(
        session.state(),
        SessionState::Failed(TerminalFailure::TransportLost)
    );
    assert_eq!(session.prepare_acl(own, &mut acl), Err(Error::InvalidState));
}

#[test]
fn newer_epoch_cancels_partial_and_requires_a_fresh_media_session() {
    let own = connection(6, 20);
    let next = connection(7, 21);
    let encoded = frame(0x77);
    let mut storage = [0u8; 94];
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0u8; 90];
    let mut pdu = [0u8; 94];
    session
        .queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu)
        .unwrap();
    let mut acl = [0u8; 52];
    session.prepare_acl(own, &mut acl).unwrap();
    session.confirm_acl_accepted(own).unwrap();

    let dropped = session.invalidate_epoch(next).unwrap().unwrap();
    assert_eq!(dropped.cancellation.accepted_pdu_bytes, 48);
    assert_eq!(
        dropped.cancellation.reason,
        CancellationReason::EpochChange { next }
    );
    assert_eq!(session.connection(), next);
    assert_eq!(session.state(), SessionState::Stopped);
    assert_eq!(
        session.queue_sbc_frames(next, &[&encoded], &mut rtp, &mut pdu),
        Err(Error::InvalidState)
    );
    assert_eq!(
        session.invalidate_epoch(connection(7, 21)),
        Err(Error::InvalidState)
    );
}

#[test]
fn mtu_and_all_capacity_failures_are_atomic() {
    let own = connection(8, 1);
    let first = frame(0x11);
    let second = frame(0x22);
    let mut storage = [0xee; 94];
    let original_storage = storage;
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0xa5; 160];
    let mut pdu = [0xa5; 164];

    assert_eq!(
        session.queue_sbc_frames(own, &[&first, &second], &mut rtp, &mut pdu),
        Err(Error::Packet(PacketError::MtuExceeded))
    );
    let mut bad = first;
    bad[0] = 0;
    assert_eq!(
        session.queue_sbc_frames(own, &[&bad], &mut rtp, &mut pdu),
        Err(Error::Packet(PacketError::InvalidFrame))
    );
    let mut short_rtp = [0xa5; 89];
    assert_eq!(
        session.queue_sbc_frames(own, &[&first], &mut short_rtp, &mut pdu),
        Err(Error::Packet(PacketError::BufferTooSmall))
    );
    let mut short_pdu = [0xa5; 93];
    assert_eq!(
        session.queue_sbc_frames(own, &[&first], &mut rtp, &mut short_pdu),
        Err(Error::L2cap(CoreError::BufferTooSmall))
    );
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (7, 1000)
    );
    assert_eq!(session.queued(), None);
    assert_eq!(session.pdu_storage(), original_storage);
    assert_eq!(short_rtp, [0xa5; 89]);
    assert_eq!(short_pdu, [0xa5; 93]);
}

#[test]
fn undersized_retained_pdu_storage_fails_before_rtp_commit() {
    let own = connection(9, 1);
    let encoded = frame(0x33);
    let mut storage = [0xee; 93];
    let original_storage = storage;
    let mut session = MediaSession::new(packetizer(90), own, 0x40, 48, &mut storage).unwrap();
    let mut rtp = [0xa5; 90];
    let mut pdu = [0xa5; 94];
    assert_eq!(
        session.queue_sbc_frames(own, &[&encoded], &mut rtp, &mut pdu),
        Err(Error::Fragment(FragmentError::PduTooLarge))
    );
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (7, 1000)
    );
    assert_eq!(rtp, [0xa5; 90]);
    assert_eq!(pdu, [0xa5; 94]);
    assert_eq!(session.pdu_storage(), original_storage);
}
