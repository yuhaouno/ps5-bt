// SPDX-License-Identifier: GPL-3.0-or-later
use bt_capture::{Budget, Evidence, Format};
use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use bt_listener::BindConfig;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    ControlOnly,
    CaptureObserve,
    GeneratedSbc,
    NativeAudio,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncoderConfig {
    pub parameters: SbcParameters,
    pub negotiated_bitpool: SbcBitpoolRange,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureConfig {
    pub budget: Budget,
    pub format: Format,
}
/// Paths are protected local configuration, never accepted from the LAN service.
/// Configuration Debug is deliberately absent to avoid accidentally logging paths.
#[derive(Clone, Copy)]
pub struct Config<'a> {
    pub mode: Mode,
    pub store_directory: &'a [u8],
    pub bind: BindConfig,
    pub capture: Option<CaptureConfig>,
    pub encoder: Option<EncoderConfig>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Directory,
    Bind,
    ModeResources,
    UnknownFormat,
    UnverifiedNativeFormat,
    Format,
    Encoder,
    RateMismatch,
    CaptureBudget,
    EncoderUnavailable,
}
impl<'a> Config<'a> {
    pub const fn control_only(store_directory: &'a [u8], bind: BindConfig) -> Self {
        Self {
            mode: Mode::ControlOnly,
            store_directory,
            bind,
            capture: None,
            encoder: None,
        }
    }
    pub fn validate(self) -> Result<(), ConfigError> {
        if self.store_directory.len() < 2
            || self.store_directory.len() > bt_store::MAX_PATH_BYTES
            || self.store_directory[0] != b'/'
            || self.store_directory.contains(&0)
        {
            return Err(ConfigError::Directory);
        }
        if self.bind.address[0] == 0
            || self.bind.address[0] >= 224
            || (self.bind.port == 0 && self.bind.address != [127, 0, 0, 1])
        {
            return Err(ConfigError::Bind);
        }
        match self.mode {
            Mode::ControlOnly if self.capture.is_none() && self.encoder.is_none() => {}
            Mode::CaptureObserve if self.capture.is_some() && self.encoder.is_none() => {}
            Mode::GeneratedSbc if self.capture.is_none() && self.encoder.is_some() => {}
            Mode::NativeAudio if self.capture.is_some() && self.encoder.is_some() => {}
            _ => return Err(ConfigError::ModeResources),
        }
        if let Some(capture) = self.capture {
            if capture.budget.max_reads == 0 || capture.budget.deadline_us == 0 {
                return Err(ConfigError::CaptureBudget);
            }
            if self.mode == Mode::CaptureObserve && capture.format != Format::UNKNOWN {
                return Err(ConfigError::ModeResources);
            }
            if self.mode == Mode::NativeAudio {
                if matches!(
                    capture.format.evidence,
                    Evidence::Unknown | Evidence::Requested
                ) {
                    return Err(ConfigError::UnknownFormat);
                }
                if capture.format.evidence != Evidence::HardwareVerified {
                    return Err(ConfigError::UnverifiedNativeFormat);
                }
                let format = capture.format.validate().map_err(|_| ConfigError::Format)?;
                if self.encoder.is_some_and(|encoder| {
                    encoder.parameters.sample_rate().hz() != format.description().sample_rate_hz
                }) {
                    return Err(ConfigError::RateMismatch);
                }
            }
        }
        if let Some(encoder) = self.encoder {
            let p = encoder.parameters;
            if p.sample_rate() != SbcSampleRate::Hz48000
                || !matches!(
                    p.channel_mode(),
                    SbcChannelMode::Stereo | SbcChannelMode::JointStereo
                )
                || p.blocks() != SbcBlocks::Sixteen
                || p.subbands() != SbcSubbands::Eight
                || p.allocation() != SbcAllocation::Loudness
                || p.bitpool() > 53
            {
                return Err(ConfigError::Encoder);
            }
            encoder
                .parameters
                .validate_negotiated_bitpool(encoder.negotiated_bitpool)
                .map_err(|_| ConfigError::Encoder)?;
        }
        Ok(())
    }
    pub const fn required(self, resource: crate::ResourceKind) -> bool {
        match resource {
            crate::ResourceKind::Clock
            | crate::ResourceKind::Store
            | crate::ResourceKind::Listener => true,
            crate::ResourceKind::Capture => self.capture.is_some(),
            crate::ResourceKind::Encoder => self.encoder.is_some(),
        }
    }
}
