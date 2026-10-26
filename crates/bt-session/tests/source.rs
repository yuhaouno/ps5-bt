// SPDX-License-Identifier: GPL-3.0-or-later
use bt_avdtp::{
    ReassemblyLimits, SignalId, SignalingContext, SignalingReassembler, StreamEndpointId,
};
use bt_core::recovery::{Action, Failure as RecoveryFailure, Policy, Recovery, Requirement};
use bt_core::{BluetoothTransport, Error as TransportError, Received};
use bt_hci::{
    ConnectionHandle,
    flow::{AclCreditOwner, CreditContext},
};
use bt_l2cap::{Connection, TerminalFailure};
use bt_media_session::{SendProgress, TransmitError, TransmitOutcome};
use bt_sdp::SinkService;
use bt_session::{Channel, Config, Error, Failure, Phase, SessionTransmitError, SourceSession};

fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(11).unwrap(),
        epoch: 9,
    }
}
fn signaling() -> Channel {
    Channel {
        connection: connection(),
        local_cid: 0x40,
        remote_cid: 0x41,
        peer_mtu: 672,
    }
}
fn media() -> Channel {
    Channel {
        connection: connection(),
        local_cid: 0x42,
        remote_cid: 0x43,
        peer_mtu: 672,
    }
}
fn config() -> Config {
    Config {
        signal_timeout_us: 1000,
        media_open_timeout_us: 2000,
        stop_timeout_us: 3000,
        bitpool: 32,
        payload_type: 96,
        ssrc: 123,
        first_sequence: 20,
        first_timestamp: 128,
    }
}
fn new_source(storage: &mut [u8]) -> SourceSession<'_> {
    SourceSession::new(
        signaling(),
        SinkService {
            avdtp_version: 0x0103,
            profile_version: 0x0103,
            supported_features: None,
        },
        StreamEndpointId::new(2).unwrap(),
        config(),
        storage,
        0,
    )
    .unwrap()
}

fn reply(source: &mut SourceSession<'_>, expected: SignalId, payload: &[u8], now: u64) {
    let mut command = [0; 64];
    let submission = source.prepare_signal(now, &mut command).unwrap().unwrap();
    assert_eq!(submission.command.signal, expected);
    assert_eq!(submission.remote_cid, signaling().remote_cid);
    source.confirm_signal_accepted(submission, now).unwrap();
    let mut response = [0; 64];
    response[0] = (submission.command.transaction_label << 4) | 2;
    response[1] = expected.raw();
    response[2..2 + payload.len()].copy_from_slice(payload);
    source
        .receive_signal(signaling(), now, &response[..2 + payload.len()])
        .unwrap();
}

fn negotiate(source: &mut SourceSession<'_>) {
    reply(source, SignalId::Discover, &[4, 8], 0);
    reply(
        source,
        SignalId::GetCapabilities,
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53],
        0,
    );
    reply(source, SignalId::SetConfiguration, &[], 0);
    reply(source, SignalId::Open, &[], 0);
    assert_eq!(source.snapshot().phase, Phase::AwaitMedia);
}

fn streaming(source: &mut SourceSession<'_>) {
    negotiate(source);
    source.bind_media(media(), 40, 1).unwrap();
    reply(source, SignalId::Start, &[], 1);
    assert_eq!(source.snapshot().phase, Phase::Streaming);
}

fn credits(reserved: u16) -> AclCreditOwner {
    AclCreditOwner::new(
        CreditContext {
            handle: connection().handle,
            epoch: connection().epoch,
        },
        reserved,
        40,
    )
    .unwrap()
}

#[derive(Default)]
struct Transport {
    calls: usize,
    error: Option<TransportError>,
}
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), TransportError> {
        panic!("signaling is L2CAP, not an HCI command")
    }
    fn send_acl(&mut self, _: &[u8]) -> Result<(), TransportError> {
        self.calls += 1;
        self.error.take().map_or(Ok(()), Err)
    }
    fn receive<'a>(
        &mut self,
        _: &'a mut [u8],
        _: u32,
    ) -> Result<Option<Received<'a>>, TransportError> {
        panic!("no implicit reader")
    }
}

fn queue(source: &mut SourceSession<'_>) {
    // Deliberately synthetic SBC bytes: packet framing only, not codec evidence.
    let mut frame = [0; 77];
    frame[0] = 0x9c;
    source
        .queue_sbc_frames(&[&frame], &mut [0; 90], &mut [0; 94])
        .unwrap();
}

