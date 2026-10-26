// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic owned transport, identity, key, clock and PCM only.
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
struct Store;
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
struct Buffers {
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
fn recovery() -> Recovery {
    Recovery::new(Policy {
        max_attempts: 3,
        episode_timeout_us: 1_000_000,
        initial_backoff_us: 10,
        max_backoff_us: 100,
        stable_success_us: 1000,
    })
    .unwrap()
}
fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(0x42).unwrap(),
        epoch: 9,
    }
}
fn app<'a>(b: &'a mut Buffers, r: &'a mut Recovery) -> Application<'a, Store> {
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
fn config() -> Config {
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

fn event(code: u8, p: &[u8]) -> Vec<u8> {
    let mut v = vec![code, p.len() as u8];
    v.extend_from_slice(p);
    v
}
fn status(opcode: u16) -> Vec<u8> {
    let [l, h] = opcode.to_le_bytes();
    event(0x0f, &[0, 1, l, h])
}
fn hci(a: &mut Application<'_, Store>, opcode: u16) {
    let s = a.prepare(1, &mut [0; 700]).unwrap().unwrap();
    assert!(matches!(s,Submission::Command{ticket,..} if ticket.opcode()==opcode));
    a.confirm_accepted(1, s).unwrap();
}
fn secure(a: &mut Application<'_, Store>) {
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
fn receive(a: &mut Application<'_, Store>, cid: u16, payload: &[u8]) {
    let mut pdu = vec![0; payload.len() + 4];
    BasicFrame::new(cid, payload)
        .unwrap()
        .encode(&mut pdu)
        .unwrap();
    a.receive_l2cap(connection(), 1, &pdu, &mut [0; 512])
        .unwrap();
}
fn signal(a: &mut Application<'_, Store>, id: u8, s: Signal) {
    let mut out = [0; 64];
    let len = SignalCommand {
        identifier: id,
        signal: s,
    }
    .encode(&mut out)
    .unwrap();
    receive(a, 1, &out[..len]);
}
fn offered_channel(a: &mut Application<'_, Store>) -> SignalCommand {
    let mut out = [0; 700];
    let s = a.prepare(1, &mut out).unwrap().unwrap();
    let Submission::Channel(p) = s else {
        panic!("expected Channel, got {s:?}")
    };
    let cmd = SignalCommand::parse(BasicFrame::parse(&out[..p.len]).unwrap().payload()).unwrap();
    a.confirm_accepted(1, s).unwrap();
    cmd
}
fn open(a: &mut Application<'_, Store>, local: u16, remote: u16, psm: u16) {
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
fn close(a: &mut Application<'_, Store>, local: u16, remote: u16) {
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
fn valid_attributes() -> Vec<u8> {
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

fn connected(a: &mut Application<'_, Store>) {
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
fn reply(a: &mut Application<'_, Store>, id: SignalId, payload: &[u8]) {
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
fn streaming(a: &mut Application<'_, Store>) {
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
#[test]
fn owner_reaches_connected_but_never_starts_audio_without_start_command() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    connected(&mut a);
    assert!(a.status().source.is_none());
    assert!(a.prepare(1, &mut [0; 700]).unwrap().is_none());
}
#[test]
fn owner_negotiates_separate_media_then_stop_and_disconnect_complete_cleanup() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    a.stop(1).unwrap();
    reply(&mut a, SignalId::Suspend, &[]);
    reply(&mut a, SignalId::Close, &[]);
    close(&mut a, 0x44, 0x45);
    a.tick(1).unwrap();
    assert_eq!(a.status().phase, Phase::Stopped);
    a.disconnect(1).unwrap();
    close(&mut a, 0x42, 0x43);
    hci(&mut a, 0x0406);
    a.receive_hci(9, 1, &status(0x0406)).unwrap();
    a.receive_hci(9, 1, &event(5, &[0, 0x42, 0, 0x13])).unwrap();
    a.tick(1).unwrap();
    assert_eq!(a.status().phase, Phase::Closed);
    assert!(!a.status().cleanup_required);
}
#[test]
fn busy_command_offer_is_identical_and_early_events_do_not_advance() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    a.connect(Peer::from_hci_bytes(PEER), 9, 0).unwrap();
    let mut x = [0; 700];
    let offer = a.prepare(1, &mut x).unwrap().unwrap();
    assert!(a.receive_hci(9, 1, &status(0x0405)).is_err());
    a.submission_failed(offer, CoreError::Busy).unwrap();
    let mut y = [0; 700];
    assert_eq!(a.prepare(1, &mut y).unwrap(), Some(offer));
    assert_eq!(x, y);
}
#[test]
fn security_revalidation_invalidates_all_source_traffic_before_key_size_reply() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    assert_eq!(
        a.receive_hci(9, 1, &event(8, &[0, 0x42, 0, 1])),
        Err(Error::SecurityChanged)
    );
    assert_eq!(a.status().phase, Phase::Failed);
    assert!(a.prepare(1, &mut [0; 700]).is_err());
    assert!(matches!(
        a.status().recovery.action,
        Action::AwaitSafety {
            requirement: Requirement::Reconciliation,
            ..
        }
    ));
}
#[test]
fn foreign_packets_are_returned_and_owned_state_preserved() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    connected(&mut a);
    let before = a.status();
    assert!(a.receive_hci(8, 1, &event(5, &[0, 0x42, 0, 0x13])).is_err());
    let mut foreign = connection();
    foreign.epoch = 8;
    assert_eq!(
        a.receive_l2cap(foreign, 1, &[0, 0, 0x42, 0], &mut [0; 512]),
        Err(Error::ForeignContext)
    );
    assert_eq!(a.status(), before);
}
#[test]
fn clock_failure_and_uncertain_submission_keep_cleanup_and_recovery_visible() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    a.connect(Peer::from_hci_bytes(PEER), 9, 0).unwrap();
    let offer = a.prepare(1, &mut [0; 700]).unwrap().unwrap();
    assert!(a.submission_failed(offer, CoreError::Io).is_err());
    assert!(a.status().cleanup_required);
    assert!(a.prepare(2, &mut [0; 700]).is_err());
    a.cancel();
    assert!(matches!(
        a.status().recovery.action,
        Action::AwaitSafety { .. }
    ));
}
#[test]
fn operator_cancel_works_after_clock_regression() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    secure(&mut a);
    assert_eq!(a.tick(0), Err(Error::ClockRegressed));
    a.cancel();
    assert_eq!(a.status().failure, Some(Error::ClockRegressed));
    assert!(a.status().cleanup_required);
}

struct Encoder {
    parameters: SbcParameters,
    calls: usize,
}
impl BlockEncoder for Encoder {
    fn parameters(&self) -> SbcParameters {
        self.parameters
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], out: &mut [u8]) -> Result<usize, CoreError> {
        assert_eq!(pcm.len(), 128);
        self.calls += 1;
        out[..77].fill(0x55);
        out[0] = 0x9c;
        Ok(77)
    }
}
#[derive(Default)]
struct Transport {
    packets: Vec<Vec<u8>>,
    busy: bool,
}
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), CoreError> {
        panic!("audio must not send commands")
    }
    fn send_acl(&mut self, p: &[u8]) -> Result<(), CoreError> {
        if self.busy {
            self.busy = false;
            return Err(CoreError::Busy);
        }
        self.packets.push(p.to_vec());
        Ok(())
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, CoreError> {
        panic!("owner has no implicit reader")
    }
}
fn credits(n: u16) -> AclCreditOwner {
    AclCreditOwner::new(
        CreditContext {
            handle: connection().handle,
            epoch: 9,
        },
        n,
        40,
    )
    .unwrap()
}
#[test]
fn audio_pump_hands_off_once_and_stop_drains_partial_pdu_without_reencoding() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let parameters = a.negotiated_parameters().unwrap();
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        bt_audio::Config {
            parameters,
            epoch_us: 1,
            max_lateness_us: 100,
            max_concealed_frames: 0,
            max_rebuffer_us: 100,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    audio.push_pcm(0, &[[7, -7]; 128]).unwrap();
    let mut encoder = Encoder {
        parameters,
        calls: 0,
    };
    let mut owner = credits(3);
    let mut transport = Transport {
        packets: vec![],
        busy: true,
    };
    assert!(matches!(
        a.pump_audio(
            1,
            &mut audio,
            &mut encoder,
            &mut owner,
            &mut transport,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44]
        )
        .unwrap(),
        AudioProgress::Queued(_)
    ));
    assert!(matches!(
        a.pump_audio(
            1,
            &mut audio,
            &mut encoder,
            &mut owner,
            &mut transport,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44]
        )
        .unwrap(),
        AudioProgress::Sending(_)
    ));
    a.pump_audio(
        1,
        &mut audio,
        &mut encoder,
        &mut owner,
        &mut transport,
        &mut [0; 90],
        &mut [0; 94],
        &mut [0; 44],
    )
    .unwrap();
    assert_eq!(transport.packets.len(), 1);
    a.stop(1).unwrap();
    assert_eq!(
        a.status().source.unwrap().phase,
        bt_session::Phase::Draining
    );
    for _ in 0..2 {
        a.pump_audio(
            1,
            &mut audio,
            &mut encoder,
            &mut owner,
            &mut transport,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44],
        )
        .unwrap();
    }
    assert_eq!((encoder.calls, transport.packets.len()), (1, 3));
    assert_eq!(audio.snapshot().phase, bt_audio::Phase::Stopped);
    assert_eq!(
        a.status().source.unwrap().phase,
        bt_session::Phase::Suspending
    );
    reply(&mut a, SignalId::Suspend, &[]);
    reply(&mut a, SignalId::Close, &[]);
    close(&mut a, 0x44, 0x45);
    a.tick(1).unwrap();
    assert_eq!(a.status().phase, Phase::Stopped);
}
#[test]
fn zero_credit_media_stop_counts_unaccepted_packet_and_never_sends() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let parameters = a.negotiated_parameters().unwrap();
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        bt_audio::Config {
            parameters,
            epoch_us: 1,
            max_lateness_us: 100,
            max_concealed_frames: 0,
            max_rebuffer_us: 100,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    audio.push_pcm(0, &[[1, 2]; 128]).unwrap();
    let mut encoder = Encoder {
        parameters,
        calls: 0,
    };
    let mut owner = credits(0);
    let mut transport = Transport::default();
    for _ in 0..2 {
        a.pump_audio(
            1,
            &mut audio,
            &mut encoder,
            &mut owner,
            &mut transport,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44],
        )
        .unwrap();
    }
    a.stop(1).unwrap();
    let dropped = a.take_cancellation().unwrap();
    assert_eq!(dropped.cancellation.accepted_pdu_bytes, 0);
    assert!(a.take_cancellation().is_none());
    assert!(transport.packets.is_empty());
    assert_eq!(encoder.calls, 1);
}
#[test]
fn clean_stop_can_start_again_with_fresh_media_cid_and_no_payload_restart() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    a.stop(1).unwrap();
    reply(&mut a, SignalId::Suspend, &[]);
    reply(&mut a, SignalId::Close, &[]);
    close(&mut a, 0x44, 0x45);
    a.tick(1).unwrap();
    assert_eq!(a.status().phase, Phase::Stopped);
    a.start(1).unwrap();
    reply(&mut a, SignalId::Discover, &[4, 8]);
    reply(
        &mut a,
        SignalId::GetCapabilities,
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53],
    );
    reply(&mut a, SignalId::SetConfiguration, &[]);
    reply(&mut a, SignalId::Open, &[]);
    let mut out = [0; 700];
    let offer = a.prepare(1, &mut out).unwrap().unwrap();
    let Submission::Channel(p) = offer else {
        panic!()
    };
    let cmd = SignalCommand::parse(BasicFrame::parse(&out[..p.len]).unwrap().payload()).unwrap();
    let Signal::ConnectionRequest {
        source_channel: fresh,
        ..
    } = cmd.signal
    else {
        panic!()
    };
    assert_ne!(fresh, 0x44);
    // Retain/retry the offer; the peer may reuse its old remote CID after cleanup.
    open(&mut a, fresh, 0x45, 0x19);
    reply(&mut a, SignalId::Start, &[]);
    assert_eq!(a.status().phase, Phase::Streaming);
}
#[test]
fn disconnect_budget_is_not_extended_by_repeated_requests() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    connected(&mut a);
    a.disconnect(1).unwrap();
    a.disconnect(100).unwrap();
    assert!(a.tick(150_001).is_err());
    assert!(a.status().cleanup_required);
    assert_ne!(a.status().phase, Phase::Closed);
}
#[test]
fn a_remote_media_close_invalidates_stream_instead_of_leaving_false_streaming() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let mut out = [0; 64];
    let len = SignalCommand {
        identifier: 201,
        signal: Signal::DisconnectionRequest {
            destination_channel: 0x44,
            source_channel: 0x45,
        },
    }
    .encode(&mut out)
    .unwrap();
    let mut pdu = [0; 68];
    let size = BasicFrame::new(1, &out[..len])
        .unwrap()
        .encode(&mut pdu)
        .unwrap();
    assert!(
        a.receive_l2cap(connection(), 1, &pdu[..size], &mut [0; 512])
            .is_err()
    );
    assert_eq!(a.status().phase, Phase::Failed);
    assert!(a.status().cleanup_required);
}

