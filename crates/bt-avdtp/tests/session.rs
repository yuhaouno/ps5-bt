// SPDX-License-Identifier: GPL-3.0-or-later

use bt_avdtp::{
    Accepted, CapabilityError, Error, Header, Initiator, Message, MessageType, Outcome, PacketType,
    RejectDetail, SignalId, StreamEndpointId, StreamState,
};

const REMOTE: StreamEndpointId = endpoint(1);
const LOCAL: StreamEndpointId = endpoint(2);

const fn endpoint(value: u8) -> StreamEndpointId {
    match StreamEndpointId::new(value) {
        Ok(endpoint) => endpoint,
        Err(_) => panic!("test endpoint must be valid"),
    }
}

fn accept(label: u8, signal: SignalId, payload: &[u8], output: &mut [u8]) -> usize {
    output[0] = (label << 4) | 0x02;
    output[1] = signal.raw();
    output[2..payload.len() + 2].copy_from_slice(payload);
    payload.len() + 2
}

fn reject(label: u8, signal: SignalId, payload: &[u8], output: &mut [u8]) -> usize {
    output[0] = (label << 4) | 0x03;
    output[1] = signal.raw();
    output[2..payload.len() + 2].copy_from_slice(payload);
    payload.len() + 2
}

fn capabilities(codec: u8) -> [u8; 10] {
    [
        0x01, 0x00, // Media Transport.
        0x07, 0x06, 0x00, codec, 0x13, 0x15, 2, 53,
    ]
}

fn configuration() -> bt_avdtp::SbcConfiguration {
    bt_avdtp::ServiceCapabilities::parse(&capabilities(0))
        .unwrap()
        .conservative_sbc()
        .unwrap()
        .select(32)
        .unwrap()
}

