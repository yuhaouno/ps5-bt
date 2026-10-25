// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted owner/transport interaction; no hardware or real reservation.

use bt_a2dp::SbcRtpPacketizer;
use bt_core::{
    BluetoothTransport, Error, Received, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters,
    SbcSampleRate, SbcSubbands,
};
use bt_hci::{
    AclPacket, ConnectionHandle,
    flow::{AclCreditOwner, CreditContext, CreditError, CreditFailureCause},
};
use bt_l2cap::{CancellationReason, Connection, FragmentError, TerminalFailure};
use bt_media_session::{MediaSession, SendProgress, SessionState, TransmitError, TransmitOutcome};

struct Transport {
    outcome: Result<(), Error>,
    attempts: Vec<Vec<u8>>,
}

impl Transport {
    fn new() -> Self {
        Self {
            outcome: Ok(()),
            attempts: Vec::new(),
        }
    }
}

impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("no command permitted")
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("no receive or wait permitted")
    }
    fn send_acl(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.attempts.push(bytes.to_vec());
        self.outcome
    }
}

fn connection() -> Connection {
    Connection {
        handle: ConnectionHandle::new(11).unwrap(),
        epoch: 3,
    }
}

fn context(connection: Connection) -> CreditContext {
    CreditContext {
        handle: connection.handle,
        epoch: connection.epoch,
    }
}

fn session(storage: &mut [u8]) -> MediaSession<'_> {
    let params = SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap();
    let packetizer = SbcRtpPacketizer::new(params, 90, 96, 1, 7, 1000).unwrap();
    let mut session = MediaSession::new(packetizer, connection(), 0x41, 40, storage).unwrap();
    let mut frame = [0x55; 77];
    frame[0] = 0x9c;
    session
        .queue_sbc_frames(connection(), &[&frame], &mut [0; 90], &mut [0; 94])
        .unwrap();
    session
}

const COMPLETED_ONE: &[u8] = &[0x13, 5, 1, 11, 0, 1, 0];

#[test]
fn busy_and_exhaustion_retain_pdu_until_three_real_acceptances_and_completions() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut credits = AclCreditOwner::new(context(own), 1, 40).unwrap();
    let mut transport = Transport::new();
    let mut output = [0; 44];
    transport.outcome = Err(Error::Busy);
    assert_eq!(
        session.transmit_next(own, &mut credits, &mut transport, &mut output),
        Ok(TransmitOutcome::Busy)
    );
    assert_eq!(credits.snapshot().accepted, 0);
    assert_eq!(session.accepted_fragments(), 0);
    transport.outcome = Ok(());
    let mut reconstructed = Vec::new();
    for fragment in 0..3 {
        let sent = session
            .transmit_next(own, &mut credits, &mut transport, &mut output)
            .unwrap();
        let latest = transport.attempts.last().unwrap();
        reconstructed.extend_from_slice(AclPacket::parse(latest).unwrap().payload());
        assert_eq!(credits.snapshot().available, 0);
        assert_eq!(credits.snapshot().outstanding, 1);
        if fragment < 2 {
            assert!(matches!(
                sent,
                TransmitOutcome::Accepted(SendProgress::More { .. })
            ));
            assert_eq!(
                session.transmit_next(own, &mut credits, &mut transport, &mut [0xa5; 44]),
                Ok(TransmitOutcome::NoCredits)
            );
            assert_eq!(transport.attempts.len(), fragment + 2);
            assert_eq!(
                session.stop(own),
                Err(bt_media_session::Error::Fragment(
                    FragmentError::CannotCancelPartial
                ))
            );
        } else {
            assert!(matches!(
                sent,
                TransmitOutcome::Accepted(SendProgress::Complete { fragments: 3, .. })
            ));
            assert!(session.queued().is_none());
        }
        assert_eq!(credits.complete(context(own), COMPLETED_ONE), Ok(1));
    }
    assert_eq!(transport.attempts[0], transport.attempts[1]);
    assert_eq!(reconstructed.len(), 94);
    assert_eq!(&reconstructed[..4], &[90, 0, 0x41, 0]);
    assert_eq!(
        &reconstructed[17..],
        &[0x9c].into_iter().chain([0x55; 76]).collect::<Vec<_>>()
    );
    assert_eq!(
        (credits.snapshot().accepted, credits.snapshot().completed),
        (3, 3)
    );
    assert_eq!(
        (session.next_sequence(), session.next_sample_timestamp()),
        (8, 1128)
    );
}

#[test]
fn foreign_completion_terminates_partial_pdu_without_a_second_submission() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut credits = AclCreditOwner::new(context(own), 1, 40).unwrap();
    let mut transport = Transport::new();
    session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap();
    assert!(matches!(
        credits.complete(context(own), &[0x13, 5, 1, 12, 0, 1, 0]),
        Err(CreditError::Failed(_))
    ));
    let error = session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap_err();
    let TransmitError::Terminal {
        failure,
        dropped: Some(dropped),
    } = error
    else {
        panic!("missing cancellation")
    };
    assert_eq!(failure.cause, CreditFailureCause::ForeignCompletionHandle);
    assert!(!failure.send_outcome_uncertain);
    assert_eq!(
        (
            dropped.cancellation.accepted_pdu_bytes,
            dropped.cancellation.accepted_fragments
        ),
        (40, 1)
    );
    assert_eq!(
        dropped.cancellation.reason,
        CancellationReason::Terminal(TerminalFailure::CreditsUnreconciled)
    );
    assert_eq!(transport.attempts.len(), 1);
    assert_eq!(credits.snapshot().outstanding, 1);
    assert_eq!(
        session.state(),
        SessionState::Failed(TerminalFailure::CreditsUnreconciled)
    );
    assert!(
        session
            .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
            .is_err()
    );
    assert_eq!(transport.attempts.len(), 1);
}

