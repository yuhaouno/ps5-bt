// SPDX-License-Identifier: GPL-3.0-or-later
use bt_capture::*;
use std::{cell::RefCell, rc::Rc};

fn format(sample_type: SampleType, channels: u8) -> ValidatedFormat {
    Format {
        sample_type,
        channels,
        sample_rate_hz: 48000,
        layout: Layout::Interleaved,
        evidence: Evidence::Declared,
    }
    .validate()
    .unwrap()
}
fn data(bytes: usize) -> ReadOutcome {
    ReadOutcome {
        kind: ReadKind::Data,
        bytes,
        ..ReadOutcome::empty()
    }
}
fn i16s(values: &[i16]) -> Vec<u8> {
    values.iter().flat_map(|x| x.to_le_bytes()).collect()
}
#[test]
fn unknown_requested_planar_and_unsupported_formats_fail_closed() {
    assert_eq!(Format::UNKNOWN.validate(), Err(FormatError::UnknownFormat));
    let mut f = format(SampleType::Signed16Le, 2).description();
    f.evidence = Evidence::Requested;
    assert_eq!(f.validate(), Err(FormatError::UnknownFormat));
    f.evidence = Evidence::HardwareVerified;
    f.channels = 3;
    assert_eq!(f.validate(), Err(FormatError::Unsupported));
    f.channels = 2;
    f.layout = Layout::Unknown;
    assert_eq!(f.validate(), Err(FormatError::Unsupported));
    f.layout = Layout::Interleaved;
    f.sample_rate_hz = 0;
    assert_eq!(f.validate(), Err(FormatError::Unsupported));
}
#[test]
fn i16_stereo_and_mono_normalization_is_bounded_atomic() {
    let mut out = [[55; 2]; 3];
    let fmt = format(SampleType::Signed16Le, 2);
    assert_eq!(
        fmt.normalize(&i16s(&[i16::MIN, i16::MAX, 0, -7]), &mut out),
        Ok(2)
    );
    assert_eq!(out, [[i16::MIN, i16::MAX], [0, -7], [55, 55]]);
    let saved = out;
    assert_eq!(
        fmt.normalize(&[1, 2, 3], &mut out),
        Err(FormatError::IncompleteFrame)
    );
    assert_eq!(out, saved);
    assert_eq!(
        fmt.normalize(&[0; 16], &mut out),
        Err(FormatError::Capacity)
    );
    assert_eq!(out, saved);
    assert_eq!(
        format(SampleType::Signed16Le, 1).normalize(&i16s(&[-9, 123]), &mut out),
        Ok(2)
    );
    assert_eq!(out[..2], [[-9, -9], [123, 123]]);
}
#[test]
fn float_conversion_saturates_truncates_and_rejects_nonfinite_atomically() {
    let values = [
        -2.0f32,
        -1.0,
        -0.5,
        -0.0,
        0.0,
        0.5,
        1.0,
        2.0,
        f32::MIN_POSITIVE,
    ];
    let input: Vec<u8> = values.iter().flat_map(|f| f.to_le_bytes()).collect();
    let mut out = [[9; 2]; 9];
    let fmt = format(SampleType::Float32Le, 1);
    assert_eq!(fmt.normalize(&input, &mut out), Ok(9));
    assert_eq!(
        out.map(|f| f[0]),
        [-32768, -32768, -16384, 0, 0, 16384, 32767, 32767, 0]
    );
    let saved = out;
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let bytes: Vec<u8> = [0.5f32, invalid]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        assert_eq!(fmt.normalize(&bytes, &mut out), Err(FormatError::NonFinite));
        assert_eq!(out, saved);
    }
    // Deterministic bit patterns compare with the explicitly defined host contract.
    let mut bits = 1u32;
    for _ in 0..10000 {
        bits = bits.wrapping_mul(1664525).wrapping_add(1013904223);
        let f = f32::from_bits(bits);
        if f.is_finite() {
            fmt.normalize(&bits.to_le_bytes(), &mut out).unwrap();
            let expected = if f >= 1.0 {
                i16::MAX
            } else if f <= -1.0 {
                i16::MIN
            } else {
                (f * 32768.0) as i16
            };
            assert_eq!(out[0], [expected; 2]);
        }
    }
}
#[test]
fn timestamp_domains_do_not_convert_or_replace_sample_positions() {
    let mut t = Timeline::new(20, Some(50));
    let stamp = t
        .observe(
            100,
            Some(NativeTime {
                value: 900,
                domain: 4,
            }),
            3,
            Gap::None,
        )
        .unwrap();
    assert_eq!(
        (stamp.host_observed_us, stamp.first_sample, t.next_sample()),
        (100, 20, 23)
    );
    let stamp = t
        .observe(
            101,
            Some(NativeTime {
                value: 850,
                domain: 4,
            }),
            2,
            Gap::Frames(4),
        )
        .unwrap();
    assert!(stamp.flags.native_regressed);
    assert_eq!(stamp.first_sample, 27);
    let stamp = t
        .observe(
            102,
            Some(NativeTime {
                value: 950,
                domain: 4,
            }),
            0,
            Gap::None,
        )
        .unwrap();
    assert!(stamp.flags.native_jumped);
    let stamp = t
        .observe(
            103,
            Some(NativeTime {
                value: 0,
                domain: 5,
            }),
            0,
            Gap::None,
        )
        .unwrap();
    assert!(stamp.flags.native_domain_changed);
    assert!(!stamp.flags.native_regressed);
    assert_eq!(
        t.observe(102, None, 1, Gap::None),
        Err(TimeError::HostRegressed)
    );
    assert_eq!(t.next_sample(), 29);
}
#[test]
fn time_overflow_and_unknown_gap_require_explicit_epoch() {
    let mut t = Timeline::new(u64::MAX - 1, None);
    assert_eq!(t.observe(1, None, 2, Gap::None), Err(TimeError::Overflow));
    assert_eq!(t.next_sample(), u64::MAX - 1);
    assert_eq!(
        t.observe(1, None, 0, Gap::Frames(2)),
        Err(TimeError::Overflow)
    );
    assert_eq!(
        t.observe(1, None, 0, Gap::Unknown),
        Err(TimeError::UnknownGap)
    );
    assert_eq!(t.observe(2, None, 1, Gap::None), Err(TimeError::UnknownGap));
    t.restart(0);
    assert_eq!(t.observe(2, None, 1, Gap::None).unwrap().first_sample, 0);
}
#[derive(Clone)]
struct Fake {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail: Option<&'static str>,
    read: ReadOutcome,
}
fn diagnostic() -> Diagnostic {
    Diagnostic {
        abi_version: ABI_VERSION,
        stage: 1,
        status: -5,
        raw_status: -99,
        ..Diagnostic::default()
    }
}
impl Fake {
    fn call(&self, name: &'static str) -> Result<(), Diagnostic> {
        self.events.borrow_mut().push(name);
        if self.fail == Some(name) {
            Err(diagnostic())
        } else {
            Ok(())
        }
    }
}
impl Backend for Fake {
    fn open(&mut self) -> Result<(), Diagnostic> {
        self.call("open")
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        self.call("start")
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.call("stop")
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.call("close")
    }
    fn read_once(&mut self, out: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        self.call("read")?;
        out.fill(8);
        Ok(self.read)
    }
}
fn fake(fail: Option<&'static str>, read: ReadOutcome) -> (Fake, Rc<RefCell<Vec<&'static str>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    (
        Fake {
            events: events.clone(),
            fail,
            read,
        },
        events,
    )
}
fn budget() -> Budget {
    Budget {
        max_reads: 2,
        deadline_us: 100,
    }
}
#[test]
fn owner_partial_initialization_and_drop_cleanup_once() {
    for fail in [Some("open"), Some("start"), None] {
        let (backend, events) = fake(fail, ReadOutcome::empty());
        let opened = CaptureOwner::open(backend, budget(), 0);
        if let Ok(mut owner) = opened {
            let _ = owner.start();
            let _ = owner.close();
            let _ = owner.close();
        }
        let expected = match fail {
            Some("open") => vec!["open"],
            Some("start") => vec!["open", "start", "close"],
            _ => vec!["open", "start", "stop", "close"],
        };
        assert_eq!(*events.borrow(), expected);
    }
    let (backend, events) = fake(None, ReadOutcome::empty());
    let mut owner = CaptureOwner::open(backend, budget(), 0).unwrap();
    owner.start().unwrap();
    drop(owner);
    assert_eq!(*events.borrow(), ["open", "start", "stop", "close"]);
}
#[test]
fn empty_reads_are_single_steps_and_read_budget_cleans_up() {
    let (backend, events) = fake(None, ReadOutcome::empty());
    let mut owner = CaptureOwner::open(backend, budget(), 0).unwrap();
    owner.start().unwrap();
    let mut out = [7; 8];
    assert_eq!(owner.read_once(1, &mut out).unwrap().kind, ReadKind::Empty);
    assert_eq!(out, [0; 8]);
    owner.read_once(2, &mut out).unwrap();
    assert_eq!(
        owner.read_once(3, &mut out).unwrap_err().error,
        CaptureError::ReadBudget
    );
    assert_eq!(owner.reads(), 2);
    drop(owner);
    assert_eq!(
        *events.borrow(),
        ["open", "start", "read", "read", "stop", "close"]
    );
}
#[test]
fn deadlines_regression_and_errors_preserve_cleanup_failure() {
    for (time, error) in [
        (100, CaptureError::Deadline),
        (0, CaptureError::ClockRegressed),
    ] {
        let (backend, events) = fake(Some("stop"), ReadOutcome::empty());
        let mut owner = CaptureOwner::open(backend, budget(), 1).unwrap();
        owner.start().unwrap();
        let failure = owner.read_once(time, &mut [0; 8]).unwrap_err();
        assert_eq!(failure.error, error);
        assert!(failure.cleanup.uncertain());
        owner.close();
        drop(owner);
        assert_eq!(*events.borrow(), ["open", "start", "stop", "close"]);
    }
}
#[test]
fn invalid_lengths_and_terminal_events_consume_owner() {
    for outcome in [
        data(9),
        ReadOutcome {
            kind: ReadKind::Empty,
            bytes: 1,
            ..ReadOutcome::empty()
        },
        ReadOutcome {
            kind: ReadKind::Overrun,
            ..ReadOutcome::empty()
        },
    ] {
        let (backend, events) = fake(None, outcome);
        let mut owner = CaptureOwner::open(backend, budget(), 0).unwrap();
        owner.start().unwrap();
        let mut out = [9; 8];
        assert_eq!(
            owner.read_once(1, &mut out).unwrap_err().error,
            CaptureError::InvalidOutcome
        );
        assert_eq!(out, [0; 8]);
        drop(owner);
        assert_eq!(events.borrow().iter().filter(|x| **x == "close").count(), 1);
    }
    for kind in [ReadKind::Ended, ReadKind::Error] {
        let (backend, events) = fake(
            None,
            ReadOutcome {
                kind,
                ..ReadOutcome::empty()
            },
        );
        let mut owner = CaptureOwner::open(backend, budget(), 0).unwrap();
        owner.start().unwrap();
        assert_eq!(owner.read_once(1, &mut [0; 8]).unwrap().kind, kind);
        assert!(owner.read_once(2, &mut [0; 8]).is_err());
        drop(owner);
        assert_eq!(*events.borrow(), ["open", "start", "read", "stop", "close"]);
    }
}
#[test]
fn bridge_stitches_short_chunks_retains_blocks_and_leftovers() {
    let mut staging = [[0; 2]; 8];
    let mut block = [[0; 2]; 3];
    let mut bridge = CaptureBridge::new(
        format(SampleType::Signed16Le, 1),
        0,
        None,
        &mut staging,
        &mut block,
    )
    .unwrap();
    bridge.ingest(data(4), &i16s(&[1, 2]), 1, None).unwrap();
    assert_eq!(bridge.prepare_block(), None);
    bridge
        .ingest(data(10), &i16s(&[3, 4, 5, 6, 7]), 2, None)
        .unwrap();
    let first = bridge.prepare_block().unwrap();
    assert_eq!(first.first_sample, 0);
    assert_eq!(first.samples, [[1, 1], [2, 2], [3, 3]]);
    assert_eq!(
        bridge.ingest(ReadOutcome::empty(), &[], 3, None),
        Err(BridgeError::Busy)
    );
    assert_eq!(
        bridge.prepare_block().unwrap().samples,
        [[1, 1], [2, 2], [3, 3]]
    );
    bridge.confirm_block().unwrap();
    let next = bridge.prepare_block().unwrap();
    assert_eq!(next.first_sample, 3);
    assert_eq!(next.samples, [[4, 4], [5, 5], [6, 6]]);
    bridge.confirm_block().unwrap();
    assert_eq!(bridge.prepare_block(), None);
    bridge.ingest(data(4), &i16s(&[8, 9]), 3, None).unwrap();
    let last = bridge.prepare_block().unwrap();
    assert_eq!(last.first_sample, 6);
    assert_eq!(last.samples, [[7, 7], [8, 8], [9, 9]]);
    assert_eq!(bridge.statistics().accepted_blocks, 2);
}
#[test]
fn bridge_known_gaps_drop_partial_and_unknown_gaps_gate_epoch() {
    let mut staging = [[0; 2]; 8];
    let mut block = [[0; 2]; 3];
    let mut bridge = CaptureBridge::new(
        format(SampleType::Signed16Le, 1),
        0,
        None,
        &mut staging,
        &mut block,
    )
    .unwrap();
    bridge.ingest(data(4), &i16s(&[1, 2]), 1, None).unwrap();
    bridge.prepare_block();
    let overrun = ReadOutcome {
        kind: ReadKind::Overrun,
        discontinuity: true,
        ..ReadOutcome::empty()
    };
    assert_eq!(
        bridge.ingest(overrun, &[], 2, Some(5)),
        Ok(BridgeEvent::Overrun {
            known_lost_frames: Some(5)
        })
    );
    bridge.ingest(data(6), &i16s(&[3, 4, 5]), 3, None).unwrap();
    assert_eq!(bridge.prepare_block().unwrap().first_sample, 7);
    bridge.confirm_block().unwrap();
    assert_eq!(
        bridge.ingest(overrun, &[], 4, None),
        Ok(BridgeEvent::Overrun {
            known_lost_frames: None
        })
    );
    assert!(bridge.needs_epoch());
    assert_eq!(
        bridge.ingest(data(2), &i16s(&[9]), 5, None),
        Err(BridgeError::NeedsEpoch)
    );
    bridge.restart_epoch(0);
    bridge.ingest(data(6), &i16s(&[6, 7, 8]), 5, None).unwrap();
    assert_eq!(bridge.prepare_block().unwrap().first_sample, 0);
    assert_eq!(bridge.statistics().dropped_partial_frames, 2);
    assert_eq!(bridge.statistics().known_gap_frames, 5);
    assert_eq!(bridge.statistics().unknown_gaps, 1);
}
#[test]
fn empty_does_not_advance_samples_and_ended_discards_partial() {
    let mut staging = [[0; 2]; 4];
    let mut block = [[0; 2]; 3];
    let mut bridge = CaptureBridge::new(
        format(SampleType::Signed16Le, 1),
        42,
        None,
        &mut staging,
        &mut block,
    )
    .unwrap();
    assert_eq!(
        bridge.ingest(ReadOutcome::empty(), &[], 1, None),
        Ok(BridgeEvent::Empty)
    );
    bridge.ingest(data(2), &i16s(&[1]), 2, None).unwrap();
    bridge.prepare_block();
    let ended = ReadOutcome {
        kind: ReadKind::Ended,
        ..ReadOutcome::empty()
    };
    assert_eq!(bridge.ingest(ended, &[], 3, None), Ok(BridgeEvent::Ended));
    assert_eq!(bridge.statistics().dropped_partial_frames, 1);
    assert_eq!(
        bridge.ingest(ReadOutcome::empty(), &[], 4, None),
        Err(BridgeError::Ended)
    );
}

#[test]
fn invalid_owner_budget_never_opens_backend_and_native_missing_time_is_not_a_reset() {
    let (backend, events) = fake(None, ReadOutcome::empty());
    assert!(
        CaptureOwner::open(
            backend,
            Budget {
                max_reads: 0,
                deadline_us: 100
            },
            0
        )
        .is_err()
    );
    assert!(events.borrow().is_empty());
    let mut timeline = Timeline::new(0, Some(10));
    timeline
        .observe(
            1,
            Some(NativeTime {
                value: 100,
                domain: 1,
            }),
            0,
            Gap::None,
        )
        .unwrap();
    timeline.observe(2, None, 0, Gap::None).unwrap();
    assert!(
        timeline
            .observe(
                3,
                Some(NativeTime {
                    value: 90,
                    domain: 1
                }),
                0,
                Gap::None
            )
            .unwrap()
            .flags
            .native_regressed
    );
    assert_eq!(
        timeline.observe(5, None, 0, Gap::Unknown),
        Err(TimeError::UnknownGap)
    );
    timeline.restart(0);
    assert_eq!(
        timeline.observe(4, None, 0, Gap::None),
        Err(TimeError::HostRegressed)
    );
}