#[test]
fn recovery_cleanup_requirement_cannot_be_bypassed_by_new_owner() {
    let mut b = Buffers::default();
    let mut r = recovery();
    r.trigger(0, bt_core::recovery::Failure::IntegrityLost)
        .unwrap();
    let mut a = app(&mut b, &mut r);
    assert_eq!(
        a.connect(Peer::from_hci_bytes(PEER), 9, 0),
        Err(Error::RecoveryBlocked)
    );
    assert_eq!(a.status().phase, Phase::Idle);
    assert!(!a.status().cleanup_required);
}
#[test]
fn nested_invalid_configuration_is_rejected_before_buffers_are_consumed() {
    for case in 0..7 {
        let mut cfg = config();
        match case {
            0 => cfg.reassembly.max_packets = 1,
            1 => cfg.reassembly.timeout_us = 0,
            2 => cfg.reassembly.max_packet_bytes = 2,
            3 => cfg.source.signal_timeout_us = 0,
            4 => cfg.source.bitpool = 54,
            5 => cfg.source.payload_type = 0,
            _ => cfg.source.ssrc = 0,
        }
        let mut b = Buffers::default();
        let mut r = recovery();
        let device = DeviceManager::new(
            Store,
            BondPolicy {
                minimum_key_size: 16,
                require_mitm: false,
                require_secure_connections: false,
            },
        )
        .unwrap();
        let result = Application::new(
            device,
            &mut r,
            cfg,
            Storage {
                channel_tx: &mut b.tx,
                sdp: &mut b.sdp,
                media_pdu: &mut b.pdu,
                avdtp: &mut b.avdtp,
            },
            0,
        );
        assert!(matches!(result, Err(Error::InvalidConfig)));
    }
}
#[test]
fn stop_during_negotiation_reports_required_cleanup_instead_of_stopped() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    connected(&mut a);
    a.start(1).unwrap();
    assert!(a.stop(1).is_err());
    assert_eq!(a.status().phase, Phase::Failed);
    assert!(a.status().cleanup_required);
}
#[test]
fn forget_revokes_live_ready_authority_and_stops_further_media_offers() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let outcome = a.forget(1).unwrap();
    assert!(outcome.persistence.is_ok());
    assert!(a.status().device.revoked);
    assert!(!a.status().device.ready);
    assert_eq!(a.status().phase, Phase::Failed);
    assert!(a.prepare(1, &mut [0; 700]).is_err());
}

