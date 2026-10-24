// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-libsbc")]

use bt_codec::{Encoder, EncoderStorage};
use bt_core::{
    Error, SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};

const STORAGE: usize = 8192;

fn parameters(mode: SbcChannelMode, bitpool: u8) -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        mode,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        bitpool,
    )
    .unwrap()
}

fn negotiated() -> SbcBitpoolRange {
    SbcBitpoolRange::new(2, 53).unwrap()
}

fn pcm(salt: i16) -> [i16; 256] {
    core::array::from_fn(|index| {
        (index as i16)
            .wrapping_mul(257)
            .wrapping_add(salt.wrapping_mul(811))
            ^ ((index as i16).wrapping_shl((index & 3) as u32))
    })
}

#[test]
fn real_frames_have_exact_lengths_and_consumption() {
    for mode in [SbcChannelMode::Stereo, SbcChannelMode::JointStereo] {
        for bitpool in [2, 32, 53] {
            let config = parameters(mode, bitpool);
            let mut storage = EncoderStorage::<STORAGE>::new();
            let mut encoder = Encoder::new(&mut storage, config, negotiated()).unwrap();
            let input = pcm(i16::from(bitpool));
            let mut output = [0xa5; 119];
            let frame = encoder.encode_frame(&input, &mut output).unwrap();
            assert_eq!(frame.pcm_frames, 128);
            assert_eq!(frame.bytes, config.frame_length_bytes());
            assert_eq!(output[0], 0x9c);
            assert_eq!(output[2], bitpool);
            assert!(output[frame.bytes..].iter().all(|byte| *byte == 0xa5));
            encoder.finish().unwrap();
        }
    }
}

#[test]
fn selection_storage_and_negotiation_are_checked_before_ffi() {
    let config = parameters(SbcChannelMode::JointStereo, 32);
    let mut tiny = EncoderStorage::<1>::new();
    assert!(matches!(
        Encoder::new(&mut tiny, config, negotiated()),
        Err(Error::BufferTooSmall)
    ));
    let mut storage = EncoderStorage::<STORAGE>::new();
    assert!(matches!(
        Encoder::new(&mut storage, config, SbcBitpoolRange::new(2, 31).unwrap()),
        Err(Error::Invalid)
    ));
    let unsupported = SbcParameters::new(
        SbcSampleRate::Hz44100,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap();
    assert!(matches!(
        Encoder::new(&mut storage, unsupported, negotiated()),
        Err(Error::Unsupported)
    ));
}

#[test]
fn rejected_calls_do_not_advance_the_encoder() {
    let config = parameters(SbcChannelMode::JointStereo, 32);
    let mut rejected_storage = EncoderStorage::<STORAGE>::new();
    let mut fresh_storage = EncoderStorage::<STORAGE>::new();
    let mut rejected = Encoder::new(&mut rejected_storage, config, negotiated()).unwrap();
    let mut fresh = Encoder::new(&mut fresh_storage, config, negotiated()).unwrap();
    let input = pcm(7);
    let mut rejected_output = [0; 119];
    let mut fresh_output = [0; 119];

    assert_eq!(
        rejected.encode_frame(&input[..255], &mut rejected_output),
        Err(Error::Invalid)
    );
    let mut extra = [0; 257];
    extra[..256].copy_from_slice(&input);
    assert_eq!(
        rejected.encode_frame(&extra, &mut rejected_output),
        Err(Error::Invalid)
    );
    assert_eq!(
        rejected.encode_frame(&input, &mut rejected_output[..76]),
        Err(Error::BufferTooSmall)
    );

    let rejected_frame = rejected.encode_frame(&input, &mut rejected_output).unwrap();
    let fresh_frame = fresh.encode_frame(&input, &mut fresh_output).unwrap();
    assert_eq!(rejected_frame, fresh_frame);
    assert_eq!(
        &rejected_output[..rejected_frame.bytes],
        &fresh_output[..fresh_frame.bytes]
    );
}

#[test]
fn queried_storage_is_not_a_copied_private_layout() {
    let requirements = Encoder::<STORAGE>::storage_requirements().unwrap();
    assert!(requirements.size > 0 && requirements.size <= STORAGE);
    assert!(requirements.alignment.is_power_of_two());
    assert!(requirements.alignment <= 64);
}