#[test]
fn uncertain_send_is_terminal_and_distinct_from_definitely_accepted_prefix() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut credits = AclCreditOwner::new(context(own), 2, 40).unwrap();
    let mut transport = Transport::new();
    session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap();
    transport.outcome = Err(Error::Io);
    let error = session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap_err();
    let TransmitError::Terminal {
        failure,
        dropped: Some(dropped),
    } = error
    else {
        panic!("missing cancellation")
    };
    assert_eq!(failure.cause, CreditFailureCause::Transport(Error::Io));
    assert!(failure.send_outcome_uncertain);
    assert_eq!(dropped.cancellation.accepted_fragments, 1);
    assert_eq!(credits.snapshot().accepted, 1);
    assert_eq!(transport.attempts.len(), 2);
    assert!(credits.complete(context(own), COMPLETED_ONE).is_err());
    assert_eq!(credits.snapshot().outstanding, 1);
}

#[test]
fn wrong_owner_and_short_storage_never_submit_or_advance() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut transport = Transport::new();
    let retained = session.pdu_storage().to_vec();
    for (ctx, max) in [
        (
            CreditContext {
                epoch: 2,
                ..context(own)
            },
            40,
        ),
        (
            CreditContext {
                handle: ConnectionHandle::new(12).unwrap(),
                ..context(own)
            },
            40,
        ),
        (context(own), 39),
    ] {
        let mut credits = AclCreditOwner::new(ctx, 1, max).unwrap();
        let before = credits.snapshot();
        let mut output = [0xa5; 44];
        assert_eq!(
            session.transmit_next(own, &mut credits, &mut transport, &mut output),
            Err(TransmitError::OwnerMismatch)
        );
        assert_eq!(output, [0xa5; 44]);
        assert_eq!(credits.snapshot(), before);
    }
    let mut credits = AclCreditOwner::new(context(own), 1, 40).unwrap();
    let mut short = [0xa5; 43];
    assert!(matches!(
        session.transmit_next(own, &mut credits, &mut transport, &mut short),
        Err(TransmitError::Session(_))
    ));
    assert_eq!(short, [0xa5; 43]);
    assert_eq!(session.pdu_storage(), retained);
    assert_eq!(session.accepted_fragments(), 0);
    assert_eq!(credits.snapshot().accepted, 0);
    assert!(transport.attempts.is_empty());
}

#[test]
fn every_non_busy_backend_error_stops_instead_of_assuming_no_acceptance() {
    for backend in [
        Error::Invalid,
        Error::BufferTooSmall,
        Error::Unsupported,
        Error::Timeout,
        Error::Disconnected,
        Error::Backend(123),
    ] {
        let mut storage = [0; 94];
        let mut session = session(&mut storage);
        let own = connection();
        let mut credits = AclCreditOwner::new(context(own), 1, 40).unwrap();
        let mut transport = Transport::new();
        transport.outcome = Err(backend);
        let error = session
            .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
            .unwrap_err();
        let TransmitError::Terminal {
            failure,
            dropped: Some(dropped),
        } = error
        else {
            panic!("missing terminal result")
        };
        assert_eq!(failure.cause, CreditFailureCause::Transport(backend));
        assert!(failure.send_outcome_uncertain);
        assert_eq!(dropped.cancellation.accepted_fragments, 0);
        assert_eq!(credits.snapshot().accepted, 0);
        assert_eq!(transport.attempts.len(), 1);
        assert!(
            session
                .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
                .is_err()
        );
        assert_eq!(transport.attempts.len(), 1);
    }
}

#[test]
fn busy_after_an_accepted_prefix_retries_the_same_continuation() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut credits = AclCreditOwner::new(context(own), 2, 40).unwrap();
    let mut transport = Transport::new();
    session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap();
    transport.outcome = Err(Error::Busy);
    let before = credits.snapshot();
    assert_eq!(
        session.transmit_next(own, &mut credits, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::Busy)
    );
    assert_eq!(credits.snapshot(), before);
    assert_eq!(
        (session.accepted_pdu_bytes(), session.accepted_fragments()),
        (40, 1)
    );
    assert_eq!(
        AclPacket::parse(&transport.attempts[1]).unwrap().boundary(),
        bt_hci::PacketBoundary::Continuation
    );
    transport.outcome = Ok(());
    session
        .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
        .unwrap();
    assert_eq!(transport.attempts[1], transport.attempts[2]);
    assert_eq!(
        (session.accepted_pdu_bytes(), session.accepted_fragments()),
        (80, 2)
    );
    assert_eq!(
        (credits.snapshot().accepted, credits.snapshot().outstanding),
        (2, 2)
    );
}

#[test]
fn failed_owner_terminates_idle_session_without_dropping_a_nonexistent_pdu() {
    let mut storage = [0; 94];
    let mut session = session(&mut storage);
    let own = connection();
    let mut credits = AclCreditOwner::new(context(own), 3, 40).unwrap();
    let mut transport = Transport::new();
    for _ in 0..3 {
        session
            .transmit_next(own, &mut credits, &mut transport, &mut [0; 44])
            .unwrap();
    }
    assert!(session.queued().is_none());
    credits.mark_integrity_lost();
    assert!(matches!(
        session.transmit_next(own, &mut credits, &mut transport, &mut [0; 44]),
        Err(TransmitError::Terminal { dropped: None, .. })
    ));
    assert_eq!(transport.attempts.len(), 3);
    assert_eq!(credits.snapshot().outstanding, 3);
}
