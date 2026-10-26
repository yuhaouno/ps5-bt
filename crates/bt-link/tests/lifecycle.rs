// SPDX-License-Identifier: GPL-3.0-or-later
use bt_link::{Config, Error, Failure, LinkSession, Notice, Peer, Phase, SecretKey, State};

const PEER: [u8; 6] = [1, 2, 3, 4, 5, 6]; // synthetic, never a real device
const EPOCH: u64 = 7;
fn config() -> Config {
    Config {
        packet_types: 0x3306,
        page_scan_repetition: 1,
        allow_role_switch: false,
        connection_timeout_us: 100,
        security_timeout_us: 200,
        cleanup_timeout_us: 40,
        minimum_key_size: 16,
    }
}
fn session() -> LinkSession {
    LinkSession::new(Peer::from_hci_bytes(PEER), EPOCH, 0, config()).unwrap()
}
fn event(code: u8, parameters: &[u8]) -> Vec<u8> {
    let mut packet = vec![code, parameters.len() as u8];
    packet.extend_from_slice(parameters);
    packet
}
fn status(opcode: u16, code: u8) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    event(0x0f, &[code, 1, lo, hi])
}
fn complete_peer(opcode: u16, code: u8) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    let mut p = vec![1, lo, hi, code];
    p.extend_from_slice(&PEER);
    event(0x0e, &p)
}
fn connected(status: u8) -> Vec<u8> {
    let mut p = vec![status, 0x42, 0];
    p.extend_from_slice(&PEER);
    p.extend_from_slice(&[1, 0]);
    event(3, &p)
}
fn accept(s: &mut LinkSession, opcode: u16) -> Vec<u8> {
    let mut bytes = [0xa5; 25];
    let ticket = s.prepare_command(&mut bytes).unwrap().unwrap();
    assert_eq!(ticket.opcode(), opcode);
    let len = s.command_len().unwrap();
    let mut retry = [0; 25];
    assert_eq!(s.prepare_command(&mut retry).unwrap(), Some(ticket));
    assert_eq!(bytes[..len], retry[..len]);
    s.confirm_accepted(ticket).unwrap();
    assert!(s.prepare_command(&mut retry).unwrap().is_none());
    assert_eq!(s.confirm_accepted(ticket), Err(Error::WrongTicket));
    bytes[..len].to_vec()
}
fn connect(s: &mut LinkSession) {
    assert_eq!(
        accept(s, 0x0405),
        [5, 4, 13, 1, 2, 3, 4, 5, 6, 6, 0x33, 1, 0, 0, 0, 0]
    );
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.receive(EPOCH, 2, &connected(0)).unwrap();
    assert_eq!(s.state(), State::Authenticating);
    assert_eq!(accept(s, 0x0411), [0x11, 4, 2, 0x42, 0]);
    s.receive(EPOCH, 3, &status(0x0411, 0)).unwrap();
    let request = match s.receive(EPOCH, 3, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(request, 3, Some(SecretKey::new([0x44; 16])))
        .unwrap();
    accept(s, 0x040b);
    s.receive(EPOCH, 3, &complete_peer(0x040b, 0)).unwrap();
}
fn finish_security(s: &mut LinkSession) {
    s.receive(EPOCH, 20, &event(6, &[0, 0x42, 0])).unwrap();
    assert!(s.ready_connection().is_none());
    assert_eq!(accept(s, 0x0413), [0x13, 4, 3, 0x42, 0, 1]);
    s.receive(EPOCH, 21, &status(0x0413, 0)).unwrap();
    s.receive(EPOCH, 22, &event(8, &[0, 0x42, 0, 1])).unwrap();
    assert!(s.ready_connection().is_none());
    assert_eq!(accept(s, 0x1408), [8, 0x14, 2, 0x42, 0]);
}
fn ready(s: &mut LinkSession) {
    finish_security(s);
    assert!(matches!(
        s.receive(EPOCH, 23, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
            .unwrap(),
        Notice::Ready
    ));
}

#[test]
fn complete_reconnect_requires_authentication_encryption_and_key_length() {
    let mut s = session();
    connect(&mut s);
    let request = match s.receive(EPOCH, 4, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(request, 5, Some(SecretKey::new([0x5a; 16])))
        .unwrap();
    let bytes = accept(&mut s, 0x040b);
    assert_eq!(&bytes[..9], &[0x0b, 4, 22, 1, 2, 3, 4, 5, 6]);
    assert_eq!(&bytes[9..], &[0x5a; 16]);
    s.receive(EPOCH, 6, &complete_peer(0x040b, 0)).unwrap();
    assert_eq!(s.supply_key(request, 7, None), Err(Error::StaleRequest));
    ready(&mut s);
    assert_eq!(s.ready_connection().unwrap().0.raw(), 0x42);
    assert_eq!(s.ready_connection().unwrap().1, EPOCH);
    s.tick(1_000_000).unwrap(); // Ready is not a finite playback timer.
}

#[test]
fn fresh_ssp_needs_explicit_approval_and_delivers_opaque_provisional_key() {
    let mut s = session();
    connect(&mut s);
    let key_request = match s.receive(EPOCH, 4, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(key_request, 5, None).unwrap();
    accept(&mut s, 0x040c);
    s.receive(EPOCH, 6, &complete_peer(0x040c, 0)).unwrap();
    s.receive(EPOCH, 7, &event(0x31, &PEER)).unwrap();
    assert_eq!(
        accept(&mut s, 0x042b),
        [0x2b, 4, 9, 1, 2, 3, 4, 5, 6, 1, 0, 4]
    );
    s.receive(EPOCH, 8, &complete_peer(0x042b, 0)).unwrap();
    let mut confirmation = PEER.to_vec();
    confirmation.extend_from_slice(&123_456u32.to_le_bytes());
    let approval = match s.receive(EPOCH, 9, &event(0x33, &confirmation)).unwrap() {
        Notice::NeedConfirmation {
            request,
            numeric_value: 123_456,
        } => request,
        _ => panic!(),
    };
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
    s.approve(approval, 10, true).unwrap();
    accept(&mut s, 0x042c);
    s.receive(EPOCH, 11, &complete_peer(0x042c, 0)).unwrap();
    let mut completed = vec![0];
    completed.extend_from_slice(&PEER);
    s.receive(EPOCH, 12, &event(0x36, &completed)).unwrap();
    let mut key_event = PEER.to_vec();
    key_event.extend_from_slice(&[0x55; 16]);
    key_event.push(4);
    match s.receive(EPOCH, 13, &event(0x18, &key_event)).unwrap() {
        Notice::ProvisionalBond { key, key_type: 4 } => assert_eq!(key.expose(), &[0x55; 16]),
        _ => panic!(),
    }
    assert!(s.ready_connection().is_none());
    ready(&mut s);
}

#[test]
fn exact_command_and_transport_acceptance_are_required() {
    let mut s = session();
    assert_eq!(
        s.receive(EPOCH, 1, &status(0x0405, 0)).err(),
        Some(Error::CommandNotAccepted)
    );
    assert_eq!(
        s.receive(EPOCH, 1, &status(0x0411, 0)).err(),
        Some(Error::UnexpectedCommand)
    );
    let mut output = [0x77; 15];
    assert_eq!(s.prepare_command(&mut output), Err(Error::OutputTooSmall));
    assert_eq!(output, [0x77; 15]);
    accept(&mut s, 0x0405);
    assert_eq!(
        s.receive(EPOCH, 2, &connected(0)).err(),
        Some(Error::UnexpectedEvent)
    );
    s.receive(EPOCH, 2, &status(0x0405, 0)).unwrap();
    assert_eq!(
        s.receive(EPOCH, 2, &status(0x0405, 0)).err(),
        Some(Error::UnexpectedCommand)
    );
}

#[test]
fn foreign_and_stale_traffic_does_not_establish_a_link() {
    let mut s = session();
    accept(&mut s, 0x0405);
    assert_eq!(
        s.receive(EPOCH - 1, 1, &status(0x0405, 0)).err(),
        Some(Error::ForeignEpoch)
    );
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    let mut other = connected(0);
    other[5] ^= 0x80;
    assert_eq!(s.receive(EPOCH, 2, &other).err(), Some(Error::ForeignPeer));
    s.receive(EPOCH, 2, &connected(0)).unwrap();
    accept(&mut s, 0x0411);
    s.receive(EPOCH, 3, &status(0x0411, 0)).unwrap();
    assert_eq!(
        s.receive(EPOCH, 4, &event(6, &[0, 0x43, 0])).err(),
        Some(Error::ForeignHandle)
    );
    assert!(s.ready_connection().is_none());
}

#[test]
fn user_rejection_is_negative_reply_then_owned_disconnect() {
    let mut s = session();
    connect(&mut s);
    let mut p = PEER.to_vec();
    p.extend_from_slice(&0u32.to_le_bytes());
    let request = match s.receive(EPOCH, 4, &event(0x33, &p)).unwrap() {
        Notice::NeedConfirmation { request, .. } => request,
        _ => panic!(),
    };
    s.approve(request, 5, false).unwrap();
    accept(&mut s, 0x042d);
    s.receive(EPOCH, 6, &complete_peer(0x042d, 0)).unwrap();
    assert_eq!(s.failure(), Some(Failure::UserRejected));
    assert_eq!(accept(&mut s, 0x0406), [6, 4, 3, 0x42, 0, 0x13]);
    s.receive(EPOCH, 7, &status(0x0406, 0)).unwrap();
    assert!(matches!(
        s.receive(EPOCH, 8, &event(5, &[0, 0x42, 0, 0x16])).unwrap(),
        Notice::Closed { reason: 0x16 }
    ));
    assert_eq!(s.state(), State::Closed);
    assert!(s.owned_connection().is_none());
}

#[test]
fn legacy_passkey_and_oob_are_explicitly_rejected() {
    for (code, opcode) in [(0x16, 0x040e), (0x34, 0x042f), (0x35, 0x0433)] {
        let mut s = session();
        connect(&mut s);
        assert!(
            matches!(s.receive(EPOCH, 4, &event(code, &PEER)).unwrap(), Notice::PairingRejected { event } if event == code)
        );
        accept(&mut s, opcode);
        s.receive(EPOCH, 5, &complete_peer(opcode, 0)).unwrap();
        assert_eq!(s.state(), State::Disconnecting);
        assert_eq!(s.failure(), Some(Failure::UnsupportedPairing(code)));
    }
}

#[test]
fn cancel_before_any_acceptance_is_local_and_exactly_once() {
    let mut s = session();
    s.cancel(1).unwrap();
    s.cancel(2).unwrap();
    assert_eq!(s.state(), State::Closed);
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
}

#[test]
fn create_cancel_waits_for_status_and_final_connection_event() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.cancel(1).unwrap();
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
    s.receive(EPOCH, 2, &status(0x0405, 0)).unwrap();
    assert_eq!(accept(&mut s, 0x0408), [8, 4, 6, 1, 2, 3, 4, 5, 6]);
    s.receive(EPOCH, 3, &complete_peer(0x0408, 0)).unwrap();
    assert_eq!(s.state(), State::Cancelling);
    assert!(matches!(
        s.receive(EPOCH, 4, &connected(2)).unwrap(),
        Notice::Closed { reason: 2 }
    ));
}

#[test]
fn connection_race_cancels_unsent_cancel_and_disconnects_established_handle() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.cancel(2).unwrap();
    s.receive(EPOCH, 3, &connected(0)).unwrap();
    assert_eq!(s.state(), State::Disconnecting);
    accept(&mut s, 0x0406);
}

#[test]
fn accepted_cancel_race_waits_for_cancel_ack_before_disconnect() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.cancel(2).unwrap();
    accept(&mut s, 0x0408);
    s.receive(EPOCH, 3, &connected(0)).unwrap();
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
    assert!(matches!(
        s.receive(EPOCH, 4, &complete_peer(0x0408, 0x0b)).unwrap(),
        Notice::CancellationRace { status: 0x0b }
    ));
    accept(&mut s, 0x0406);
}

#[test]
fn finite_deadlines_do_not_reset_on_repeated_cancel() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.cancel(10).unwrap();
    s.cancel(49).unwrap();
    assert_eq!(
        s.tick(50),
        Err(Error::Failed(Failure::Timeout(Phase::Cleanup)))
    );
    let mut s = session();
    connect(&mut s);
    assert_eq!(
        s.tick(202),
        Err(Error::Failed(Failure::Timeout(Phase::Security)))
    );
    assert!(s.owned_connection().is_some());
    s.cancel(203).unwrap();
    accept(&mut s, 0x0406);
}

