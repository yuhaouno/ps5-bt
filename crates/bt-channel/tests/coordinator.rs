// SPDX-License-Identifier: GPL-3.0-or-later
use bt_channel::{
    ChannelConfig, ChannelCoordinator, ChannelRole, CoordinatorPhase, Error, Notice, PreparedSdu,
    SduKind,
};
use bt_hci::ConnectionHandle;
use bt_l2cap::{
    BasicFrame, ConfigurationOptions, ConfigurationResult, Connection, ConnectionResult,
    ConnectionStatus, Signal, SignalCommand,
};
use bt_sdp::DiscoveryLimits;

fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(0x42).unwrap(),
        epoch: 7,
    }
}
fn config() -> ChannelConfig {
    ChannelConfig {
        local_cids: [0x40, 0x41, 0x42],
        local_mtu: 128,
        channel_timeout_us: 1000,
        discovery_timeout_us: 2000,
        cleanup_timeout_us: 100,
        discovery_limits: DiscoveryLimits {
            max_chunk_bytes: 64,
            max_total_bytes: 256,
            max_nodes: 64,
            max_depth: 8,
            max_chunks: 8,
        },
    }
}
fn coordinator<'a>(tx: &'a mut [u8], sdp: &'a mut [u8]) -> ChannelCoordinator<'a> {
    ChannelCoordinator::new(connection(), 0, config(), tx, sdp).unwrap()
}
fn basic(cid: u16, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; data.len() + 4];
    BasicFrame::new(cid, data)
        .unwrap()
        .encode(&mut bytes)
        .unwrap();
    bytes
}
fn signal(identifier: u8, signal: Signal) -> Vec<u8> {
    let mut output = [0; 32];
    let len = SignalCommand { identifier, signal }
        .encode(&mut output)
        .unwrap();
    basic(1, &output[..len])
}
fn prepare(c: &mut ChannelCoordinator<'_>) -> (PreparedSdu, Vec<u8>) {
    let mut out = [0; 256];
    let p = c.prepare_sdu(10, &mut out).unwrap().unwrap();
    let mut retry = [0; 256];
    assert_eq!(c.prepare_sdu(10, &mut retry).unwrap(), Some(p));
    assert_eq!(&out[..p.len], &retry[..p.len]);
    (p, out[..p.len].to_vec())
}
fn accept(c: &mut ChannelCoordinator<'_>) -> (PreparedSdu, Vec<u8>) {
    let (p, data) = prepare(c);
    c.confirm_accepted(p.ticket, 10).unwrap();
    assert_eq!(c.confirm_accepted(p.ticket, 10), Err(Error::WrongTicket));
    (p, data)
}
fn receive(c: &mut ChannelCoordinator<'_>, bytes: &[u8]) -> Notice<'static> {
    match c.receive(connection(), 10, bytes, &mut [0; 256]).unwrap() {
        Notice::None => Notice::None,
        Notice::Opened { role, binding } => Notice::Opened { role, binding },
        Notice::Closed { role } => Notice::Closed { role },
        Notice::ServiceDiscovered(service) => Notice::ServiceDiscovered(service),
        Notice::Data { .. } => panic!("expected protocol progress"),
    }
}
fn complete_channel(
    c: &mut ChannelCoordinator<'_>,
    role: ChannelRole,
    remote: u16,
    mtu: u16,
) -> Notice<'static> {
    let (p, bytes) = accept(c);
    assert_eq!(p.kind, SduKind::ChannelSignal(role));
    let request = SignalCommand::parse(BasicFrame::parse(&bytes).unwrap().payload()).unwrap();
    let Signal::ConnectionRequest {
        source_channel: local,
        psm,
    } = request.signal
    else {
        panic!()
    };
    assert_eq!(psm, if role == ChannelRole::Sdp { 1 } else { 0x19 });
    receive(
        c,
        &signal(
            request.identifier,
            Signal::ConnectionResponse {
                destination_channel: remote,
                source_channel: local,
                result: ConnectionResult::Success,
                status: ConnectionStatus::NoFurtherInformation,
            },
        ),
    );
    let (_, bytes) = accept(c);
    let config = SignalCommand::parse(BasicFrame::parse(&bytes).unwrap().payload()).unwrap();
    assert!(
        matches!(config.signal, Signal::ConfigurationRequest { destination_channel, .. } if destination_channel == remote)
    );
    receive(
        c,
        &signal(
            0xa0,
            Signal::ConfigurationRequest {
                destination_channel: local,
                options: ConfigurationOptions { mtu: Some(mtu) },
            },
        ),
    );
    assert!(c.binding(role).is_none());
    accept(c); // reciprocal Configuration Response must be accepted as a whole.
    receive(
        c,
        &signal(
            config.identifier,
            Signal::ConfigurationResponse {
                source_channel: remote,
                result: ConfigurationResult::Success,
                options: ConfigurationOptions::default(),
            },
        ),
    )
}
fn attributes() -> Vec<u8> {
    vec![
        0x35, 0x32, 0x35, 0x30, 0x09, 0, 1, 0x35, 3, 0x19, 0x11, 0x0b, 0x09, 0, 4, 0x35, 0x10,
        0x35, 6, 0x19, 1, 0, 0x09, 0, 0x19, 0x35, 6, 0x19, 0, 0x19, 0x09, 1, 3, 0x09, 0, 9, 0x35,
        8, 0x35, 6, 0x19, 0x11, 0x0d, 0x09, 1, 3, 0x09, 3, 0x11, 0x09, 0, 1,
    ]
}
fn sdp(transaction: u16, data: &[u8], token: &[u8]) -> Vec<u8> {
    let mut bytes = vec![7];
    bytes.extend(transaction.to_be_bytes());
    bytes.extend(((data.len() + 3 + token.len()) as u16).to_be_bytes());
    bytes.extend((data.len() as u16).to_be_bytes());
    bytes.extend(data);
    bytes.push(token.len() as u8);
    bytes.extend(token);
    basic(0x40, &bytes)
}
fn finish_disconnect(c: &mut ChannelCoordinator<'_>) -> Notice<'static> {
    let (_, bytes) = accept(c);
    let command = SignalCommand::parse(BasicFrame::parse(&bytes).unwrap().payload()).unwrap();
    let Signal::DisconnectionRequest {
        destination_channel,
        source_channel,
    } = command.signal
    else {
        panic!()
    };
    receive(
        c,
        &signal(
            command.identifier,
            Signal::DisconnectionResponse {
                destination_channel,
                source_channel,
            },
        ),
    )
}
fn discover(c: &mut ChannelCoordinator<'_>) {
    c.begin_discovery(10).unwrap();
    complete_channel(c, ChannelRole::Sdp, 0x80, 80);
    let (p, request) = accept(c);
    assert_eq!(p.kind, SduKind::SdpRequest);
    assert_eq!(p.destination_cid, 0x80);
    let req = BasicFrame::parse(&request).unwrap();
    let transaction = u16::from_be_bytes([req.payload()[1], req.payload()[2]]);
    assert!(matches!(
        receive(c, &sdp(transaction, &attributes(), &[])),
        Notice::ServiceDiscovered(_)
    ));
    assert_eq!(c.phase(), CoordinatorPhase::ClosingDiscovery);
    assert!(!c.sdp_closed());
    finish_disconnect(c);
    assert!(c.sdp_closed());
    assert_eq!(c.phase(), CoordinatorPhase::Discovered);
}

#[test]
fn full_discovery_signaling_media_close_and_fresh_media_reopen() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    assert!(!c.media_closed());
    discover(&mut c);
    c.open_signaling(10).unwrap();
    assert!(matches!(
        complete_channel(&mut c, ChannelRole::Signaling, 0x81, 90),
        Notice::Opened {
            role: ChannelRole::Signaling,
            ..
        }
    ));
    c.open_media(10).unwrap();
    let Notice::Opened {
        role: ChannelRole::Media,
        binding,
    } = complete_channel(&mut c, ChannelRole::Media, 0x82, 65)
    else {
        panic!()
    };
    assert_eq!(
        (
            binding.local_cid,
            binding.remote_cid,
            binding.local_mtu,
            binding.peer_mtu
        ),
        (0x42, 0x82, 128, 65)
    );
    c.queue_data(ChannelRole::Signaling, 10, &[0, 1]).unwrap();
    let (p, data) = accept(&mut c);
    assert_eq!(p.kind, SduKind::Application(ChannelRole::Signaling));
    assert_eq!(BasicFrame::parse(&data).unwrap().payload(), &[0, 1]);
    assert_eq!(
        c.queue_data(ChannelRole::Media, 10, &[0; 66]),
        Err(Error::MtuExceeded)
    );
    let packet = basic(0x41, &[2, 1, 4, 8]);
    assert!(matches!(
        c.receive(connection(), 10, &packet, &mut []),
        Ok(Notice::Data {
            role: ChannelRole::Signaling,
            payload: [2, 1, 4, 8],
            ..
        })
    ));
    c.close_media(10).unwrap();
    assert!(!c.media_closed());
    assert!(c.binding(ChannelRole::Media).is_none());
    assert_eq!(
        finish_disconnect(&mut c),
        Notice::Closed {
            role: ChannelRole::Media
        }
    );
    assert!(c.media_closed());
    c.open_media(10).unwrap();
    let Notice::Opened { binding: fresh, .. } =
        complete_channel(&mut c, ChannelRole::Media, 0x82, 65)
    else {
        panic!()
    };
    assert_eq!(fresh.local_cid, 0x43);
    assert_eq!(
        c.receive(connection(), 10, &basic(0x42, &[1]), &mut []),
        Err(Error::ForeignChannel)
    );
    c.cancel(10).unwrap();
    assert!(c.needs_cleanup());
    finish_disconnect(&mut c); // Media before signaling.
    finish_disconnect(&mut c);
    assert!(c.is_closed());
    assert!(!c.needs_cleanup());
}

