// SPDX-License-Identifier: GPL-3.0-or-later

use bt_hci::ConnectionHandle;
use bt_l2cap::{
    ChannelError, ChannelEvent, ChannelPhase, ClassicChannel, ConfigurationOptions,
    ConfigurationResult, Connection, ConnectionResult, ConnectionStatus, Signal, SignalCommand,
    SignalError,
};

const LOCAL_CID: u16 = 0x0040;
const REMOTE_CID: u16 = 0x0041;
const AVDTP_PSM: u16 = 0x0019;

fn connection(epoch: u64) -> Connection {
    Connection {
        handle: ConnectionHandle::new(0x000b).unwrap(),
        epoch,
    }
}

fn channel(epoch: u64) -> ClassicChannel {
    ClassicChannel::new(connection(epoch), AVDTP_PSM, LOCAL_CID, 672, 1).unwrap()
}

fn response(identifier: u8, signal: Signal) -> SignalCommand {
    SignalCommand { identifier, signal }
}

fn advance_to_configuring(channel: &mut ClassicChannel, output: &mut [u8]) {
    let started = channel.connect(connection(1), output).unwrap();
    assert_eq!(started.event, ChannelEvent::ConnectionStarted);
    assert_eq!(
        &output[..started.output_len],
        &[2, 1, 4, 0, 0x19, 0, 0x40, 0]
    );

    let configuring = channel
        .receive(
            connection(1),
            response(
                1,
                Signal::ConnectionResponse {
                    destination_channel: REMOTE_CID,
                    source_channel: LOCAL_CID,
                    result: ConnectionResult::Success,
                    status: ConnectionStatus::NoFurtherInformation,
                },
            ),
            output,
        )
        .unwrap();
    assert_eq!(configuring.event, ChannelEvent::ConfigurationStarted);
    assert_eq!(
        &output[..configuring.output_len],
        &[4, 2, 8, 0, 0x41, 0, 0, 0, 1, 2, 0xa0, 2]
    );
    assert_eq!(channel.phase(), ChannelPhase::Configuring);
}

fn advance_to_open(channel: &mut ClassicChannel, output: &mut [u8]) {
    advance_to_configuring(channel, output);
    let peer = channel
        .receive(
            connection(1),
            response(
                9,
                Signal::ConfigurationRequest {
                    destination_channel: LOCAL_CID,
                    options: ConfigurationOptions { mtu: Some(500) },
                },
            ),
            output,
        )
        .unwrap();
    assert_eq!(peer.event, ChannelEvent::PeerConfigured);
    assert_eq!(
        &output[..peer.output_len],
        &[5, 9, 6, 0, 0x40, 0, 0, 0, 0, 0]
    );
    assert_eq!(channel.peer_mtu(), 500);

    let opened = channel
        .receive(
            connection(1),
            response(
                2,
                Signal::ConfigurationResponse {
                    source_channel: REMOTE_CID,
                    result: ConfigurationResult::Success,
                    options: ConfigurationOptions::default(),
                },
            ),
            output,
        )
        .unwrap();
    assert_eq!(opened.event, ChannelEvent::Opened);
    assert_eq!(opened.output_len, 0);
    assert_eq!(channel.phase(), ChannelPhase::Open);
}

#[test]
fn scripted_peer_opens_and_locally_disconnects() {
    let mut channel = channel(1);
    let mut output = [0xa5; 32];
    advance_to_open(&mut channel, &mut output);

    let disconnect = channel.disconnect(connection(1), &mut output).unwrap();
    assert_eq!(disconnect.event, ChannelEvent::DisconnectStarted);
    assert_eq!(
        &output[..disconnect.output_len],
        &[6, 3, 4, 0, 0x41, 0, 0x40, 0]
    );
    assert_eq!(channel.phase(), ChannelPhase::WaitDisconnection);

    let closed = channel
        .receive(
            connection(1),
            response(
                3,
                Signal::DisconnectionResponse {
                    destination_channel: REMOTE_CID,
                    source_channel: LOCAL_CID,
                },
            ),
            &mut output,
        )
        .unwrap();
    assert_eq!(closed.event, ChannelEvent::Closed);
    assert_eq!(channel.phase(), ChannelPhase::Closed);
    assert_eq!(channel.remote_channel(), None);
}

