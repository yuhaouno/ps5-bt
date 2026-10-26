// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_audio::{AudioScheduler, BlockEncoder};
use bt_core::{BluetoothTransport, Error, Received, SbcParameters};
use bt_hci::flow::{AclCreditOwner, CreditContext};
use bt_l2cap::BasicFrame;
use bt_runner::{CommandSink, InputPump, OutputPump, PumpError, PumpProgress};
use common::{Buffers, Store, app, connection, recovery, secure, streaming};

#[derive(Default)]
struct Io {
    acl: Vec<Vec<u8>>,
    commands: Vec<Vec<u8>>,
    busy: bool,
    error: Option<Error>,
}
impl CommandSink for Io {
    fn send_owned_command(&mut self, _: u64, packet: &[u8]) -> Result<(), Error> {
        self.commands.push(packet.to_vec());
        Ok(())
    }
}
impl BluetoothTransport for Io {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        panic!("commands require separately proven allowance")
    }
    fn send_acl(&mut self, packet: &[u8]) -> Result<(), Error> {
        if self.busy {
            self.busy = false;
            return Err(Error::Busy);
        }
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        self.acl.push(packet.to_vec());
        Ok(())
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        panic!("no hidden reader")
    }
}
fn owner(n: u16, max: usize) -> AclCreditOwner {
    AclCreditOwner::new(
        CreditContext {
            handle: connection().handle,
            epoch: connection().epoch,
        },
        n,
        max,
    )
    .unwrap()
}
fn app_small<'a>(
    b: &'a mut Buffers,
    r: &'a mut bt_core::recovery::Recovery,
) -> bt_app::Application<'a, Store> {
    let mut config = common::config();
    config.acl_data_max = 8;
    // Separate fixed storage; fields are private in shared fixtures, so use the
    // public constructor helper with the explicit proof below instead.
    common::app_with_config(b, r, config)
}
#[test]
fn whole_control_handoff_waits_for_real_credit_and_busy_never_replaces_it() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    secure(&mut a);
    let mut retained = [0; 512];
    let mut pump = OutputPump::new(connection(), 40, &mut retained, 100, 1).unwrap();
    let mut credits = owner(1, 40);
    let mut cmd = Io::default();
    let mut transport = Io {
        busy: true,
        ..Io::default()
    };
    let mut offer = [0; 700];
    let mut basic = [0; 704];
    let mut acl = [0; 44];
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut cmd,
            &mut transport,
            &mut credits,
            &mut offer,
            &mut basic,
            &mut acl
        ),
        Ok(PumpProgress::ControlQueued)
    );
    assert!(pump.pending_control());
    let before = credits.snapshot();
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut cmd,
            &mut transport,
            &mut credits,
            &mut offer,
            &mut basic,
            &mut acl
        ),
        Ok(PumpProgress::WouldBlock)
    );
    assert_eq!(credits.snapshot(), before);
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut cmd,
            &mut transport,
            &mut credits,
            &mut offer,
            &mut basic,
            &mut acl
        ),
        Ok(PumpProgress::ControlAccepted { complete: true })
    );
    assert_eq!(transport.acl.len(), 1);
    assert_eq!(
        BasicFrame::parse(&transport.acl[0][4..])
            .unwrap()
            .channel_id(),
        1
    );
}
#[test]
fn partial_control_budget_is_aggregate_and_prefix_is_consumed_once() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app_small(&mut b, &mut r);
    secure(&mut a);
    let mut retained = [0; 512];
    let mut pump = OutputPump::new(connection(), 8, &mut retained, 10, 1).unwrap();
    let mut credits = owner(1, 8);
    let mut cmd = Io::default();
    let mut io = Io::default();
    pump.poll_control(
        &mut a,
        1,
        &mut cmd,
        &mut io,
        &mut credits,
        &mut [0; 700],
        &mut [0; 704],
        &mut [0; 12],
    )
    .unwrap();
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut cmd,
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 12]
        ),
        Ok(PumpProgress::ControlAccepted { complete: false })
    );
    let before = credits.snapshot();
    assert_eq!(
        pump.poll_control(
            &mut a,
            11,
            &mut cmd,
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 12]
        ),
        Err(PumpError::Deadline)
    );
    let dropped = pump.take_cancellation().unwrap();
    assert_eq!(dropped.accepted_pdu_bytes, 8);
    assert!(pump.take_cancellation().is_none());
    assert_eq!(credits.snapshot().available, before.available);
    assert_eq!(credits.snapshot().outstanding, before.outstanding);
    assert!(credits.snapshot().failure.is_some());
}
struct Encoder(SbcParameters);
impl BlockEncoder for Encoder {
    fn parameters(&self) -> SbcParameters {
        self.0
    }
    fn encode_stereo(&mut self, _: &[[i16; 2]], out: &mut [u8]) -> Result<usize, Error> {
        out[..77].fill(0);
        out[0] = 0x9c;
        Ok(77)
    }
}
#[test]
fn healthy_media_prefix_drains_before_new_control_pdu_is_queued() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let parameters = a.negotiated_parameters().unwrap();
    let mut pcm = [bt_core::media::TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        bt_audio::Config {
            parameters,
            epoch_us: 1,
            max_lateness_us: 100,
            max_concealed_frames: 0,
            max_rebuffer_us: 100,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    audio.push_pcm(0, &[[1, 2]; 128]).unwrap();
    let mut retained = [0; 512];
    let mut pump = OutputPump::new(connection(), 40, &mut retained, 100, 1).unwrap();
    let mut credits = owner(3, 40);
    let mut io = Io::default();
    let mut encoder = Encoder(parameters);
    for _ in 0..2 {
        pump.poll_audio(
            &mut a,
            1,
            &mut audio,
            &mut encoder,
            &mut credits,
            &mut io,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44],
        )
        .unwrap();
    }
    a.stop(1).unwrap();
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut Io::default(),
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        ),
        Ok(PumpProgress::MediaDrain)
    );
    assert_eq!(io.acl.len(), 1);
    for _ in 0..2 {
        pump.poll_audio(
            &mut a,
            1,
            &mut audio,
            &mut encoder,
            &mut credits,
            &mut io,
            &mut [0; 90],
            &mut [0; 94],
            &mut [0; 44],
        )
        .unwrap();
    }
    assert_eq!(io.acl.len(), 3);
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut Io::default(),
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        ),
        Ok(PumpProgress::ControlQueued)
    );
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut Io::default(),
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        ),
        Ok(PumpProgress::NoCredits)
    );
}
#[test]
fn terminal_native_send_cancels_control_without_credit_refund_or_replay() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    secure(&mut a);
    let mut retained = [0; 512];
    let mut pump = OutputPump::new(connection(), 40, &mut retained, 100, 1).unwrap();
    let mut credits = owner(2, 40);
    let mut io = Io {
        error: Some(Error::Io),
        ..Io::default()
    };
    pump.poll_control(
        &mut a,
        1,
        &mut Io::default(),
        &mut io,
        &mut credits,
        &mut [0; 700],
        &mut [0; 704],
        &mut [0; 44],
    )
    .unwrap();
    assert!(
        pump.poll_control(
            &mut a,
            1,
            &mut Io::default(),
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        )
        .is_err()
    );
    let before = credits.snapshot();
    assert!(before.failure.unwrap().send_outcome_uncertain);
    assert_eq!(pump.take_cancellation().unwrap().accepted_fragments, 0);
    assert!(
        pump.poll_control(
            &mut a,
            2,
            &mut Io::default(),
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        )
        .is_err()
    );
    assert_eq!(credits.snapshot(), before);
}
fn packet(cid: u16, payload: &[u8]) -> Vec<u8> {
    let mut basic = vec![0; 4 + payload.len()];
    BasicFrame::new(cid, payload)
        .unwrap()
        .encode(&mut basic)
        .unwrap();
    let mut acl = vec![0x42, 0x20];
    acl.extend_from_slice(&(basic.len() as u16).to_le_bytes());
    acl.extend_from_slice(&basic);
    acl
}
#[test]
fn rejected_owned_input_is_retained_and_blocks_overwrite_until_forwarded() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let mut storage = [0; 512];
    let mut inbox = [0; 512];
    let mut pump = InputPump::new(connection(), &mut storage, &mut inbox, 10, 1).unwrap();
    let acl = packet(0x44, b"foreign media");
    assert!(
        pump.receive(&mut a, connection(), 1, &acl, &mut [0; 512])
            .is_err()
    );
    let original = pump.pending_basic().unwrap().to_vec();
    assert_eq!(
        pump.receive(
            &mut a,
            connection(),
            2,
            &packet(0x44, b"new"),
            &mut [0; 512]
        ),
        Err(PumpError::Busy)
    );
    assert_eq!(pump.pending_basic().unwrap(), original);
    pump.confirm_forwarded();
    pump.tick(&mut a, 100).unwrap();
    assert!(pump.pending_basic().is_none());
}
#[test]
fn retained_input_timeout_is_terminal_and_never_claims_delivery() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    streaming(&mut a);
    let mut storage = [0; 512];
    let mut inbox = [0; 512];
    let mut pump = InputPump::new(connection(), &mut storage, &mut inbox, 10, 1).unwrap();
    assert!(
        pump.receive(
            &mut a,
            connection(),
            1,
            &packet(0x44, b"held"),
            &mut [0; 512]
        )
        .is_err()
    );
    assert_eq!(pump.tick(&mut a, 11), Err(PumpError::Deadline));
    assert!(pump.pending_basic().is_some());
    assert_eq!(a.status().phase, bt_app::Phase::Failed);
}
#[test]
fn foreign_handle_does_not_enter_the_owned_reassembler_or_extend_deadline() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    secure(&mut a);
    let mut storage = [0; 512];
    let mut inbox = [0; 512];
    let mut pump = InputPump::new(connection(), &mut storage, &mut inbox, 10, 1).unwrap();
    let mut acl = packet(0x44, b"x");
    acl[0] = 0x43;
    assert_eq!(
        pump.receive(&mut a, connection(), 1, &acl, &mut [0; 512]),
        Err(PumpError::ForeignContext)
    );
    assert!(pump.pending_basic().is_none());
    pump.tick(&mut a, 100).unwrap();
}

