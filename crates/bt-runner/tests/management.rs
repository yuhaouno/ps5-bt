// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_control::management::{Action, OwnerPhase};
use bt_control::{Capability, CommandOutcome};
use bt_discovery::ResultSlot;
use bt_runner::{Capabilities, CommandSink, Config, Route, Runner};
use common::{Buffers, app, event, recovery};

fn config() -> Config {
    Config {
        session: 77,
        first_scan_epoch: 1,
        first_link_epoch: 8,
        result_valid_us: 1000,
        inquiry: bt_discovery::Config {
            inquiry_length: 1,
            max_responses: 8,
            scan_timeout_us: 100,
            cleanup_timeout_us: 20,
            max_result_events: 8,
        },
    }
}
fn caps() -> Capabilities {
    Capabilities {
        transport: Capability::Ready,
        discovery: Capability::Ready,
        capture: Capability::Unavailable,
        codec: Capability::Ready,
        playback: Capability::Unavailable,
    }
}
#[derive(Default)]
struct Commands {
    sent: Vec<(u64, Vec<u8>)>,
    busy: bool,
}
impl CommandSink for Commands {
    fn send_owned_command(&mut self, epoch: u64, p: &[u8]) -> Result<(), bt_core::Error> {
        if self.busy {
            self.busy = false;
            return Err(bt_core::Error::Busy);
        }
        self.sent.push((epoch, p.to_vec()));
        Ok(())
    }
}
fn submit<const N: usize>(r: &mut Runner<'_, N>, action: Action) {
    let gen_ = r.manager().generation();
    r.controls().1.submit(gen_, action).unwrap();
}
fn scan<S: bt_device::BondStore>(r: &mut Runner<'_, 2>, a: &mut bt_app::Application<'_, S>) {
    r.step(a, 0).unwrap();
    submit(r, Action::Scan);
    r.step(a, 0).unwrap();
    assert!(
        !r.manager()
            .can_submit(Action::Select(bt_control::management::DeviceId(1)))
    );
    r.pump_discovery(a, 0, &mut Commands::default(), &mut [0; 32])
        .unwrap();
    let epoch = r.scan_epoch();
    r.receive_hci(a, Route::Discovery, epoch, 1, &event(0x0f, &[0, 1, 1, 4]))
        .unwrap();
    r.receive_hci(
        a,
        Route::Discovery,
        epoch,
        2,
        &event(2, &[1, 1, 2, 3, 4, 5, 6, 2, 0, 0, 4, 0x24, 0, 0x34, 0x12]),
    )
    .unwrap();
    r.receive_hci(a, Route::Discovery, epoch, 3, &event(1, &[0]))
        .unwrap();
    r.step(a, 3).unwrap();
}
#[test]
fn unavailable_platform_exposes_status_but_no_scan_connect_or_playback() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), Capabilities::default(), &mut slots, 0).unwrap();
    r.step(&mut a, 0).unwrap();
    assert!(!r.manager().can_submit(Action::Scan));
    assert!(!r.manager().can_submit(Action::Start));
    assert_eq!(
        r.manager().snapshot().status.transport,
        Capability::Unavailable
    );
}
#[test]
fn authenticated_selection_maps_to_real_peer_and_observed_page_repetition() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let device = *r.manager().devices().next().unwrap();
    assert_eq!(device.bonded, Some(true));
    assert_eq!(device.audio_sink, None);
    assert_eq!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Success
    );
    submit(&mut r, Action::Select(device.id));
    r.step(&mut a, 3).unwrap();
    submit(&mut r, Action::Connect);
    r.step(&mut a, 3).unwrap();
    let mut command = Commands::default();
    let mut scratch = [0xa5; 64];
    r.pump_application_commands(&mut a, 3, &mut command, &mut scratch)
        .unwrap();
    assert_eq!(command.sent.len(), 1);
    assert_eq!(&command.sent[0].1[3..9], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(command.sent[0].1[11], 2);
    assert_eq!(scratch, [0; 64]);
    assert_eq!(r.active_link_epoch(), Some(9));
    assert_eq!(r.manager().snapshot().owner_phase, OwnerPhase::Connecting);
    assert!(r.manager().pending().is_some());
}
#[test]
fn cancel_search_keeps_both_tickets_until_real_cancel_acknowledgement() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    r.step(&mut a, 0).unwrap();
    submit(&mut r, Action::Scan);
    r.step(&mut a, 0).unwrap();
    r.pump_discovery(&mut a, 0, &mut Commands::default(), &mut [0; 32])
        .unwrap();
    let epoch = r.scan_epoch();
    r.receive_hci(
        &mut a,
        Route::Discovery,
        epoch,
        1,
        &event(0x0f, &[0, 1, 1, 4]),
    )
    .unwrap();
    submit(&mut r, Action::CancelScan);
    r.step(&mut a, 2).unwrap();
    assert!(r.manager().pending().is_some());
    assert!(r.manager().urgent().is_some());
    r.pump_discovery(&mut a, 2, &mut Commands::default(), &mut [0; 32])
        .unwrap();
    r.receive_hci(
        &mut a,
        Route::Discovery,
        epoch,
        3,
        &event(0x0e, &[1, 2, 4, 0]),
    )
    .unwrap();
    assert!(r.manager().pending().is_none());
    assert!(r.manager().urgent().is_none());
    assert!(!r.manager().snapshot().scanning);
}
#[test]
fn polling_is_stable_and_new_scan_rejects_old_selection_ids() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let old = r.manager().devices().next().unwrap().id;
    let generation = r.manager().generation();
    r.step(&mut a, 3).unwrap();
    assert_eq!(r.manager().generation(), generation);
    submit(&mut r, Action::Scan);
    r.step(&mut a, 4).unwrap();
    assert!(!r.manager().can_submit(Action::Select(old)));
    assert!(r.manager().devices().next().is_none());
}
#[test]
fn expired_device_and_unproven_audio_cannot_enable_connection_or_start() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let id = r.manager().devices().next().unwrap().id;
    submit(&mut r, Action::Select(id));
    r.step(&mut a, 3).unwrap();
    r.step(&mut a, 1003).unwrap();
    assert!(!r.manager().can_submit(Action::Connect));
    assert!(!r.manager().can_submit(Action::Start));
}
#[test]
fn never_connected_bond_can_be_forgotten_without_false_transport_failure() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let id = r.manager().devices().next().unwrap().id;
    submit(&mut r, Action::Select(id));
    r.step(&mut a, 3).unwrap();
    submit(&mut r, Action::Forget);
    r.step(&mut a, 3).unwrap();
    assert_eq!(a.status().phase, bt_app::Phase::Idle);
    assert!(!a.status().cleanup_required);
    assert_eq!(r.manager().devices().next().unwrap().bonded, Some(false));
    assert_eq!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Success
    );
}
#[test]
fn scan_cleanup_timeout_completes_failure_and_preserves_uncertainty() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    r.step(&mut a, 0).unwrap();
    submit(&mut r, Action::Scan);
    r.step(&mut a, 0).unwrap();
    r.pump_discovery(&mut a, 0, &mut Commands::default(), &mut [0; 32])
        .unwrap();
    r.step(&mut a, 100).unwrap();
    r.step(&mut a, 120).unwrap();
    assert_eq!(
        r.manager().snapshot().owner_phase,
        OwnerPhase::RequiresCleanup
    );
    assert!(r.manager().snapshot().cleanup_required);
    assert!(r.manager().pending().is_none());
    assert!(matches!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Failed(_)
    ));
}

