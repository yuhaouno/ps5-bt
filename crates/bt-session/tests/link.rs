// SPDX-License-Identifier: GPL-3.0-or-later
//! Cross-crate selected-peer security -> application negotiation binding.
use bt_avdtp::StreamEndpointId;
use bt_l2cap::Connection;
use bt_link::{Config as LinkConfig, LinkSession, Notice, Peer, SecretKey};
use bt_sdp::SinkService;
use bt_session::{Channel, Config, Error, SourceSession};

const PEER: [u8; 6] = [1, 2, 3, 4, 5, 6]; // synthetic test identity/key, never hardware data.
const EPOCH: u64 = 9;

fn event(code: u8, parameters: &[u8]) -> Vec<u8> {
    let mut out = vec![code, parameters.len() as u8];
    out.extend_from_slice(parameters);
    out
}
fn status(opcode: u16) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    event(0x0f, &[0, 1, lo, hi])
}
fn accept(link: &mut LinkSession, opcode: u16) {
    let ticket = link.prepare_command(&mut [0; 25]).unwrap().unwrap();
    assert_eq!(ticket.opcode(), opcode);
    link.confirm_accepted(ticket).unwrap();
}

#[test]
fn raw_hci_application_binding_requires_verified_link_ready_and_exact_epoch() {
    let mut link = LinkSession::new(
        Peer::from_hci_bytes(PEER),
        EPOCH,
        0,
        LinkConfig {
            packet_types: 0x3306,
            page_scan_repetition: 1,
            allow_role_switch: false,
            connection_timeout_us: 100,
            security_timeout_us: 200,
            cleanup_timeout_us: 40,
            minimum_key_size: 16,
        },
    )
    .unwrap();
    let config = Config {
        signal_timeout_us: 1000,
        media_open_timeout_us: 2000,
        stop_timeout_us: 3000,
        bitpool: 32,
        payload_type: 96,
        ssrc: 123,
        first_sequence: 0,
        first_timestamp: 0,
    };
    let service = SinkService {
        avdtp_version: 0x0103,
        profile_version: 0x0103,
        supported_features: None,
    };
    let local = StreamEndpointId::new(2).unwrap();
    let mut storage = [0; 128];
    let channel = Channel {
        connection: Connection {
            handle: bt_hci::ConnectionHandle::new(0x42).unwrap(),
            epoch: EPOCH,
        },
        local_cid: 0x40,
        remote_cid: 0x41,
        peer_mtu: 672,
    };
    assert!(matches!(
        SourceSession::from_link(&link, channel, service, local, config, &mut storage, 0),
        Err(Error::LinkNotReady)
    ));

    accept(&mut link, 0x0405);
    link.receive(EPOCH, 1, &status(0x0405)).unwrap();
    let mut connected = vec![0, 0x42, 0];
    connected.extend_from_slice(&PEER);
    connected.extend_from_slice(&[1, 0]);
    link.receive(EPOCH, 2, &event(3, &connected)).unwrap();
    accept(&mut link, 0x0411);
    link.receive(EPOCH, 3, &status(0x0411)).unwrap();
    let Notice::NeedKey { request } = link.receive(EPOCH, 3, &event(0x17, &PEER)).unwrap() else {
        panic!("expected key lookup")
    };
    link.supply_key(request, 3, Some(SecretKey::new([0x44; 16])))
        .unwrap();
    accept(&mut link, 0x040b);
    let mut completed = vec![1, 0x0b, 4, 0];
    completed.extend_from_slice(&PEER);
    link.receive(EPOCH, 3, &event(0x0e, &completed)).unwrap();
    link.receive(EPOCH, 20, &event(6, &[0, 0x42, 0])).unwrap();
    accept(&mut link, 0x0413);
    link.receive(EPOCH, 21, &status(0x0413)).unwrap();
    link.receive(EPOCH, 22, &event(8, &[0, 0x42, 0, 1]))
        .unwrap();
    // Encryption enabled is insufficient before the bounded key-size query.
    assert!(link.ready_connection().is_none());
    accept(&mut link, 0x1408);
    assert!(matches!(
        link.receive(EPOCH, 23, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
            .unwrap(),
        Notice::Ready
    ));

    let mut stale = channel;
    stale.connection.epoch += 1;
    assert!(matches!(
        SourceSession::from_link(&link, stale, service, local, config, &mut storage, 24),
        Err(Error::ForeignContext)
    ));
    let mut application =
        SourceSession::from_link(&link, channel, service, local, config, &mut storage, 24).unwrap();
    let mut output = [0; 64];
    let discover = application
        .prepare_signal(24, &mut output)
        .unwrap()
        .unwrap();
    assert_eq!(&output[..discover.command.bytes], &[0, 1]);
    assert_eq!(application.connection(), channel.connection);
    assert_eq!(discover.connection, channel.connection);
}
