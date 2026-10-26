#![allow(dead_code, unused_imports)]
// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic shared selected-peer protocol fixtures; no real identities or keys.
use bt_app::{Application, AudioProgress, Config, Error, Phase, Storage, Submission};
use bt_audio::{AudioScheduler, BlockEncoder};
use bt_avdtp::{ReassemblyLimits, SignalId, StreamEndpointId};
use bt_channel::ChannelConfig;
use bt_core::media::TimedStereoFrame;
use bt_core::recovery::{Action, Policy, Recovery, Requirement};
use bt_core::{BluetoothTransport, Error as CoreError, Received, SbcParameters};
use bt_device::{
    BOND_RECORD_BYTES, BondPolicy, BondRecord, BondStore, DeviceManager, KeyType, StoreError,
};
use bt_hci::{
    ConnectionHandle,
    flow::{AclCreditOwner, CreditContext},
};
use bt_l2cap::{
    BasicFrame, ConfigurationOptions, ConfigurationResult, Connection, ConnectionResult,
    ConnectionStatus, Signal, SignalCommand,
};
use bt_link::{Peer, SecretKey};
use bt_sdp::DiscoveryLimits;

const PEER: [u8; 6] = [1, 2, 3, 4, 5, 6];
pub struct Store;
impl BondStore for Store {
    fn load(
        &mut self,
        peer: Peer,
        out: &mut [u8; BOND_RECORD_BYTES],
    ) -> Result<Option<usize>, StoreError> {
        BondRecord::new(
            peer,
            SecretKey::new([0x44; 16]),
            KeyType::UnauthenticatedP192,
            16,
        )
        .unwrap()
        .encode(out)
        .unwrap();
        Ok(Some(BOND_RECORD_BYTES))
    }
    fn save_atomic(&mut self, _: Peer, _: &[u8; BOND_RECORD_BYTES]) -> Result<(), StoreError> {
        Ok(())
    }
    fn delete_atomic(&mut self, _: Peer) -> Result<(), StoreError> {
        Ok(())
    }
}
pub struct Buffers {
    tx: [u8; 512],
    sdp: [u8; 512],
    pdu: [u8; 512],
    avdtp: [u8; 512],
}
impl Default for Buffers {
    fn default() -> Self {
        Self {
            tx: [0; 512],
            sdp: [0; 512],
            pdu: [0; 512],
            avdtp: [0; 512],
        }
    }
}
pub fn recovery() -> Recovery {
    Recovery::new(Policy {
        max_attempts: 3,
        episode_timeout_us: 1_000_000,
        initial_backoff_us: 10,
        max_backoff_us: 100,
        stable_success_us: 1000,
    })
    .unwrap()
}
pub fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(0x42).unwrap(),
        epoch: 9,
    }
}
pub fn app<'a>(b: &'a mut Buffers, r: &'a mut Recovery) -> Application<'a, Store> {
    Application::new(
        DeviceManager::new(
            Store,
            BondPolicy {
                minimum_key_size: 16,
                require_mitm: false,
                require_secure_connections: false,
            },
        )
        .unwrap(),
        r,
        config(),
        Storage {
            channel_tx: &mut b.tx,
            sdp: &mut b.sdp,
            media_pdu: &mut b.pdu,
            avdtp: &mut b.avdtp,
        },
        0,
    )
    .unwrap()
}

pub fn app_with_config<'a>(
    b: &'a mut Buffers,
    r: &'a mut Recovery,
    custom: Config,
) -> Application<'a, Store> {
    Application::new(
        DeviceManager::new(
            Store,
            BondPolicy {
                minimum_key_size: 16,
                require_mitm: false,
                require_secure_connections: false,
            },
        )
        .unwrap(),
        r,
        custom,
        Storage {
            channel_tx: &mut b.tx,
            sdp: &mut b.sdp,
            media_pdu: &mut b.pdu,
            avdtp: &mut b.avdtp,
        },
        0,
    )
    .unwrap()
}
pub fn config() -> Config {
    Config {
        link: bt_link::Config {
            packet_types: 0x3306,
            page_scan_repetition: 1,
            allow_role_switch: false,
            connection_timeout_us: 1_000_000,
            security_timeout_us: 1_000_000,
            cleanup_timeout_us: 100_000,
            minimum_key_size: 16,
        },
        channels: ChannelConfig {
            local_cids: [0x40, 0x42, 0x44],
            local_mtu: 672,
            channel_timeout_us: 100_000,
            discovery_timeout_us: 100_000,
            cleanup_timeout_us: 100_000,
            discovery_limits: DiscoveryLimits {
                max_chunk_bytes: 64,
                max_total_bytes: 256,
                max_nodes: 64,
                max_depth: 8,
                max_chunks: 8,
            },
        },
        source: bt_session::Config {
            signal_timeout_us: 100_000,
            media_open_timeout_us: 100_000,
            stop_timeout_us: 100_000,
            bitpool: 32,
            payload_type: 96,
            ssrc: 123,
            first_sequence: 0,
            first_timestamp: 128,
        },
        local_endpoint: StreamEndpointId::new(2).unwrap(),
        reassembly: ReassemblyLimits {
            max_packets: 4,
            max_packet_bytes: 672,
            timeout_us: 100_000,
        },
        acl_data_max: 40,
        disconnect_timeout_us: 150_000,
        media_pdu_timeout_us: 100_000,
    }
}