#[test]
fn expired_selected_identity_can_still_be_forgotten() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let id = r.manager().devices().next().unwrap().id;
    submit(&mut r, Action::Select(id));
    r.step(&mut a, 3).unwrap();
    r.step(&mut a, 1003).unwrap();
    assert!(!r.manager().can_submit(Action::Connect));
    assert!(r.manager().can_submit(Action::Forget));
    submit(&mut r, Action::Forget);
    r.step(&mut a, 1003).unwrap();
    assert_eq!(r.manager().devices().next().unwrap().bonded, Some(false));
    assert_eq!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Success
    );
    assert_eq!(a.status().phase, bt_app::Phase::Idle);
}

#[test]
fn lookup_failure_is_retained_without_claiming_an_absent_bond() {
    struct BrokenStore;
    impl bt_device::BondStore for BrokenStore {
        fn load(
            &mut self,
            _: bt_link::Peer,
            _: &mut [u8; bt_device::BOND_RECORD_BYTES],
        ) -> Result<Option<usize>, bt_device::StoreError> {
            Err(bt_device::StoreError::Backend(-73))
        }
        fn save_atomic(
            &mut self,
            _: bt_link::Peer,
            _: &[u8; bt_device::BOND_RECORD_BYTES],
        ) -> Result<(), bt_device::StoreError> {
            Err(bt_device::StoreError::Unavailable)
        }
        fn delete_atomic(&mut self, _: bt_link::Peer) -> Result<(), bt_device::StoreError> {
            Err(bt_device::StoreError::Unavailable)
        }
    }
    let mut recovery = recovery();
    let (mut tx, mut sdp, mut media, mut avdtp) = ([0; 512], [0; 512], [0; 512], [0; 512]);
    let device = bt_device::DeviceManager::new(
        BrokenStore,
        bt_device::BondPolicy {
            minimum_key_size: 16,
            require_mitm: false,
            require_secure_connections: false,
        },
    )
    .unwrap();
    let mut a = bt_app::Application::new(
        device,
        &mut recovery,
        common::config(),
        bt_app::Storage {
            channel_tx: &mut tx,
            sdp: &mut sdp,
            media_pdu: &mut media,
            avdtp: &mut avdtp,
        },
        0,
    )
    .unwrap();
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let d = *r.manager().devices().next().unwrap();
    assert_eq!(d.bonded, None);
    assert_eq!(d.audio_sink, None);
    assert_eq!(
        r.lookup_failure(),
        Some(bt_runner::LookupFailure {
            device: d.id,
            error: bt_app::Error::Device(bt_device::Error::Storage(bt_device::StorageFailure {
                operation: bt_device::StoreOperation::Load,
                source: bt_device::StoreError::Backend(-73),
            })),
        })
    );
    assert_eq!(r.manager().snapshot().status.last_error, Some(-106));
    let retained = r.lookup_failure();
    r.step(&mut a, 4).unwrap();
    assert_eq!(r.lookup_failure(), retained);
    assert!(!r.manager().can_submit(Action::Pair));
}

