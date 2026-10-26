// SPDX-License-Identifier: GPL-3.0-or-later
use bt_capture::{Budget, Evidence, Format, Layout, SampleType};
use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use bt_listener::BindConfig;
use ps5_app::*;
use std::{cell::RefCell, rc::Rc};
fn parameters() -> EncoderConfig {
    EncoderConfig {
        parameters: SbcParameters::new(
            SbcSampleRate::Hz48000,
            SbcChannelMode::JointStereo,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            SbcAllocation::Loudness,
            32,
        )
        .unwrap(),
        negotiated_bitpool: SbcBitpoolRange::new(2, 53).unwrap(),
    }
}
fn audio_config() -> Config<'static> {
    Config {
        mode: Mode::NativeAudio,
        store_directory: b"/protected-test",
        bind: BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
        capture: Some(CaptureConfig {
            budget: Budget {
                max_reads: 2,
                deadline_us: 100,
            },
            format: Format {
                sample_type: SampleType::Signed16Le,
                channels: 2,
                sample_rate_hz: 48000,
                layout: Layout::Interleaved,
                evidence: Evidence::HardwareVerified,
            },
        }),
        encoder: Some(parameters()),
    }
}
#[derive(Default)]
struct Log {
    events: Vec<(bool, ResourceKind)>,
    fail_open: Option<ResourceKind>,
    fail_close: Option<ResourceKind>,
}
struct Ports(Rc<RefCell<Log>>);
impl ResourcePorts for Ports {
    fn acquire(&mut self, resource: ResourceKind, _: u64) -> Result<(), ResourceError> {
        let mut l = self.0.borrow_mut();
        l.events.push((true, resource));
        if l.fail_open == Some(resource) {
            Err(ResourceError {
                raw_status: -44,
                ..ResourceError::local(-5)
            })
        } else {
            Ok(())
        }
    }
    fn cleanup(&mut self, resource: ResourceKind) -> Result<(), ResourceError> {
        let mut l = self.0.borrow_mut();
        l.events.push((false, resource));
        if l.fail_close == Some(resource) {
            Err(ResourceError {
                flags: 1,
                ..ResourceError::local(-77)
            })
        } else {
            Ok(())
        }
    }
}
const ORDER: [ResourceKind; 5] = [
    ResourceKind::Clock,
    ResourceKind::Store,
    ResourceKind::Listener,
    ResourceKind::Capture,
    ResourceKind::Encoder,
];
#[test]
fn every_acquisition_failure_unwinds_prior_actual_owners_once_in_reverse() {
    for (index, failed) in ORDER.iter().enumerate() {
        let log = Rc::new(RefCell::new(Log {
            fail_open: Some(*failed),
            ..Log::default()
        }));
        let mut factory = ResourceFactory::new(audio_config(), Ports(log.clone())).unwrap();
        for resource in &ORDER[..index] {
            factory.acquire(*resource, 0).unwrap();
        }
        let failure = factory.acquire(*failed, 0).unwrap_err();
        assert_eq!(failure.resource, *failed);
        assert_eq!(failure.error.raw_status, -44);
        assert_eq!(failure.report.phase, FactoryPhase::Failed);
        assert_eq!(failure.report.acquired, [false; 5]);
        factory.shutdown();
        drop(factory);
        let expected: Vec<_> = ORDER[..=index]
            .iter()
            .map(|r| (true, *r))
            .chain(ORDER[..index].iter().rev().map(|r| (false, *r)))
            .collect();
        assert_eq!(log.borrow().events, expected);
    }
}
#[test]
fn cleanup_failure_preserves_primary_and_continues_reverse_cleanup() {
    let log = Rc::new(RefCell::new(Log {
        fail_open: Some(ResourceKind::Encoder),
        fail_close: Some(ResourceKind::Capture),
        ..Log::default()
    }));
    let mut factory = ResourceFactory::new(audio_config(), Ports(log.clone())).unwrap();
    for resource in &ORDER[..4] {
        factory.acquire(*resource, 0).unwrap();
    }
    let failure = factory.acquire(ResourceKind::Encoder, 0).unwrap_err();
    assert_eq!(failure.error.code, -5);
    assert!(failure.report.cleanup_uncertain());
    assert_eq!(
        failure.report.cleanup[ResourceKind::Capture as usize]
            .unwrap()
            .unwrap_err()
            .code,
        -77
    );
    drop(factory);
    assert_eq!(
        log.borrow()
            .events
            .iter()
            .filter(|event| **event == (false, ResourceKind::Capture))
            .count(),
        1
    );
    assert_eq!(
        log.borrow().events.last(),
        Some(&(false, ResourceKind::Clock))
    );
}
#[test]
fn ordering_duplicate_calls_disabled_resources_and_drop_are_bounded() {
    let log = Rc::new(RefCell::new(Log::default()));
    let config = Config::control_only(
        b"/protected-test",
        BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
    );
    let mut factory = ResourceFactory::new(config, Ports(log.clone())).unwrap();
    assert!(factory.acquire(ResourceKind::Store, 0).is_err());
    assert!(log.borrow().events.is_empty());
    for resource in ORDER {
        factory.acquire(resource, 0).unwrap();
        factory.acquire(resource, 0).unwrap();
    }
    assert_eq!(factory.report().phase, FactoryPhase::Ready);
    assert_eq!(factory.report().acquired, [true, true, true, false, false]);
    assert!(factory.cleanup(ResourceKind::Clock).is_err());
    factory.cleanup(ResourceKind::Listener).unwrap();
    factory.cleanup(ResourceKind::Listener).unwrap();
    drop(factory);
    assert_eq!(
        log.borrow().events,
        [
            (true, ResourceKind::Clock),
            (true, ResourceKind::Store),
            (true, ResourceKind::Listener),
            (false, ResourceKind::Listener),
            (false, ResourceKind::Store),
            (false, ResourceKind::Clock)
        ]
    );
}
#[test]
fn native_format_and_resource_mode_gates_precede_any_acquisition() {
    let mut config = audio_config();
    config.capture.as_mut().unwrap().format = Format::UNKNOWN;
    assert_eq!(config.validate(), Err(ConfigError::UnknownFormat));
    config = audio_config();
    config.capture.as_mut().unwrap().format.evidence = Evidence::Declared;
    assert_eq!(config.validate(), Err(ConfigError::UnverifiedNativeFormat));
    config = audio_config();
    config.capture.as_mut().unwrap().format.sample_rate_hz = 44100;
    assert_eq!(config.validate(), Err(ConfigError::RateMismatch));
    config = audio_config();
    config.capture.as_mut().unwrap().format.channels = 3;
    assert_eq!(config.validate(), Err(ConfigError::Format));
    config = audio_config();
    config.mode = Mode::CaptureObserve;
    assert_eq!(config.validate(), Err(ConfigError::ModeResources));
    config = audio_config();
    config.store_directory = b"/bad\0path";
    assert_eq!(config.validate(), Err(ConfigError::Directory));
}