#[test]
fn scripted_sink_reaches_streaming_and_stops_boundedly() {
    let mut session = Initiator::new(7);
    let mut command = [0xa5; 32];
    let mut response = [0u8; 32];

    let discover = session.discover(&mut command).unwrap();
    assert_eq!(discover.bytes, 2);
    assert_eq!(&command[..2], &[0x00, 0x01]);
    let length = accept(0, SignalId::Discover, &[REMOTE.wire(), 0x08], &mut response);
    let endpoints = match session.receive(7, &response[..length]).unwrap() {
        Outcome::Accepted(Accepted::Discovery(endpoints)) => endpoints,
        other => panic!("unexpected discovery outcome: {other:?}"),
    };
    assert_eq!(endpoints.first_available_audio_sink().unwrap().id, REMOTE);

    let get = session.get_capabilities(REMOTE, &mut command).unwrap();
    assert_eq!((get.transaction_label, get.bytes), (1, 3));
    assert_eq!(&command[..3], &[0x10, 0x02, REMOTE.wire()]);
    let length = accept(
        1,
        SignalId::GetCapabilities,
        &capabilities(0),
        &mut response,
    );
    let caps = match session.receive(7, &response[..length]).unwrap() {
        Outcome::Accepted(Accepted::Capabilities(caps)) => caps,
        other => panic!("unexpected capability outcome: {other:?}"),
    };
    let sbc = caps.conservative_sbc().unwrap();
    assert!(sbc.supports_stereo());
    assert!(sbc.supports_joint_stereo());
    assert_eq!(
        (sbc.peer_bitpool().min(), sbc.peer_bitpool().max()),
        (2, 53)
    );
    let selected = sbc.select(32).unwrap();

    let set = session
        .set_configuration(REMOTE, LOCAL, selected, &mut command)
        .unwrap();
    assert_eq!((set.transaction_label, set.bytes), (2, 14));
    assert_eq!(
        &command[..14],
        &[
            0x20,
            0x03,
            REMOTE.wire(),
            LOCAL.wire(),
            0x01,
            0x00,
            0x07,
            0x06,
            0x00,
            0x00,
            0x11,
            0x15,
            32,
            32,
        ]
    );
    let length = accept(2, SignalId::SetConfiguration, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    assert_eq!(session.state(), StreamState::Configured);
    assert_eq!(session.configuration(), Some(selected));

    let open = session.open(&mut command).unwrap();
    assert_eq!(&command[..open.bytes], &[0x30, 0x06, REMOTE.wire()]);
    let length = accept(3, SignalId::Open, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    assert_eq!(session.state(), StreamState::TransportOpening);
    assert_eq!(session.start(&mut command), Err(Error::InvalidState));
    session.transport_opened(7).unwrap();

    let start = session.start(&mut command).unwrap();
    assert_eq!(&command[..start.bytes], &[0x40, 0x07, REMOTE.wire()]);
    let length = accept(4, SignalId::Start, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    assert_eq!(session.state(), StreamState::Streaming);

    let suspend = session.suspend(&mut command).unwrap();
    assert_eq!(&command[..suspend.bytes], &[0x50, 0x09, REMOTE.wire()]);
    let length = accept(5, SignalId::Suspend, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    assert_eq!(session.state(), StreamState::Open);

    session.start(&mut command).unwrap();
    let length = accept(6, SignalId::Start, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    let close = session.close(&mut command).unwrap();
    assert_eq!(session.state(), StreamState::TransportClosing);
    assert_eq!(&command[..close.bytes], &[0x70, 0x08, REMOTE.wire()]);
    let length = accept(7, SignalId::Close, &[], &mut response);
    session.receive(7, &response[..length]).unwrap();
    session.transport_closed(7).unwrap();
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(session.configuration(), None);
}

#[test]
fn malformed_and_truncated_responses_are_atomic() {
    let mut session = Initiator::new(1);
    let mut output = [0xa5; 8];
    session.discover(&mut output).unwrap();
    let pending = session.pending();
    assert_eq!(
        session.receive(1, &[]),
        Err(Error::Malformed(bt_avdtp::ParseError::Truncated))
    );
    assert_eq!(session.pending(), pending);

    // Accepted Discover must contain a non-empty, even-sized endpoint list.
    assert_eq!(
        session.receive(1, &[0x02, 0x01, REMOTE.wire()]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidEndpointList))
    );
    assert_eq!(session.pending(), pending);

    // A fragmented response is explicit unsupported input, never a partial transaction.
    assert_eq!(
        session.receive(1, &[0x06, 0x02, 0x01]),
        Err(Error::FragmentationUnsupported)
    );
    assert_eq!(session.pending(), pending);
}

#[test]
fn local_send_failures_preserve_output_label_and_state() {
    let mut session = Initiator::new(3);
    let mut short = [0xa5; 1];
    assert_eq!(session.discover(&mut short), Err(Error::BufferTooSmall));
    assert_eq!(short, [0xa5]);
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(session.pending(), None);

    let mut output = [0xa5; 4];
    let command = session.discover(&mut output).unwrap();
    assert_eq!(command.transaction_label, 0);
    let before = output;
    assert_eq!(session.discover(&mut output), Err(Error::Busy));
    assert_eq!(output, before);
}

#[test]
fn reject_preserves_wire_error_and_rolls_back_transition() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 20];
    let mut response = [0u8; 8];
    session
        .set_configuration(REMOTE, LOCAL, configuration(), &mut output)
        .unwrap();
    let length = reject(0, SignalId::SetConfiguration, &[0x07, 0x29], &mut response);
    assert_eq!(
        session.receive(1, &response[..length]).unwrap(),
        Outcome::Rejected(bt_avdtp::RemoteReject {
            signal: 0x03,
            error_code: Some(0x29),
            detail: RejectDetail::ServiceCategory(0x07),
        })
    );
    assert_eq!(session.state(), StreamState::Idle);
    assert_eq!(session.pending(), None);

    session
        .set_configuration(REMOTE, LOCAL, configuration(), &mut output)
        .unwrap();
    let length = accept(1, SignalId::SetConfiguration, &[], &mut response);
    session.receive(1, &response[..length]).unwrap();
    session.open(&mut output).unwrap();
    let length = accept(2, SignalId::Open, &[], &mut response);
    session.receive(1, &response[..length]).unwrap();
    session.transport_opened(1).unwrap();

    session.start(&mut output).unwrap();
    let pending = session.pending();
    let length = reject(3, SignalId::Start, &[LOCAL.wire(), 0x31], &mut response);
    assert_eq!(
        session.receive(1, &response[..length]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidEndpoint))
    );
    assert_eq!(session.pending(), pending);
    let length = reject(
        3,
        SignalId::Start,
        &[REMOTE.wire() | 1, 0x31],
        &mut response,
    );
    assert_eq!(
        session.receive(1, &response[..length]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidEndpoint))
    );
    assert_eq!(session.pending(), pending);
    let length = reject(3, SignalId::Start, &[REMOTE.wire(), 0x31], &mut response);
    assert!(matches!(
        session.receive(1, &response[..length]),
        Ok(Outcome::Rejected(_))
    ));
    assert_eq!(session.state(), StreamState::Open);

    session.close(&mut output).unwrap();
    assert_eq!(session.state(), StreamState::TransportClosing);
    let length = reject(4, SignalId::Close, &[0x31], &mut response);
    let outcome = session.receive(1, &response[..length]).unwrap();
    assert_eq!(
        outcome,
        Outcome::Rejected(bt_avdtp::RemoteReject {
            signal: 0x08,
            error_code: Some(0x31),
            detail: RejectDetail::None,
        })
    );
    assert_eq!(session.state(), StreamState::Open);
}