#[test]
fn complete_negotiation_binds_media_and_requires_remote_close_cleanup() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    assert_eq!(source.configuration().unwrap().parameters().bitpool(), 32);
    assert_eq!(source.media_channel(), Some(media()));
    source.stop(2).unwrap();
    assert_eq!(source.snapshot().phase, Phase::Suspending);
    reply(&mut source, SignalId::Suspend, &[], 3);
    reply(&mut source, SignalId::Close, &[], 4);
    assert_eq!(source.snapshot().phase, Phase::AwaitMediaClose);
    let mut foreign = media();
    foreign.connection.epoch += 1;
    assert_eq!(source.media_closed(foreign, 5), Err(Error::ForeignContext));
    source.media_closed(media(), 5).unwrap();
    assert_eq!(source.snapshot().phase, Phase::Stopped);
    source.stop(6).unwrap();
}

#[test]
fn signaling_busy_retries_identical_sdu_and_rejects_early_or_stale_completion() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    let before = source.snapshot();
    assert_eq!(
        source.receive_signal(signaling(), 0, &[2, 1, 4, 8]),
        Err(Error::ResponseBeforeAcceptance)
    );
    assert_eq!(source.snapshot(), before);
    let mut first = [0; 64];
    let ticket = source.prepare_signal(0, &mut first).unwrap().unwrap();
    source
        .signal_send_failed(ticket, TransportError::Busy)
        .unwrap();
    let mut second = [0; 64];
    assert_eq!(source.prepare_signal(0, &mut second).unwrap(), Some(ticket));
    assert_eq!(first, second);
    source.confirm_signal_accepted(ticket, 0).unwrap();
    assert_eq!(
        source.confirm_signal_accepted(ticket, 0),
        Err(Error::StaleSubmission)
    );
    source
        .receive_signal(signaling(), 0, &[2, 1, 4, 8])
        .unwrap();
    assert_eq!(
        source.confirm_signal_accepted(ticket, 0),
        Err(Error::StaleSubmission)
    );
}

#[test]
fn foreign_channels_and_inadequate_media_limits_do_not_bind() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    let mut wrong = signaling();
    wrong.local_cid += 1;
    assert_eq!(
        source.receive_signal(wrong, 0, &[2, 1]),
        Err(Error::ForeignContext)
    );
    negotiate(&mut source);
    let before = source.snapshot();
    assert_eq!(
        source.bind_media(signaling(), 40, 1),
        Err(Error::ForeignContext)
    );
    assert_eq!(
        source.bind_media(media(), 0, 1),
        Err(Error::InvalidAclLimit)
    );
    let mut small = media();
    small.peer_mtu = 48;
    assert!(matches!(
        source.bind_media(small, 40, 1),
        Err(Error::Packet(_))
    ));
    assert_eq!(source.snapshot().phase, before.phase);
    source.bind_media(media(), 40, 1).unwrap();
}

#[test]
fn start_ack_is_required_before_any_media_queues() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    negotiate(&mut source);
    source.bind_media(media(), 40, 1).unwrap();
    assert_eq!(
        source.queue_sbc_frames(&[], &mut [], &mut []),
        Err(Error::InvalidState)
    );
    reply(&mut source, SignalId::Start, &[], 1);
    queue(&mut source);
}

#[test]
fn zero_credit_and_busy_do_not_advance_or_invoke_unowned_sends() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    let mut transport = Transport::default();
    let mut none = credits(0);
    assert_eq!(
        source.transmit_next(2, &mut none, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::NoCredits)
    );
    assert_eq!(transport.calls, 0);
    let mut owner = credits(3);
    transport.error = Some(TransportError::Busy);
    assert_eq!(
        source.transmit_next(3, &mut owner, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::Busy)
    );
    assert_eq!(owner.snapshot().accepted, 0);
    assert!(matches!(
        source.transmit_next(4, &mut owner, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::Accepted(SendProgress::More {
            accepted_fragments: 1,
            ..
        }))
    ));
    assert_eq!(owner.snapshot().accepted, 1);
}