fn retry_ready() -> Recovery {
    let mut r = recovery();
    r.trigger(0, bt_core::recovery::Failure::IntegrityLost)
        .unwrap();
    let Action::AwaitSafety {
        ticket,
        requirement,
    } = r.snapshot().action
    else {
        panic!()
    };
    r.confirm_safety(0, ticket, requirement).unwrap();
    r
}
#[test]
fn active_retry_enforces_fault_episode_deadline_and_preserves_cleanup() {
    let mut b = Buffers::default();
    let mut r = retry_ready();
    let mut a = app(&mut b, &mut r);
    a.connect(Peer::from_hci_bytes(PEER), 9, 0).unwrap();
    assert!(matches!(a.status().recovery.action, Action::InFlight(_)));
    assert_eq!(a.tick(1_000_000), Err(Error::RecoveryBlocked));
    assert_eq!(a.status().phase, Phase::Failed);
    assert!(a.status().cleanup_required);
    assert_eq!(a.status().recovery.attempts, 1);
    assert!(matches!(a.status().recovery.action, Action::Exhausted(_)));
}
#[test]
fn explicit_working_attestation_requires_media_then_stable_interval_resets_budget() {
    let mut b = Buffers::default();
    let mut r = retry_ready();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    assert_eq!(a.confirm_recovered_working(1), Err(Error::InvalidState));
    assert_eq!(a.status().recovery.attempts, 1);
    let parameters = a.negotiated_parameters().unwrap();
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        bt_audio::Config {
            parameters,
            epoch_us: 1,
            max_lateness_us: 100,
            max_concealed_frames: 0,
            max_rebuffer_us: 100,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    audio.push_pcm(0, &[[1, 2]; 128]).unwrap();
    let mut encoder = Encoder {
        parameters,
        calls: 0,
    };
    let mut owner = credits(3);
    let mut transport = Transport::default();
    for _ in 0..4 {
        a.pump_audio(
            1,
            &mut audio,
            &mut encoder,
            &mut owner,
            &mut transport,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44],
        )
        .unwrap();
    }
    // Synthetic external attestation tests policy; accepted ACL is not audibility evidence.
    a.confirm_recovered_working(1).unwrap();
    assert!(matches!(
        a.status().recovery.action,
        Action::StabilizingUntil(1001)
    ));
    a.tick(1000).unwrap();
    assert_eq!(a.status().recovery.attempts, 1);
    a.tick(1001).unwrap();
    assert_eq!(a.status().recovery.action, Action::Healthy);
    assert_eq!(a.status().recovery.attempts, 0);
}

