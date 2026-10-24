// SPDX-License-Identifier: GPL-3.0-or-later
//! Host-only offline proof: PcmQueue render output enters the real SBC encoder.

use bt_codec::{Encoder, EncoderStorage};
use bt_core::media::{PcmQueue, RenderOutcome, TimedStereoFrame};
use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};

fn main() {
    let mut queue_storage = [TimedStereoFrame::default(); 128];
    let mut queue = PcmQueue::new(&mut queue_storage, 0).unwrap();
    let source: [[i16; 2]; 128] = core::array::from_fn(|index| {
        let sample = ((index as i16 % 40) - 20) * 800;
        [sample, -sample]
    });
    queue.push(0, &source).unwrap();
    let mut rendered = [[0_i16; 2]; 128];
    let report = match queue.render(0, &mut rendered).unwrap() {
        RenderOutcome::Rendered(report) => report,
        RenderOutcome::Rebuffer { .. } => panic!("complete offline frame rebuffered"),
    };
    assert_eq!((report.source_frames, report.silence_frames), (128, 0));

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
    let mut output = [0_u8; 119];
    let frame = encoder.encode_frame(&interleaved, &mut output).unwrap();
    encoder.finish().unwrap();

    assert_eq!((frame.pcm_frames, frame.bytes), (128, 77));
    assert_eq!(output[0], 0x9c);
    println!(
        "host-only real SBC frame: pcm_frames={} bytes={} sync=0x{:02x}; no RF/audio claim",
        frame.pcm_frames, frame.bytes, output[0]
    );
}
