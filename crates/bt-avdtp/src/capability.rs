// SPDX-License-Identifier: GPL-3.0-or-later

use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};

pub const MEDIA_TRANSPORT_CATEGORY: u8 = 0x01;
pub const MEDIA_CODEC_CATEGORY: u8 = 0x07;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    Truncated,
    InvalidCategory(u8),
    DuplicateCategory(u8),
    InvalidMediaTransport,
    MissingMediaTransport,
    MissingMediaCodec,
    InvalidMediaCodec,
    UnsupportedMediaType(u8),
    UnsupportedCodec(u8),
    UnsupportedSbcParameters,
    InvalidBitpoolRange,
    BitpoolNotSupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceCapability<'a> {
    pub category: u8,
    pub value: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceCapabilities<'a> {
    bytes: &'a [u8],
}

impl<'a> ServiceCapabilities<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, CapabilityError> {
        let mut remaining = bytes;
        let mut categories = 0u16;
        while !remaining.is_empty() {
            if remaining.len() < 2 {
                return Err(CapabilityError::Truncated);
            }
            let category = remaining[0];
            let length = usize::from(remaining[1]);
            if remaining.len() < length + 2 {
                return Err(CapabilityError::Truncated);
            }
            if !(1..=8).contains(&category) {
                return Err(CapabilityError::InvalidCategory(category));
            }
            if category < 16 && category != 0x04 {
                let bit = 1u16 << category;
                if categories & bit != 0 {
                    return Err(CapabilityError::DuplicateCategory(category));
                }
                categories |= bit;
            }
            if category == MEDIA_TRANSPORT_CATEGORY && length != 0 {
                return Err(CapabilityError::InvalidMediaTransport);
            }
            remaining = &remaining[length + 2..];
        }
        Ok(Self { bytes })
    }

    pub fn iter(self) -> ServiceCapabilityIter<'a> {
        ServiceCapabilityIter {
            remaining: self.bytes,
        }
    }

    pub fn conservative_sbc(self) -> Result<SbcCapabilities, CapabilityError> {
        let mut transport = false;
        let mut codec = None;
        for capability in self.iter() {
            if capability.category == MEDIA_TRANSPORT_CATEGORY {
                transport = true;
            } else if capability.category == MEDIA_CODEC_CATEGORY {
                codec = Some(SbcCapabilities::parse(capability.value)?);
            }
        }
        if !transport {
            return Err(CapabilityError::MissingMediaTransport);
        }
        codec.ok_or(CapabilityError::MissingMediaCodec)
    }
}

pub struct ServiceCapabilityIter<'a> {
    remaining: &'a [u8],
}

impl<'a> Iterator for ServiceCapabilityIter<'a> {
    type Item = ServiceCapability<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let category = self.remaining[0];
        let length = usize::from(self.remaining[1]);
        let value = &self.remaining[2..length + 2];
        self.remaining = &self.remaining[length + 2..];
        Some(ServiceCapability { category, value })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbcCapabilities {
    stereo: bool,
    joint_stereo: bool,
    peer_bitpool: SbcBitpoolRange,
}

impl SbcCapabilities {
    /// Parses the six octets following a Media Codec category and retains only
    /// the conservative source choices this crate can actually configure.
    pub fn parse(value: &[u8]) -> Result<Self, CapabilityError> {
        if value.len() != 6 {
            return Err(CapabilityError::InvalidMediaCodec);
        }
        let media_type = value[0] >> 4;
        if value[0] & 0x0f != 0 {
            return Err(CapabilityError::InvalidMediaCodec);
        }
        if media_type != 0 {
            return Err(CapabilityError::UnsupportedMediaType(media_type));
        }
        if value[1] != 0 {
            return Err(CapabilityError::UnsupportedCodec(value[1]));
        }
        let modes = value[2] & 0x0f;
        let fixed = value[3];
        if value[2] & 0x10 == 0 || fixed & 0x10 == 0 || fixed & 0x04 == 0 || fixed & 0x01 == 0 {
            return Err(CapabilityError::UnsupportedSbcParameters);
        }
        let stereo = modes & 0x02 != 0;
        let joint_stereo = modes & 0x01 != 0;
        if !stereo && !joint_stereo {
            return Err(CapabilityError::UnsupportedSbcParameters);
        }
        let peer_bitpool = SbcBitpoolRange::new(value[4], value[5])
            .map_err(|_| CapabilityError::InvalidBitpoolRange)?;
        Ok(Self {
            stereo,
            joint_stereo,
            peer_bitpool,
        })
    }

    pub const fn supports_stereo(self) -> bool {
        self.stereo
    }

    pub const fn supports_joint_stereo(self) -> bool {
        self.joint_stereo
    }

    pub const fn peer_bitpool(self) -> SbcBitpoolRange {
        self.peer_bitpool
    }

    /// Selects one fixed conservative bitpool. Joint stereo is preferred, with
    /// stereo as the only fallback. Other fields are fixed at 48 kHz, 16 blocks,
    /// 8 subbands and loudness allocation.
    pub fn select(self, bitpool: u8) -> Result<SbcConfiguration, CapabilityError> {
        if !self.peer_bitpool.contains(bitpool) {
            return Err(CapabilityError::BitpoolNotSupported);
        }
        let channel_mode = if self.joint_stereo {
            SbcChannelMode::JointStereo
        } else if self.stereo {
            SbcChannelMode::Stereo
        } else {
            return Err(CapabilityError::UnsupportedSbcParameters);
        };
        let parameters = SbcParameters::new(
            SbcSampleRate::Hz48000,
            channel_mode,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            SbcAllocation::Loudness,
            bitpool,
        )
        .map_err(|_| CapabilityError::BitpoolNotSupported)?;
        Ok(SbcConfiguration {
            parameters,
            peer_bitpool: self.peer_bitpool,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbcConfiguration {
    parameters: SbcParameters,
    peer_bitpool: SbcBitpoolRange,
}

impl SbcConfiguration {
    pub fn new(
        parameters: SbcParameters,
        peer_bitpool: SbcBitpoolRange,
    ) -> Result<Self, CapabilityError> {
        if parameters.sample_rate() != SbcSampleRate::Hz48000
            || !matches!(
                parameters.channel_mode(),
                SbcChannelMode::Stereo | SbcChannelMode::JointStereo
            )
            || parameters.blocks() != SbcBlocks::Sixteen
            || parameters.subbands() != SbcSubbands::Eight
            || parameters.allocation() != SbcAllocation::Loudness
            || !peer_bitpool.contains(parameters.bitpool())
        {
            return Err(CapabilityError::UnsupportedSbcParameters);
        }
        Ok(Self {
            parameters,
            peer_bitpool,
        })
    }

    pub const fn parameters(self) -> SbcParameters {
        self.parameters
    }

    pub const fn peer_bitpool(self) -> SbcBitpoolRange {
        self.peer_bitpool
    }

    pub(crate) fn encode_media_codec(self, output: &mut [u8; 8]) {
        output[0] = MEDIA_CODEC_CATEGORY;
        output[1] = 6;
        output[2] = 0x00; // Audio media type, RFA zero.
        output[3] = 0x00; // SBC.
        output[4] = 0x10
            | match self.parameters.channel_mode() {
                SbcChannelMode::JointStereo => 0x01,
                SbcChannelMode::Stereo => 0x02,
                _ => unreachable!(),
            };
        output[5] = 0x10 | 0x04 | 0x01;
        // A fixed range prevents an encoder from silently changing quality.
        output[6] = self.parameters.bitpool();
        output[7] = self.parameters.bitpool();
    }
}
