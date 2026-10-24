// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline-capable PCM policy. No transport, codec, allocation or HCI credits.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaError {
    Invalid,
    Busy,
    Stale,
    Overflow,
}

/// A nominal sample timeline, not a clock-drift estimator or resampler.
#[derive(Clone, Copy, Debug)]
pub struct SampleClock {
    rate_hz: u32,
    epoch_us: u64,
}

impl SampleClock {
    pub fn new(rate_hz: u32, epoch_us: u64) -> Result<Self, MediaError> {
        if rate_hz == 0 {
            return Err(MediaError::Invalid);
        }
        Ok(Self { rate_hz, epoch_us })
    }

    /// Ceiling of the absolute sample deadline: never schedule a frame early.
    /// Derive every deadline from the sample index, not a rounded block duration.
    pub fn deadline_us(&self, frame: u64) -> Result<u64, MediaError> {
        let rate = u64::from(self.rate_hz);
        let whole = (frame / rate)
            .checked_mul(1_000_000)
            .ok_or(MediaError::Overflow)?;
        let fraction = ((frame % rate) * 1_000_000).div_ceil(rate);
        self.epoch_us
            .checked_add(whole)
            .and_then(|value| value.checked_add(fraction))
            .ok_or(MediaError::Overflow)
    }

    /// Last nominal frame whose deadline is no later than now.
    pub fn frame_at(&self, now_us: u64) -> Result<u64, MediaError> {
        let elapsed = now_us.checked_sub(self.epoch_us).ok_or(MediaError::Stale)?;
        let rate = u64::from(self.rate_hz);
        (elapsed / 1_000_000)
            .checked_mul(rate)
            .and_then(|whole| whole.checked_add((elapsed % 1_000_000) * rate / 1_000_000))
            .ok_or(MediaError::Overflow)
    }
}

/// Position counts whole stereo PCM frames, independent of channel count.
/// Storage is caller-owned and holds only unencoded, unsent audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimedStereoFrame {
    pub position: u64,
    pub samples: [i16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderReport {
    pub first_frame: u64,
    pub skipped_timeline_frames: u64,
    pub source_frames: usize,
    pub silence_frames: usize,
    pub expired_frames: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOutcome {
    Rendered(RenderReport),
    /// No queue, timeline, concealment counter or output changes. The caller
    /// must wait/rebuffer or explicitly end the media session, not retry-spin.
    Rebuffer {
        consecutive_missing_frames: usize,
    },
}

/// Ordered PCM staging with observable expiry and bounded zero concealment.
/// This is not an HCI/ACL queue. Never put already-fragmented media in it.
pub struct PcmQueue<'a> {
    storage: &'a mut [TimedStereoFrame],
    head: usize,
    len: usize,
    input_end: Option<u64>,
    rendered_end: Option<u64>,
    missing_run: usize,
    max_missing_frames: usize,
}

impl<'a> PcmQueue<'a> {
    pub fn new(
        storage: &'a mut [TimedStereoFrame],
        max_missing_frames: usize,
    ) -> Result<Self, MediaError> {
        if storage.is_empty() {
            return Err(MediaError::Invalid);
        }
        Ok(Self {
            storage,
            head: 0,
            len: 0,
            input_end: None,
            rendered_end: None,
            missing_run: 0,
            max_missing_frames,
        })
    }

    pub fn queued_frames(&self) -> usize {
        self.len
    }

    pub fn capacity_frames(&self) -> usize {
        self.storage.len()
    }

    fn at(&self, offset: usize) -> TimedStereoFrame {
        self.storage[(self.head + offset) % self.storage.len()]
    }

    /// All-or-nothing. Gaps in capture positions are allowed and remain gaps;
    /// overlapping/reordered/already-rendered input is rejected, never relabeled.
    pub fn push(&mut self, first_frame: u64, samples: &[[i16; 2]]) -> Result<(), MediaError> {
        if samples.is_empty() {
            return Err(MediaError::Invalid);
        }
        let count = u64::try_from(samples.len()).map_err(|_| MediaError::Overflow)?;
        let end = first_frame.checked_add(count).ok_or(MediaError::Overflow)?;
        if self.input_end.is_some_and(|value| first_frame < value)
            || self.rendered_end.is_some_and(|value| first_frame < value)
        {
            return Err(MediaError::Stale);
        }
        if samples.len() > self.storage.len() - self.len {
            return Err(MediaError::Busy);
        }
        for (index, sample) in samples.iter().enumerate() {
            let slot = (self.head + self.len) % self.storage.len();
            self.storage[slot] = TimedStereoFrame {
                position: first_frame + index as u64,
                samples: *sample,
            };
            self.len += 1;
        }
        self.input_end = Some(end);
        Ok(())
    }

    /// Explicit policy action: drop only old, unencoded PCM and report the count.
    /// This does not advance time, add input, modify credits or reset concealment.
    pub fn discard_before(&mut self, first_frame: u64) -> usize {
        let mut dropped = 0;
        while self.len > 0 && self.at(0).position < first_frame {
            self.head = (self.head + 1) % self.storage.len();
            self.len -= 1;
            dropped += 1;
        }
        dropped
    }

    /// Render the explicitly chosen sample window. Its position must not move
    /// backwards; a forward jump is caller-authorized catch-up and is reported
    /// through skipped_timeline_frames/expired_frames. No transport readiness is assumed.
    /// Errors and Rebuffer leave the entire output and queue unchanged.
    pub fn render(
        &mut self,
        first_frame: u64,
        output: &mut [[i16; 2]],
    ) -> Result<RenderOutcome, MediaError> {
        if output.is_empty() || output.len() > self.storage.len() {
            return Err(MediaError::Invalid);
        }
        let count = u64::try_from(output.len()).map_err(|_| MediaError::Overflow)?;
        let end = first_frame.checked_add(count).ok_or(MediaError::Overflow)?;
        if self.rendered_end.is_some_and(|value| first_frame < value) {
            return Err(MediaError::Stale);
        }
        let skipped = self
            .rendered_end
            .map_or(0, |previous| first_frame - previous);
        let mut offset = 0;
        while offset < self.len && self.at(offset).position < first_frame {
            offset += 1;
        }
        let expired = offset;
        let mut missing = self.missing_run;
        let mut source = 0;
        // Preflight prevents partially consuming/outputting a block on rebuffer.
        for position in first_frame..end {
            if offset < self.len && self.at(offset).position == position {
                offset += 1;
                source += 1;
                missing = 0;
            } else {
                missing = missing.checked_add(1).ok_or(MediaError::Overflow)?;
                if missing > self.max_missing_frames {
                    return Ok(RenderOutcome::Rebuffer {
                        consecutive_missing_frames: missing,
                    });
                }
            }
        }
        offset = expired;
        for (index, frame) in output.iter_mut().enumerate() {
            if offset < self.len && self.at(offset).position == first_frame + index as u64 {
                *frame = self.at(offset).samples;
                offset += 1;
            } else {
                *frame = [0; 2];
            }
        }
        self.head = (self.head + offset) % self.storage.len();
        self.len -= offset;
        self.rendered_end = Some(end);
        self.missing_run = missing;
        Ok(RenderOutcome::Rendered(RenderReport {
            first_frame,
            skipped_timeline_frames: skipped,
            source_frames: source,
            silence_frames: output.len() - source,
            expired_frames: expired,
        }))
    }
}
