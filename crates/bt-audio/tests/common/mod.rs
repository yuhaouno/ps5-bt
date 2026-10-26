// SPDX-License-Identifier: GPL-3.0-or-later
use bt_avdtp::{SignalId, StreamEndpointId};
use bt_core::{
    BluetoothTransport, Error as TransportError, Received, SbcAllocation, SbcBlocks,
    SbcChannelMode, SbcParameters, SbcSampleRate, SbcSubbands,
};
use bt_hci::{
    ConnectionHandle,
    flow::{AclCreditOwner, CreditContext},
};
use bt_l2cap::Connection;
use bt_sdp::SinkService;
use bt_session::{Channel, Config, SourceSession};

pub fn parameters() -> SbcParameters {
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
pub fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(11).unwrap(),
        epoch: 9,
    }
}
pub fn signaling() -> Channel {
    Channel {
        connection: connection(),
        local_cid: 0x40,
        remote_cid: 0x41,
        peer_mtu: 672,
    }
}
pub fn media() -> Channel {
    Channel {
        connection: connection(),
        local_cid: 0x42,
        remote_cid: 0x43,
        peer_mtu: 672,
    }
}
pub fn reply(source: &mut SourceSession<'_>, signal: SignalId, payload: &[u8]) {
    let offer = source.prepare_signal(0, &mut [0; 64]).unwrap().unwrap();
    assert_eq!(offer.command.signal, signal);
    source.confirm_signal_accepted(offer, 0).unwrap();
    let mut response = vec![(offer.command.transaction_label << 4) | 2, signal.raw()];
    response.extend_from_slice(payload);
    source.receive_signal(signaling(), 0, &response).unwrap();
}
pub fn streaming(storage: &mut [u8]) -> SourceSession<'_> {
    let mut source = SourceSession::new(
        signaling(),
        SinkService {
            avdtp_version: 0x0103,
            profile_version: 0x0103,
            supported_features: None,
        },
        StreamEndpointId::new(2).unwrap(),
        Config {
            signal_timeout_us: 1_000,
            media_open_timeout_us: 2_000,
            stop_timeout_us: 3_000,
            bitpool: 32,
            payload_type: 96,
            ssrc: 123,
            first_sequence: 20,
            first_timestamp: 128,
        },
        storage,
        0,
    )
    .unwrap();
    reply(&mut source, SignalId::Discover, &[4, 8]);
    reply(
        &mut source,
        SignalId::GetCapabilities,
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53],
    );
    reply(&mut source, SignalId::SetConfiguration, &[]);
    reply(&mut source, SignalId::Open, &[]);
    source.bind_media(media(), 40, 0).unwrap();
    reply(&mut source, SignalId::Start, &[]);
    source
}
pub fn credits() -> AclCreditOwner {
    AclCreditOwner::new(
        CreditContext {
            handle: connection().handle,
            epoch: connection().epoch,
        },
        1,
        40,
    )
    .unwrap()
}
pub fn complete(credits: &mut AclCreditOwner) {
    credits
        .complete(
            CreditContext {
                handle: connection().handle,
                epoch: connection().epoch,
            },
            &[0x13, 5, 1, 11, 0, 1, 0],
        )
        .unwrap();
}
#[derive(Default)]
pub struct Transport {
    pub accepted: Vec<Vec<u8>>,
}
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), TransportError> {
        panic!("no implicit HCI command")
    }
    fn receive<'a>(
        &mut self,
        _: &'a mut [u8],
        _: u32,
    ) -> Result<Option<Received<'a>>, TransportError> {
        panic!("no implicit reader")
    }
    fn send_acl(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.accepted.push(bytes.to_vec());
        Ok(())
    }
}
