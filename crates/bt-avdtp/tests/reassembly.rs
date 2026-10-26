// SPDX-License-Identifier: GPL-3.0-or-later

use bt_avdtp::{
    Accepted, Error, Initiator, MessageType, Outcome, ParseError, ReassemblyError,
    ReassemblyLimits, ReceiveError, SignalId, SignalingContext, SignalingReassembler,
    StreamEndpointId, StreamState,
};

const CONTEXT: SignalingContext = SignalingContext {
    epoch: 7,
    channel_id: 0x0040,
};

const LIMITS: ReassemblyLimits = ReassemblyLimits {
    max_packets: 8,
    max_packet_bytes: 64,
    timeout_us: 100,
};

fn receiver(storage: &mut [u8]) -> SignalingReassembler<'_> {
    SignalingReassembler::new(CONTEXT, storage, LIMITS).unwrap()
}

#[test]
fn different_header_sizes_reconstruct_complete_capabilities() {
    let mut storage = [0u8; 10];
    let mut rx = receiver(&mut storage);
    // Each SDU includes its AVDTP header. Start and Continue are both six
    // bytes; their actual message payload contributions are three and five.
    assert_eq!(rx.push(CONTEXT, 1, &[0x36, 3, 2, 1, 0, 7]), Ok(None));
    assert_eq!(rx.push(CONTEXT, 2, &[0x3a, 6, 0, 0, 0x13, 0x15]), Ok(None));
    let message = rx.push(CONTEXT, 3, &[0x3e, 2, 53]).unwrap().unwrap();
    assert_eq!(message.transaction_label(), 3);
    assert_eq!(message.message_type(), MessageType::Accept);
    assert_eq!(message.signal_id(), Some(SignalId::GetCapabilities));
    assert_eq!(message.payload(), &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53]);
    assert_eq!(rx.partial(), None);
    assert_eq!(rx.push(CONTEXT, 4, &[0x3e]), Err(ReassemblyError::NoStart));
}

#[test]
fn two_packet_message_and_empty_fragment_payloads_are_bounded() {
    let mut storage = [0u8; 2];
    let mut rx = receiver(&mut storage);
    // The spec constrains SDU sizes, not a nonzero payload in every fragment.
    assert_eq!(rx.push(CONTEXT, 0, &[0x06, 2, 1]), Ok(None));
    assert_eq!(
        rx.push(CONTEXT, 1, &[0x0e, 4, 8])
            .unwrap()
            .unwrap()
            .payload(),
        &[4, 8]
    );
    assert_eq!(rx.push(CONTEXT, 2, &[0x16, 2, 6]), Ok(None));
    assert_eq!(
        rx.push(CONTEXT, 3, &[0x1e]).unwrap().unwrap().payload(),
        &[]
    );
}

#[test]
fn count_order_type_and_full_sdu_lengths_are_enforced_atomically() {
    let mut storage = [0u8; 32];
    let mut rx = receiver(&mut storage);
    assert_eq!(rx.push(CONTEXT, 10, &[0x06, 3, 2, 1, 0, 7]), Ok(None));
    let partial = rx.partial();
    for (packet, expected) in [
        (&[0x0e, 2][..], ReassemblyError::UnexpectedPacketCount),
        (
            &[0x1a, 6, 0, 0, 0x13, 0x15],
            ReassemblyError::TransactionMismatch,
        ),
        (
            &[0x0b, 6, 0, 0, 0x13, 0x15],
            ReassemblyError::MessageTypeMismatch,
        ),
        (&[0x0a, 6, 0, 0], ReassemblyError::InvalidPacketLength),
        (
            &[0x0a, 6, 0, 0, 0x13, 0x15, 9],
            ReassemblyError::InvalidPacketLength,
        ),
        (&[0x02, 2, 1, 0], ReassemblyError::InProgress),
        (&[0x06, 2, 2], ReassemblyError::InProgress),
        (&[0x01], ReassemblyError::InProgress),
    ] {
        assert_eq!(rx.push(CONTEXT, 11, packet), Err(expected));
        assert_eq!(rx.partial(), partial);
    }
    assert_eq!(rx.push(CONTEXT, 12, &[0x0a, 6, 0, 0, 0x13, 0x15]), Ok(None));
    let partial = rx.partial();
    assert_eq!(
        rx.push(CONTEXT, 13, &[0x0a, 2, 53, 0, 0, 0]),
        Err(ReassemblyError::UnexpectedPacketCount)
    );
    assert_eq!(
        rx.push(CONTEXT, 13, &[0x0e, 2, 53, 0, 0, 0, 0]),
        Err(ReassemblyError::InvalidPacketLength)
    );
    assert_eq!(rx.partial(), partial);
    assert_eq!(
        rx.push(CONTEXT, 14, &[0x0e, 2, 53])
            .unwrap()
            .unwrap()
            .payload(),
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53]
    );
}

