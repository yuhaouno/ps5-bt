// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure SBC selections shared by negotiation and encoding; no codec dependency.
//!
//! Counts and bitpool/bitrate limits follow A2DP SBC. See
//! `docs/SBC-INTEGRATION.md` for source evidence and the narrower MVP encoder
//! proposal. Representing a selection does not advertise encoder support.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcSampleRate {
    Hz16000,
    Hz32000,
    Hz44100,
    Hz48000,
}

impl SbcSampleRate {
    pub const fn hz(self) -> u32 {
        match self {
            Self::Hz16000 => 16000,
            Self::Hz32000 => 32000,
            Self::Hz44100 => 44100,
            Self::Hz48000 => 48000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcChannelMode {
    Mono,
    DualChannel,
    Stereo,
    JointStereo,
}

impl SbcChannelMode {
    pub const fn channels(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::DualChannel | Self::Stereo | Self::JointStereo => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcBlocks {
    Four,
    Eight,
    Twelve,
    Sixteen,
}

impl SbcBlocks {
    pub const fn count(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Twelve => 12,
            Self::Sixteen => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcSubbands {
    Four,
    Eight,
}

impl SbcSubbands {
    pub const fn count(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbcAllocation {
    Loudness,
    Snr,
}

/// Inclusive advertised/negotiated bounds, distinct from a selected bitpool.
/// Does not assert all values are valid for a particular mode or frame size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbcBitpoolRange {
    min: u8,
    max: u8,
}

impl SbcBitpoolRange {
    pub fn new(min: u8, max: u8) -> Result<Self, Error> {
        if min < 2 || max > 250 || min > max {
            return Err(Error::Invalid);
        }
        Ok(Self { min, max })
    }

    pub const fn min(self) -> u8 {
        self.min
    }

    pub const fn max(self) -> u8 {
        self.max
    }

    pub const fn contains(self, bitpool: u8) -> bool {
        bitpool >= self.min && bitpool <= self.max
    }

    /// Empty intersections are invalid; never substitute a fallback bitpool.
    pub fn intersect(self, peer: Self) -> Result<Self, Error> {
        Self::new(self.min.max(peer.min), self.max.min(peer.max))
    }
}

/// One fully selected SBC configuration. Enum values are not capability masks.
/// Private fields preserve validated mode/subband and A2DP bitrate bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbcParameters {
    sample_rate: SbcSampleRate,
    channel_mode: SbcChannelMode,
    blocks: SbcBlocks,
    subbands: SbcSubbands,
    allocation: SbcAllocation,
    bitpool: u8,
}

impl SbcParameters {
    pub fn new(
        sample_rate: SbcSampleRate,
        channel_mode: SbcChannelMode,
        blocks: SbcBlocks,
        subbands: SbcSubbands,
        allocation: SbcAllocation,
        bitpool: u8,
    ) -> Result<Self, Error> {
        let factor = match channel_mode {
            SbcChannelMode::Mono | SbcChannelMode::DualChannel => 16,
            SbcChannelMode::Stereo | SbcChannelMode::JointStereo => 32,
        };
        if !(2..=250).contains(&bitpool) || usize::from(bitpool) > factor * subbands.count() {
            return Err(Error::Invalid);
        }
        let parameters = Self {
            sample_rate,
            channel_mode,
            blocks,
            subbands,
            allocation,
            bitpool,
        };
        let limit = if channel_mode == SbcChannelMode::Mono {
            320_000
        } else {
            512_000
        };
        // Compare the exact rational bitrate; rounding must not admit >limit.
        if parameters.frame_length_bytes() as u64 * 8 * u64::from(sample_rate.hz())
            > limit * parameters.pcm_frames() as u64
        {
            return Err(Error::Invalid);
        }
        Ok(parameters)
    }

    pub const fn sample_rate(self) -> SbcSampleRate {
        self.sample_rate
    }

    pub const fn channel_mode(self) -> SbcChannelMode {
        self.channel_mode
    }

    pub const fn blocks(self) -> SbcBlocks {
        self.blocks
    }

    pub const fn subbands(self) -> SbcSubbands {
        self.subbands
    }

    pub const fn allocation(self) -> SbcAllocation {
        self.allocation
    }

    pub const fn bitpool(self) -> u8 {
        self.bitpool
    }

    /// PCM frames per encoded SBC frame, irrespective of channel count.
    /// Also the RTP timestamp increment at the selected sample clock rate.
    pub const fn pcm_frames(self) -> usize {
        self.blocks.count() * self.subbands.count()
    }

    /// Individual interleaved i16 samples; distinct from PCM frames and bytes.
    pub const fn pcm_samples(self) -> usize {
        self.pcm_frames() * self.channel_mode.channels()
    }

    pub const fn pcm_bytes(self) -> usize {
        self.pcm_samples() * 2
    }

    /// Header + scale factors + rounded-up audio/joint bits; no RTP header.
    pub const fn frame_length_bytes(self) -> usize {
        let subbands = self.subbands.count();
        let channels = self.channel_mode.channels();
        let audio_bits = match self.channel_mode {
            SbcChannelMode::Mono | SbcChannelMode::DualChannel => {
                self.blocks.count() * channels * self.bitpool as usize
            }
            SbcChannelMode::Stereo => self.blocks.count() * self.bitpool as usize,
            SbcChannelMode::JointStereo => subbands + self.blocks.count() * self.bitpool as usize,
        };
        4 + (4 * subbands * channels) / 8 + audio_bits.div_ceil(8)
    }

    /// Check the selected value against the peer-agreed inclusive range.
    /// This does not prove other configuration fields were negotiated.
    pub fn validate_negotiated_bitpool(self, range: SbcBitpoolRange) -> Result<(), Error> {
        if range.contains(self.bitpool) {
            Ok(())
        } else {
            Err(Error::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(
        frequency: SbcSampleRate,
        mode: SbcChannelMode,
        blocks: SbcBlocks,
        subbands: SbcSubbands,
        bitpool: u8,
    ) -> Result<SbcParameters, Error> {
        SbcParameters::new(
            frequency,
            mode,
            blocks,
            subbands,
            SbcAllocation::Loudness,
            bitpool,
        )
    }

    #[test]
    fn published_quality_examples_and_whole_frame_units() {
        // A2DP recommended joint-stereo examples: 44.1k/53 and 48k/51.
        for (rate, pool, length) in [
            (SbcSampleRate::Hz44100, 53, 119),
            (SbcSampleRate::Hz48000, 51, 115),
        ] {
            let config = selected(
                rate,
                SbcChannelMode::JointStereo,
                SbcBlocks::Sixteen,
                SbcSubbands::Eight,
                pool,
            )
            .unwrap();
            assert_eq!(config.frame_length_bytes(), length);
            assert_eq!(config.pcm_frames(), 128);
            assert_eq!(config.pcm_samples(), 256);
            assert_eq!(config.pcm_bytes(), 512);
        }
        let mono = selected(
            SbcSampleRate::Hz48000,
            SbcChannelMode::Mono,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            29,
        )
        .unwrap();
        assert_eq!(mono.frame_length_bytes(), 66);
        let dual = selected(
            SbcSampleRate::Hz48000,
            SbcChannelMode::DualChannel,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            32,
        )
        .unwrap();
        assert_eq!(dual.frame_length_bytes(), 140);
        assert_eq!(dual.pcm_samples(), 256);
        assert_eq!(
            (mono.pcm_frames(), mono.pcm_samples(), mono.pcm_bytes()),
            (128, 128, 256)
        );
    }

    #[test]
    fn mvp_frame_bounds_and_joint_bit_rounding() {
        for (mode, lengths) in [
            (SbcChannelMode::Stereo, [16, 76, 118]),
            (SbcChannelMode::JointStereo, [17, 77, 119]),
        ] {
            for (pool, expected) in [2, 32, 53].into_iter().zip(lengths) {
                let config = selected(
                    SbcSampleRate::Hz48000,
                    mode,
                    SbcBlocks::Sixteen,
                    SbcSubbands::Eight,
                    pool,
                )
                .unwrap();
                assert_eq!(config.frame_length_bytes(), expected);
            }
        }
        // Four joint bits plus twelve audio bits occupy exactly two octets.
        let config = selected(
            SbcSampleRate::Hz16000,
            SbcChannelMode::JointStereo,
            SbcBlocks::Four,
            SbcSubbands::Four,
            3,
        )
        .unwrap();
        assert_eq!(config.frame_length_bytes(), 10);
        assert_eq!(config.pcm_frames(), 16);
    }

    #[test]
    fn bitpool_and_bitrate_limits_are_separate() {
        let make = |mode, pool| {
            selected(
                SbcSampleRate::Hz16000,
                mode,
                SbcBlocks::Sixteen,
                SbcSubbands::Four,
                pool,
            )
        };
        for pool in [0, 1, 251, 255] {
            assert_eq!(make(SbcChannelMode::Stereo, pool), Err(Error::Invalid));
        }
        assert!(make(SbcChannelMode::Mono, 64).is_ok());
        assert_eq!(make(SbcChannelMode::Mono, 65), Err(Error::Invalid));
        // Four-subband stereo at its mode limit already exceeds the bitrate cap.
        assert_eq!(make(SbcChannelMode::Stereo, 128), Err(Error::Invalid));
        assert!(
            selected(
                SbcSampleRate::Hz16000,
                SbcChannelMode::Stereo,
                SbcBlocks::Sixteen,
                SbcSubbands::Eight,
                250,
            )
            .is_ok()
        );
        assert_eq!(make(SbcChannelMode::Stereo, 129), Err(Error::Invalid));
        // Both are legal bitpools for 8 subbands, but one exceeds A2DP bitrate.
        let stereo = |pool| {
            selected(
                SbcSampleRate::Hz48000,
                SbcChannelMode::Stereo,
                SbcBlocks::Sixteen,
                SbcSubbands::Eight,
                pool,
            )
        };
        assert!(stereo(79).is_ok()); // 510000 bit/s.
        assert_eq!(stereo(80), Err(Error::Invalid)); // 516000 bit/s.
        let mono = |pool| {
            selected(
                SbcSampleRate::Hz48000,
                SbcChannelMode::Mono,
                SbcBlocks::Sixteen,
                SbcSubbands::Eight,
                pool,
            )
        };
        assert!(mono(49).is_ok());
        assert_eq!(mono(50), Err(Error::Invalid));
    }

    #[test]
    fn ranges_reject_invalid_and_disjoint_peer_selections() {
        for (min, max) in [(0, 53), (1, 53), (2, 251), (54, 53)] {
            assert_eq!(SbcBitpoolRange::new(min, max), Err(Error::Invalid));
        }
        let local = SbcBitpoolRange::new(2, 53).unwrap();
        let agreed = local
            .intersect(SbcBitpoolRange::new(32, 250).unwrap())
            .unwrap();
        assert_eq!((agreed.min(), agreed.max()), (32, 53));
        assert!(agreed.contains(32) && agreed.contains(53));
        assert!(!agreed.contains(31) && !agreed.contains(54));
        assert_eq!(
            local.intersect(SbcBitpoolRange::new(54, 60).unwrap()),
            Err(Error::Invalid)
        );
        let config = selected(
            SbcSampleRate::Hz48000,
            SbcChannelMode::Stereo,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            32,
        )
        .unwrap();
        assert_eq!(config.validate_negotiated_bitpool(agreed), Ok(()));
        assert_eq!(
            config.validate_negotiated_bitpool(SbcBitpoolRange::new(33, 53).unwrap()),
            Err(Error::Invalid)
        );
    }
}