#[test]
fn peer_disconnect_response_echoes_cid_order() {
    let mut channel = channel(1);
    let mut output = [0; 32];
    advance_to_open(&mut channel, &mut output);

    let closed = channel
        .receive(
            connection(1),
            response(
                44,
                Signal::DisconnectionRequest {
                    destination_channel: LOCAL_CID,
                    source_channel: REMOTE_CID,
                },
            ),
            &mut output,
        )
        .unwrap();
    assert_eq!(closed.event, ChannelEvent::Closed);
    assert_eq!(
        &output[..closed.output_len],
        &[7, 44, 4, 0, 0x40, 0, 0x41, 0]
    );
}

#[test]
fn signal_codec_rejects_malformed_and_unsupported_inputs_atomically() {
    let bytes = [4, 7, 8, 0, 0x40, 0, 0, 0, 1, 2, 0xa0, 2];
    let parsed = SignalCommand::parse(&bytes).unwrap();
    assert_eq!(
        parsed,
        response(
            7,
            Signal::ConfigurationRequest {
                destination_channel: LOCAL_CID,
                options: ConfigurationOptions { mtu: Some(672) },
            }
        )
    );
    for length in 0..bytes.len() {
        assert_eq!(
            SignalCommand::parse(&bytes[..length]),
            Err(SignalError::Length)
        );
    }
    let mut trailing = [0; 13];
    trailing[..12].copy_from_slice(&bytes);
    assert_eq!(SignalCommand::parse(&trailing), Err(SignalError::Length));

    assert_eq!(
        SignalCommand::parse(&[4, 1, 8, 0, 0x40, 0, 1, 0, 1, 2, 0xa0, 2]),
        Err(SignalError::Continuation)
    );
    assert_eq!(
        SignalCommand::parse(&[4, 1, 7, 0, 0x40, 0, 0, 0, 0x81, 1, 0]),
        Err(SignalError::UnsupportedOption)
    );
    assert_eq!(
        SignalCommand::parse(&[2, 0, 4, 0, 0x19, 0, 0x40, 0]),
        Err(SignalError::Identifier)
    );
    assert_eq!(
        SignalCommand::parse(&[2, 1, 4, 0, 0x18, 0, 0x40, 0]),
        Err(SignalError::InvalidPsm)
    );

    let command = response(
        1,
        Signal::ConnectionRequest {
            psm: AVDTP_PSM,
            source_channel: LOCAL_CID,
        },
    );
    let mut short = [0xa5; 7];
    assert_eq!(command.encode(&mut short), Err(SignalError::BufferTooSmall));
    assert_eq!(short, [0xa5; 7]);
}

