// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_audio::{AudioScheduler, BlockEncoder, FinishEncoder};
use bt_capture::{
    Backend, Budget, CaptureBridge, CaptureOwner, Diagnostic, Evidence, Format, Layout, ReadKind,
    ReadOutcome, SampleType,
};
use bt_control::{
    Capability,
    http::Policy,
    pump::{ConnectionPump, IoError, SocketIo},
};
use bt_core::{
    BluetoothTransport, Error, Received, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters,
    SbcSampleRate, SbcSubbands, media::TimedStereoFrame,
};
use bt_discovery::ResultSlot;
use bt_hci::flow::{AclCreditOwner, CreditContext};
use bt_runner::{
    ApplicationLoop, ApplicationLoopError, AudioPipeline, Capabilities, CommandSink, Config,
    HttpWork, InputPump, Lane, LoopBuffers, NoHttp, OutputPump, OwnedInput, OwnedPacket,
    OwnedPacketKind, Runner,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
const POLICY: Policy<'_> = Policy {
    host: b"console",
    origin: b"http://console",
    bearer_token: b"synthetic-secret-token",
};
#[derive(Default)]
struct State {
    captures: usize,
    encodes: usize,
    encode_error: bool,
    closes: usize,
    finishes: usize,
    acl: usize,
    input_calls: usize,
    input: VecDeque<(OwnedPacket, Vec<u8>)>,
    commands: Vec<Vec<u8>>,
}
struct Capture(Rc<RefCell<State>>);
impl Backend for Capture {
    fn open(&mut self) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn read_once(&mut self, out: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        self.0.borrow_mut().captures += 1;
        out.fill(0);
        Ok(ReadOutcome {
            kind: ReadKind::Data,
            bytes: out.len(),
            ..ReadOutcome::empty()
        })
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.0.borrow_mut().closes += 1;
        Ok(())
    }
}
struct Encoder(Rc<RefCell<State>>);
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
impl BlockEncoder for Encoder {
    fn parameters(&self) -> SbcParameters {
        parameters()
    }
    fn encode_stereo(&mut self, _: &[[i16; 2]], out: &mut [u8]) -> Result<usize, Error> {
        self.0.borrow_mut().encodes += 1;
        if self.0.borrow().encode_error {
            return Err(Error::Backend(-91));
        }
        out.fill(0);
        out[0] = 0x9c;
        Ok(parameters().frame_length_bytes())
    }
}
impl FinishEncoder for Encoder {
    fn finish(self) -> Result<(), Error> {
        self.0.borrow_mut().finishes += 1;
        Ok(())
    }
}
struct Commands(Rc<RefCell<State>>);
impl CommandSink for Commands {
    fn send_owned_command(&mut self, _: u64, p: &[u8]) -> Result<(), Error> {
        self.0.borrow_mut().commands.push(p.to_vec());
        Ok(())
    }
}
struct Transport(Rc<RefCell<State>>);
impl BluetoothTransport for Transport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("owned commands")
    }
    fn send_acl(&mut self, _: &[u8]) -> Result<(), Error> {
        self.0.borrow_mut().acl += 1;
        Ok(())
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("separate owned router")
    }
}
struct Input(Rc<RefCell<State>>);
impl OwnedInput for Input {
    fn read_once(&mut self, out: &mut [u8]) -> Result<Option<OwnedPacket>, i32> {
        let mut s = self.0.borrow_mut();
        s.input_calls += 1;
        if let Some((packet, data)) = s.input.pop_front() {
            out[..data.len()].copy_from_slice(&data);
            Ok(Some(packet))
        } else {
            Ok(None)
        }
    }
}
type Work<'a, 's, H> =
    ApplicationLoop<'a, 's, common::Store, Capture, Encoder, Commands, Transport, Input, H, 2>;
