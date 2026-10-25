// SPDX-License-Identifier: GPL-3.0-or-later
//! Host-only real SBC through bounded, completion-accounted ACL submissions.

use bt_a2dp::SbcRtpPacketizer;
use bt_codec::{Encoder, EncoderStorage};
use bt_core::media::{PcmQueue, RenderOutcome, TimedStereoFrame};
use bt_core::{
    BluetoothTransport, Error, Received, SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode,
    SbcParameters, SbcSampleRate, SbcSubbands,
};
use bt_hci::{
    AclPacket, ConnectionHandle,
    flow::{AclCreditOwner, CreditContext},
};
use bt_l2cap::{BasicFrame, Connection};
use bt_media_session::{MediaSession, SendProgress, TransmitOutcome};

/// Bounded scripted sink. Its one credit and all completion events below are
/// synthetic; they never establish a real controller reservation.
struct HostTransport {
    busy_once: bool,
    retry: Option<[u8; 44]>,
    accepted: [[u8; 44]; 3],
    lengths: [usize; 3],
    count: usize,
    attempts: usize,
}

impl BluetoothTransport for HostTransport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), Error> {
        unreachable!()
    }
    fn receive<'a>(&mut self, _: &'a mut [u8], _: u32) -> Result<Option<Received<'a>>, Error> {
        unreachable!()
    }
    fn send_acl(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.attempts += 1;
        assert!(bytes.len() <= 44 && self.count < 3);
        if self.busy_once {
            self.busy_once = false;
            self.retry = Some(bytes.try_into().unwrap());
            return Err(Error::Busy);
        }
        if let Some(retry) = self.retry.take() {
            assert_eq!(bytes, retry);
        }
        self.accepted[self.count][..bytes.len()].copy_from_slice(bytes);
        self.lengths[self.count] = bytes.len();
        self.count += 1;
        Ok(())
    }
}

fn main() {
    let mut queue_storage = [TimedStereoFrame::default(); 128];
    let mut queue = PcmQueue::new(&mut queue_storage, 0).unwrap();
    let source: [[i16; 2]; 128] = core::array::from_fn(|index| {
        let sample = ((index as i16 % 40) - 20) * 800;
        [sample, -sample]
    });
    queue.push(0, &source).unwrap();
    let mut rendered = [[0_i16; 2]; 128];
    assert!(matches!(
        queue.render(0, &mut rendered).unwrap(),
        RenderOutcome::Rendered(_)
    ));
    let mut interleaved = [0_i16; 256];
    for (destination, frame) in interleaved.chunks_exact_mut(2).zip(rendered) {
        destination.copy_from_slice(&frame);
    }

    let parameters = SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap();
    let mut encoder_storage = EncoderStorage::<8192>::new();
    let mut encoder = Encoder::new(
        &mut encoder_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut sbc = [0_u8; 119];
    let encoded = encoder.encode_frame(&interleaved, &mut sbc).unwrap();
    encoder.finish().unwrap();
    assert_eq!((encoded.pcm_frames, encoded.bytes), (128, 77));

    let mut rtp_storage = [0_u8; 90];
    let mut packetizer = SbcRtpPacketizer::new(parameters, 90, 96, 0x1122_3344, 0, 0).unwrap();
    packetizer.mark_next_packet();
    let connection = Connection {
        handle: ConnectionHandle::new(0x000b).unwrap(),
        epoch: 1,
    };
    let context = CreditContext {
        handle: connection.handle,
        epoch: connection.epoch,
    };
    let mut retained = [0; 94];
    let mut session = MediaSession::new(packetizer, connection, 0x41, 40, &mut retained).unwrap();
    let mut l2cap_storage = [0_u8; 94];
    let packet = session
        .queue_sbc_frames(
            connection,
            &[&sbc[..encoded.bytes]],
            &mut rtp_storage,
            &mut l2cap_storage,
        )
        .unwrap();
    assert_eq!(
        (packet.rtp.bytes, packet.rtp.frames, packet.rtp.timestamp),
        (90, 1, 0)
    );
    assert_eq!(packet.l2cap_pdu_bytes, 94);
    let mut credits = AclCreditOwner::new(context, 1, 40).unwrap();
    let mut transport = HostTransport {
        busy_once: true,
        retry: None,
        accepted: [[0; 44]; 3],
        lengths: [0; 3],
        count: 0,
        attempts: 0,
    };
    let mut acl_storage = [0; 44];
    assert_eq!(
        session.transmit_next(connection, &mut credits, &mut transport, &mut acl_storage),
        Ok(TransmitOutcome::Busy)
    );
    let completion = [0x13, 5, 1, 11, 0, 1, 0];
    for index in 0..3 {
        let outcome = session
            .transmit_next(connection, &mut credits, &mut transport, &mut acl_storage)
            .unwrap();
        if index < 2 {
            assert!(matches!(
                outcome,
                TransmitOutcome::Accepted(SendProgress::More { .. })
            ));
            let attempts = transport.attempts;
            assert_eq!(
                session.transmit_next(connection, &mut credits, &mut transport, &mut acl_storage),
                Ok(TransmitOutcome::NoCredits)
            );
            assert_eq!(transport.attempts, attempts);
        } else {
            assert!(matches!(
                outcome,
                TransmitOutcome::Accepted(SendProgress::Complete { fragments: 3, .. })
            ));
        }
        credits.complete(context, &completion).unwrap();
    }
    assert_eq!((transport.count, transport.attempts), (3, 4));
    assert_eq!(
        (credits.snapshot().accepted, credits.snapshot().completed),
        (3, 3)
    );
    let mut reconstructed = [0; 94];
    let mut cursor = 0;
    for (bytes, length) in transport.accepted.iter().zip(transport.lengths) {
        let acl = AclPacket::parse(&bytes[..length]).unwrap();
        reconstructed[cursor..cursor + acl.payload().len()].copy_from_slice(acl.payload());
        cursor += acl.payload().len();
    }
    assert_eq!(cursor, 94);
    assert_eq!(reconstructed, l2cap_storage);
    let decoded_l2cap = BasicFrame::parse(&reconstructed).unwrap();
    assert_eq!(decoded_l2cap.channel_id(), 0x0041);
    assert_eq!(&decoded_l2cap.payload()[13..], &sbc[..77]);
    assert_eq!(decoded_l2cap.payload()[0], 0x80);
    assert_eq!(decoded_l2cap.payload()[1], 0xe0);
    assert_eq!(decoded_l2cap.payload()[12], 1);

    println!(
        "host-only pipeline: 128 PCM frames -> 77 SBC bytes -> 90 RTP/A2DP bytes -> 94 L2CAP bytes -> 3 accepted ACL fragments (44/44/18 bytes); one Busy retry, two credit stops, three synthetic completions; no PS5/RF/audio claim"
    );
}
