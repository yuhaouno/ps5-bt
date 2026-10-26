// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_audio::{AudioScheduler, BlockEncoder, FinishEncoder};
use bt_capture::{
    Backend, Budget, CaptureBridge, CaptureOwner, Diagnostic, Evidence, Format, Layout, ReadKind,
    ReadOutcome, SampleType,
};
use bt_core::{
    BluetoothTransport, Error, Received, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters,
    SbcSampleRate, SbcSubbands, media::TimedStereoFrame,
};
use bt_hci::flow::{AclCreditOwner, CreditContext};
use bt_runner::{AudioPipeline, OutputPump, PipelineError, SendBuffers};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Counts {
    opened: usize,
    started: usize,
    reads: usize,
    stopped: usize,
    closed: usize,
    encodes: Vec<Vec<[i16; 2]>>,
    sends: usize,
    finishes: usize,
    finish_error: bool,
}
struct Capture {
    counts: Rc<RefCell<Counts>>,
    kind: ReadKind,
}
impl Backend for Capture {
    fn open(&mut self) -> Result<(), Diagnostic> {
        self.counts.borrow_mut().opened += 1;
        Ok(())
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        self.counts.borrow_mut().started += 1;
        Ok(())
    }
    fn read_once(&mut self, out: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        self.counts.borrow_mut().reads += 1;
        let bytes = if self.kind == ReadKind::Data {
            for f in out.chunks_exact_mut(4) {
                f.copy_from_slice(&[1, 0, 2, 0]);
            }
            out.len()
        } else {
            0
        };
        Ok(ReadOutcome {
            kind: self.kind,
            bytes,
            discontinuity: self.kind == ReadKind::Overrun,
            ..ReadOutcome::empty()
        })
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.counts.borrow_mut().stopped += 1;
        Ok(())
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.counts.borrow_mut().closed += 1;
        Ok(())
    }
}
struct Encoder {
    counts: Rc<RefCell<Counts>>,
    parameters: SbcParameters,
}
impl BlockEncoder for Encoder {
    fn parameters(&self) -> SbcParameters {
        self.parameters
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], out: &mut [u8]) -> Result<usize, Error> {
        self.counts.borrow_mut().encodes.push(pcm.to_vec());
        out.fill(0);
        out[0] = 0x9c;
        Ok(self.parameters.frame_length_bytes())
    }
}
impl FinishEncoder for Encoder {
    fn finish(self) -> Result<(), Error> {
        self.counts.borrow_mut().finishes += 1;
        if self.counts.borrow().finish_error {
            Err(Error::Backend(-77))
        } else {
            Ok(())
        }
    }
}
fn parameters() -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap()
}
struct Storage {
    staging: [[i16; 2]; 256],
    block: [[i16; 2]; 128],
    pcm: [TimedStereoFrame; 128],
    render: [[i16; 2]; 128],
    encoded: [u8; 128],
    raw: [u8; 512],
}
impl Storage {
    fn new() -> Self {
        Self {
            staging: [[0; 2]; 256],
            block: [[0; 2]; 128],
            pcm: [TimedStereoFrame::default(); 128],
            render: [[0; 2]; 128],
            encoded: [0; 128],
            raw: [0; 512],
        }
    }
}
#[allow(clippy::result_large_err)]
fn pipeline<'a>(
    b: &'a mut Storage,
    counts: Rc<RefCell<Counts>>,
    kind: ReadKind,
    rate: u32,
    block: usize,
) -> Result<AudioPipeline<'a, Capture, Encoder>, bt_runner::PipelineFailure> {
    let format = Format {
        sample_type: SampleType::Signed16Le,
        channels: 2,
        sample_rate_hz: rate,
        layout: Layout::Interleaved,
        evidence: Evidence::Declared,
    }
    .validate()
    .unwrap();
    let bridge =
        CaptureBridge::new(format, 0, None, &mut b.staging, &mut b.block[..block]).unwrap();
    let audio = AudioScheduler::new(
        bt_audio::Config {
            parameters: parameters(),
            epoch_us: 1,
            max_lateness_us: 10000,
            max_concealed_frames: 0,
            max_rebuffer_us: 10000,
            deadline_us: Some(100000),
        },
        &mut b.pcm,
        &mut b.render,
        &mut b.encoded,
    )
    .unwrap();
    let owner = CaptureOwner::open(
        Capture {
            counts: counts.clone(),
            kind,
        },
        Budget {
            max_reads: 100,
            deadline_us: 100000,
        },
        0,
    )
    .unwrap();
    AudioPipeline::new(
        owner,
        bridge,
        audio,
        Encoder {
            counts,
            parameters: parameters(),
        },
        &mut b.raw,
    )
}
struct Transport(Rc<RefCell<Counts>>);
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("no command")
    }
    fn send_acl(&mut self, _: &[u8]) -> Result<(), Error> {
        self.0.borrow_mut().sends += 1;
        Ok(())
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("no hidden reader")
    }
}
#[test]
fn rejects_rate_or_block_mismatch_and_closes_without_starting() {
    for (rate, block) in [(44100, 128), (48000, 64)] {
        let mut b = Storage::new();
        let counts = Rc::new(RefCell::new(Counts::default()));
        let result = pipeline(&mut b, counts.clone(), ReadKind::Data, rate, block);
        assert!(matches!(
            result,
            Err(bt_runner::PipelineFailure {
                error: PipelineError::InvalidConfiguration,
                ..
            })
        ));
        assert_eq!(counts.borrow().opened, 1);
        assert_eq!(counts.borrow().started, 0);
        assert_eq!(counts.borrow().closed, 1);
    }
}
#[test]
fn capture_queue_busy_retains_block_without_reading_or_encoding_again() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Data, 48000, 128).unwrap();
    p.capture_step(1).unwrap(); // read first block
    p.capture_step(1).unwrap(); // enqueue first block
    p.capture_step(1).unwrap(); // read second, queue still full
    assert!(!p.capture_step(1).unwrap().progressed);
    assert!(!p.capture_step(1).unwrap().progressed);
    assert_eq!(counts.borrow().reads, 2);
    assert!(counts.borrow().encodes.is_empty());
    assert!(p.encode_step(1).unwrap().progressed);
    assert_eq!(counts.borrow().encodes.len(), 1);
    assert_eq!(counts.borrow().encodes[0], vec![[1, 2]; 128]);
    assert!(p.capture_step(1).unwrap().progressed);
    assert_eq!(counts.borrow().reads, 2);
    assert!(!p.encode_step(1).unwrap().progressed);
    assert_eq!(counts.borrow().encodes.len(), 1);
}
#[test]
fn empty_is_observable_and_stop_prevents_all_later_reads_or_encodes() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Empty, 48000, 128).unwrap();
    let q = p.capture_step(1).unwrap();
    assert!(!q.progressed);
    assert_eq!(q.continuity.capture_empty, 1);
    p.stop();
    p.stop();
    assert_eq!(p.capture_step(2), Err(PipelineError::Stopped));
    assert_eq!(p.encode_step(2), Err(PipelineError::Stopped));
    assert_eq!(counts.borrow().reads, 1);
    assert_eq!(counts.borrow().stopped, 1);
    assert_eq!(counts.borrow().closed, 1);
    assert_eq!(counts.borrow().finishes, 1);
    drop(p);
    assert_eq!(counts.borrow().closed, 1);
}
#[test]
fn unknown_overrun_demands_new_epoch_without_fabricated_gap_samples() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Overrun, 48000, 128).unwrap();
    assert_eq!(p.capture_step(1).unwrap().continuity.capture_overruns, 1);
    assert_eq!(p.capture_step(2), Err(PipelineError::UnknownGap));
    assert_eq!(counts.borrow().reads, 1);
    assert_eq!(p.snapshot().bridge.unknown_gaps, 1);
    assert_eq!(p.snapshot().bridge.known_gap_frames, 0);
}
#[test]
fn send_does_not_encode_and_stop_preserves_healthy_media_prefix() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Data, 48000, 128).unwrap();
    let mut ab = common::Buffers::default();
    let mut recovery = common::recovery();
    let mut app = common::app(&mut ab, &mut recovery);
    common::streaming(&mut app);
    let mut storage = [0; 512];
    let mut output = OutputPump::new(common::connection(), 40, &mut storage, 10000, 1).unwrap();
    let mut credits = AclCreditOwner::new(
        CreditContext {
            handle: common::connection().handle,
            epoch: 9,
        },
        10,
        40,
    )
    .unwrap();
    let mut transport = Transport(counts.clone());
    let mut rtp = [0; 128];
    let mut basic = [0; 132];
    let mut acl = [0; 44];
    let mut send = |p: &mut AudioPipeline<'_, Capture, Encoder>| {
        p.send_step(
            &mut app,
            &mut output,
            &mut credits,
            &mut transport,
            1,
            SendBuffers {
                rtp: &mut rtp,
                basic: &mut basic,
                acl: &mut acl,
            },
        )
    };
    assert!(!send(&mut p).unwrap().progressed);
    assert!(counts.borrow().encodes.is_empty());
    p.capture_step(1).unwrap();
    p.capture_step(1).unwrap();
    p.encode_step(1).unwrap();
    assert!(send(&mut p).unwrap().progressed); // copy whole media PDU
    assert!(send(&mut p).unwrap().progressed); // first ACL fragment
    p.stop();
    assert!(send(&mut p).unwrap().progressed);
    assert!(send(&mut p).unwrap().progressed);
    assert_eq!(counts.borrow().encodes.len(), 1);
    assert_eq!(counts.borrow().sends, 3);
    assert!(!send(&mut p).unwrap().progressed);
    assert_eq!(counts.borrow().sends, 3);
}