#[test]
fn responses_cannot_precede_whole_sdu_acceptance_or_cross_contexts() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    let (prepared, bytes) = prepare(&mut c);
    let mut short = [0xa5; 3];
    assert_eq!(c.prepare_sdu(10, &mut short), Err(Error::OutputTooSmall));
    assert_eq!(short, [0xa5; 3]);
    let response = signal(
        1,
        Signal::ConnectionResponse {
            destination_channel: 0x80,
            source_channel: 0x40,
            result: ConnectionResult::Success,
            status: ConnectionStatus::NoFurtherInformation,
        },
    );
    assert_eq!(
        c.receive(connection(), 10, &response, &mut []),
        Err(Error::ResponseBeforeAcceptance)
    );
    assert_eq!(prepare(&mut c), (prepared, bytes));
    let mut foreign = connection();
    foreign.epoch += 1;
    assert_eq!(
        c.receive(foreign, 10, &response, &mut []),
        Err(Error::ForeignConnection)
    );
    foreign = connection();
    foreign.handle = ConnectionHandle::new(0x43).unwrap();
    assert_eq!(
        c.receive(foreign, 10, &response, &mut []),
        Err(Error::ForeignConnection)
    );
    c.confirm_accepted(prepared.ticket, 10).unwrap();
    assert_eq!(
        c.receive(
            connection(),
            10,
            &signal(
                2,
                Signal::ConnectionResponse {
                    destination_channel: 0x80,
                    source_channel: 0x40,
                    result: ConnectionResult::Success,
                    status: ConnectionStatus::NoFurtherInformation
                }
            ),
            &mut []
        ),
        Err(Error::UnexpectedResponse)
    );
    receive(&mut c, &response);
    assert!(c.binding(ChannelRole::Sdp).is_none());
}