#[test]
fn short_outputs_and_rejected_options_do_not_advance_state() {
    let mut channel = channel(1);
    let mut short = [0xa5; 7];
    assert_eq!(
        channel.connect(connection(1), &mut short),
        Err(ChannelError::Signal(SignalError::BufferTooSmall))
    );
    assert_eq!(short, [0xa5; 7]);
    assert_eq!(channel.phase(), ChannelPhase::Closed);

    let mut output = [0; 32];
    let started = channel.connect(connection(1), &mut output).unwrap();
    assert_eq!(output[1], 1, "failed connect must not consume identifier");
    assert_eq!(started.output_len, 8);
    let pending = channel.pending_identifier();
    let mut short = [0xa5; 11];
    assert_eq!(
        channel.receive(
            connection(1),
            response(
                1,
                Signal::ConnectionResponse {
                    destination_channel: REMOTE_CID,
                    source_channel: LOCAL_CID,
                    result: ConnectionResult::Success,
                    status: ConnectionStatus::NoFurtherInformation,
                }
            ),
            &mut short,
        ),
        Err(ChannelError::Signal(SignalError::BufferTooSmall))
    );
    assert_eq!(short, [0xa5; 11]);
    assert_eq!(channel.phase(), ChannelPhase::WaitConnection);
    assert_eq!(channel.pending_identifier(), pending);

    channel
        .receive(
            connection(1),
            response(
                1,
                Signal::ConnectionResponse {
                    destination_channel: REMOTE_CID,
                    source_channel: LOCAL_CID,
                    result: ConnectionResult::Success,
                    status: ConnectionStatus::NoFurtherInformation,
                },
            ),
            &mut output,
        )
        .unwrap();
    assert_eq!(channel.pending_identifier(), Some(2));
    assert_eq!(
        channel.receive(
            connection(1),
            response(
                2,
                Signal::ConfigurationResponse {
                    source_channel: REMOTE_CID,
                    result: ConfigurationResult::Success,
                    options: ConfigurationOptions { mtu: Some(500) },
                }
            ),
            &mut output,
        ),
        Err(ChannelError::Signal(SignalError::UnsupportedOption))
    );
    assert_eq!(channel.phase(), ChannelPhase::Configuring);
    assert_eq!(channel.pending_identifier(), Some(2));
}

#[test]
fn rejection_timeout_cancel_and_epoch_checks_are_bounded() {
    let mut rejected = channel(1);
    let mut output = [0; 32];
    rejected.connect(connection(1), &mut output).unwrap();
    let result = rejected
        .receive(
            connection(1),
            response(
                1,
                Signal::ConnectionResponse {
                    destination_channel: 0,
                    source_channel: LOCAL_CID,
                    result: ConnectionResult::PsmNotSupported,
                    status: ConnectionStatus::NoFurtherInformation,
                },
            ),
            &mut output,
        )
        .unwrap();
    assert_eq!(
        result.event,
        ChannelEvent::ConnectionRejected(ConnectionResult::PsmNotSupported)
    );
    assert_eq!(rejected.phase(), ChannelPhase::Closed);

    let mut timed = channel(1);
    timed.connect(connection(1), &mut output).unwrap();
    assert_eq!(
        timed.timeout(connection(1), 2),
        Err(ChannelError::StaleIdentifier)
    );
    assert_eq!(timed.phase(), ChannelPhase::WaitConnection);
    assert_eq!(
        timed.timeout(connection(1), 1).unwrap().event,
        ChannelEvent::ConnectionTimedOut
    );
    assert_eq!(timed.phase(), ChannelPhase::Closed);

    assert_eq!(
        timed.connect(connection(2), &mut output),
        Err(ChannelError::ForeignConnection)
    );
    assert_eq!(timed.reset(connection(1)), Err(ChannelError::InvalidEpoch));
    timed.reset(connection(2)).unwrap();
    assert_eq!(timed.connection(), connection(2));
    assert_eq!(timed.phase(), ChannelPhase::Closed);

    timed.connect(connection(2), &mut output).unwrap();
    assert_eq!(
        timed.cancel(connection(2), &mut output).unwrap().event,
        ChannelEvent::Closed
    );
    assert_eq!(timed.phase(), ChannelPhase::Closed);
}

#[test]
fn constructed_invalid_command_is_rejected_before_state_change() {
    let mut channel = channel(1);
    let mut output = [0; 32];
    channel.connect(connection(1), &mut output).unwrap();
    let before = channel.pending_identifier();
    assert_eq!(
        channel.receive(
            connection(1),
            response(
                0,
                Signal::ConnectionResponse {
                    destination_channel: REMOTE_CID,
                    source_channel: LOCAL_CID,
                    result: ConnectionResult::Success,
                    status: ConnectionStatus::NoFurtherInformation,
                }
            ),
            &mut output,
        ),
        Err(ChannelError::Signal(SignalError::Identifier))
    );
    assert_eq!(channel.phase(), ChannelPhase::WaitConnection);
    assert_eq!(channel.pending_identifier(), before);
}