pub fn event(code: u8, p: &[u8]) -> Vec<u8> {
    let mut v = vec![code, p.len() as u8];
    v.extend_from_slice(p);
    v
}
pub fn status(opcode: u16) -> Vec<u8> {
    let [l, h] = opcode.to_le_bytes();
    event(0x0f, &[0, 1, l, h])
}
pub fn hci(a: &mut Application<'_, Store>, opcode: u16) {
    let s = a.prepare(1, &mut [0; 700]).unwrap().unwrap();
    assert!(matches!(s,Submission::Command{ticket,..} if ticket.opcode()==opcode));
    a.confirm_accepted(1, s).unwrap();
}
pub fn secure(a: &mut Application<'_, Store>) {
    a.connect(Peer::from_hci_bytes(PEER), 9, 0).unwrap();
    hci(a, 0x0405);
    a.receive_hci(9, 1, &status(0x0405)).unwrap();
    let mut p = vec![0, 0x42, 0];
    p.extend_from_slice(&PEER);
    p.extend_from_slice(&[1, 0]);
    a.receive_hci(9, 1, &event(3, &p)).unwrap();
    hci(a, 0x0411);
    a.receive_hci(9, 1, &status(0x0411)).unwrap();
    a.receive_hci(9, 1, &event(0x17, &PEER)).unwrap();
    hci(a, 0x040b);
    let mut p = vec![1, 0x0b, 4, 0];
    p.extend_from_slice(&PEER);
    a.receive_hci(9, 1, &event(0x0e, &p)).unwrap();
    a.receive_hci(9, 1, &event(6, &[0, 0x42, 0])).unwrap();
    hci(a, 0x0413);
    a.receive_hci(9, 1, &status(0x0413)).unwrap();
    a.receive_hci(9, 1, &event(8, &[0, 0x42, 0, 1])).unwrap();
    hci(a, 0x1408);
    a.receive_hci(9, 1, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
        .unwrap();
}
pub fn receive(a: &mut Application<'_, Store>, cid: u16, payload: &[u8]) {
    let mut pdu = vec![0; payload.len() + 4];
    BasicFrame::new(cid, payload)
        .unwrap()
        .encode(&mut pdu)
        .unwrap();
    a.receive_l2cap(connection(), 1, &pdu, &mut [0; 512])
        .unwrap();
}
pub fn signal(a: &mut Application<'_, Store>, id: u8, s: Signal) {
    let mut out = [0; 64];
    let len = SignalCommand {
        identifier: id,
        signal: s,
    }
    .encode(&mut out)
    .unwrap();
    receive(a, 1, &out[..len]);
}
pub fn offered_channel(a: &mut Application<'_, Store>) -> SignalCommand {
    let mut out = [0; 700];
    let s = a.prepare(1, &mut out).unwrap().unwrap();
    let Submission::Channel(p) = s else {
        panic!("expected Channel, got {s:?}")
    };
    let cmd = SignalCommand::parse(BasicFrame::parse(&out[..p.len]).unwrap().payload()).unwrap();
    a.confirm_accepted(1, s).unwrap();
    cmd
}
pub fn open(a: &mut Application<'_, Store>, local: u16, remote: u16, psm: u16) {
    let cmd = offered_channel(a);
    assert_eq!(
        cmd.signal,
        Signal::ConnectionRequest {
            psm,
            source_channel: local
        }
    );
    signal(
        a,
        cmd.identifier,
        Signal::ConnectionResponse {
            destination_channel: remote,
            source_channel: local,
            result: ConnectionResult::Success,
            status: ConnectionStatus::NoFurtherInformation,
        },
    );
    let cfg = offered_channel(a);
    assert!(
        matches!(cfg.signal,Signal::ConfigurationRequest{destination_channel,..} if destination_channel==remote)
    );
    signal(
        a,
        200,
        Signal::ConfigurationRequest {
            destination_channel: local,
            options: ConfigurationOptions { mtu: Some(672) },
        },
    );
    let reply = offered_channel(a);
    assert!(matches!(reply.signal, Signal::ConfigurationResponse { .. }));
    signal(
        a,
        cfg.identifier,
        Signal::ConfigurationResponse {
            source_channel: remote,
            result: ConfigurationResult::Success,
            options: ConfigurationOptions::default(),
        },
    );
}
pub fn close(a: &mut Application<'_, Store>, local: u16, remote: u16) {
    let cmd = offered_channel(a);
    assert_eq!(
        cmd.signal,
        Signal::DisconnectionRequest {
            destination_channel: remote,
            source_channel: local
        }
    );
    signal(
        a,
        cmd.identifier,
        Signal::DisconnectionResponse {
            destination_channel: remote,
            source_channel: local,
        },
    );
}
pub fn valid_attributes() -> Vec<u8> {
    vec![
        0x35, 0x32, // all service records
        0x35, 0x30, // one service record
        0x09, 0x00, 0x01, 0x35, 0x03, 0x19, 0x11, 0x0b, // ServiceClassIDList
        0x09, 0x00, 0x04, 0x35, 0x10, // ProtocolDescriptorList
        0x35, 0x06, 0x19, 0x01, 0x00, 0x09, 0x00, 0x19, // L2CAP / AVDTP PSM
        0x35, 0x06, 0x19, 0x00, 0x19, 0x09, 0x01, 0x03, // AVDTP 1.3
        0x09, 0x00, 0x09, 0x35, 0x08, // BluetoothProfileDescriptorList
        0x35, 0x06, 0x19, 0x11, 0x0d, 0x09, 0x01, 0x03, // A2DP 1.3
        0x09, 0x03, 0x11, 0x09, 0x00, 0x01, // SupportedFeatures
    ]
}

pub fn connected(a: &mut Application<'_, Store>) {
    secure(a);
    open(a, 0x40, 0x41, 1);
    let mut out = [0; 700];
    let s = a.prepare(1, &mut out).unwrap().unwrap();
    let Submission::Channel(p) = s else { panic!() };
    let request = BasicFrame::parse(&out[..p.len]).unwrap().payload();
    let transaction = [request[1], request[2]];
    a.confirm_accepted(1, s).unwrap();
    let attrs = valid_attributes();
    let mut response = vec![7, transaction[0], transaction[1]];
    response.extend_from_slice(&((attrs.len() + 3) as u16).to_be_bytes());
    response.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
    response.extend_from_slice(&attrs);
    response.push(0);
    receive(a, 0x40, &response);
    close(a, 0x40, 0x41);
    open(a, 0x42, 0x43, 0x19);
    a.tick(1).unwrap();
    assert_eq!(a.status().phase, Phase::Connected);
}
pub fn reply(a: &mut Application<'_, Store>, id: SignalId, payload: &[u8]) {
    let mut out = [0; 700];
    let s = a.prepare(1, &mut out).unwrap().unwrap();
    let Submission::Signal(p) = s else {
        panic!("expected Signal, got {s:?}")
    };
    assert_eq!(p.command.signal, id);
    a.confirm_accepted(1, s).unwrap();
    let mut v = vec![(p.command.transaction_label << 4) | 2, id.raw()];
    v.extend_from_slice(payload);
    receive(a, 0x42, &v);
}
pub fn streaming(a: &mut Application<'_, Store>) {
    connected(a);
    a.start(1).unwrap();
    reply(a, SignalId::Discover, &[4, 8]);
    reply(
        a,
        SignalId::GetCapabilities,
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53],
    );
    reply(a, SignalId::SetConfiguration, &[]);
    reply(a, SignalId::Open, &[]);
    open(a, 0x44, 0x45, 0x19);
    reply(a, SignalId::Start, &[]);
    assert_eq!(a.status().phase, Phase::Streaming);
}
