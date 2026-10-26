// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_audio::{Action, AudioScheduler, Backpressure, BlockEncoder, Config};
use bt_core::{Error as CodecError, SbcParameters, media::TimedStereoFrame};
use bt_media_session::{Error as MediaError, SendProgress, TransmitOutcome};
use bt_session::{Error as SourceError, Phase};

struct SyntheticEncoder;
impl BlockEncoder for SyntheticEncoder {
    fn parameters(&self) -> SbcParameters {
        common::parameters()
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], output: &mut [u8]) -> Result<usize, CodecError> {
        assert_eq!(pcm.len(), 128);
        output.fill(0);
        output[0] = 0x9c;
        Ok(77)
    }
}
fn config() -> Config {
    Config {
        parameters: common::parameters(),
        epoch_us: 0,
        max_lateness_us: 4_000,
        max_concealed_frames: 0,
        max_rebuffer_us: 10_000,
        deadline_us: None,
    }
}

#[test]
fn expired_scheduler_block_preserves_source_partial_pdu_and_next_rtp_sample_position() {
    let mut source_storage = [0; 128];
    let mut source = common::streaming(&mut source_storage);
    let mut pcm = [TimedStereoFrame::default(); 512];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[7, -7]; 384]).unwrap();
    let mut encoder = SyntheticEncoder;
    let Action::Ready(first) = audio.pull(0, &mut encoder).unwrap() else {
        panic!("first block")
    };
    let first_packet = source
        .queue_sbc_frames_at(
            first.first_sample,
            &[audio.encoded_frame(first.ticket).unwrap()],
            &mut [0; 90],
            &mut [0; 94],
        )
        .unwrap();
    assert_eq!(first_packet.rtp.timestamp, 128);
    audio.confirm_accepted(0, first.ticket).unwrap();
    let mut credits = common::credits();
    let mut transport = common::Transport::default();
    assert!(matches!(
        source.transmit_next(0, &mut credits, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::Accepted(SendProgress::More {
            accepted_fragments: 1,
            ..
        }))
    ));
    assert_eq!(
        source.transmit_next(1, &mut credits, &mut transport, &mut [0; 44]),
        Ok(TransmitOutcome::NoCredits)
    );
    let Action::Ready(second) = audio.pull(2_667, &mut encoder).unwrap() else {
        panic!("second block")
    };
    assert_eq!(
        source.queue_sbc_frames_at(
            second.first_sample,
            &[audio.encoded_frame(second.ticket).unwrap()],
            &mut [0; 90],
            &mut [0; 94]
        ),
        Err(SourceError::Media(MediaError::Busy))
    );
    audio
        .reject(2_667, second.ticket, Backpressure::NoCredits)
        .unwrap();
    let Action::Ready(third) = audio.pull(6_667, &mut encoder).unwrap() else {
        panic!("third block")
    };
    assert_eq!(third.first_sample, 256);
    assert_eq!(audio.snapshot().statistics.expired_encoded_blocks, 1);
    assert_eq!(source.take_cancellation(), None); // Scheduler never owns the partial PDU.
    assert_eq!(
        (credits.snapshot().accepted, credits.snapshot().outstanding),
        (1, 1)
    );
    for now in [6_668, 6_669] {
        common::complete(&mut credits);
        source
            .transmit_next(now, &mut credits, &mut transport, &mut [0; 44])
            .unwrap();
    }
    let third_packet = source
        .queue_sbc_frames_at(
            third.first_sample,
            &[audio.encoded_frame(third.ticket).unwrap()],
            &mut [0; 90],
            &mut [0; 94],
        )
        .unwrap();
    assert_eq!(
        (third_packet.rtp.sequence, third_packet.rtp.timestamp),
        (21, 384)
    );
    audio.confirm_accepted(6_669, third.ticket).unwrap();
    for now in 6_670..6_673 {
        common::complete(&mut credits);
        source
            .transmit_next(now, &mut credits, &mut transport, &mut [0; 44])
            .unwrap();
    }
    assert_eq!(transport.accepted.len(), 6);
    assert_eq!(source.take_cancellation(), None);
    assert_eq!(audio.snapshot().statistics.accepted_blocks, 2);
    assert_eq!(audio.snapshot().statistics.expired_encoded_frames, 128);
    assert_eq!(credits.snapshot().outstanding, 1); // Last fragment not completed yet.
}

#[test]
fn scheduler_stop_does_not_cancel_or_refund_an_already_accepted_source_prefix() {
    let mut source_storage = [0; 128];
    let mut source = common::streaming(&mut source_storage);
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[7, -7]; 128]).unwrap();
    let Action::Ready(block) = audio.pull(0, &mut SyntheticEncoder).unwrap() else {
        panic!("block")
    };
    source
        .queue_sbc_frames_at(
            0,
            &[audio.encoded_frame(block.ticket).unwrap()],
            &mut [0; 90],
            &mut [0; 94],
        )
        .unwrap();
    audio.confirm_accepted(0, block.ticket).unwrap();
    let mut credits = common::credits();
    let mut transport = common::Transport::default();
    source
        .transmit_next(0, &mut credits, &mut transport, &mut [0; 44])
        .unwrap();
    audio.stop();
    source.stop(1).unwrap();
    assert_eq!(source.snapshot().phase, Phase::Draining);
    assert_eq!(source.take_cancellation(), None);
    assert_eq!(audio.snapshot().statistics.stopped_encoded_blocks, 0);
    assert_eq!(credits.snapshot().outstanding, 1);
    for now in [2, 3] {
        common::complete(&mut credits);
        source
            .transmit_next(now, &mut credits, &mut transport, &mut [0; 44])
            .unwrap();
    }
    assert_eq!(source.snapshot().phase, Phase::Suspending);
    assert_eq!(source.take_cancellation(), None);
    assert_eq!(credits.snapshot().accepted, 3);
    assert_eq!(credits.snapshot().outstanding, 1);
}