#[test]
fn collisions_and_stale_responses_do_not_consume_transaction() {
    let mut session = Initiator::new(4);
    let mut output = [0u8; 8];
    session.discover(&mut output).unwrap();
    let pending = session.pending();
    assert_eq!(session.receive(4, &[0x00, 0x01]), Err(Error::Collision));
    assert_eq!(session.receive(4, &[0x50, 0x01]), Err(Error::Collision));
    assert_eq!(session.pending(), pending);
    assert_eq!(
        session.receive(4, &[0x12, 0x01, REMOTE.wire(), 0x08]),
        Err(Error::StaleTransaction)
    );
    assert_eq!(
        session.receive(4, &[0x02, 0x02]),
        Err(Error::StaleTransaction)
    );
    assert_eq!(session.pending(), pending);
    session
        .receive(4, &[0x02, 0x01, REMOTE.wire(), 0x08])
        .unwrap();
    assert_eq!(
        session.receive(4, &[0x02, 0x01, REMOTE.wire(), 0x08]),
        Err(Error::NoPendingTransaction)
    );
}

#[test]
fn general_reject_is_a_completed_preserved_outcome() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 4];
    session.discover(&mut output).unwrap();
    assert_eq!(
        session.receive(1, &[0x01]).unwrap(),
        Outcome::Rejected(bt_avdtp::RemoteReject {
            signal: 0x01,
            error_code: None,
            detail: RejectDetail::None,
        })
    );
    assert_eq!(session.pending(), None);
}

#[test]
fn reserved_bits_and_general_reject_shape_are_strict() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 16];
    session.discover(&mut output).unwrap();
    let pending = session.pending();

    // General Reject has no Signal Identifier or payload.
    assert_eq!(
        session.receive(1, &[0x01, 0x01]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidHeader))
    );
    assert_eq!(session.pending(), pending);
    // Signal Identifier octet reserves its upper two bits.
    assert_eq!(
        session.receive(1, &[0x02, 0x41, REMOTE.wire(), 0x08]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidHeader))
    );
    assert_eq!(session.pending(), pending);
    // Discover SEID bit 0 and endpoint-description bits 2..0 are RFA.
    assert_eq!(
        session.receive(1, &[0x02, 0x01, REMOTE.wire() | 1, 0x08]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidEndpoint))
    );
    assert_eq!(
        session.receive(1, &[0x02, 0x01, REMOTE.wire(), 0x09]),
        Err(Error::Malformed(bt_avdtp::ParseError::InvalidEndpointList))
    );
    assert_eq!(session.pending(), pending);
}

#[test]
fn malformed_capabilities_do_not_consume_transaction() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 8];
    session.get_capabilities(REMOTE, &mut output).unwrap();
    let pending = session.pending();
    assert_eq!(
        session.receive(1, &[0x02, 0x02, 0x01]),
        Err(Error::Capability(CapabilityError::Truncated))
    );
    assert_eq!(session.pending(), pending);
}

#[test]
fn transaction_labels_wrap_only_after_completed_commands() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 4];
    let mut response = [0u8; 4];
    for index in 0..17 {
        let command = session.discover(&mut output).unwrap();
        assert_eq!(command.transaction_label, index & 0x0f);
        let length = accept(
            command.transaction_label,
            SignalId::Discover,
            &[REMOTE.wire(), 0x08],
            &mut response,
        );
        session.receive(1, &response[..length]).unwrap();
    }
}

#[test]
fn timeout_and_disconnect_require_a_new_epoch() {
    let mut session = Initiator::new(10);
    let mut output = [0u8; 8];
    session.discover(&mut output).unwrap();
    let pending = session.pending().unwrap();
    assert_eq!(session.timeout(10, 1, 11), Err(Error::StaleTransaction));
    assert_eq!(session.pending(), Some(pending));
    assert_eq!(session.timeout(10, 0, 10), Err(Error::EpochNotAdvanced));
    assert_eq!(session.pending(), Some(pending));
    assert_eq!(session.timeout(10, 0, 11).unwrap(), pending);
    assert_eq!((session.epoch(), session.state()), (11, StreamState::Idle));
    assert_eq!(session.receive(10, &[0x02, 0x01]), Err(Error::InvalidEpoch));

    let command = session.discover(&mut output).unwrap();
    assert_eq!(command.transaction_label, 0);
    session.disconnected(11, 12).unwrap();
    assert_eq!((session.epoch(), session.pending()), (12, None));
    assert_eq!(session.receive(11, &[0x02, 0x01]), Err(Error::InvalidEpoch));
}

