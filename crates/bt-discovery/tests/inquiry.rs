// SPDX-License-Identifier: GPL-3.0-or-later
use bt_discovery::{
    Config, DeviceId, Error, Failure, Inquiry, Notice, ResultSlot, State, StopReason, WireError,
};

const EPOCH: u64 = 7;
fn config() -> Config {
    Config {
        inquiry_length: 1,
        max_responses: 8,
        scan_timeout_us: 100,
        cleanup_timeout_us: 20,
        max_result_events: 8,
    }
}
fn event(code: u8, p: &[u8]) -> Vec<u8> {
    let mut out = vec![code, p.len() as u8];
    out.extend_from_slice(p);
    out
}
fn status(op: u16, code: u8) -> Vec<u8> {
    let [lo, hi] = op.to_le_bytes();
    event(0x0f, &[code, 1, lo, hi])
}
fn complete(op: u16, code: u8) -> Vec<u8> {
    let [lo, hi] = op.to_le_bytes();
    event(0x0e, &[1, lo, hi, code])
}
fn result(seed: u8, rep: u8) -> [u8; 14] {
    [seed, 2, 3, 4, 5, 6, rep, 0, 0, 0x04, 0x24, 0, 0x34, 0x12]
}
fn results(records: &[[u8; 14]]) -> Vec<u8> {
    let mut p = vec![records.len() as u8];
    for r in records {
        p.extend_from_slice(r);
    }
    event(2, &p)
}
fn accept(q: &mut Inquiry<'_>, now: u64, op: u16) -> Vec<u8> {
    let mut out = [0xa5; 8];
    let ticket = q.prepare_command(now, &mut out).unwrap().unwrap();
    assert_eq!(ticket.opcode(), op);
    let mut retry = [0xa5; 8];
    assert_eq!(q.prepare_command(now, &mut retry).unwrap(), Some(ticket));
    assert_eq!(out, retry);
    q.confirm_accepted(ticket).unwrap();
    assert_eq!(q.confirm_accepted(ticket), Err(Error::WrongTicket));
    assert!(q.prepare_command(now, &mut retry).unwrap().is_none());
    out[..ticket.len()].to_vec()
}
fn scanning(q: &mut Inquiry<'_>) {
    assert_eq!(accept(q, 0, 0x0401), [1, 4, 5, 0x33, 0x8b, 0x9e, 1, 8]);
    q.receive(EPOCH, 1, &status(0x0401, 0)).unwrap();
}