fn exercise<H: HttpWork>(
    http: H,
    stream: bool,
    check: impl FnOnce(&mut Work<'_, '_, H>, Rc<RefCell<State>>),
) {
    let state = Rc::new(RefCell::new(State::default()));
    let mut b = common::Buffers::default();
    let mut recovery = common::recovery();
    let mut app = common::app(&mut b, &mut recovery);
    if stream {
        common::streaming(&mut app);
    }
    let mut slots = [ResultSlot::empty(); 2];
    let mut runner = Runner::new(
        Config {
            session: 77,
            first_scan_epoch: 1,
            first_link_epoch: 8,
            result_valid_us: 10000,
            inquiry: bt_discovery::Config {
                inquiry_length: 1,
                max_responses: 8,
                scan_timeout_us: 10000,
                cleanup_timeout_us: 1000,
                max_result_events: 8,
            },
        },
        Capabilities {
            transport: Capability::Ready,
            discovery: Capability::Ready,
            capture: Capability::Ready,
            codec: Capability::Ready,
            playback: Capability::Ready,
        },
        &mut slots,
        1,
    )
    .unwrap();
    runner.step(&mut app, 1).unwrap();
    let mut outstorage = [0; 512];
    let mut reassembly = [0; 512];
    let mut inbox = [0; 512];
    let mut raw = [0; 512];
    let mut staging = [[0; 2]; 128];
    let mut block = [[0; 2]; 128];
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 128];
    let format = Format {
        sample_type: SampleType::Signed16Le,
        channels: 2,
        sample_rate_hz: 48000,
        layout: Layout::Interleaved,
        evidence: Evidence::Declared,
    }
    .validate()
    .unwrap();
    let bridge = CaptureBridge::new(format, 0, None, &mut staging, &mut block).unwrap();
    let audio = AudioScheduler::new(
        bt_audio::Config {
            parameters: parameters(),
            epoch_us: 1,
            max_lateness_us: 10000,
            max_concealed_frames: 0,
            max_rebuffer_us: 10000,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let capture = CaptureOwner::open(
        Capture(state.clone()),
        Budget {
            max_reads: 10,
            deadline_us: 10000,
        },
        1,
    )
    .unwrap();
    let mut pipeline =
        AudioPipeline::new(capture, bridge, audio, Encoder(state.clone()), &mut raw).unwrap();
    let mut output = OutputPump::new(common::connection(), 40, &mut outstorage, 10000, 1).unwrap();
    let mut inputpump =
        InputPump::new(common::connection(), &mut reassembly, &mut inbox, 10000, 1).unwrap();
    let mut credits = AclCreditOwner::new(
        CreditContext {
            handle: common::connection().handle,
            epoch: 9,
        },
        1,
        40,
    )
    .unwrap();
    let mut commands = Commands(state.clone());
    let mut transport = Transport(state.clone());
    let mut input = Input(state.clone());
    let mut inputbuf = [0; 1024];
    let mut offer = [0; 1024];
    let mut sdp = [0; 1024];
    let mut rtp = [0; 1024];
    let mut basic = [0; 1024];
    let mut acl = [0; 44];
    let mut work = ApplicationLoop::new(
        &mut app,
        &mut runner,
        &mut commands,
        &mut transport,
        &mut input,
        http,
        POLICY,
        LoopBuffers {
            input: &mut inputbuf,
            offer: &mut offer,
            sdp: &mut sdp,
            rtp: &mut rtp,
            basic: &mut basic,
            acl: &mut acl,
        },
    )
    .unwrap();
    if stream {
        work.attach_connection(&mut inputpump, &mut output, &mut credits)
            .unwrap();
        work.attach_audio(&mut pipeline).unwrap();
    }
    check(&mut work, state);
}
#[derive(Default)]
struct SocketState {
    request: Option<Vec<u8>>,
    closes: usize,
    reads: usize,
}
struct Socket(Rc<RefCell<SocketState>>);
impl SocketIo for Socket {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, IoError> {
        let mut s = self.0.borrow_mut();
        s.reads += 1;
        if let Some(r) = s.request.take() {
            out[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        } else {
            Err(IoError::WouldBlock)
        }
    }
    fn write(&mut self, p: &[u8]) -> Result<usize, IoError> {
        Ok(p.len())
    }
    fn close(&mut self) -> Result<(), i32> {
        self.0.borrow_mut().closes += 1;
        Ok(())
    }
}
#[test]
fn actual_http_stop_is_applied_before_next_capture_or_encode_quantum() {
    let socket=Rc::new(RefCell::new(SocketState {request:Some(b"POST /v1/stop HTTP/1.1\r\nHost: console\r\nAuthorization: Bearer synthetic-secret-token\r\nOrigin: http://console\r\nX-PS5-BT-Control: 1\r\n\r\n".to_vec()),..SocketState::default()}));
    let mut request = [0; 1024];
    let mut response = [0; 2048];
    let http =
        ConnectionPump::new(Socket(socket.clone()), 1, 1000, &mut request, &mut response).unwrap();
    exercise(http, true, |work, state| {
        work.quantum(Lane::Network, 1).unwrap();
        work.quantum(Lane::Control, 1).unwrap();
        assert_eq!(work.application_status().phase, bt_app::Phase::Stopping);
        work.quantum(Lane::Capture, 1).unwrap();
        work.quantum(Lane::Encode, 1).unwrap();
        assert_eq!(state.borrow().captures, 0);
        assert_eq!(state.borrow().encodes, 0);
        assert_eq!(state.borrow().closes, 1);
        assert_eq!(state.borrow().finishes, 1);
        work.request_stop(1).unwrap();
    });
    assert_eq!(socket.borrow().closes, 1);
    assert_eq!(socket.borrow().reads, 1);
}
#[test]
fn input_completions_release_only_real_shared_credit_for_retained_media() {
    exercise(NoHttp, true, |work, state| {
        work.quantum(Lane::Capture, 1).unwrap();
        work.quantum(Lane::Capture, 1).unwrap();
        work.quantum(Lane::Encode, 1).unwrap();
        work.quantum(Lane::Send, 1).unwrap();
        work.quantum(Lane::Send, 1).unwrap();
        assert_eq!(state.borrow().acl, 1);
        assert_eq!(
            work.quantum(Lane::Send, 1).unwrap().continuity.send_waits,
            1
        );
        assert_eq!(state.borrow().acl, 1);
        let event = common::event(0x13, &[1, 0x42, 0, 1, 0]);
        state.borrow_mut().input.push_back((
            OwnedPacket {
                kind: OwnedPacketKind::Completed(CreditContext {
                    handle: common::connection().handle,
                    epoch: 9,
                }),
                bytes: event.len(),
            },
            event,
        ));
        work.quantum(Lane::Input, 1).unwrap();
        work.quantum(Lane::Send, 1).unwrap();
        assert_eq!(state.borrow().acl, 2);
        assert_eq!(state.borrow().encodes, 1);
        assert_eq!(state.borrow().input_calls, 1);
    });
}
#[test]
fn bad_owned_input_is_retained_for_diagnosis_and_never_overwritten() {
    exercise(NoHttp, true, |work, state| {
        let packet = OwnedPacket {
            kind: OwnedPacketKind::Completed(CreditContext {
                handle: common::connection().handle,
                epoch: 8,
            }),
            bytes: 2,
        };
        state.borrow_mut().input.push_back((packet, vec![0x13, 0]));
        assert!(matches!(
            work.quantum(Lane::Input, 1),
            Err(ApplicationLoopError::Pump(_))
        ));
        assert_eq!(work.snapshot().pending_input, Some(packet));
        assert_eq!(state.borrow().input_calls, 1);
    });
}
#[test]
fn global_stop_before_connection_closes_partial_http_and_never_reads_capture() {
    let socket = Rc::new(RefCell::new(SocketState::default()));
    let mut request = [0; 1024];
    let mut response = [0; 2048];
    let http =
        ConnectionPump::new(Socket(socket.clone()), 1, 1000, &mut request, &mut response).unwrap();
    exercise(http, false, |work, state| {
        work.quantum(Lane::Network, 1).unwrap();
        work.request_stop(1).unwrap();
        assert!(work.poll_stop(1).unwrap());
        work.quantum(Lane::Capture, 1).unwrap();
        assert_eq!(state.borrow().captures, 0);
        assert!(work.snapshot().protocols_closed);
    });
    assert_eq!(socket.borrow().closes, 1);
}
#[test]
fn individual_http_timeout_is_recorded_without_stopping_audio_owner() {
    let socket = Rc::new(RefCell::new(SocketState::default()));
    let mut request = [0; 1024];
    let mut response = [0; 2048];
    let http =
        ConnectionPump::new(Socket(socket.clone()), 1, 2, &mut request, &mut response).unwrap();
    exercise(http, true, |work, state| {
        work.quantum(Lane::Network, 3).unwrap();
        assert!(work.snapshot().http_error.is_some());
        assert!(!work.snapshot().stopping);
        work.quantum(Lane::Capture, 3).unwrap();
        assert_eq!(state.borrow().captures, 1);
        work.quantum(Lane::Network, 4).unwrap();
    });
    assert_eq!(socket.borrow().closes, 1);
}

#[test]
fn main_loop_quotas_drive_the_typed_application_pipeline_and_stop_before_next_read() {
    use bt_runner::{LoopConfig, LoopPhase, LoopResources, MainLoop, Quantum, Resource};
    // These test resources are constructed above from declared synthetic input.
    // Native factories must implement actual acquisition/cleanup rather than
    // treating this prepared-resource test wrapper as a PS5 backend.
    struct Prepared<'a, 'owner, 'storage, H: HttpWork>(&'a mut Work<'owner, 'storage, H>);
    impl<H: HttpWork> LoopResources for Prepared<'_, '_, '_, H> {
        fn acquire(&mut self, _: Resource, _: u64) -> Result<(), i32> {
            Ok(())
        }
        fn take_stop(&mut self) -> bool {
            false
        }
        fn quantum(&mut self, lane: Lane, now: u64) -> Result<Quantum, i32> {
            self.0.quantum(lane, now).map_err(|_| -1)
        }
        fn request_stop(&mut self, now: u64) -> Result<(), i32> {
            self.0.request_stop(now).map_err(|_| -2)
        }
        fn poll_stop(&mut self, now: u64) -> Result<bool, i32> {
            self.0.poll_stop(now).map_err(|_| -3)
        }
        fn abort_stop(&mut self) {
            self.0.abort_stop();
        }
        fn cleanup(&mut self, _: Resource) -> Result<(), i32> {
            Ok(())
        }
    }
    exercise(NoHttp, true, |work, state| {
        let mut owner = MainLoop::new(
            Prepared(work),
            LoopConfig {
                quotas: [1, 1, 1, 2, 1, 1],
                max_wait_us: 1,
                stop_timeout_us: 10,
                deadline_us: Some(100),
            },
            1,
        )
        .unwrap();
        for _ in 0..5 {
            owner.step(1);
        }
        let turn = owner.step(1);
        assert_eq!(turn.phase, LoopPhase::Running);
        assert_eq!(turn.quanta, [1, 1, 1, 2, 1, 1]);
        assert_eq!(state.borrow().captures, 1);
        assert_eq!(state.borrow().encodes, 1);
        owner.request_stop();
        owner.step(2);
        assert_eq!(state.borrow().captures, 1);
        assert_eq!(state.borrow().finishes, 1);
        assert_eq!(state.borrow().closes, 1);
    });
}

#[test]
fn failed_encode_and_cleanup_remain_visible_in_owner_snapshot() {
    exercise(NoHttp, true, |work, state| {
        state.borrow_mut().encode_error = true;
        work.quantum(Lane::Capture, 1).unwrap();
        work.quantum(Lane::Capture, 1).unwrap();
        assert!(matches!(
            work.quantum(Lane::Encode, 1),
            Err(ApplicationLoopError::Pipeline(_))
        ));
        assert!(work.snapshot().last_error.is_some());
        assert_eq!(work.snapshot().pipeline.unwrap().encode_failures, 1);
        work.request_stop(1).unwrap();
        assert_eq!(work.snapshot().pipeline.unwrap().encode_failures, 1);
        assert!(
            work.snapshot()
                .audio_cleanup
                .unwrap()
                .capture
                .close
                .unwrap()
                .is_ok()
        );
        assert_eq!(work.snapshot().audio_cleanup.unwrap().encoder, Some(Ok(())));
    });
}