#[test]
fn pending_external_handoff_defers_cleanup_and_does_not_invent_success() {
    struct Deferred {
        events: Rc<RefCell<Vec<ResourceKind>>>,
        pending: Rc<RefCell<bool>>,
    }
    impl ResourcePorts for Deferred {
        fn acquire(&mut self, _: ResourceKind, _: u64) -> Result<(), ResourceError> {
            Ok(())
        }
        fn cleanup(&mut self, r: ResourceKind) -> Result<(), ResourceError> {
            self.events.borrow_mut().push(r);
            Ok(())
        }
        fn cleanup_deferred(&self, r: ResourceKind) -> bool {
            r == ResourceKind::Store && *self.pending.borrow()
        }
    }
    let events = Rc::new(RefCell::new(Vec::new()));
    let pending = Rc::new(RefCell::new(true));
    let config = Config::control_only(
        b"/protected-test",
        BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
    );
    let mut factory = ResourceFactory::new(
        config,
        Deferred {
            events: events.clone(),
            pending: pending.clone(),
        },
    )
    .unwrap();
    for r in ORDER {
        factory.acquire(r, 0).unwrap();
    }
    let report = factory.shutdown();
    assert!(report.cleanup_uncertain());
    assert_eq!(report.acquired, [true, true, false, false, false]);
    assert_eq!(*events.borrow(), [ResourceKind::Listener]);
    *pending.borrow_mut() = false;
    assert!(!factory.shutdown().cleanup_uncertain());
    drop(factory);
    assert_eq!(
        *events.borrow(),
        [
            ResourceKind::Listener,
            ResourceKind::Store,
            ResourceKind::Clock
        ]
    );
}