#[test]
fn controller_failures_are_preserved_without_claiming_cleanup() {
    let mut s = session();
    accept(&mut s, 0x0405);
    assert_eq!(
        s.receive(EPOCH, 1, &status(0x0405, 0x0c)).err(),
        Some(Error::Failed(Failure::Controller {
            opcode: 0x0405,
            status: 0x0c
        }))
    );
    s.cancel(2).unwrap();
    assert_eq!(s.state(), State::Closed);
    let mut s = session();
    connect(&mut s);
    assert_eq!(
        s.receive(EPOCH, 4, &event(6, &[5, 0x42, 0])).err(),
        Some(Error::Failed(Failure::ControllerEvent {
            event: 6,
            status: 5
        }))
    );
    assert!(s.owned_connection().is_some());
    assert!(s.ready_connection().is_none());
}

#[test]
fn weak_or_disabled_encryption_and_debug_keys_never_become_ready() {
    let mut s = session();
    connect(&mut s);
    finish_security(&mut s);
    assert_eq!(
        s.receive(EPOCH, 23, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 7]))
            .err(),
        Some(Error::Failed(Failure::WeakKey(7)))
    );
    let mut s = session();
    connect(&mut s);
    ready(&mut s);
    assert_eq!(
        s.receive(EPOCH, 24, &event(8, &[0, 0x42, 0, 0])).err(),
        Some(Error::Failed(Failure::EncryptionDisabled))
    );
    assert!(s.ready_connection().is_none());
    let mut s = session();
    connect(&mut s);
    let mut p = PEER.to_vec();
    p.extend_from_slice(&[0x66; 16]);
    p.push(3);
    assert_eq!(
        s.receive(EPOCH, 4, &event(0x18, &p)).err(),
        Some(Error::Failed(Failure::UnsupportedKeyType(3)))
    );
}

