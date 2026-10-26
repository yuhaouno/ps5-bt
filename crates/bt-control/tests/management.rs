// SPDX-License-Identifier: GPL-3.0-or-later
use bt_control::{Capability, CommandOutcome, LinkState, Status, StreamState, management::*};
fn snapshot() -> Snapshot {
    Snapshot {
        status: Status {
            transport: Capability::Ready,
            capture: Capability::Ready,
            codec: Capability::Ready,
            playback: Capability::Ready,
            link: LinkState::Disconnected,
            stream: StreamState::Stopped,
            last_error: None,
            revision: 0,
        },
        owner_phase: OwnerPhase::Idle,
        cleanup_required: false,
        scanning: false,
        selected: None,
        approval: None,
        available: Actions::none()
            .with(Action::Scan)
            .with(Action::CancelScan)
            .with(Action::Select(DeviceId(1)))
            .with(Action::Pair)
            .with(Action::Approve { token: 1 })
            .with(Action::Reject { token: 1 })
            .with(Action::Connect)
            .with(Action::Disconnect)
            .with(Action::Start)
            .with(Action::Stop)
            .with(Action::Forget),
    }
}
fn device() -> Device {
    Device {
        id: DeviceId(1),
        name: Name::new("Headset").unwrap(),
        bonded: Some(false),
        audio_sink: Some(true),
    }
}
#[test]
fn observes_while_owned_without_invalidating_terminal_ticket() {
    let mut m = Manager::<2>::new(7, snapshot()).unwrap();
    let g = m.generation();
    let ticket = m.submit(g, Action::Scan).unwrap();
    assert_eq!(
        m.finish(ticket, CommandOutcome::Success, snapshot()),
        Err(Error::NotClaimed)
    );
    assert_eq!(m.submit(g, Action::Scan), Err(Error::StaleGeneration));
    assert_eq!(m.take_pending(), Some(ticket));
    assert_eq!(m.take_pending(), None);
    let mut next = snapshot();
    next.scanning = true;
    next.owner_phase = OwnerPhase::Scanning;
    m.observe(next, &[device()]).unwrap();
    assert_eq!(m.pending().unwrap().ticket, ticket);
    m.finish(ticket, CommandOutcome::Success, snapshot())
        .unwrap();
    assert_eq!(
        m.finish(ticket, CommandOutcome::Success, snapshot()),
        Err(Error::StaleTicket)
    );
    assert_eq!(m.completion().unwrap().ticket, ticket);
}
#[test]
fn urgent_cancellation_does_not_replace_primary_ownership() {
    let mut m = Manager::<1>::new(7, snapshot()).unwrap();
    let scan = m.submit(m.generation(), Action::Scan).unwrap();
    assert_eq!(m.take_pending(), Some(scan));
    let cancel = m.submit(m.generation(), Action::CancelScan).unwrap();
    assert_eq!(m.pending().unwrap().ticket, scan);
    assert_eq!(m.take_pending(), Some(cancel));
    assert_eq!(m.take_pending(), None);
    assert_eq!(
        m.submit(m.generation(), Action::Disconnect),
        Err(Error::Busy)
    );
    m.finish(scan, CommandOutcome::Cancelled, snapshot())
        .unwrap();
    assert_eq!(m.urgent().unwrap().ticket, cancel);
    m.finish(cancel, CommandOutcome::Success, snapshot())
        .unwrap();
    assert!(m.pending().is_none() && m.urgent().is_none());
}
#[test]
fn gates_capability_selection_approval_and_unknown_bonds() {
    let mut m = Manager::<2>::new(7, snapshot()).unwrap();
    assert_eq!(
        m.submit(m.generation(), Action::Connect),
        Err(Error::Unavailable)
    );
    assert_eq!(
        m.submit(m.generation(), Action::Select(DeviceId(3))),
        Err(Error::Invalid)
    );
    let mut s = snapshot();
    s.selected = Some(DeviceId(1));
    let mut d = device();
    d.bonded = None;
    m.observe(s, &[d]).unwrap();
    assert_eq!(
        m.submit(m.generation(), Action::Pair),
        Err(Error::Unavailable)
    );
    d.bonded = Some(false);
    s.approval = Some(Approval {
        token: 19,
        device: d.id,
        number: Some(123),
    });
    m.observe(s, &[d]).unwrap();
    assert_eq!(
        m.submit(m.generation(), Action::Approve { token: 18 }),
        Err(Error::Invalid)
    );
    let pair = m.submit(m.generation(), Action::Pair).unwrap();
    m.take_pending();
    let reject = m
        .submit(m.generation(), Action::Reject { token: 19 })
        .unwrap();
    m.take_pending();
    m.finish(reject, CommandOutcome::Success, s).unwrap();
    m.finish(pair, CommandOutcome::Cancelled, s).unwrap();
    s.available = Actions::none();
    m.observe(s, &[d]).unwrap();
    assert_eq!(
        m.submit(m.generation(), Action::Scan),
        Err(Error::Unavailable)
    );
}
#[test]
fn bad_snapshots_and_foreign_completions_are_atomic() {
    assert!(Manager::<1>::new(0, snapshot()).is_err());
    assert!(Name::new(&"é".repeat(33)).is_err());
    let mut m = Manager::<1>::new(7, snapshot()).unwrap();
    let generation = m.generation();
    assert_eq!(
        m.observe(snapshot(), &[device(), device()]),
        Err(Error::Capacity)
    );
    let mut s = snapshot();
    s.selected = Some(DeviceId(2));
    assert_eq!(m.observe(s, &[device()]), Err(Error::Invalid));
    assert_eq!(m.generation(), generation);
    let ticket = m.submit(m.generation(), Action::Scan).unwrap();
    m.take_pending();
    let mut foreign = ticket;
    foreign.generation.session += 1;
    assert_eq!(
        m.finish(foreign, CommandOutcome::Success, snapshot()),
        Err(Error::StaleTicket)
    );
    assert_eq!(
        m.finish(ticket, CommandOutcome::Success, s),
        Err(Error::Invalid)
    );
    assert_eq!(m.pending().unwrap().ticket, ticket);
    m.finish(ticket, CommandOutcome::Failed(-19), snapshot())
        .unwrap();
    assert_eq!(m.snapshot().status.last_error, Some(-19));
}

#[test]
fn unchanged_poll_does_not_make_ui_stale_and_connect_ssp_is_approvable() {
    let mut m = Manager::<1>::new(7, snapshot()).unwrap();
    let g = m.generation();
    m.observe(snapshot(), &[]).unwrap();
    assert_eq!(m.generation(), g);
    let mut s = snapshot();
    s.selected = Some(DeviceId(1));
    m.observe(s, &[device()]).unwrap();
    let connect = m.submit(m.generation(), Action::Connect).unwrap();
    m.take_pending();
    s.approval = Some(Approval {
        token: 99,
        device: DeviceId(1),
        number: Some(123456),
    });
    s.owner_phase = OwnerPhase::Pairing;
    m.observe(s, &[device()]).unwrap();
    let approve = m
        .submit(m.generation(), Action::Approve { token: 99 })
        .unwrap();
    assert_eq!(m.take_pending(), Some(approve));
    s.approval = None;
    m.finish(approve, CommandOutcome::Success, s).unwrap();
    assert_eq!(m.pending().unwrap().ticket, connect);
    m.finish(connect, CommandOutcome::Success, s).unwrap();
}