#[test]
fn stale_epoch_or_another_channel_cannot_modify_partial_message() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    rx.push(CONTEXT, 10, &[0x06, 2, 1, 4]).unwrap();
    let partial = rx.partial();
    for context in [
        SignalingContext {
            epoch: 6,
            ..CONTEXT
        },
        SignalingContext {
            epoch: 8,
            ..CONTEXT
        },
        SignalingContext {
            channel_id: 0x41,
            ..CONTEXT
        },
    ] {
        assert_eq!(
            rx.push(context, 11, &[0x0e, 8]),
            Err(ReassemblyError::ContextMismatch)
        );
        assert_eq!(rx.cancel(context), Err(ReassemblyError::ContextMismatch));
        assert_eq!(
            rx.expire(context, 1000),
            Err(ReassemblyError::ContextMismatch)
        );
        assert_eq!(rx.partial(), partial);
    }
    assert_eq!(
        rx.push(CONTEXT, 12, &[0x0e, 8]).unwrap().unwrap().payload(),
        &[4, 8]
    );
}

#[test]
fn fixed_deadline_expires_once_and_requires_a_new_epoch() {
    let mut storage = [0u8; 16];
    let mut rx = receiver(&mut storage);
    rx.push(CONTEXT, 10, &[0x06, 3, 1, 4]).unwrap();
    rx.push(CONTEXT, 109, &[0x0a, 8, 4, 8]).unwrap();
    assert_eq!(rx.partial().unwrap().deadline_us, 110);
    assert_eq!(
        rx.push(CONTEXT, 110, &[0x0e]),
        Err(ReassemblyError::Timeout)
    );
    let cancelled = rx.expire(CONTEXT, 110).unwrap().unwrap();
    assert_eq!(
        (cancelled.received_packets, cancelled.payload_bytes),
        (2, 4)
    );
    assert!(rx.requires_reset());
    assert_eq!(rx.expire(CONTEXT, 111), Ok(None));
    assert_eq!(
        rx.push(CONTEXT, 112, &[0x02, 1, 4, 8]),
        Err(ReassemblyError::ResetRequired)
    );
    let next = SignalingContext {
        epoch: 8,
        channel_id: 0x41,
    };
    assert_eq!(rx.reset(next), Ok(None));
    assert!(!rx.requires_reset());
    assert_eq!(
        rx.push(CONTEXT, 112, &[0x0e]),
        Err(ReassemblyError::ContextMismatch)
    );
    assert_eq!(rx.push(next, 0, &[0x0e]), Err(ReassemblyError::NoStart));
    assert!(rx.push(next, 1, &[0x02, 1, 4, 8]).unwrap().is_some());
}