#[test]
fn stop_drains_partial_pdu_then_suspends_and_never_queues_new_audio() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    let mut transport = Transport::default();
    let mut owner = credits(3);
    source
        .transmit_next(2, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    source.stop(3).unwrap();
    assert_eq!(source.snapshot().phase, Phase::Draining);
    assert_eq!(
        source.queue_sbc_frames(&[], &mut [], &mut []),
        Err(Error::InvalidState)
    );
    assert_eq!(source.prepare_signal(3, &mut [0; 64]).unwrap(), None);
    source
        .transmit_next(4, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    source
        .transmit_next(5, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    assert_eq!(source.snapshot().phase, Phase::Suspending);
    assert_eq!(source.take_cancellation(), None);
    assert_eq!(owner.snapshot().outstanding, 3); // stop did not refund credits.
}

#[test]
fn stop_before_first_fragment_cancels_unsent_media_with_accounting() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    source.stop(2).unwrap();
    let dropped = source.take_cancellation().unwrap();
    assert_eq!(dropped.cancellation.accepted_fragments, 0);
    assert_eq!(source.snapshot().phase, Phase::Suspending);
    assert_eq!(source.take_cancellation(), None);
}

#[test]
fn drain_deadline_terminates_partial_without_refilling_credits() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    let mut transport = Transport::default();
    let mut owner = credits(1);
    source
        .transmit_next(2, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    source.stop(3).unwrap();
    assert_eq!(source.tick(3003), Err(Error::Failed(Failure::Deadline)));
    assert_eq!(
        source
            .take_cancellation()
            .unwrap()
            .cancellation
            .accepted_fragments,
        1
    );
    assert_eq!(owner.snapshot().outstanding, 1);
    assert_eq!(owner.snapshot().available, 0);
}

#[test]
fn native_uncertain_send_preserves_prefix_evidence_and_requires_reconciliation() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    let mut transport = Transport::default();
    let mut owner = credits(3);
    source
        .transmit_next(2, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    transport.error = Some(TransportError::Io);
    assert!(matches!(
        source.transmit_next(3, &mut owner, &mut transport, &mut [0; 44]),
        Err(SessionTransmitError::Transmit(
            TransmitError::Terminal { .. }
        ))
    ));
    assert_eq!(source.snapshot().failure, Some(Failure::MediaSubmission));
    assert_eq!(
        source
            .take_cancellation()
            .unwrap()
            .cancellation
            .accepted_fragments,
        1
    );
    assert!(owner.snapshot().failure.unwrap().send_outcome_uncertain);

    // Integrate the application's failed send with the recovery boundary.
    let mut recovery = Recovery::new(Policy {
        max_attempts: 2,
        episode_timeout_us: 1000,
        initial_backoff_us: 10,
        max_backoff_us: 100,
        stable_success_us: 100,
    })
    .unwrap();
    assert_eq!(
        source.snapshot().recovery_failure(),
        Some(RecoveryFailure::IntegrityLost)
    );
    recovery
        .trigger(3, source.snapshot().recovery_failure().unwrap())
        .unwrap();
    let Action::AwaitSafety {
        ticket,
        requirement,
    } = recovery.poll(3).unwrap()
    else {
        panic!("must await safety")
    };
    assert_eq!(requirement, Requirement::Reconciliation);
    assert!(
        recovery
            .confirm_safety(4, ticket, Requirement::Cleanup)
            .is_err()
    );
    assert!(matches!(
        recovery.poll(4).unwrap(),
        Action::AwaitSafety { .. }
    ));
    assert_eq!(owner.snapshot().available, 2); // uncertain current send was never refunded or retried.
}

#[test]
fn fragmented_discovery_drives_the_same_session_and_cannot_hide_assembly_timeout() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    let mut command = [0; 64];
    let ticket = source.prepare_signal(0, &mut command).unwrap().unwrap();
    source.confirm_signal_accepted(ticket, 0).unwrap();
    let context = SignalingContext {
        epoch: connection().epoch,
        channel_id: signaling().local_cid,
    };
    let mut buffer = [0; 64];
    let mut reassembly = SignalingReassembler::new(
        context,
        &mut buffer,
        ReassemblyLimits {
            max_packets: 4,
            max_packet_bytes: 64,
            timeout_us: 50,
        },
    )
    .unwrap();
    // Start has one payload byte; equal-sized Continue would have three.
    assert!(
        !source
            .receive_signal_packet(&mut reassembly, signaling(), 1, &[0x06, 2, 1, 4])
            .unwrap()
    );
    assert_eq!(source.snapshot().phase, Phase::Discovering);
    assert!(
        source
            .receive_signal_packet(&mut reassembly, signaling(), 2, &[0x0e, 8])
            .unwrap()
    );
    assert_eq!(source.snapshot().phase, Phase::Capabilities);

    let ticket = source.prepare_signal(3, &mut command).unwrap().unwrap();
    source.confirm_signal_accepted(ticket, 3).unwrap();
    assert!(
        !source
            .receive_signal_packet(&mut reassembly, signaling(), 4, &[0x16, 2, 2, 1])
            .unwrap()
    );
    assert!(matches!(
        source.receive_signal_packet(&mut reassembly, signaling(), 54, &[0x1e, 0]),
        Err(Error::Receive(_))
    ));
    assert_eq!(source.snapshot().failure, Some(Failure::Deadline));
}