#[test]
fn uncertain_send_blocks_replay_and_preserves_raw_failure() {
    let mut s = session();
    assert_eq!(
        s.fail_transport(bt_core::Error::Backend(-81)),
        Err(Error::Failed(Failure::Transport(bt_core::Error::Backend(
            -81
        ))))
    );
    assert!(s.prepare_command(&mut [0; 25]).is_err());
    assert!(s.cancel(2).is_err());
    assert_eq!(
        s.failure(),
        Some(Failure::Transport(bt_core::Error::Backend(-81)))
    );
}

#[test]
fn malformed_events_and_clock_regression_do_not_advance_protocol() {
    let mut s = session();
    connect(&mut s);
    assert_eq!(
        s.receive(EPOCH, 4, &[0x33, 10, 1]).err(),
        Some(Error::Wire(bt_link::WireError::Framing))
    );
    assert_eq!(s.tick(2), Err(Error::ClockRegression));
    assert_eq!(s.state(), State::Authenticating);
    let mut other = PEER;
    other[0] ^= 0x80;
    assert_eq!(
        s.receive(EPOCH, 5, &event(0x31, &other)).err(),
        Some(Error::ForeignPeer)
    );
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
}

#[test]
fn no_unsolicited_auth_success_after_unapproved_pairing() {
    let mut s = session();
    connect(&mut s);
    s.receive(EPOCH, 4, &event(0x31, &PEER)).unwrap();
    accept(&mut s, 0x042b);
    s.receive(EPOCH, 5, &complete_peer(0x042b, 0)).unwrap();
    assert_eq!(
        s.receive(EPOCH, 6, &event(6, &[0, 0x42, 0])).err(),
        Some(Error::Failed(Failure::UserRejected))
    );
}