#[test]
fn cancel_reports_prefix_and_reset_rejects_non_newer_contexts() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    assert_eq!(rx.cancel(CONTEXT), Ok(None));
    rx.push(CONTEXT, 10, &[0x06, 2, 1, 4]).unwrap();
    let partial = rx.partial();
    assert_eq!(rx.reset(CONTEXT), Err(ReassemblyError::EpochNotAdvanced));
    assert_eq!(
        rx.reset(SignalingContext {
            epoch: 6,
            ..CONTEXT
        }),
        Err(ReassemblyError::EpochNotAdvanced)
    );
    assert_eq!(rx.partial(), partial);
    assert_eq!(rx.cancel(CONTEXT), Ok(partial));
    assert_eq!(rx.cancel(CONTEXT), Ok(None));
    assert_eq!(
        rx.push(CONTEXT, 11, &[0x0e, 8]),
        Err(ReassemblyError::ResetRequired)
    );
    let next = SignalingContext {
        epoch: 8,
        ..CONTEXT
    };
    assert_eq!(rx.reset(next), Ok(None));
    rx.push(next, 1, &[0x06, 2, 1, 4]).unwrap();
    let partial = rx.partial();
    assert_eq!(
        rx.reset(SignalingContext {
            epoch: 9,
            ..CONTEXT
        }),
        Ok(partial)
    );
}

#[test]
fn capacity_limits_fail_without_overwriting_storage() {
    let mut storage = [0xa5; 4];
    {
        let mut rx = receiver(&mut storage);
        assert_eq!(
            rx.push(CONTEXT, 0, &[0x06, 4, 2, 1, 0, 7]),
            Err(ReassemblyError::BufferTooSmall)
        );
        assert_eq!(
            rx.push(CONTEXT, 0, &[0x06, 9, 2]),
            Err(ReassemblyError::PacketLimit)
        );
        assert_eq!(
            rx.push(CONTEXT, 0, &[0x02, 2, 1, 2, 3, 4, 5]),
            Err(ReassemblyError::BufferTooSmall)
        );
        assert_eq!(rx.partial(), None);
    }
    assert_eq!(storage, [0xa5; 4]);
    {
        let mut rx = receiver(&mut storage);
        rx.push(CONTEXT, 0, &[0x06, 2, 2, 1, 2, 3]).unwrap();
        let partial = rx.partial();
        assert_eq!(
            rx.push(CONTEXT, 1, &[0x0e, 4, 5]),
            Err(ReassemblyError::BufferTooSmall)
        );
        assert_eq!(rx.partial(), partial);
        assert_eq!(
            rx.push(CONTEXT, 2, &[0x0e, 4]).unwrap().unwrap().payload(),
            &[1, 2, 3, 4]
        );
    }
    assert_eq!(storage, [1, 2, 3, 4]);
}

#[test]
fn header_shape_count_clock_and_packet_bounds_are_checked() {
    let mut storage = [0xa5; 32];
    let mut rx = receiver(&mut storage);
    for (packet, expected) in [
        (&[][..], ReassemblyError::Malformed(ParseError::Truncated)),
        (&[0x06], ReassemblyError::Malformed(ParseError::Truncated)),
        (
            &[0x06, 2],
            ReassemblyError::Malformed(ParseError::Truncated),
        ),
        (
            &[0x06, 0, 1],
            ReassemblyError::Malformed(ParseError::InvalidPacketCount),
        ),
        (
            &[0x06, 1, 1],
            ReassemblyError::Malformed(ParseError::InvalidPacketCount),
        ),
        (
            &[0x06, 2, 0],
            ReassemblyError::Malformed(ParseError::ReservedSignal),
        ),
        (
            &[0x06, 2, 0x41],
            ReassemblyError::Malformed(ParseError::InvalidHeader),
        ),
        (
            &[0x06, 2, 0x0e],
            ReassemblyError::Malformed(ParseError::ReservedSignal),
        ),
        (
            &[0x05, 2, 1],
            ReassemblyError::Malformed(ParseError::InvalidHeader),
        ),
        (
            &[0x01, 1],
            ReassemblyError::Malformed(ParseError::InvalidHeader),
        ),
        (&[0x0a], ReassemblyError::NoStart),
        (&[0x0e], ReassemblyError::NoStart),
    ] {
        assert_eq!(rx.push(CONTEXT, 0, packet), Err(expected));
        assert_eq!(rx.partial(), None);
    }
    assert_eq!(
        rx.push(CONTEXT, 0, &[0; 65]),
        Err(ReassemblyError::PacketTooLarge)
    );
    assert_eq!(
        rx.push(CONTEXT, u64::MAX, &[0x06, 2, 1]),
        Err(ReassemblyError::DeadlineOverflow)
    );
    rx.push(CONTEXT, 10, &[0x06, 2, 1, 4]).unwrap();
    let partial = rx.partial();
    assert_eq!(
        rx.push(CONTEXT, 9, &[0x0e, 8]),
        Err(ReassemblyError::ClockRegressed)
    );
    assert_eq!(rx.expire(CONTEXT, 9), Err(ReassemblyError::ClockRegressed));
    assert_eq!(rx.partial(), partial);
}