#[test]
fn clock_regression_publishes_terminal_state_for_claimed_and_unstarted_work() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    r.step(&mut a, 0).unwrap();
    submit(&mut r, Action::Scan);
    r.step(&mut a, 0).unwrap();
    r.pump_discovery(&mut a, 0, &mut Commands::default(), &mut [0; 32])
        .unwrap();
    let epoch = r.scan_epoch();
    r.receive_hci(
        &mut a,
        Route::Discovery,
        epoch,
        1,
        &event(0x0f, &[0, 1, 1, 4]),
    )
    .unwrap();
    submit(&mut r, Action::CancelScan); // urgent ticket has not been claimed
    assert!(r.manager().pending().unwrap().claimed);
    assert!(!r.manager().urgent().unwrap().claimed);
    assert_eq!(r.step(&mut a, 0), Err(bt_runner::Error::ClockRegressed));
    let s = r.manager().snapshot();
    assert_eq!(s.owner_phase, OwnerPhase::RequiresCleanup);
    assert!(s.cleanup_required);
    assert_eq!(s.status.last_error, Some(-100));
    assert_eq!(s.status.transport, Capability::Unavailable);
    assert!(!s.scanning);
    assert!(s.approval.is_none());
    assert!(r.manager().pending().is_none() && r.manager().urgent().is_none());
    assert_eq!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Failed(-100)
    );
    assert!(!r.manager().can_submit(Action::Scan));
    assert_eq!(
        r.controls().0.submit(bt_control::Command::Connect),
        Err(bt_control::SubmitError::Unavailable)
    );
    let generation = r.manager().generation();
    assert_eq!(r.step(&mut a, 1000), Err(bt_runner::Error::ClockRegressed));
    assert_eq!(r.manager().generation(), generation);
    assert!(r.inquiry_snapshot().cleanup_required);
}