#[test]
fn opening_is_not_published_until_reciprocal_response_accepted() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c);
    c.open_signaling(10).unwrap();
    accept(&mut c);
    receive(
        &mut c,
        &signal(
            5,
            Signal::ConnectionResponse {
                destination_channel: 0x81,
                source_channel: 0x41,
                result: ConnectionResult::Success,
                status: ConnectionStatus::NoFurtherInformation,
            },
        ),
    );
    accept(&mut c);
    receive(
        &mut c,
        &signal(
            6,
            Signal::ConfigurationResponse {
                source_channel: 0x81,
                result: ConfigurationResult::Success,
                options: ConfigurationOptions::default(),
            },
        ),
    );
    receive(
        &mut c,
        &signal(
            44,
            Signal::ConfigurationRequest {
                destination_channel: 0x41,
                options: ConfigurationOptions { mtu: Some(80) },
            },
        ),
    );
    assert!(c.binding(ChannelRole::Signaling).is_none());
    assert_eq!(
        c.receive(connection(), 10, &basic(0x41, &[1]), &mut []),
        Err(Error::InvalidState)
    );
    let (p, _) = prepare(&mut c);
    assert!(matches!(
        c.confirm_accepted(p.ticket, 10),
        Ok(Notice::Opened {
            role: ChannelRole::Signaling,
            ..
        })
    ));
    assert_eq!(c.binding(ChannelRole::Signaling).unwrap().peer_mtu, 80);
}