#[test]
fn ended_and_native_error_are_immediate_terminal_events() {
    for kind in [ReadKind::Ended, ReadKind::Error] {
        let mut b = Storage::new();
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut p = pipeline(&mut b, counts.clone(), kind, 48000, 128).unwrap();
        assert_eq!(
            p.capture_step(1),
            Err(if kind == ReadKind::Ended {
                PipelineError::Ended
            } else {
                PipelineError::NativeError(0)
            })
        );
        assert_eq!(counts.borrow().reads, 1);
        p.stop();
        assert_eq!(counts.borrow().closed, 1);
        assert_eq!(counts.borrow().finishes, 1);
    }
}

#[test]
fn explicit_stop_preserves_codec_finish_error_and_never_retries_it() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    counts.borrow_mut().finish_error = true;
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Empty, 48000, 128).unwrap();
    assert_eq!(p.stop().encoder, Some(Err(Error::Backend(-77))));
    assert_eq!(p.stop().encoder, Some(Err(Error::Backend(-77))));
    assert_eq!(p.snapshot().cleanup.encoder, Some(Err(Error::Backend(-77))));
    drop(p);
    assert_eq!(counts.borrow().finishes, 1);
    assert_eq!(counts.borrow().closed, 1);
}

#[test]
fn unknown_gap_gates_encode_in_same_turn_before_another_capture_quantum() {
    let mut b = Storage::new();
    let counts = Rc::new(RefCell::new(Counts::default()));
    let mut p = pipeline(&mut b, counts.clone(), ReadKind::Overrun, 48000, 128).unwrap();
    assert_eq!(p.capture_step(1).unwrap().continuity.capture_overruns, 1);
    assert_eq!(p.encode_step(1), Err(PipelineError::UnknownGap));
    assert!(counts.borrow().encodes.is_empty());
    assert_eq!(counts.borrow().reads, 1);
    assert_eq!(counts.borrow().closed, 1);
    assert_eq!(counts.borrow().finishes, 1);
}