#[test]
fn real_inquiry_lifecycle_deduplicates_stable_ids_and_gates_selection() {
    let mut storage = [ResultSlot::empty(); 2];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    assert_eq!(
        q.receive(EPOCH, 2, &results(&[result(1, 0), result(9, 2)])),
        Ok(Notice::Results {
            added: 2,
            updated: 0,
            unretained: 0,
            list_full: true
        })
    );
    let first = *q.results().next().unwrap();
    assert_eq!(first.id.parts(), (EPOCH, 0));
    assert_eq!(first.peer.hci_bytes(), [1, 2, 3, 4, 5, 6]);
    assert_eq!(first.page_scan_repetition, 0);
    assert!(matches!(
        q.connection_candidate(first.id),
        Err(Error::NotQuiescent)
    ));
    q.receive(EPOCH, 3, &results(&[result(1, 1)])).unwrap();
    assert_eq!(q.get(first.id).unwrap().page_scan_repetition, 1);
    assert_eq!(q.get(first.id).unwrap().reports, 2);
    assert_eq!(
        q.receive(EPOCH, 4, &event(1, &[0])),
        Ok(Notice::InquiryComplete { status: 0 })
    );
    assert_eq!(q.snapshot().state, State::Complete);
    assert!(q.is_quiescent());
    assert_eq!(
        q.connection_candidate(first.id)
            .unwrap()
            .page_scan_repetition,
        1
    );
    assert!(q.snapshot().list_full);
    assert!(!q.snapshot().truncated);
}
#[test]
fn full_list_reports_unretained_reports_and_keeps_existing_identity() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    q.receive(EPOCH, 2, &results(&[result(1, 0)])).unwrap();
    let id = q.results().next().unwrap().id;
    assert_eq!(
        q.receive(EPOCH, 3, &results(&[result(9, 1), result(1, 2)])),
        Ok(Notice::Results {
            added: 0,
            updated: 1,
            unretained: 1,
            list_full: true
        })
    );
    assert_eq!(q.snapshot().unretained_reports, 1);
    assert!(q.snapshot().truncated);
    assert_eq!(q.get(id).unwrap().peer.hci_bytes()[0], 1);
    assert_eq!(q.get(id).unwrap().page_scan_repetition, 2);
    assert!(q.get(DeviceId::from_parts(EPOCH, 1)).is_err());
}
#[test]
fn cancellation_waits_for_start_status_then_finishes_without_inquiry_complete() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    accept(&mut q, 0, 0x0401);
    q.cancel(1).unwrap();
    assert!(q.prepare_command(1, &mut [0; 8]).unwrap().is_none());
    q.receive(EPOCH, 2, &status(0x0401, 0)).unwrap();
    assert_eq!(accept(&mut q, 3, 0x0402), [2, 4, 0]);
    assert_eq!(
        q.receive(EPOCH, 4, &complete(0x0402, 0)),
        Ok(Notice::Cancelled)
    );
    assert_eq!(q.snapshot().state, State::Cancelled);
    assert!(q.is_quiescent());
    assert!(q.snapshot().truncated);
    assert_eq!(
        q.receive(EPOCH, 5, &event(1, &[0])),
        Err(Error::UnexpectedEvent)
    );
}
#[test]
fn natural_complete_discards_unsent_cancel_but_waits_for_accepted_cancel_ack() {
    for accepted in [false, true] {
        let mut storage = [ResultSlot::empty(); 1];
        let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
        scanning(&mut q);
        q.cancel(2).unwrap();
        if accepted {
            accept(&mut q, 3, 0x0402);
        }
        q.receive(EPOCH, 4, &event(1, &[0])).unwrap();
        if accepted {
            assert_eq!(q.snapshot().state, State::Stopping);
            assert!(!q.is_quiescent());
            assert_eq!(
                q.receive(EPOCH, 5, &complete(0x0402, 0x0c)),
                Ok(Notice::CancelRace { status: 0x0c })
            );
        }
        assert_eq!(q.snapshot().state, State::Complete);
        assert!(q.is_quiescent());
        assert!(q.prepare_command(5, &mut [0; 8]).unwrap().is_none());
    }
}
#[test]
fn scan_deadline_requests_cancel_cleanup_deadline_does_not_extend() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    assert_eq!(
        q.tick(100),
        Ok(Notice::StopRequested(StopReason::ScanDeadline))
    );
    assert_eq!(q.snapshot().deadline_us, 120);
    q.cancel(110).unwrap();
    assert_eq!(q.snapshot().deadline_us, 120);
    accept(&mut q, 111, 0x0402);
    assert_eq!(q.tick(120), Err(Error::Failed(Failure::CleanupDeadline)));
    assert!(q.snapshot().cleanup_required);
    // A late exact completion may establish quiescence but cannot erase failure.
    q.receive(EPOCH, 121, &complete(0x0402, 0)).unwrap();
    assert!(q.is_quiescent());
    assert_eq!(q.snapshot().failure, Some(Failure::CleanupDeadline));
}
#[test]
fn event_budget_cancels_and_suppresses_additional_reports_without_storage_growth() {
    let mut storage = [ResultSlot::empty(); 4];
    let mut q = Inquiry::new(
        EPOCH,
        0,
        Config {
            max_result_events: 1,
            ..config()
        },
        &mut storage,
    )
    .unwrap();
    scanning(&mut q);
    q.receive(EPOCH, 2, &results(&[result(1, 0)])).unwrap();
    assert_eq!(q.snapshot().stop_reason, Some(StopReason::EventBudget));
    assert_eq!(q.snapshot().stored_results, 1);
    assert_eq!(
        q.receive(EPOCH, 3, &results(&[result(9, 0)])),
        Ok(Notice::Results {
            added: 0,
            updated: 0,
            unretained: 1,
            list_full: false
        })
    );
    assert_eq!(q.snapshot().result_events, 1);
    assert_eq!(q.snapshot().unretained_reports, 1);
    accept(&mut q, 4, 0x0402);
    q.receive(EPOCH, 5, &complete(0x0402, 0)).unwrap();
    assert!(q.is_quiescent());
    assert!(q.snapshot().truncated);
}
#[test]
fn local_cancel_and_expiry_before_acceptance_do_not_emit_radio_commands() {
    for expired in [false, true] {
        let mut storage = [ResultSlot::empty(); 1];
        let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
        if expired {
            q.tick(100).unwrap();
        } else {
            q.cancel(1).unwrap();
        }
        assert!(q.is_quiescent());
        assert_eq!(q.snapshot().state, State::Cancelled);
        assert!(q.prepare_command(100, &mut [0; 8]).unwrap().is_none());
    }
}
#[test]
fn malformed_suffix_and_foreign_events_do_not_modify_results_or_correlation() {
    let mut storage = [ResultSlot::empty(); 2];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    let before = q.snapshot();
    let mut bad = result(9, 3);
    bad[6] = 3;
    assert_eq!(
        q.receive(EPOCH, 2, &results(&[result(1, 0), bad])),
        Err(Error::Wire(WireError::InvalidPageScanRepetition))
    );
    assert_eq!(q.snapshot(), before);
    assert_eq!(
        q.receive(EPOCH - 1, 2, &results(&[result(1, 0)])),
        Err(Error::ForeignEpoch)
    );
    assert_eq!(
        q.receive(EPOCH, 2, &event(5, &[0, 1, 0, 0x13])),
        Err(Error::Wire(WireError::ForeignEvent(5)))
    );
    assert_eq!(
        q.receive(
            EPOCH,
            2,
            &event(0x0e, &[1, 5, 0x10, 0, 1, 2, 3, 4, 5, 6, 7])
        ),
        Err(Error::Wire(WireError::ForeignOpcode(0x1005)))
    );
    assert_eq!(q.snapshot(), before);
}
#[test]
fn acceptance_and_ack_kind_are_exact_with_unknown_command_exception() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    assert_eq!(
        q.receive(EPOCH, 0, &status(0x0401, 0)),
        Err(Error::CommandNotAccepted)
    );
    let mut out = [0xa5; 7];
    assert_eq!(q.prepare_command(0, &mut out), Err(Error::BufferTooSmall));
    assert_eq!(out, [0xa5; 7]);
    accept(&mut q, 0, 0x0401);
    assert_eq!(
        q.receive(EPOCH, 1, &complete(0x0401, 0)),
        Err(Error::UnexpectedCommand)
    );
    assert_eq!(
        q.receive(EPOCH, 1, &complete(0x0401, 1)),
        Err(Error::Failed(Failure::Controller {
            opcode: 0x0401,
            status: 1
        }))
    );
    assert!(q.is_quiescent());
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    q.cancel(2).unwrap();
    accept(&mut q, 3, 0x0402);
    assert_eq!(
        q.receive(EPOCH, 4, &status(0x0402, 0)),
        Err(Error::UnexpectedCommand)
    );
    assert_eq!(
        q.receive(EPOCH, 4, &status(0x0402, 1)),
        Err(Error::Failed(Failure::Controller {
            opcode: 0x0402,
            status: 1
        }))
    );
    assert!(!q.is_quiescent());
}
#[test]
fn reset_requires_quiescence_and_new_epoch_invalidates_ids_and_command_tickets() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    let ticket = q.prepare_command(0, &mut [0; 8]).unwrap().unwrap();
    assert_eq!(q.reset(EPOCH + 1, 1, config()), Err(Error::NotQuiescent));
    q.confirm_accepted(ticket).unwrap();
    q.receive(EPOCH, 1, &status(0x0401, 0)).unwrap();
    q.receive(EPOCH, 2, &results(&[result(1, 0)])).unwrap();
    let id = q.results().next().unwrap().id;
    q.receive(EPOCH, 3, &event(1, &[0])).unwrap();
    assert_eq!(q.reset(EPOCH, 4, config()), Err(Error::EpochNotAdvanced));
    q.reset(EPOCH + 1, 4, config()).unwrap();
    assert_eq!(q.get(id).err(), Some(Error::StaleId));
    assert_eq!(q.confirm_accepted(ticket), Err(Error::WrongTicket));
    assert_eq!(
        q.receive(EPOCH, 5, &event(1, &[0])),
        Err(Error::ForeignEpoch)
    );
}
#[test]
fn clock_failure_never_restores_results_and_uncertainty_never_authorizes_cancel_replay() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    assert_eq!(q.tick(0), Err(Error::Failed(Failure::ClockRegressed)));
    assert!(q.snapshot().cleanup_required);
    assert!(q.receive(EPOCH, 2, &results(&[result(1, 0)])).is_err());
    q.cancel(2).unwrap();
    accept(&mut q, 3, 0x0402);
    q.receive(EPOCH, 4, &complete(0x0402, 0)).unwrap();
    assert!(q.is_quiescent());
    assert!(
        q.connection_candidate(DeviceId::from_parts(EPOCH, 0))
            .is_err()
    );
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    assert_eq!(
        q.fail_transport(bt_core::Error::Backend(-8)),
        Err(Error::Failed(Failure::Transport(bt_core::Error::Backend(
            -8
        ))))
    );
    assert!(q.cancel(1).is_err());
    assert!(q.prepare_command(1, &mut [0; 8]).is_err());
    assert!(!q.is_quiescent());
    assert_eq!(q.reset(EPOCH + 1, 2, config()), Err(Error::NotQuiescent));
}
#[test]
fn failure_statuses_remain_visible_and_no_selection_claims_success() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 0, config(), &mut storage).unwrap();
    scanning(&mut q);
    q.receive(EPOCH, 2, &results(&[result(1, 0)])).unwrap();
    let id = q.results().next().unwrap().id;
    assert_eq!(
        q.receive(EPOCH, 3, &event(1, &[4])),
        Ok(Notice::InquiryComplete { status: 4 })
    );
    assert!(q.is_quiescent());
    assert_eq!(
        q.connection_candidate(id).err(),
        Some(Error::Failed(Failure::InquiryComplete(4)))
    );
}

#[test]
fn never_accepted_scan_clock_failure_can_clean_locally_without_erasing_failure() {
    let mut storage = [ResultSlot::empty(); 1];
    let mut q = Inquiry::new(EPOCH, 10, config(), &mut storage).unwrap();
    assert_eq!(q.tick(9), Err(Error::Failed(Failure::ClockRegressed)));
    q.cancel(10).unwrap();
    assert!(q.is_quiescent());
    assert_eq!(q.snapshot().state, State::Failed);
    assert_eq!(q.snapshot().failure, Some(Failure::ClockRegressed));
}