#[test]
fn peer_request_busy_preserves_offer_and_can_be_retried_in_order() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    accept(&mut c);
    receive(
        &mut c,
        &signal(
            1,
            Signal::ConnectionResponse {
                destination_channel: 0x80,
                source_channel: 0x40,
                result: ConnectionResult::Success,
                status: ConnectionStatus::NoFurtherInformation,
            },
        ),
    );
    let request = signal(
        44,
        Signal::ConfigurationRequest {
            destination_channel: 0x40,
            options: ConfigurationOptions { mtu: Some(80) },
        },
    );
    let offer = prepare(&mut c);
    assert_eq!(
        c.receive(connection(), 10, &request, &mut []),
        Err(Error::Busy)
    );
    assert_eq!(prepare(&mut c), offer);
    c.confirm_accepted(offer.0.ticket, 10).unwrap();
    receive(&mut c, &request);
    accept(&mut c);
    assert!(!c.has_pending());
}

#[test]
fn sdp_continuation_acceptance_transaction_and_channel_are_isolated() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    complete_channel(&mut c, ChannelRole::Sdp, 0x80, 80);
    assert_eq!(
        c.receive(
            connection(),
            10,
            &sdp(1, &attributes()[..1], &[0xaa]),
            &mut []
        ),
        Err(Error::ResponseBeforeAcceptance)
    );
    accept(&mut c);
    receive(&mut c, &sdp(1, &attributes()[..1], &[0xaa]));
    let offer = prepare(&mut c);
    assert_eq!(offer.0.kind, SduKind::SdpRequest);
    assert_eq!(
        c.receive(
            connection(),
            10,
            &sdp(2, &attributes()[1..], &[]),
            &mut [0; 256]
        ),
        Err(Error::ResponseBeforeAcceptance)
    );
    c.confirm_accepted(offer.0.ticket, 10).unwrap();
    assert!(matches!(
        c.receive(
            connection(),
            10,
            &sdp(1, &attributes()[1..], &[]),
            &mut [0; 256]
        ),
        Err(Error::Discovery(_))
    ));
    assert!(c.service().is_none());
    assert!(matches!(
        receive(&mut c, &sdp(2, &attributes()[1..], &[])),
        Notice::ServiceDiscovered(_)
    ));
    assert!(c.open_signaling(10).is_err());
    finish_disconnect(&mut c);
    c.open_signaling(10).unwrap();
}