#[test]
fn clock_regression_finishes_legacy_ownership_before_disabling_it() {
    let mut b = Buffers::default();
    let mut recovery = recovery();
    let mut a = app(&mut b, &mut recovery);
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let id = r.manager().devices().next().unwrap().id;
    submit(&mut r, Action::Select(id));
    r.step(&mut a, 3).unwrap();
    r.controls().0.submit(bt_control::Command::Connect).unwrap();
    r.step(&mut a, 3).unwrap();
    assert!(r.controls().0.pending().is_some());
    assert_eq!(r.step(&mut a, 2), Err(bt_runner::Error::ClockRegressed));
    let legacy = r.controls().0;
    assert!(legacy.pending().is_none());
    assert_eq!(legacy.status().last_error, Some(-100));
    assert_eq!(legacy.status().transport, Capability::Unavailable);
    assert_eq!(
        legacy.submit(bt_control::Command::Connect),
        Err(bt_control::SubmitError::Unavailable)
    );
}

#[test]
fn selected_unbonded_connection_uses_current_opaque_ssp_approval_without_finishing_early() {
    struct AbsentStore;
    impl bt_device::BondStore for AbsentStore {
        fn load(
            &mut self,
            _: bt_link::Peer,
            _: &mut [u8; bt_device::BOND_RECORD_BYTES],
        ) -> Result<Option<usize>, bt_device::StoreError> {
            Ok(None)
        }
        fn save_atomic(
            &mut self,
            _: bt_link::Peer,
            _: &[u8; bt_device::BOND_RECORD_BYTES],
        ) -> Result<(), bt_device::StoreError> {
            Ok(())
        }
        fn delete_atomic(&mut self, _: bt_link::Peer) -> Result<(), bt_device::StoreError> {
            Ok(())
        }
    }
    fn command<S: bt_device::BondStore>(
        r: &mut Runner<'_, 2>,
        a: &mut bt_app::Application<'_, S>,
        opcode: u16,
    ) {
        let mut commands = Commands::default();
        assert!(
            r.pump_application_commands(a, 4, &mut commands, &mut [0; 64])
                .unwrap()
        );
        assert_eq!(commands.sent.len(), 1);
        assert_eq!(&commands.sent[0].1[..2], &opcode.to_le_bytes());
    }
    fn receive<S: bt_device::BondStore>(
        r: &mut Runner<'_, 2>,
        a: &mut bt_app::Application<'_, S>,
        code: u8,
        params: &[u8],
    ) {
        r.receive_hci(
            a,
            Route::Application,
            r.active_link_epoch().unwrap(),
            4,
            &event(code, params),
        )
        .unwrap();
    }
    fn complete_peer<S: bt_device::BondStore>(
        r: &mut Runner<'_, 2>,
        a: &mut bt_app::Application<'_, S>,
        opcode: u16,
    ) {
        let mut p = vec![1];
        p.extend_from_slice(&opcode.to_le_bytes());
        p.push(0);
        p.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        receive(r, a, 0x0e, &p);
    }
    let mut recovery = recovery();
    let (mut tx, mut sdp, mut media, mut avdtp) = ([0; 512], [0; 512], [0; 512], [0; 512]);
    let device = bt_device::DeviceManager::new(
        AbsentStore,
        bt_device::BondPolicy {
            minimum_key_size: 16,
            require_mitm: false,
            require_secure_connections: false,
        },
    )
    .unwrap();
    let mut a = bt_app::Application::new(
        device,
        &mut recovery,
        common::config(),
        bt_app::Storage {
            channel_tx: &mut tx,
            sdp: &mut sdp,
            media_pdu: &mut media,
            avdtp: &mut avdtp,
        },
        0,
    )
    .unwrap();
    let mut slots = [ResultSlot::empty(); 2];
    let mut r = Runner::<2>::new(config(), caps(), &mut slots, 0).unwrap();
    scan(&mut r, &mut a);
    let d = *r.manager().devices().next().unwrap();
    assert_eq!(d.bonded, Some(false));
    submit(&mut r, Action::Select(d.id));
    r.step(&mut a, 3).unwrap();
    submit(&mut r, Action::Connect);
    let primary = r.manager().pending().unwrap().ticket;
    r.step(&mut a, 4).unwrap();
    command(&mut r, &mut a, 0x0405);
    receive(&mut r, &mut a, 0x0f, &[0, 1, 5, 4]);
    receive(&mut r, &mut a, 3, &[0, 0x42, 0, 1, 2, 3, 4, 5, 6, 1, 0]);
    command(&mut r, &mut a, 0x0411);
    receive(&mut r, &mut a, 0x0f, &[0, 1, 0x11, 4]);
    receive(&mut r, &mut a, 0x17, &[1, 2, 3, 4, 5, 6]);
    command(&mut r, &mut a, 0x040c); // confirmed missing local bond
    complete_peer(&mut r, &mut a, 0x040c);
    receive(&mut r, &mut a, 0x31, &[1, 2, 3, 4, 5, 6]);
    command(&mut r, &mut a, 0x042b);
    complete_peer(&mut r, &mut a, 0x042b);
    let mut prompt = vec![1, 2, 3, 4, 5, 6];
    prompt.extend_from_slice(&123456u32.to_le_bytes());
    receive(&mut r, &mut a, 0x33, &prompt);
    assert!(a.pending_confirmation().is_some());
    let approval = r.manager().snapshot().approval.unwrap();
    assert_eq!(approval.device, d.id);
    assert_eq!(approval.number, Some(123456));
    assert_eq!(r.manager().pending().unwrap().ticket, primary);
    assert_eq!(r.manager().snapshot().owner_phase, OwnerPhase::Pairing);
    assert!(
        !r.pump_application_commands(&mut a, 4, &mut Commands::default(), &mut [0; 64])
            .unwrap()
    );
    let prompt_generation = r.manager().generation();
    assert_eq!(
        r.controls().1.submit(
            prompt_generation,
            Action::Approve {
                token: approval.token + 1
            }
        ),
        Err(bt_control::management::Error::Invalid)
    );
    let before_approval = r.manager().generation();
    submit(
        &mut r,
        Action::Approve {
            token: approval.token,
        },
    );
    let decision = r.manager().urgent().unwrap().ticket;
    r.step(&mut a, 4).unwrap();
    assert!(r.manager().snapshot().approval.is_none());
    assert!(a.pending_confirmation().is_none());
    assert_eq!(r.manager().completion().unwrap().ticket, decision);
    assert_eq!(
        r.manager().completion().unwrap().outcome,
        CommandOutcome::Success
    );
    assert_eq!(r.manager().pending().unwrap().ticket, primary);
    let current = r.manager().generation();
    assert_eq!(
        r.controls().1.submit(
            before_approval,
            Action::Approve {
                token: approval.token
            }
        ),
        Err(bt_control::management::Error::StaleGeneration)
    );
    assert_eq!(
        r.controls().1.submit(
            current,
            Action::Approve {
                token: approval.token
            }
        ),
        Err(bt_control::management::Error::Unavailable)
    );
    command(&mut r, &mut a, 0x042c);
    complete_peer(&mut r, &mut a, 0x042c);
    assert_eq!(r.manager().pending().unwrap().ticket, primary);
    let epoch = r.active_link_epoch().unwrap();
    assert!(
        r.receive_hci(
            &mut a,
            Route::Application,
            epoch,
            4,
            &event(6, &[5, 0x42, 0])
        )
        .is_err()
    );
    assert!(r.manager().pending().is_none());
    let completion = r.manager().completion().unwrap();
    assert_eq!(completion.ticket, primary);
    assert!(matches!(completion.outcome, CommandOutcome::Failed(_)));
    assert!(!a.status().device.ready);
    assert!(r.manager().snapshot().cleanup_required);
    assert!(r.manager().snapshot().approval.is_none());
}