#[test]
fn unsupported_codec_is_reported_without_guessing() {
    let mut session = Initiator::new(1);
    let mut output = [0u8; 16];
    let mut response = [0u8; 16];
    session.get_capabilities(REMOTE, &mut output).unwrap();
    let length = accept(
        0,
        SignalId::GetCapabilities,
        &capabilities(2),
        &mut response,
    );
    let caps = match session.receive(1, &response[..length]).unwrap() {
        Outcome::Accepted(Accepted::Capabilities(caps)) => caps,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(
        caps.conservative_sbc(),
        Err(CapabilityError::UnsupportedCodec(2))
    );
}

#[test]
fn media_codec_reserved_bits_are_rejected() {
    let mut reserved = capabilities(0);
    reserved[4] = 0x01;
    let capabilities = bt_avdtp::ServiceCapabilities::parse(&reserved).unwrap();
    assert_eq!(
        capabilities.conservative_sbc(),
        Err(CapabilityError::InvalidMediaCodec)
    );
}

#[test]
fn invalid_transitions_and_abort_are_bounded() {
    let mut session = Initiator::new(1);
    let mut output = [0xa5; 20];
    assert_eq!(session.open(&mut output), Err(Error::InvalidState));
    assert_eq!(session.start(&mut output), Err(Error::InvalidState));
    assert_eq!(session.suspend(&mut output), Err(Error::InvalidState));
    assert_eq!(session.close(&mut output), Err(Error::InvalidState));
    assert_eq!(session.abort(&mut output), Err(Error::InvalidState));
    assert_eq!(output, [0xa5; 20]);

    session
        .set_configuration(REMOTE, LOCAL, configuration(), &mut output)
        .unwrap();
    let mut response = [0u8; 8];
    let length = accept(0, SignalId::SetConfiguration, &[], &mut response);
    session.receive(1, &response[..length]).unwrap();
    let abort = session.abort(&mut output).unwrap();
    assert_eq!(&output[..abort.bytes], &[0x10, 0x0a, REMOTE.wire()]);
    assert_eq!(session.state(), StreamState::Aborting);
    let length = accept(1, SignalId::Abort, &[], &mut response);
    session.receive(1, &response[..length]).unwrap();
    session.transport_closed(1).unwrap();
    assert_eq!(session.state(), StreamState::Idle);
}

#[test]
fn typed_headers_cover_all_packet_types() {
    let mut bytes = [0xa5; 4];
    let single = Header::single(0x0f, MessageType::Accept, SignalId::Open).unwrap();
    assert_eq!(single.encode(&mut bytes).unwrap(), 2);
    assert_eq!(&bytes[..2], &[0xf2, 0x06]);
    let parsed = Message::parse(&bytes[..2]).unwrap().header();
    assert_eq!(parsed, single);

    let general = Header::general_reject(4).unwrap();
    assert_eq!(general.encode(&mut bytes).unwrap(), 1);
    assert_eq!(bytes[0], 0x41);
    assert_eq!(Message::parse(&bytes[..1]).unwrap().header(), general);
    assert_eq!(
        Header::single(4, MessageType::GeneralReject, SignalId::Open),
        Err(bt_avdtp::ParseError::InvalidHeader)
    );

    let start = Header::start(2, MessageType::Reject, SignalId::Discover, 3).unwrap();
    assert_eq!(start.encode(&mut bytes).unwrap(), 3);
    assert_eq!(&bytes[..3], &[0x27, 0x03, 0x01]);
    assert_eq!(Message::parse(&bytes[..3]).unwrap().header(), start);
    assert_eq!(
        Header::start(2, MessageType::Reject, SignalId::Discover, 1),
        Err(bt_avdtp::ParseError::InvalidHeader)
    );

    for (packet_type, expected) in [(PacketType::Continue, 0x38), (PacketType::End, 0x3c)] {
        let header = Header::continuation(3, packet_type, MessageType::Command).unwrap();
        assert_eq!(header.encode(&mut bytes).unwrap(), 1);
        assert_eq!(bytes[0], expected);
        assert_eq!(Message::parse(&bytes[..1]).unwrap().header(), header);
    }
}