#[test]
fn stalled_whole_or_partial_media_has_a_total_budget_and_never_refunds_credits() {
    for reserved in [0, 1] {
        let mut b = Buffers::default();
        let mut r = recovery();
        let mut a = app(&mut b, &mut r);
        streaming(&mut a);
        let parameters = a.negotiated_parameters().unwrap();
        let mut pcm = [TimedStereoFrame::default(); 128];
        let mut render = [[0; 2]; 128];
        let mut encoded = [0; 77];
        let mut audio = AudioScheduler::new(
            bt_audio::Config {
                parameters,
                epoch_us: 1,
                max_lateness_us: 100,
                max_concealed_frames: 0,
                max_rebuffer_us: 100,
                deadline_us: None,
            },
            &mut pcm,
            &mut render,
            &mut encoded,
        )
        .unwrap();
        audio.push_pcm(0, &[[1, 2]; 128]).unwrap();
        let mut encoder = Encoder {
            parameters,
            calls: 0,
        };
        let mut owner = credits(reserved);
        let mut transport = Transport::default();
        for _ in 0..2 {
            a.pump_audio(
                1,
                &mut audio,
                &mut encoder,
                &mut owner,
                &mut transport,
                &mut [0; 90],
                &mut [0; 94],
                &mut [0; 44],
            )
            .unwrap();
        }
        let before = owner.snapshot();
        assert_eq!(a.tick(100_001), Err(Error::Deadline));
        let dropped = a.take_cancellation().unwrap();
        assert_eq!(
            dropped.cancellation.accepted_pdu_bytes,
            40 * reserved as usize
        );
        assert_eq!(owner.snapshot(), before);
        assert_eq!(a.status().phase, Phase::Failed);
    }
}
