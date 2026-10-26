// SPDX-License-Identifier: GPL-3.0-or-later
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Unknown,
    Requested,
    /// Explicit offline input contract; never evidence about native capture.
    Declared,
    HardwareVerified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleType {
    Unknown,
    Signed16Le,
    Float32Le,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Unknown,
    Interleaved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub sample_type: SampleType,
    pub channels: u8,
    pub sample_rate_hz: u32,
    pub layout: Layout,
    pub evidence: Evidence,
}
impl Format {
    pub const UNKNOWN: Self = Self {
        sample_type: SampleType::Unknown,
        channels: 0,
        sample_rate_hz: 0,
        layout: Layout::Unknown,
        evidence: Evidence::Unknown,
    };
    pub fn validate(self) -> Result<ValidatedFormat, FormatError> {
        if !matches!(
            self.evidence,
            Evidence::Declared | Evidence::HardwareVerified
        ) {
            return Err(FormatError::UnknownFormat);
        }
        let width = match self.sample_type {
            SampleType::Signed16Le => 2,
            SampleType::Float32Le => 4,
            SampleType::Unknown => return Err(FormatError::UnknownFormat),
        };
        if !matches!(self.channels, 1 | 2)
            || self.layout != Layout::Interleaved
            || !(8000..=192000).contains(&self.sample_rate_hz)
        {
            return Err(FormatError::Unsupported);
        }
        Ok(ValidatedFormat {
            format: self,
            frame_bytes: width * self.channels as usize,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatError {
    UnknownFormat,
    Unsupported,
    IncompleteFrame,
    Capacity,
    NonFinite,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedFormat {
    format: Format,
    frame_bytes: usize,
}
impl ValidatedFormat {
    pub const fn description(self) -> Format {
        self.format
    }
    pub const fn frame_bytes(self) -> usize {
        self.frame_bytes
    }
    /// Atomic: invalid lengths, capacity or nonfinite samples leave output intact.
    /// Mono is duplicated. Float saturation/truncation uses IEEE-754 bit arithmetic,
    /// requiring no native floating-point ABI, SIMD state or soft-float runtime.
    pub fn normalize(self, input: &[u8], output: &mut [[i16; 2]]) -> Result<usize, FormatError> {
        if !input.len().is_multiple_of(self.frame_bytes) {
            return Err(FormatError::IncompleteFrame);
        }
        let frames = input.len() / self.frame_bytes;
        if frames > output.len() {
            return Err(FormatError::Capacity);
        }
        if self.format.sample_type == SampleType::Float32Le {
            for sample in input.chunks_exact(4) {
                float_sample(sample)?;
            }
        }
        for (frame, target) in input.chunks_exact(self.frame_bytes).zip(output.iter_mut()) {
            let (left, right) = match self.format.sample_type {
                SampleType::Signed16Le => {
                    let l = i16::from_le_bytes([frame[0], frame[1]]);
                    (
                        l,
                        if self.format.channels == 1 {
                            l
                        } else {
                            i16::from_le_bytes([frame[2], frame[3]])
                        },
                    )
                }
                SampleType::Float32Le => {
                    let l = float_sample(&frame[..4])?;
                    (
                        l,
                        if self.format.channels == 1 {
                            l
                        } else {
                            float_sample(&frame[4..8])?
                        },
                    )
                }
                SampleType::Unknown => return Err(FormatError::UnknownFormat),
            };
            *target = [left, right];
        }
        Ok(frames)
    }
}
fn float_sample(bytes: &[u8]) -> Result<i16, FormatError> {
    let bits = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let exponent = (bits >> 23) & 255;
    if exponent == 255 {
        return Err(FormatError::NonFinite);
    }
    let negative = bits >> 31 != 0;
    if exponent >= 127 {
        return Ok(if negative { i16::MIN } else { i16::MAX });
    }
    if exponent < 112 {
        return Ok(0);
    }
    let magnitude = (((bits & 0x7fffff) | 0x800000) >> (135 - exponent)) as i16;
    Ok(if negative { -magnitude } else { magnitude })
}
