// SPDX-License-Identifier: GPL-3.0-or-later
//! Declared host-only PCM fixture, real persistent SBC, synthetic transport.
#![cfg(feature = "host-libsbc")]
mod common;
use bt_app::Application;
use bt_audio::{AudioScheduler, BlockEncoder, FinishEncoder, SbcEncoder};
use bt_capture::{
    Backend, Budget, CaptureBridge, CaptureOwner, Diagnostic, Evidence, Format, Layout, ReadKind,
    ReadOutcome, SampleType,
};
use bt_codec::{Encoder, EncoderStorage};
use bt_core::{
    BluetoothTransport, Error, Received, SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode,
    SbcParameters, SbcSampleRate, SbcSubbands,
    media::{SampleClock, TimedStereoFrame},
};
use bt_hci::{
    AclPacket,
    flow::{AclCreditOwner, CreditContext},
};
use bt_l2cap::BasicFrame;
use bt_runner::{AudioPipeline, OutputPump, PipelineError, SendBuffers};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Calls {
    open: usize,
    start: usize,
    reads: usize,
    stop: usize,
    close: usize,
    encodes: usize,
    finish: usize,
}
fn sample(position: usize) -> [i16; 2] {
    let v = ((position % 101) as i16 - 50) * 200;
    [v, -v]
}
struct Capture {
    calls: Rc<RefCell<Calls>>,
    position: usize,
}
impl Backend for Capture {
    fn open(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().open += 1;
        Ok(())
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().start += 1;
        Ok(())
    }
    fn read_once(&mut self, output: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        let index = self.calls.borrow().reads;
        // Deliberately cross all 128-frame encoder boundaries with short chunks
        // and a chunk containing more than two encoder blocks.
        let frames = [37, 260, 19, 196][index];
        self.calls.borrow_mut().reads += 1;
        let bytes = frames * 4;
        assert!(bytes <= output.len());
        for (i, destination) in output[..bytes].chunks_exact_mut(4).enumerate() {
            let [l, r] = sample(self.position + i);
            destination[..2].copy_from_slice(&l.to_le_bytes());
            destination[2..].copy_from_slice(&r.to_le_bytes());
        }
        self.position += frames;
        Ok(ReadOutcome {
            kind: ReadKind::Data,
            bytes,
            raw_status: bytes as i32,
            ..ReadOutcome::empty()
        })
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().stop += 1;
        Ok(())
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().close += 1;
        Ok(())
    }
}
struct Counted<E> {
    codec: E,
    calls: Rc<RefCell<Calls>>,
}
impl<E: BlockEncoder> BlockEncoder for Counted<E> {
    fn parameters(&self) -> SbcParameters {
        self.codec.parameters()
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], output: &mut [u8]) -> Result<usize, Error> {
        self.calls.borrow_mut().encodes += 1;
        self.codec.encode_stereo(pcm, output)
    }
}
impl<E: FinishEncoder> FinishEncoder for Counted<E> {
    fn finish(self) -> Result<(), Error> {
        self.calls.borrow_mut().finish += 1;
        self.codec.finish()
    }
}
#[derive(Default)]
struct Transport {
    busy: bool,
    offered: Vec<Vec<u8>>,
    accepted: Vec<Vec<u8>>,
}
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("no implicit command")
    }
    fn send_acl(&mut self, packet: &[u8]) -> Result<(), Error> {
        self.offered.push(packet.to_vec());
        if self.busy {
            self.busy = false;
            return Err(Error::Busy);
        }
        self.accepted.push(packet.to_vec());
        Ok(())
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("no implicit reader")
    }
}
struct Wire<'a> {
    app: Application<'a, common::Store>,
    output: OutputPump<'a>,
    credits: AclCreditOwner,
    transport: Transport,
}
impl Wire<'_> {
    fn send<B: Backend, E: FinishEncoder>(
        &mut self,
        pipeline: &mut AudioPipeline<'_, B, E>,
        now: u64,
    ) -> bt_runner::Quantum {
        pipeline
            .send_step(
                &mut self.app,
                &mut self.output,
                &mut self.credits,
                &mut self.transport,
                now,
                SendBuffers {
                    rtp: &mut [0; 128],
                    basic: &mut [0; 132],
                    acl: &mut [0; 44],
                },
            )
            .unwrap()
    }
    fn complete(&mut self, now: u64) {
        self.output
            .complete(
                &mut self.app,
                &mut self.credits,
                now,
                CreditContext {
                    handle: common::connection().handle,
                    epoch: 9,
                },
                &common::event(0x13, &[1, 0x42, 0, 1, 0]),
            )
            .unwrap();
    }
    fn compare_last_frame(&self, index: usize, expected: &[u8]) {
        let mut reassembled = Vec::new();
        for packet in &self.transport.accepted[index * 3..(index + 1) * 3] {
            reassembled.extend_from_slice(AclPacket::parse(packet).unwrap().payload());
        }
        assert_eq!(reassembled.len(), 94);
        let frame = BasicFrame::parse(&reassembled).unwrap();
        assert_eq!(frame.channel_id(), 0x45);
        let payload = frame.payload();
        assert_eq!(
            u16::from_be_bytes(payload[2..4].try_into().unwrap()),
            index as u16
        );
        assert_eq!(
            u32::from_be_bytes(payload[4..8].try_into().unwrap()),
            128 + index as u32 * 128
        );
        assert_eq!(payload[12], 1); // one whole SBC frame
        assert_eq!(&payload[13..], expected);
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

#[test]
fn declared_capture_chunks_reach_real_sbc_and_owned_fragments_without_loss_or_reencode() {
    let parameters = parameters();
    let calls = Rc::new(RefCell::new(Calls::default()));
    let mut codec_storage = EncoderStorage::<8192>::new();
    let codec = Encoder::new(
        &mut codec_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut interleaved = [0; 256];
    let encoder = Counted {
        codec: SbcEncoder::new(codec, &mut interleaved).unwrap(),
        calls: calls.clone(),
    };
    let owner = CaptureOwner::open(
        Capture {
            calls: calls.clone(),
            position: 0,
        },
        Budget {
            max_reads: 4,
            deadline_us: 100_000,
        },
        0,
    )
    .unwrap();
    let format = Format {
        sample_type: SampleType::Signed16Le,
        channels: 2,
        sample_rate_hz: 48_000,
        layout: Layout::Interleaved,
        evidence: Evidence::Declared,
    }
    .validate()
    .unwrap();
    let mut staging = [[0; 2]; 260];
    let mut block = [[0; 2]; 128];
    let bridge = CaptureBridge::new(format, 0, None, &mut staging, &mut block).unwrap();
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 119];
    let audio = AudioScheduler::new(
        bt_audio::Config {
            parameters,
            epoch_us: 1,
            max_lateness_us: 10_000,
            max_concealed_frames: 0,
            max_rebuffer_us: 10_000,
            deadline_us: Some(100_000),
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let mut raw = [0; 1040];
    let mut pipeline = AudioPipeline::new(owner, bridge, audio, encoder, &mut raw).unwrap();
    let mut reference_storage = EncoderStorage::<8192>::new();
    let mut reference = Encoder::new(
        &mut reference_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut app_buffers = common::Buffers::default();
    let mut recovery = common::recovery();
    let mut app = common::app(&mut app_buffers, &mut recovery);
    common::streaming(&mut app);
    let mut output_storage = [0; 512];
    let mut wire = Wire {
        app,
        output: OutputPump::new(common::connection(), 40, &mut output_storage, 100_000, 1).unwrap(),
        credits: AclCreditOwner::new(
            CreditContext {
                handle: common::connection().handle,
                epoch: 9,
            },
            1,
            40,
        )
        .unwrap(),
        transport: Transport {
            busy: true,
            ..Transport::default()
        },
    };
    let clock = SampleClock::new(48_000, 1).unwrap();

    pipeline.capture_step(1).unwrap(); // 37
    pipeline.capture_step(1).unwrap(); // retain 37, read 260
    pipeline.capture_step(1).unwrap(); // block zero queued, 169 remain
    assert!(!pipeline.capture_step(1).unwrap().progressed); // block one retained: queue Busy
    assert!(!pipeline.capture_step(1).unwrap().progressed);
    assert_eq!(calls.borrow().reads, 2);
    assert_eq!(calls.borrow().encodes, 0);
    assert!(pipeline.encode_step(1).unwrap().progressed);
    pipeline.capture_step(1).unwrap(); // block one accepted
    pipeline.capture_step(1).unwrap(); // retain 41, read 19
    pipeline.capture_step(1).unwrap(); // retain 60, read 196
    assert!(!pipeline.capture_step(1).unwrap().progressed); // block two retained
    assert_eq!(calls.borrow().reads, 4);

    for index in 0..4 {
        let now = clock.deadline_us(index as u64 * 128).unwrap();
        if index > 0 {
            assert!(pipeline.encode_step(now).unwrap().progressed);
            if index < 3 {
                assert!(pipeline.capture_step(now).unwrap().progressed);
                if index == 1 {
                    assert!(!pipeline.capture_step(now).unwrap().progressed);
                }
            }
        }
        assert_eq!(calls.borrow().encodes, index + 1);
        let mut input = [0; 256];
        for (i, frame) in input.chunks_exact_mut(2).enumerate() {
            frame.copy_from_slice(&sample(index * 128 + i));
        }
        let mut expected = [0; 119];
        let encoded = reference.encode_frame(&input, &mut expected).unwrap();
        assert_eq!(encoded.bytes, 77);
        assert!(wire.send(&mut pipeline, now).progressed); // complete PDU ownership transferred
        if index == 0 {
            assert_eq!(wire.send(&mut pipeline, now).continuity.send_waits, 1); // backend Busy
        }
        assert!(wire.send(&mut pipeline, now).progressed); // first accepted fragment, allowance exhausted
        if index == 0 {
            assert_eq!(wire.transport.offered[0], wire.transport.offered[1]);
        }
        assert_eq!(wire.send(&mut pipeline, now).continuity.send_waits, 1); // explicit NoCredits
        if index == 3 {
            let cleanup = pipeline.stop(); // finish codec while healthy prefix remains owned downstream
            assert_eq!(cleanup.encoder, Some(Ok(())));
            assert!(!cleanup.capture.uncertain());
            assert_eq!(pipeline.stop(), cleanup);
            assert_eq!(calls.borrow().finish, 1);
        }
        wire.complete(now); // only an explicit synthetic completion restores allowance
        assert!(wire.send(&mut pipeline, now).progressed);
        wire.complete(now);
        assert!(wire.send(&mut pipeline, now).progressed);
        wire.complete(now);
        wire.compare_last_frame(index, &expected[..encoded.bytes]);
        assert_eq!(calls.borrow().encodes, index + 1);
    }
    let snapshot = pipeline.snapshot();
    assert_eq!(snapshot.bridge.data_frames, 512);
    assert_eq!(snapshot.bridge.accepted_blocks, 4);
    assert_eq!(snapshot.audio.statistics.accepted_blocks, 4);
    assert_eq!(snapshot.audio.statistics.concealed_frames, 0);
    assert_eq!(snapshot.audio.statistics.expired_pcm_frames, 0);
    assert_eq!(snapshot.audio.statistics.expired_encoded_blocks, 0);
    assert_eq!(wire.credits.snapshot().accepted, 12);
    assert_eq!(wire.credits.snapshot().completed, 12);
    assert_eq!(wire.credits.snapshot().outstanding, 0);
    assert!(!wire.send(&mut pipeline, 8_001).progressed);
    assert_eq!(pipeline.capture_step(8_001), Err(PipelineError::Stopped));
    assert_eq!(pipeline.encode_step(8_001), Err(PipelineError::Stopped));
    reference.finish().unwrap();
    drop(pipeline);
    let counts = calls.borrow();
    assert_eq!(
        (
            counts.open,
            counts.start,
            counts.reads,
            counts.stop,
            counts.close,
            counts.finish
        ),
        (1, 1, 4, 1, 1, 1)
    );
}