#[test]
fn cancel_before_acceptance_is_local_but_accepted_connect_waits_for_remote_cid() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    let old = prepare(&mut c).0.ticket;
    c.cancel(10).unwrap();
    assert!(c.is_closed());
    assert!(!c.needs_cleanup());
    assert_eq!(c.confirm_accepted(old, 10), Err(Error::WrongTicket));
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    accept(&mut c);
    c.cancel(10).unwrap();
    assert!(!c.is_closed());
    assert!(c.needs_cleanup());
    assert!(c.prepare_sdu(10, &mut [0; 256]).unwrap().is_none());
    receive(
        &mut c,
        &signal(
            1,
            Signal::ConnectionResponse {
                destination_channel: 0x80,
                source_channel: 0x40,
                result: ConnectionResult::Success,
                status: ConnectionStatus::NoFurtherInformation,
            },
        ),
    );
    let (_, pdu) = prepare(&mut c);
    assert!(matches!(
        SignalCommand::parse(BasicFrame::parse(&pdu).unwrap().payload())
            .unwrap()
            .signal,
        Signal::DisconnectionRequest { .. }
    ));
    finish_disconnect(&mut c);
    assert!(c.is_closed());
}

#[test]
fn peer_disconnect_is_not_clean_until_response_is_accepted() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c);
    c.open_signaling(10).unwrap();
    complete_channel(&mut c, ChannelRole::Signaling, 0x81, 80);
    c.open_media(10).unwrap();
    complete_channel(&mut c, ChannelRole::Media, 0x82, 80);
    assert_eq!(
        receive(
            &mut c,
            &signal(
                31,
                Signal::DisconnectionRequest {
                    destination_channel: 0x42,
                    source_channel: 0x82
                }
            )
        ),
        Notice::None
    );
    assert!(c.binding(ChannelRole::Media).is_none());
    assert!(!c.media_closed());
    assert!(c.needs_cleanup());
    let (p, _) = prepare(&mut c);
    assert_eq!(
        c.confirm_accepted(p.ticket, 10),
        Ok(Notice::Closed {
            role: ChannelRole::Media
        })
    );
    assert!(c.media_closed());
}

#[test]
fn finite_deadlines_prevent_expired_offers_and_do_not_fake_cleanup() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    accept(&mut c);
    c.cancel(20).unwrap();
    c.cancel(119).unwrap();
    assert_eq!(
        c.tick(120),
        Err(Error::Timeout(CoordinatorPhase::Cancelling))
    );
    assert_eq!(c.phase(), CoordinatorPhase::Failed);
    assert!(c.needs_cleanup());
    assert!(!c.is_closed());
    assert_eq!(c.prepare_sdu(120, &mut [0; 256]), Err(Error::Failed));
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    let mut out = [0xa5; 256];
    assert_eq!(
        c.prepare_sdu(1010, &mut out),
        Err(Error::Timeout(CoordinatorPhase::Discovering))
    );
    assert_eq!(out, [0xa5; 256]);
    assert!(!c.needs_cleanup());
}

#[test]
fn pending_connection_status_does_not_extend_fixed_deadline() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    accept(&mut c);
    let pending = signal(
        1,
        Signal::ConnectionResponse {
            destination_channel: 0,
            source_channel: 0x40,
            result: ConnectionResult::Pending,
            status: ConnectionStatus::AuthenticationPending,
        },
    );
    assert_eq!(
        c.receive(connection(), 1009, &pending, &mut []),
        Ok(Notice::None)
    );
    assert_eq!(
        c.tick(1010),
        Err(Error::Timeout(CoordinatorPhase::Discovering))
    );
    assert!(c.needs_cleanup());
}