#[test]
fn signal_deadline_clock_regression_and_negotiation_stop_are_terminal() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    assert_eq!(source.tick(1000), Err(Error::Failed(Failure::Deadline)));
    source.transport_lost(TerminalFailure::Disconnected); // preserves first cause.
    assert_eq!(source.snapshot().failure, Some(Failure::Deadline));
    let mut storage = [0; 128];
    let mut another = new_source(&mut storage);
    another.tick(1).unwrap();
    assert_eq!(another.tick(0), Err(Error::Failed(Failure::ClockRegressed)));
    let mut storage = [0; 128];
    let mut cancelled = new_source(&mut storage);
    cancelled.stop(0).unwrap();
    assert_eq!(cancelled.snapshot().failure, Some(Failure::Cancelled));
}

#[test]
fn rejected_configuration_and_small_storage_never_claim_streaming() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    reply(&mut source, SignalId::Discover, &[4, 8], 0);
    reply(
        &mut source,
        SignalId::GetCapabilities,
        &[1, 0, 7, 6, 0, 0, 0x13, 0x15, 2, 53],
        0,
    );
    let ticket = source.prepare_signal(0, &mut [0; 64]).unwrap().unwrap();
    source.confirm_signal_accepted(ticket, 0).unwrap();
    let bytes = [(ticket.command.transaction_label << 4) | 3, 3, 7, 0x29];
    assert!(matches!(
        source.receive_signal(signaling(), 0, &bytes),
        Err(Error::Failed(Failure::PeerRejected(_)))
    ));
    assert_eq!(source.snapshot().phase, Phase::Failed);

    let mut small = [0; 4];
    let mut small_source = new_source(&mut small);
    negotiate(&mut small_source);
    assert_eq!(
        small_source.bind_media(media(), 40, 1),
        Err(Error::PduStorageTooSmall)
    );
    assert_eq!(small_source.snapshot().phase, Phase::AwaitMedia);
}

#[test]
fn expired_signaling_offer_never_writes_or_returns_sendable_bytes() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    let mut output = [0xa5; 64];
    assert_eq!(
        source.prepare_signal(1000, &mut output),
        Err(Error::Failed(Failure::Deadline))
    );
    assert_eq!(output, [0xa5; 64]);
    assert!(source.prepare_signal(1001, &mut output).is_err());
    source.cancel();
    assert_eq!(source.snapshot().failure, Some(Failure::Deadline));
}

#[test]
fn integrity_evidence_survives_idle_loss_and_later_escalation() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    source.transport_lost(TerminalFailure::IntegrityLost);
    assert_eq!(
        source.snapshot().terminal_failure,
        Some(TerminalFailure::IntegrityLost)
    );
    assert_eq!(
        source.snapshot().recovery_failure(),
        Some(RecoveryFailure::IntegrityLost)
    );
    assert_eq!(source.take_cancellation(), None);
    source.transport_lost(TerminalFailure::Disconnected);
    assert_eq!(
        source.snapshot().terminal_failure,
        Some(TerminalFailure::IntegrityLost)
    );
    source.transport_lost(TerminalFailure::CreditsUnreconciled);
    assert_eq!(
        source.snapshot().terminal_failure,
        Some(TerminalFailure::CreditsUnreconciled)
    );
    assert_eq!(source.snapshot().failure, Some(Failure::Disconnected));
}

#[test]
fn repeated_stop_does_not_extend_budget_across_suspend_close_and_cleanup() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    source.stop(2).unwrap();
    source.stop(100).unwrap();
    assert_eq!(source.snapshot().deadline_us, Some(1002));
    reply(&mut source, SignalId::Suspend, &[], 1000);
    source.stop(1001).unwrap();
    assert_eq!(source.snapshot().deadline_us, Some(2000));
    reply(&mut source, SignalId::Close, &[], 1999);
    assert_eq!(source.snapshot().deadline_us, Some(3002));
    source.stop(3001).unwrap();
    assert_eq!(
        source.media_closed(media(), 3002),
        Err(Error::Failed(Failure::Deadline))
    );
}

#[test]
fn clock_failure_cancels_prefix_once_and_does_not_accept_late_signaling() {
    let mut storage = [0; 128];
    let mut source = new_source(&mut storage);
    streaming(&mut source);
    queue(&mut source);
    let mut transport = Transport::default();
    let mut owner = credits(3);
    source
        .transmit_next(2, &mut owner, &mut transport, &mut [0; 44])
        .unwrap();
    assert_eq!(source.tick(1), Err(Error::Failed(Failure::ClockRegressed)));
    assert_eq!(
        source
            .take_cancellation()
            .unwrap()
            .cancellation
            .accepted_fragments,
        1
    );
    source.cancel();
    assert_eq!(source.take_cancellation(), None);
    assert!(source.receive_signal(signaling(), 3, &[0x42, 7]).is_err());
    assert_eq!(owner.snapshot().outstanding, 1);
}