#[test]
fn invalid_constructor_limits_and_context_are_rejected() {
    for limits in [
        ReassemblyLimits {
            max_packets: 1,
            ..LIMITS
        },
        ReassemblyLimits {
            max_packet_bytes: 2,
            ..LIMITS
        },
        ReassemblyLimits {
            max_packet_bytes: 65536,
            ..LIMITS
        },
        ReassemblyLimits {
            timeout_us: 0,
            ..LIMITS
        },
    ] {
        assert!(matches!(
            SignalingReassembler::new(CONTEXT, &mut [], limits),
            Err(ReassemblyError::InvalidLimits)
        ));
    }
    assert!(matches!(
        SignalingReassembler::new(
            SignalingContext {
                channel_id: 0x3f,
                ..CONTEXT
            },
            &mut [],
            LIMITS
        ),
        Err(ReassemblyError::InvalidContext)
    ));
}

#[test]
fn maximum_wire_packet_count_does_not_wrap() {
    let mut storage = [0u8; 508];
    let mut rx = SignalingReassembler::new(
        CONTEXT,
        &mut storage,
        ReassemblyLimits {
            max_packets: 255,
            ..LIMITS
        },
    )
    .unwrap();
    rx.push(CONTEXT, 0, &[0x06, 255, 1]).unwrap();
    for _ in 0..253 {
        assert_eq!(rx.push(CONTEXT, 1, &[0x0a, 4, 8]), Ok(None));
    }
    assert_eq!(rx.partial().unwrap().received_packets, 254);
    let message = rx.push(CONTEXT, 2, &[0x0e, 4, 8]).unwrap().unwrap();
    assert_eq!(message.payload().len(), 508);
    assert!(message.payload().chunks_exact(2).all(|pair| pair == [4, 8]));
}

#[test]
fn initiator_validates_before_start_storage_and_accepts_discovery_directly() {
    let mut storage = [0xa5; 16];
    let mut session = Initiator::new(7);
    let mut command = [0u8; 16];
    session.discover(&mut command).unwrap();
    let pending = session.pending();
    {
        let mut rx = receiver(&mut storage);
        for (packet, expected) in [
            (&[0x16, 2, 1, 4][..], Error::StaleTransaction),
            (&[0x06, 2, 2, 4], Error::StaleTransaction),
            (&[0x04, 2, 1, 4], Error::Collision),
            (&[0x04, 2, 6, 4], Error::UnexpectedCommand),
        ] {
            assert_eq!(
                session.receive_packet(&mut rx, CONTEXT, 0, packet),
                Err(ReceiveError::Session(expected))
            );
            assert_eq!(rx.partial(), None);
            assert_eq!(session.pending(), pending);
        }
    }
    assert_eq!(storage, [0xa5; 16]);
    let mut rx = receiver(&mut storage);
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 1, &[0x06, 2, 1, 4]),
        Ok(None)
    );
    assert_eq!(session.pending(), pending);
    let outcome = session
        .receive_packet(&mut rx, CONTEXT, 2, &[0x0e, 8])
        .unwrap()
        .unwrap();
    let Outcome::Accepted(Accepted::Discovery(endpoints)) = outcome else {
        panic!("wrong outcome")
    };
    assert_eq!(
        endpoints.first_available_audio_sink().unwrap().id.value(),
        1
    );
    assert_eq!(session.pending(), None);
}

