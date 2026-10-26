// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::recovery::{Action, Failure, Policy, Recovery, Requirement};
use bt_runner::{
    RebuildError, RebuildFactory, RebuildOwner, RebuildResource, ReopenFailure, ResourceCleanup,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Counts {
    opens: Vec<u64>,
    closes: usize,
    fail_opens: usize,
    uncertain_close: bool,
}
struct Resource(Rc<RefCell<Counts>>);
impl RebuildResource for Resource {
    fn close(self) -> ResourceCleanup {
        let mut c = self.0.borrow_mut();
        c.closes += 1;
        ResourceCleanup {
            confirmed: !c.uncertain_close,
            other_error: c.uncertain_close.then_some(-4),
            ..ResourceCleanup::default()
        }
    }
}
struct Factory(Rc<RefCell<Counts>>);
impl RebuildFactory for Factory {
    type Resource = Resource;
    fn reopen(&mut self, epoch: u64, _: u64) -> Result<Resource, ReopenFailure<Resource>> {
        let mut c = self.0.borrow_mut();
        c.opens.push(epoch);
        if c.fail_opens > 0 {
            c.fail_opens -= 1;
            Err(ReopenFailure {
                error: -3,
                failure: Failure::Retryable,
                partial: Some(Resource(self.0.clone())),
            })
        } else {
            Ok(Resource(self.0.clone()))
        }
    }
}
fn recovery() -> Recovery {
    Recovery::new(Policy {
        max_attempts: 2,
        episode_timeout_us: 100,
        initial_backoff_us: 5,
        max_backoff_us: 10,
        stable_success_us: 20,
    })
    .unwrap()
}
fn owner(c: Rc<RefCell<Counts>>) -> RebuildOwner<Factory> {
    RebuildOwner::new(Factory(c.clone()), 9, Some(Resource(c))).unwrap()
}
fn retry(r: &mut Recovery, o: &mut RebuildOwner<Factory>, now: u64) -> Action {
    o.step(r, now).unwrap();
    o.step(r, now).unwrap();
    o.step(r, now).unwrap()
}
#[test]
fn cleanup_reopen_new_epoch_and_working_attestation_share_one_fault_budget() {
    let c = Rc::new(RefCell::new(Counts::default()));
    let mut o = owner(c.clone());
    let mut r = recovery();
    r.trigger(0, Failure::Retryable).unwrap();
    let action = retry(&mut r, &mut o, 0);
    let Action::InFlight(ticket) = action else {
        panic!("{action:?}")
    };
    assert_eq!(c.borrow().closes, 1);
    assert_eq!(c.borrow().opens, vec![10]);
    assert_eq!(r.snapshot().attempts, 1);
    assert_eq!(r.snapshot().episode_deadline_us, Some(100));
    o.confirm_working(&mut r, 1, ticket).unwrap();
    assert!(matches!(
        o.step(&mut r, 2).unwrap(),
        Action::StabilizingUntil(21)
    ));
    r.trigger(3, Failure::Retryable).unwrap();
    assert!(matches!(
        o.step(&mut r, 3).unwrap(),
        Action::AwaitSafety { .. }
    ));
    assert!(matches!(o.step(&mut r, 3).unwrap(), Action::WaitUntil(8)));
    assert!(matches!(o.step(&mut r, 8).unwrap(), Action::InFlight(_)));
    assert_eq!(r.snapshot().attempts, 2);
    assert_eq!(c.borrow().opens, vec![10, 11]);
    assert_eq!(r.snapshot().episode_deadline_us, Some(100));
    o.stop(&mut r);
    o.stop(&mut r);
    drop(o);
    assert_eq!(c.borrow().closes, 3);
}
#[test]
fn failed_partial_open_closes_once_and_never_restarts_attempt_budget() {
    let c = Rc::new(RefCell::new(Counts {
        fail_opens: 2,
        ..Counts::default()
    }));
    let mut o = owner(c.clone());
    let mut r = recovery();
    r.trigger(0, Failure::Retryable).unwrap();
    let action = retry(&mut r, &mut o, 0);
    assert!(matches!(action, Action::AwaitSafety { .. }));
    assert_eq!(c.borrow().closes, 1);
    assert!(o.snapshot().resource_present);
    assert_eq!(o.snapshot().open_error, Some(-3));
    o.step(&mut r, 0).unwrap();
    assert_eq!(c.borrow().closes, 2);
    assert!(!o.snapshot().resource_present);
    assert!(matches!(o.step(&mut r, 0).unwrap(), Action::WaitUntil(5)));
    o.step(&mut r, 5).unwrap();
    assert_eq!(c.borrow().opens, vec![10, 11]);
    o.step(&mut r, 5).unwrap();
    assert_eq!(c.borrow().closes, 3);
    for now in 6..15 {
        assert!(matches!(o.step(&mut r, now).unwrap(), Action::Exhausted(_)));
    }
    assert_eq!(r.snapshot().attempts, 2);
    assert_eq!(c.borrow().opens.len(), 2);
    assert_eq!(c.borrow().closes, 3);
}
#[test]
fn integrity_fault_cannot_reopen_from_cleanup_or_elapsed_time() {
    let c = Rc::new(RefCell::new(Counts::default()));
    let mut o = owner(c.clone());
    let mut r = recovery();
    r.trigger(0, Failure::IntegrityLost).unwrap();
    o.step(&mut r, 0).unwrap();
    let Action::AwaitSafety {
        ticket,
        requirement: Requirement::Reconciliation,
    } = o.step(&mut r, 1).unwrap()
    else {
        panic!()
    };
    for now in 2..10 {
        assert!(matches!(
            o.step(&mut r, now).unwrap(),
            Action::AwaitSafety { .. }
        ));
    }
    assert!(c.borrow().opens.is_empty());
    assert_eq!(
        o.confirm_reconciled(&mut r, 10, ticket, 8),
        Err(RebuildError::InvalidState)
    );
    o.confirm_reconciled(&mut r, 10, ticket, 9).unwrap();
    assert!(matches!(o.step(&mut r, 10).unwrap(), Action::InFlight(_)));
    assert_eq!(c.borrow().opens, vec![10]);
    assert!(o.confirm_reconciled(&mut r, 10, ticket, 9).is_err());
}
#[test]
fn uncertain_cleanup_requires_explicit_external_evidence() {
    let c = Rc::new(RefCell::new(Counts {
        uncertain_close: true,
        ..Counts::default()
    }));
    let mut o = owner(c.clone());
    let mut r = recovery();
    r.trigger(0, Failure::Retryable).unwrap();
    o.step(&mut r, 0).unwrap();
    let Action::AwaitSafety { ticket, .. } = o.step(&mut r, 1).unwrap() else {
        panic!()
    };
    assert!(!o.snapshot().cleanup.unwrap().confirmed);
    assert!(c.borrow().opens.is_empty());
    o.confirm_reconciled(&mut r, 2, ticket, 9).unwrap();
    assert!(matches!(o.step(&mut r, 2).unwrap(), Action::InFlight(_)));
    assert_eq!(c.borrow().closes, 1);
}
#[test]
fn deadline_stop_and_clock_failure_cleanup_active_resources_and_do_not_reopen() {
    for mode in 0..3 {
        let c = Rc::new(RefCell::new(Counts::default()));
        let mut o = owner(c.clone());
        let mut r = recovery();
        r.trigger(0, Failure::Retryable).unwrap();
        let _ = retry(&mut r, &mut o, 0);
        match mode {
            0 => {
                assert!(matches!(o.step(&mut r, 100).unwrap(), Action::Exhausted(_)));
            }
            1 => o.stop(&mut r),
            _ => {
                o.step(&mut r, 20).unwrap();
                assert!(matches!(o.step(&mut r, 19), Err(RebuildError::Recovery(_))));
            }
        }
        drop(o);
        assert_eq!(c.borrow().opens, vec![10]);
        assert_eq!(c.borrow().closes, 2);
    }
}
#[test]
fn epoch_exhaustion_stops_before_any_native_open() {
    let c = Rc::new(RefCell::new(Counts::default()));
    let mut o = RebuildOwner::new(Factory(c.clone()), u64::MAX, None).unwrap();
    let mut r = recovery();
    r.trigger(0, Failure::Retryable).unwrap();
    o.step(&mut r, 0).unwrap();
    assert_eq!(o.step(&mut r, 0), Err(RebuildError::EpochExhausted));
    assert!(c.borrow().opens.is_empty());
    assert_eq!(r.snapshot().action, Action::Stopped);
}

#[test]
fn real_capture_encoder_pair_factory_reopens_and_consumes_each_lifecycle_once() {
    use bt_audio::{BlockEncoder, FinishEncoder};
    use bt_capture::{Backend, Budget, CaptureOwner, Diagnostic, ReadOutcome};
    use bt_runner::CaptureEncoderResources;
    #[derive(Default)]
    struct Native {
        opens: usize,
        starts: usize,
        stops: usize,
        closes: usize,
        finishes: usize,
    }
    struct Capture(Rc<RefCell<Native>>);
    impl Backend for Capture {
        fn open(&mut self) -> Result<(), Diagnostic> {
            self.0.borrow_mut().opens += 1;
            Ok(())
        }
        fn start(&mut self) -> Result<(), Diagnostic> {
            self.0.borrow_mut().starts += 1;
            Ok(())
        }
        fn read_once(&mut self, _: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
            Ok(ReadOutcome::empty())
        }
        fn stop(&mut self) -> Result<(), Diagnostic> {
            self.0.borrow_mut().stops += 1;
            Ok(())
        }
        fn close(&mut self) -> Result<(), Diagnostic> {
            self.0.borrow_mut().closes += 1;
            Ok(())
        }
    }
    struct Encoder(Rc<RefCell<Native>>);
    impl BlockEncoder for Encoder {
        fn parameters(&self) -> bt_core::SbcParameters {
            bt_core::SbcParameters::new(
                bt_core::SbcSampleRate::Hz48000,
                bt_core::SbcChannelMode::JointStereo,
                bt_core::SbcBlocks::Sixteen,
                bt_core::SbcSubbands::Eight,
                bt_core::SbcAllocation::Loudness,
                32,
            )
            .unwrap()
        }
        fn encode_stereo(&mut self, _: &[[i16; 2]], _: &mut [u8]) -> Result<usize, bt_core::Error> {
            panic!("reopen is not encoding success")
        }
    }
    impl FinishEncoder for Encoder {
        fn finish(self) -> Result<(), bt_core::Error> {
            self.0.borrow_mut().finishes += 1;
            Ok(())
        }
    }
    struct NativeFactory(Rc<RefCell<Native>>);
    impl RebuildFactory for NativeFactory {
        type Resource = CaptureEncoderResources<Capture, Encoder>;
        fn reopen(
            &mut self,
            _: u64,
            now: u64,
        ) -> Result<Self::Resource, ReopenFailure<Self::Resource>> {
            let mut capture = CaptureOwner::open(
                Capture(self.0.clone()),
                Budget {
                    max_reads: 4,
                    deadline_us: now + 50,
                },
                now,
            )
            .unwrap();
            capture.start().unwrap();
            Ok(CaptureEncoderResources {
                capture,
                encoder: Encoder(self.0.clone()),
            })
        }
    }
    let counts = Rc::new(RefCell::new(Native::default()));
    let mut o = RebuildOwner::new(NativeFactory(counts.clone()), 1, None).unwrap();
    let mut r = recovery();
    r.trigger(0, Failure::Retryable).unwrap();
    o.step(&mut r, 0).unwrap();
    assert!(matches!(o.step(&mut r, 0).unwrap(), Action::InFlight(_)));
    assert_eq!(counts.borrow().opens, 1);
    assert_eq!(counts.borrow().starts, 1);
    assert_eq!(counts.borrow().finishes, 0);
    r.trigger(1, Failure::Retryable).unwrap();
    o.step(&mut r, 1).unwrap();
    o.step(&mut r, 1).unwrap();
    o.step(&mut r, 6).unwrap();
    assert_eq!(counts.borrow().opens, 2);
    assert_eq!(counts.borrow().stops, 1);
    assert_eq!(counts.borrow().closes, 1);
    assert_eq!(counts.borrow().finishes, 1);
    o.stop(&mut r);
    drop(o);
    assert_eq!(counts.borrow().stops, 2);
    assert_eq!(counts.borrow().closes, 2);
    assert_eq!(counts.borrow().finishes, 2);
}