#[test]
fn foreign_application_is_rejected_before_any_platform_handoff() {
    let mut b = Buffers::default();
    let mut r = recovery();
    let mut a = app(&mut b, &mut r);
    secure(&mut a);
    let mut other = connection();
    other.epoch += 1;
    let mut retained = [0; 512];
    let mut pump = OutputPump::new(other, 40, &mut retained, 100, 1).unwrap();
    let mut credits = AclCreditOwner::new(
        CreditContext {
            handle: other.handle,
            epoch: other.epoch,
        },
        2,
        40,
    )
    .unwrap();
    let before = credits.snapshot();
    let mut cmd = Io::default();
    let mut io = Io::default();
    assert_eq!(
        pump.poll_control(
            &mut a,
            1,
            &mut cmd,
            &mut io,
            &mut credits,
            &mut [0; 700],
            &mut [0; 704],
            &mut [0; 44]
        ),
        Err(PumpError::ForeignContext)
    );
    assert!(cmd.commands.is_empty() && io.acl.is_empty());
    assert!(!pump.pending_control());
    assert_eq!(credits.snapshot().available, before.available);
    assert_eq!(credits.snapshot().outstanding, before.outstanding);
    assert!(credits.snapshot().failure.is_some());
    assert_eq!(a.status().phase, bt_app::Phase::Failed);
}