#[test]
fn fragmented_capabilities_share_semantic_parser_with_single_receive() {
    let mut storage = [0u8; 16];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    let remote = StreamEndpointId::new(1).unwrap();
    session.get_capabilities(remote, &mut [0; 16]).unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 1, &[0x06, 3, 2, 1, 0, 7]),
        Ok(None)
    );
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 2, &[0x0a, 6, 0, 0, 0x13, 0x15]),
        Ok(None)
    );
    let outcome = session
        .receive_packet(&mut rx, CONTEXT, 3, &[0x0e, 2, 53])
        .unwrap()
        .unwrap();
    let Outcome::Accepted(Accepted::Capabilities(caps)) = outcome else {
        panic!("wrong outcome")
    };
    let configuration = caps.conservative_sbc().unwrap().select(32).unwrap();
    assert_eq!(configuration.parameters().bitpool(), 32);
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(session.pending(), None);
}

#[test]
fn reassembled_setup_keeps_open_and_start_gated_on_complete_responses() {
    let mut storage = [0u8; 16];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    let remote = StreamEndpointId::new(1).unwrap();
    let local = StreamEndpointId::new(2).unwrap();
    let configuration =
        bt_avdtp::ServiceCapabilities::parse(&[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53])
            .unwrap()
            .conservative_sbc()
            .unwrap()
            .select(32)
            .unwrap();
    session
        .set_configuration(remote, local, configuration, &mut [0; 16])
        .unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 0, &[0x06, 2, 3]),
        Ok(None)
    );
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(
        session
            .receive_packet(&mut rx, CONTEXT, 1, &[0x0e])
            .unwrap(),
        Some(Outcome::Accepted(Accepted::Acknowledged(
            SignalId::SetConfiguration
        )))
    );
    assert_eq!(session.state(), StreamState::Configured);
    session.open(&mut [0; 8]).unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 2, &[0x16, 2, 6]),
        Ok(None)
    );
    assert_eq!(session.transport_opened(7), Err(Error::InvalidState));
    session
        .receive_packet(&mut rx, CONTEXT, 3, &[0x1e])
        .unwrap();
    assert_eq!(session.state(), StreamState::TransportOpening);
    assert_eq!(session.start(&mut [0; 8]), Err(Error::InvalidState));
    session.transport_opened(7).unwrap();
    session.start(&mut [0; 8]).unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 4, &[0x26, 2, 7]),
        Ok(None)
    );
    assert_eq!(session.state(), StreamState::Open);
    session
        .receive_packet(&mut rx, CONTEXT, 5, &[0x2e])
        .unwrap();
    assert_eq!(session.state(), StreamState::Streaming);
    session.suspend(&mut [0; 8]).unwrap();
    session
        .receive_packet(&mut rx, CONTEXT, 6, &[0x32, 9])
        .unwrap();
    assert_eq!(session.state(), StreamState::Open);
    session.close(&mut [0; 8]).unwrap();
    session
        .receive_packet(&mut rx, CONTEXT, 7, &[0x42, 8])
        .unwrap();
    assert_eq!(session.state(), StreamState::TransportClosing);
    session.transport_closed(7).unwrap();
    assert_eq!(session.state(), StreamState::Idle);
}

#[test]
fn a_fragmented_reject_preserves_peer_error_details() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    let remote = StreamEndpointId::new(1).unwrap();
    let local = StreamEndpointId::new(2).unwrap();
    let configuration =
        bt_avdtp::ServiceCapabilities::parse(&[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53])
            .unwrap()
            .conservative_sbc()
            .unwrap()
            .select(32)
            .unwrap();
    session
        .set_configuration(remote, local, configuration, &mut [0; 16])
        .unwrap();
    session
        .receive_packet(&mut rx, CONTEXT, 0, &[0x07, 2, 3, 7])
        .unwrap();
    let outcome = session
        .receive_packet(&mut rx, CONTEXT, 1, &[0x0f, 0x29])
        .unwrap()
        .unwrap();
    assert_eq!(
        outcome,
        Outcome::Rejected(bt_avdtp::RemoteReject {
            signal: 3,
            error_code: Some(0x29),
            detail: bt_avdtp::RejectDetail::ServiceCategory(7)
        })
    );
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(session.configuration(), None);
    assert_eq!(session.pending(), None);
}