#[test]
fn expired_pending_command_cannot_be_sent_and_cleanup_wait_is_finite() {
    let mut s = session();
    assert_eq!(
        s.tick(100),
        Err(Error::Failed(Failure::Timeout(Phase::Connection)))
    );
    assert!(s.prepare_command(&mut [0; 25]).is_err());
    s.cancel(101).unwrap();
    assert_eq!(s.state(), State::Closed);
    let mut s = session();
    accept(&mut s, 0x0405);
    assert_eq!(
        s.tick(100),
        Err(Error::Failed(Failure::Timeout(Phase::Connection)))
    );
    s.cancel(101).unwrap();
    assert_eq!(s.state(), State::Stopping);
    assert_eq!(
        s.tick(141),
        Err(Error::Failed(Failure::Timeout(Phase::Cleanup)))
    );
}

#[test]
fn spontaneous_disconnect_retains_command_correlation_until_ack_or_deadline() {
    let mut s = session();
    connect(&mut s);
    finish_security(&mut s);
    s.receive(EPOCH, 23, &event(5, &[0, 0x42, 0, 0x08]))
        .unwrap();
    assert_eq!(s.state(), State::Stopping);
    assert!(s.has_pending_command());
    assert!(s.ready_connection().is_none());
    assert!(s.owned_connection().is_none());
    s.receive(EPOCH, 24, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
        .unwrap();
    assert_eq!(s.state(), State::Closed);
    assert!(!s.has_pending_command());
    let mut s = session();
    connect(&mut s);
    finish_security(&mut s);
    s.receive(EPOCH, 23, &event(5, &[0, 0x42, 0, 0x08]))
        .unwrap();
    assert_eq!(
        s.tick(63),
        Err(Error::Failed(Failure::Timeout(Phase::Cleanup)))
    );
}

#[test]
fn encryption_change_during_ready_revokes_readiness_and_rechecks_size() {
    let mut s = session();
    connect(&mut s);
    ready(&mut s);
    s.receive(EPOCH, 1000, &event(0x59, &[0, 0x42, 0, 2, 16]))
        .unwrap();
    assert!(s.ready_connection().is_none());
    accept(&mut s, 0x1408);
    assert!(matches!(
        s.receive(EPOCH, 1001, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
            .unwrap(),
        Notice::Ready
    ));
}

#[test]
fn no_authentication_from_unproven_controller_cached_key() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.receive(EPOCH, 2, &connected(0)).unwrap();
    accept(&mut s, 0x0411);
    s.receive(EPOCH, 3, &status(0x0411, 0)).unwrap();
    assert_eq!(
        s.receive(EPOCH, 4, &event(6, &[0, 0x42, 0])).err(),
        Some(Error::Failed(Failure::UnprovenKey))
    );
}

#[test]
fn pairing_event_collision_requires_retention_and_retry_after_owned_ack() {
    let mut s = session();
    connect(&mut s);
    let request = match s.receive(EPOCH, 4, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(request, 5, None).unwrap();
    accept(&mut s, 0x040c);
    let retained = event(0x31, &PEER);
    assert_eq!(
        s.receive(EPOCH, 6, &retained).err(),
        Some(Error::CommandBusy)
    );
    s.receive(EPOCH, 6, &complete_peer(0x040c, 0)).unwrap();
    s.receive(EPOCH, 6, &retained).unwrap();
    accept(&mut s, 0x042b);
}

#[test]
fn fresh_ssp_does_not_reuse_prior_stored_key_authorization() {
    let mut s = session();
    connect(&mut s); // Prior stored key was supplied.
    s.receive(EPOCH, 4, &event(0x31, &PEER)).unwrap();
    accept(&mut s, 0x042b);
    s.receive(EPOCH, 5, &complete_peer(0x042b, 0)).unwrap();
    let mut confirmation = PEER.to_vec();
    confirmation.extend_from_slice(&123_456u32.to_le_bytes());
    let request = match s.receive(EPOCH, 6, &event(0x33, &confirmation)).unwrap() {
        Notice::NeedConfirmation { request, .. } => request,
        _ => panic!(),
    };
    s.approve(request, 7, true).unwrap();
    accept(&mut s, 0x042c);
    s.receive(EPOCH, 8, &complete_peer(0x042c, 0)).unwrap();
    let mut paired = vec![0];
    paired.extend_from_slice(&PEER);
    s.receive(EPOCH, 9, &event(0x36, &paired)).unwrap();
    assert_eq!(
        s.receive(EPOCH, 10, &event(6, &[0, 0x42, 0])).err(),
        Some(Error::Failed(Failure::UnprovenKey))
    );
    assert!(s.ready_connection().is_none());
}

#[test]
fn failed_connection_before_cancel_ack_retains_original_cleanup_deadline() {
    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.cancel(2).unwrap();
    accept(&mut s, 0x0408);
    assert!(matches!(
        s.receive(EPOCH, 3, &connected(2)).unwrap(),
        Notice::Closed { reason: 2 }
    ));
    assert_eq!(s.state(), State::Stopping);
    assert!(s.has_pending_command());
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());
    s.receive(EPOCH, 4, &complete_peer(0x0408, 0)).unwrap();
    assert_eq!(s.state(), State::Closed);
    assert!(!s.has_pending_command());

    let mut s = session();
    accept(&mut s, 0x0405);
    s.receive(EPOCH, 1, &status(0x0405, 0)).unwrap();
    s.cancel(2).unwrap();
    accept(&mut s, 0x0408);
    s.receive(EPOCH, 40, &connected(2)).unwrap();
    assert_eq!(
        s.tick(42),
        Err(Error::Failed(Failure::Timeout(Phase::Cleanup)))
    );
}

#[test]
fn failed_command_complete_still_correlates_peer_and_handle() {
    let mut s = session();
    connect(&mut s);
    let request = match s.receive(EPOCH, 4, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(request, 5, Some(SecretKey::new([0x44; 16])))
        .unwrap();
    accept(&mut s, 0x040b);
    let mut foreign = complete_peer(0x040b, 5);
    foreign[6] ^= 0x80;
    assert_eq!(
        s.receive(EPOCH, 6, &foreign).err(),
        Some(Error::ForeignPeer)
    );
    assert!(s.has_pending_command());
    assert_eq!(s.failure(), None);
    assert_eq!(
        s.receive(EPOCH, 6, &complete_peer(0x040b, 5)).err(),
        Some(Error::Failed(Failure::Controller {
            opcode: 0x040b,
            status: 5
        }))
    );
    assert!(!s.has_pending_command());

    let mut s = session();
    connect(&mut s);
    finish_security(&mut s);
    for p in [
        &[1, 8, 0x14, 0x0c, 0x43, 0][..],
        &[1, 8, 0x14, 0x0c, 0x43, 0, 0xff][..],
    ] {
        assert_eq!(
            s.receive(EPOCH, 23, &event(0x0e, p)).err(),
            Some(Error::ForeignHandle)
        );
        assert!(s.has_pending_command());
        assert_eq!(s.failure(), None);
    }
    // The valid mandatory handle remains; the undefined key size is legally omitted.
    assert_eq!(
        s.receive(EPOCH, 23, &event(0x0e, &[1, 8, 0x14, 0x0c, 0x42, 0]))
            .err(),
        Some(Error::Failed(Failure::Controller {
            opcode: 0x1408,
            status: 0x0c
        }))
    );
    assert!(!s.has_pending_command());
    assert!(s.ready_connection().is_none());
}

#[test]
fn unknown_command_uses_either_ack_kind_without_accepting_wrong_kind_success() {
    let mut s = session();
    accept(&mut s, 0x0405);
    assert!(s.receive(EPOCH, 1, &event(0x0e, &[1, 5, 4, 0])).is_err());
    assert!(s.has_pending_command());
    assert_eq!(
        s.receive(EPOCH, 1, &event(0x0e, &[1, 5, 4, 1])).err(),
        Some(Error::Failed(Failure::Controller {
            opcode: 0x0405,
            status: 1
        }))
    );
    assert!(!s.has_pending_command());
    s.cancel(2).unwrap();
    assert_eq!(s.state(), State::Closed);
    assert!(s.prepare_command(&mut [0; 25]).unwrap().is_none());

    let mut s = session();
    connect(&mut s);
    let request = match s.receive(EPOCH, 4, &event(0x17, &PEER)).unwrap() {
        Notice::NeedKey { request } => request,
        _ => panic!(),
    };
    s.supply_key(request, 5, None).unwrap();
    accept(&mut s, 0x040c);
    assert_eq!(
        s.receive(EPOCH, 6, &status(0x040c, 0)).err(),
        Some(Error::UnexpectedCommand)
    );
    assert!(s.has_pending_command());
    assert_eq!(
        s.receive(EPOCH, 6, &status(0x040b, 1)).err(),
        Some(Error::UnexpectedCommand)
    );
    assert!(s.has_pending_command());
    assert_eq!(
        s.receive(EPOCH, 6, &status(0x040c, 1)).err(),
        Some(Error::Failed(Failure::Controller {
            opcode: 0x040c,
            status: 1
        }))
    );
    assert!(!s.has_pending_command());
    assert!(s.owned_connection().is_some());
    s.cancel(7).unwrap();
    accept(&mut s, 0x0406);
}