#[test]
fn uncertain_send_revokes_bindings_and_requires_external_reconciliation() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    prepare(&mut c);
    assert_eq!(
        c.fail_transport(bt_core::Error::Busy),
        Err(Error::InvalidState)
    );
    assert_eq!(
        c.fail_transport(bt_core::Error::Backend(-9)),
        Err(Error::Transport(bt_core::Error::Backend(-9)))
    );
    assert!(c.needs_cleanup());
    assert_eq!(c.cancel(10), Err(Error::Failed));
    assert_eq!(c.prepare_sdu(10, &mut [0; 256]), Err(Error::Failed));
}

#[test]
fn invalid_configuration_foreign_traffic_malformed_frames_and_mtu_are_explicit() {
    for cfg in [
        ChannelConfig {
            local_cids: [0x40, 0x40, 0x42],
            ..config()
        },
        ChannelConfig {
            local_mtu: 47,
            ..config()
        },
        ChannelConfig {
            channel_timeout_us: 0,
            ..config()
        },
    ] {
        assert!(
            ChannelCoordinator::new(connection(), 0, cfg, &mut [0; 256], &mut [0; 256]).is_err()
        );
    }
    assert!(
        ChannelCoordinator::new(connection(), 0, config(), &mut [0; 46], &mut [0; 256]).is_err()
    );
    assert!(
        ChannelCoordinator::new(
            connection(),
            u64::MAX,
            config(),
            &mut [0; 256],
            &mut [0; 256]
        )
        .is_err()
    );
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    assert!(matches!(
        c.receive(connection(), 10, &[0, 0, 1], &mut []),
        Err(Error::Basic(_))
    ));
    assert_eq!(
        c.receive(connection(), 10, &basic(0x66, &[]), &mut []),
        Err(Error::ForeignChannel)
    );
    assert_eq!(
        c.receive(connection(), 10, &basic(1, &[0; 129]), &mut []),
        Err(Error::MtuExceeded)
    );
    assert_eq!(c.tick(9), Err(Error::ClockRegression));
    assert_eq!(c.phase(), CoordinatorPhase::Idle);
}

#[test]
fn active_remote_cid_cannot_be_reassigned_to_media() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c);
    c.open_signaling(10).unwrap();
    complete_channel(&mut c, ChannelRole::Signaling, 0x81, 80);
    c.open_media(10).unwrap();
    accept(&mut c);
    let response = signal(
        9,
        Signal::ConnectionResponse {
            destination_channel: 0x81,
            source_channel: 0x42,
            result: ConnectionResult::Success,
            status: ConnectionStatus::NoFurtherInformation,
        },
    );
    assert_eq!(
        c.receive(connection(), 10, &response, &mut []),
        Err(Error::RemoteCidCollision)
    );
    assert!(c.binding(ChannelRole::Signaling).is_some());
    assert!(c.binding(ChannelRole::Media).is_none());
}

#[test]
fn accepted_callback_records_cleanup_before_deadline_or_clock_failure() {
    for (now, expected) in [
        (1010, Error::Timeout(CoordinatorPhase::Discovering)),
        (9, Error::ClockRegression),
    ] {
        let (mut tx, mut storage) = ([0; 256], [0; 256]);
        let mut c = coordinator(&mut tx, &mut storage);
        c.begin_discovery(10).unwrap();
        let (prepared, _) = prepare(&mut c);
        assert_eq!(c.confirm_accepted(prepared.ticket, now), Err(expected));
        assert_eq!(c.phase(), CoordinatorPhase::Failed);
        assert_eq!(c.failure(), Some(expected));
        assert!(c.needs_cleanup());
        assert!(!c.has_pending());
        assert!(!c.is_closed());
        assert_eq!(
            c.confirm_accepted(prepared.ticket, 1010),
            Err(Error::WrongTicket)
        );
        assert_eq!(c.cancel(1010), Err(Error::Failed));
        assert_eq!(c.prepare_sdu(1010, &mut [0; 256]), Err(Error::Failed));
    }
}