#[test]
fn standalone_and_session_reassembly_cannot_mix_partial_ownership() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    session.discover(&mut [0; 2]).unwrap();
    rx.push(CONTEXT, 0, &[0x06, 2, 1, 4]).unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 1, &[0x0e, 8]),
        Err(ReceiveError::Reassembly(
            ReassemblyError::TransactionMismatch
        ))
    );
    assert!(rx.partial().is_some());
    assert!(session.pending().is_some());
}

#[test]
fn malformed_reassembled_payload_does_not_complete_pending_transaction() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    session.discover(&mut [0; 8]).unwrap();
    let pending = session.pending();
    session
        .receive_packet(&mut rx, CONTEXT, 1, &[0x06, 2, 1, 4])
        .unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 2, &[0x0e]),
        Err(ReceiveError::Session(Error::Malformed(
            ParseError::InvalidEndpointList
        )))
    );
    assert_eq!(session.pending(), pending);
    assert_eq!(rx.partial(), None);
    assert_eq!(
        session
            .receive_packet(&mut rx, CONTEXT, 3, &[0x03, 1, 0x29])
            .unwrap(),
        Some(Outcome::Rejected(bt_avdtp::RemoteReject {
            signal: 1,
            error_code: Some(0x29),
            detail: bt_avdtp::RejectDetail::None
        }))
    );
}

#[test]
fn general_reject_single_needs_no_signal_or_payload_storage() {
    let mut rx = receiver(&mut []);
    let mut session = Initiator::new(7);
    session.discover(&mut [0; 2]).unwrap();
    assert!(
        matches!(session.receive_packet(&mut rx, CONTEXT, 1, &[0x01]), Ok(Some(Outcome::Rejected(reject))) if reject.signal == 1 && reject.error_code.is_none())
    );
}

#[test]
fn timeout_then_new_epoch_rejects_late_fragments() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    session.discover(&mut [0; 2]).unwrap();
    session
        .receive_packet(&mut rx, CONTEXT, 0, &[0x06, 2, 1, 4])
        .unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 100, &[0x0e, 8]),
        Err(ReceiveError::Reassembly(ReassemblyError::Timeout))
    );
    assert!(session.pending().is_some());
    rx.expire(CONTEXT, 100).unwrap().unwrap();
    session.timeout(7, 0, 8).unwrap();
    let next = SignalingContext {
        epoch: 8,
        ..CONTEXT
    };
    rx.reset(next).unwrap();
    session.discover(&mut [0; 2]).unwrap();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 101, &[0x0e, 8]),
        Err(ReceiveError::Session(Error::InvalidEpoch))
    );
    assert_eq!(
        session.receive_packet(&mut rx, next, 101, &[0x0e, 8]),
        Err(ReceiveError::Reassembly(ReassemblyError::NoStart))
    );
    assert!(session.pending().is_some());
}

#[test]
fn legacy_completion_cannot_reuse_old_partial_after_label_wrap() {
    let mut storage = [0u8; 8];
    let mut rx = receiver(&mut storage);
    let mut session = Initiator::new(7);
    session.discover(&mut [0; 2]).unwrap();
    session
        .receive_packet(&mut rx, CONTEXT, 0, &[0x06, 2, 1, 4])
        .unwrap();
    session.receive(7, &[0x02, 1, 4, 8]).unwrap();
    for _ in 0..15 {
        let command = session.discover(&mut [0; 2]).unwrap();
        session
            .receive(7, &[(command.transaction_label << 4) | 2, 1, 4, 8])
            .unwrap();
    }
    let command = session.discover(&mut [0; 2]).unwrap();
    assert_eq!(command.transaction_label, 0);
    let partial = rx.partial();
    assert_eq!(
        session.receive_packet(&mut rx, CONTEXT, 1, &[0x0e, 8]),
        Err(ReceiveError::Reassembly(
            ReassemblyError::TransactionMismatch
        ))
    );
    assert_eq!(rx.partial(), partial);
    assert_eq!(session.pending().unwrap().transaction_label, 0);
}
