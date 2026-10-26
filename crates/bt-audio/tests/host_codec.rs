// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-libsbc")]
mod common;
use bt_audio::{Action, AudioScheduler, BlockEncoder, Config, HostSbcEncoder};
use bt_codec::{Encoder, EncoderStorage};
use bt_core::{
    Error, SbcBitpoolRange,
    media::{SampleClock, TimedStereoFrame},
};
use bt_hci::AclPacket;
use bt_l2cap::BasicFrame;
use bt_media_session::{SendProgress, TransmitOutcome};

#[test]
fn real_continuous_pcm_to_sbc_rtp_acl_matches_independent_encoder_and_sample_clock() {
    let parameters = common::parameters();
    let mut encoder_storage = EncoderStorage::<8192>::new();
    let encoder = Encoder::new(
        &mut encoder_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut interleaved = [0; 256];
    let mut encoder = HostSbcEncoder::new(encoder, &mut interleaved).unwrap();
    let mut reference_storage = EncoderStorage::<8192>::new();
    let mut reference = Encoder::new(
        &mut reference_storage,
        parameters,
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut source_storage = [0; 128];
    let mut source = common::streaming(&mut source_storage);
    let mut pcm_storage = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 119];
    let mut audio = AudioScheduler::new(
        Config {
            parameters,
            epoch_us: 1_000,
            max_lateness_us: 1_000,
            max_concealed_frames: 128,
            max_rebuffer_us: 10_000,
            deadline_us: Some(3_000_000),
        },
        &mut pcm_storage,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let clock = SampleClock::new(48_000, 1_000).unwrap();
    let mut credits = common::credits();
    let mut transport = common::Transport::default();
    // 96,000 normalized PCM frames at nominal 48 kHz. No sleeps/physical timing.
    for index in 0..750_u64 {
        let first_sample = index * 128;
        let now = clock.deadline_us(first_sample).unwrap();
        let input: [[i16; 2]; 128] = core::array::from_fn(|i| {
            let value = (((first_sample + i as u64) % 101) as i16 - 50) * 200;
            [value, -value]
        });
        audio.push_pcm(first_sample, &input).unwrap();
        let Action::Ready(block) = audio.pull(now, &mut encoder).unwrap() else {
            panic!("complete SBC block")
        };
        let mut reference_pcm = [0; 256];
        for (frame, out) in input.iter().zip(reference_pcm.chunks_exact_mut(2)) {
            out.copy_from_slice(frame);
        }
        let mut reference_bytes = [0; 119];
        let reference_frame = reference
            .encode_frame(&reference_pcm, &mut reference_bytes)
            .unwrap();
        assert_eq!(
            audio.encoded_frame(block.ticket).unwrap(),
            &reference_bytes[..reference_frame.bytes]
        );
        assert_eq!(reference_frame.bytes, 77);
        let queued = source
            .queue_sbc_frames_at(
                block.first_sample,
                &[audio.encoded_frame(block.ticket).unwrap()],
                &mut [0; 90],
                &mut [0; 94],
            )
            .unwrap();
        assert_eq!(
            (queued.rtp.sequence, queued.rtp.timestamp),
            (20 + index as u16, 128 + first_sample as u32)
        );
        audio.confirm_accepted(now, block.ticket).unwrap();
        let packet_start = transport.accepted.len();
        for fragment in 0..3 {
            let outcome = source
                .transmit_next(now, &mut credits, &mut transport, &mut [0; 44])
                .unwrap();
            if fragment == 2 {
                assert!(matches!(
                    outcome,
                    TransmitOutcome::Accepted(SendProgress::Complete { fragments: 3, .. })
                ));
            } else {
                assert!(matches!(
                    outcome,
                    TransmitOutcome::Accepted(SendProgress::More { .. })
                ));
            }
            // Only this explicit synthetic completion restores the one credit.
            common::complete(&mut credits);
        }
        let mut reconstructed = [0; 94];
        let mut cursor = 0;
        for bytes in &transport.accepted[packet_start..] {
            let packet = AclPacket::parse(bytes).unwrap();
            reconstructed[cursor..cursor + packet.payload().len()]
                .copy_from_slice(packet.payload());
            cursor += packet.payload().len();
        }
        assert_eq!(cursor, 94);
        let frame = BasicFrame::parse(&reconstructed).unwrap();
        assert_eq!(frame.channel_id(), common::media().remote_cid);
        assert_eq!(&frame.payload()[13..], &reference_bytes[..77]);
    }
    let stats = audio.snapshot().statistics;
    assert_eq!((stats.encoded_blocks, stats.accepted_blocks), (750, 750));
    assert_eq!(
        (stats.rendered_source_frames, stats.concealed_frames),
        (96_000, 0)
    );
    assert_eq!(
        stats.expired_encoded_blocks
            + stats.expired_pcm_frames
            + stats.skipped_sample_frames
            + stats.underrun_windows,
        0
    );
    assert_eq!(
        (
            credits.snapshot().accepted,
            credits.snapshot().completed,
            credits.snapshot().outstanding
        ),
        (2_250, 2_250, 0)
    );
    audio.stop();
    encoder.finish().unwrap();
    reference.finish().unwrap();
}

#[test]
fn host_adapter_checks_complete_input_and_caller_interleaving_storage() {
    let mut storage = EncoderStorage::<8192>::new();
    let encoder = Encoder::new(
        &mut storage,
        common::parameters(),
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        HostSbcEncoder::new(encoder, &mut [0; 255]),
        Err(Error::BufferTooSmall)
    ));
    let mut storage = EncoderStorage::<8192>::new();
    let encoder = Encoder::new(
        &mut storage,
        common::parameters(),
        SbcBitpoolRange::new(2, 53).unwrap(),
    )
    .unwrap();
    let mut scratch = [0; 256];
    let mut encoder = HostSbcEncoder::new(encoder, &mut scratch).unwrap();
    assert_eq!(
        encoder.encode_stereo(&[[0; 2]; 127], &mut [0; 119]),
        Err(Error::Invalid)
    );
    assert_eq!(
        encoder.encode_stereo(&[[0; 2]; 128], &mut [0; 76]),
        Err(Error::BufferTooSmall)
    );
    let mut output = [0; 119];
    assert_eq!(encoder.encode_stereo(&[[0; 2]; 128], &mut output), Ok(77));
    assert_eq!(output[0], 0x9c);
    bt_audio::FinishEncoder::finish(encoder).unwrap();
}