#[test]
fn freed_peer_cids_are_reusable_but_old_local_media_cid_is_stale() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c); // SDP remote CID 0x80 is now released.
    c.open_signaling(10).unwrap();
    complete_channel(&mut c, ChannelRole::Signaling, 0x80, 80);
    assert_eq!(c.binding(ChannelRole::Signaling).unwrap().remote_cid, 0x80);
    c.open_media(10).unwrap();
    complete_channel(&mut c, ChannelRole::Media, 0x81, 80);
    let old = c.binding(ChannelRole::Media).unwrap();
    c.close_media(10).unwrap();
    finish_disconnect(&mut c);
    c.open_media(10).unwrap();
    complete_channel(&mut c, ChannelRole::Media, 0x81, 80);
    let new = c.binding(ChannelRole::Media).unwrap();
    assert_eq!(new.remote_cid, old.remote_cid);
    assert_ne!(new.local_cid, old.local_cid);
    assert_eq!(
        c.receive(connection(), 10, &basic(old.local_cid, &[1]), &mut []),
        Err(Error::ForeignChannel)
    );
    let packet = basic(new.local_cid, &[2]);
    assert!(matches!(
        c.receive(connection(), 10, &packet, &mut []),
        Ok(Notice::Data {
            role: ChannelRole::Media,
            payload: [2],
            ..
        })
    ));
}

#[test]
fn peer_disconnect_reply_has_a_fixed_cleanup_deadline() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c);
    c.open_signaling(10).unwrap();
    complete_channel(&mut c, ChannelRole::Signaling, 0x81, 80);
    receive(
        &mut c,
        &signal(
            44,
            Signal::DisconnectionRequest {
                destination_channel: 0x41,
                source_channel: 0x81,
            },
        ),
    );
    assert_eq!(c.phase(), CoordinatorPhase::ClosingSignaling);
    assert!(!c.signaling_closed());
    assert!(c.needs_cleanup());
    assert_eq!(
        c.prepare_sdu(110, &mut [0; 256]),
        Err(Error::Timeout(CoordinatorPhase::ClosingSignaling))
    );
    assert!(c.needs_cleanup()); // the response still has not been accepted.
    assert!(!c.signaling_closed());
}

#[test]
fn premature_peer_closure_does_not_publish_a_usable_session() {
    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    c.begin_discovery(10).unwrap();
    complete_channel(&mut c, ChannelRole::Sdp, 0x80, 80);
    accept(&mut c); // accepted SDP query, then peer closes without a record.
    receive(
        &mut c,
        &signal(
            44,
            Signal::DisconnectionRequest {
                destination_channel: 0x40,
                source_channel: 0x80,
            },
        ),
    );
    let (p, _) = prepare(&mut c);
    assert_eq!(
        c.confirm_accepted(p.ticket, 10),
        Err(Error::PeerClosed(ChannelRole::Sdp))
    );
    assert_eq!(c.phase(), CoordinatorPhase::Failed);
    assert!(c.service().is_none());
    assert!(!c.needs_cleanup());

    let (mut tx, mut storage) = ([0; 256], [0; 256]);
    let mut c = coordinator(&mut tx, &mut storage);
    discover(&mut c);
    c.open_signaling(10).unwrap();
    complete_channel(&mut c, ChannelRole::Signaling, 0x81, 80);
    c.open_media(10).unwrap();
    complete_channel(&mut c, ChannelRole::Media, 0x82, 80);
    receive(
        &mut c,
        &signal(
            44,
            Signal::DisconnectionRequest {
                destination_channel: 0x41,
                source_channel: 0x81,
            },
        ),
    );
    let (p, _) = prepare(&mut c);
    assert_eq!(
        c.confirm_accepted(p.ticket, 10),
        Err(Error::PeerClosed(ChannelRole::Signaling))
    );
    assert!(c.binding(ChannelRole::Media).is_none());
    assert!(c.needs_cleanup());
}
