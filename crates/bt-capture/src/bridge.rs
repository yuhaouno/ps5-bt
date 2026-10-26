// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    CaptureStamp, FormatError, Gap, ReadKind, ReadOutcome, TimeError, Timeline, ValidatedFormat,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Busy,
    InvalidOutcome,
    Format(FormatError),
    Time(TimeError),
    NeedsEpoch,
    Ended,
    InvalidBuffers,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeEvent {
    Data { frames: usize, stamp: CaptureStamp },
    Empty,
    Overrun { known_lost_frames: Option<u64> },
    Ended,
    NativeError(i32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BridgeStatistics {
    pub data_frames: u64,
    pub empty_reads: u64,
    pub overruns: u64,
    pub known_gap_frames: u64,
    pub unknown_gaps: u64,
    pub dropped_partial_frames: u64,
    pub accepted_blocks: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmBlock<'a> {
    pub first_sample: u64,
    pub samples: &'a [[i16; 2]],
}
/// One capture chunk plus one complete encoding block in caller-owned storage.
/// Ingestion refuses to overwrite retained input; confirm only after downstream
/// `push_pcm` succeeds. A known gap discards a partial block with accounting;
/// unknown loss requires an explicit fresh stream epoch before further data.
pub struct CaptureBridge<'a> {
    format: ValidatedFormat,
    timeline: Timeline,
    staging: &'a mut [[i16; 2]],
    staged: usize,
    cursor: usize,
    staged_first: u64,
    block: &'a mut [[i16; 2]],
    filled: usize,
    block_first: u64,
    needs_epoch: bool,
    ended: bool,
    statistics: BridgeStatistics,
}
impl<'a> CaptureBridge<'a> {
    pub fn new(
        format: ValidatedFormat,
        first_sample: u64,
        max_native_step: Option<u64>,
        staging: &'a mut [[i16; 2]],
        block: &'a mut [[i16; 2]],
    ) -> Result<Self, BridgeError> {
        if staging.is_empty() || block.is_empty() {
            return Err(BridgeError::InvalidBuffers);
        }
        Ok(Self {
            format,
            timeline: Timeline::new(first_sample, max_native_step),
            staging,
            staged: 0,
            cursor: 0,
            staged_first: 0,
            block,
            filled: 0,
            block_first: 0,
            needs_epoch: false,
            ended: false,
            statistics: BridgeStatistics::default(),
        })
    }
    pub const fn block_frames(&self) -> usize {
        self.block.len()
    }
    pub const fn format(&self) -> ValidatedFormat {
        self.format
    }
    pub const fn statistics(&self) -> BridgeStatistics {
        self.statistics
    }
    pub const fn needs_epoch(&self) -> bool {
        self.needs_epoch
    }
    pub fn has_retained_input(&self) -> bool {
        self.cursor < self.staged || self.filled == self.block.len()
    }
    pub fn ingest(
        &mut self,
        outcome: ReadOutcome,
        input: &[u8],
        host_us: u64,
        known_lost_frames: Option<u64>,
    ) -> Result<BridgeEvent, BridgeError> {
        if self.ended {
            return Err(BridgeError::Ended);
        }
        if self.needs_epoch {
            return Err(BridgeError::NeedsEpoch);
        }
        if self.has_retained_input() {
            return Err(BridgeError::Busy);
        }
        if outcome.bytes > input.len()
            || (outcome.kind != ReadKind::Data && outcome.bytes != 0)
            || (outcome.kind == ReadKind::Data && outcome.bytes == 0)
            || (known_lost_frames.is_some() && !outcome.discontinuity)
            || (outcome.kind == ReadKind::Overrun && !outcome.discontinuity)
        {
            return Err(BridgeError::InvalidOutcome);
        }
        let gap = if outcome.discontinuity {
            known_lost_frames.map_or(Gap::Unknown, Gap::Frames)
        } else {
            Gap::None
        };
        // Validate/convert before advancing the sample clock. Rejected format is atomic.
        let frames = if outcome.kind == ReadKind::Data {
            self.format
                .normalize(&input[..outcome.bytes], self.staging)
                .map_err(BridgeError::Format)?
        } else {
            0
        };
        let stamp = match self
            .timeline
            .observe(host_us, outcome.native_time, frames, gap)
        {
            Ok(stamp) => stamp,
            Err(TimeError::UnknownGap) => {
                self.needs_epoch = true;
                self.statistics.unknown_gaps = self.statistics.unknown_gaps.saturating_add(1);
                self.discard_partial();
                if outcome.kind == ReadKind::Overrun {
                    self.statistics.overruns = self.statistics.overruns.saturating_add(1);
                    return Ok(BridgeEvent::Overrun {
                        known_lost_frames: None,
                    });
                }
                return Err(BridgeError::Time(TimeError::UnknownGap));
            }
            Err(error) => return Err(BridgeError::Time(error)),
        };
        if outcome.discontinuity {
            self.discard_partial();
            self.statistics.known_gap_frames = self
                .statistics
                .known_gap_frames
                .saturating_add(known_lost_frames.unwrap_or(0));
        }
        match outcome.kind {
            ReadKind::Data => {
                self.staged = frames;
                self.cursor = 0;
                self.staged_first = stamp.first_sample;
                self.statistics.data_frames =
                    self.statistics.data_frames.saturating_add(frames as u64);
                Ok(BridgeEvent::Data { frames, stamp })
            }
            ReadKind::Empty | ReadKind::Zero => {
                self.statistics.empty_reads = self.statistics.empty_reads.saturating_add(1);
                Ok(BridgeEvent::Empty)
            }
            ReadKind::Overrun => {
                self.statistics.overruns = self.statistics.overruns.saturating_add(1);
                Ok(BridgeEvent::Overrun { known_lost_frames })
            }
            ReadKind::Ended | ReadKind::Error => {
                self.ended = true;
                self.discard_partial();
                Ok(if outcome.kind == ReadKind::Ended {
                    BridgeEvent::Ended
                } else {
                    BridgeEvent::NativeError(outcome.raw_status)
                })
            }
        }
    }
    /// Copy at most one block's remainder; caller bounds the number of pumps/round.
    pub fn prepare_block(&mut self) -> Option<PcmBlock<'_>> {
        if self.filled < self.block.len() && self.cursor < self.staged {
            if self.filled == 0 {
                self.block_first = self.staged_first + self.cursor as u64;
            }
            let count = (self.block.len() - self.filled).min(self.staged - self.cursor);
            self.block[self.filled..self.filled + count]
                .copy_from_slice(&self.staging[self.cursor..self.cursor + count]);
            self.filled += count;
            self.cursor += count;
        }
        if self.filled == self.block.len() {
            Some(PcmBlock {
                first_sample: self.block_first,
                samples: self.block,
            })
        } else {
            None
        }
    }
    pub fn confirm_block(&mut self) -> Result<(), BridgeError> {
        if self.filled != self.block.len() {
            return Err(BridgeError::InvalidOutcome);
        }
        self.filled = 0;
        self.statistics.accepted_blocks = self.statistics.accepted_blocks.saturating_add(1);
        Ok(())
    }
    /// Caller must coordinate this with a new downstream stream epoch.
    pub fn restart_epoch(&mut self, first_sample: u64) {
        self.discard_partial();
        self.statistics.dropped_partial_frames = self
            .statistics
            .dropped_partial_frames
            .saturating_add((self.staged - self.cursor) as u64);
        self.staged = 0;
        self.cursor = 0;
        self.timeline.restart(first_sample);
        self.needs_epoch = false;
        self.ended = false;
    }
    fn discard_partial(&mut self) {
        self.statistics.dropped_partial_frames = self
            .statistics
            .dropped_partial_frames
            .saturating_add(self.filled as u64);
        self.filled = 0;
    }
}
