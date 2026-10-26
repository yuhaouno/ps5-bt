// SPDX-License-Identifier: GPL-3.0-or-later
//! Finite observed-clock generated PCM -> existing SBC scheduler -> discard sink.
//! No capture, radio, connection, packetization or controller credit exists here.
use bt_audio::{Action, AudioScheduler, BlockEncoder};
use bt_core::{
    SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate, SbcSubbands,
    media::{MediaError, SampleClock, TimedStereoFrame},
};

pub const DEFAULT_BLOCKS: u32 = 750;
pub const DEFAULT_DURATION_US: u64 = 2_000_000;
pub const DEFAULT_OPERATIONS: u32 = 60_000;
pub const DEFAULT_LATENESS_US: u64 = 100_000;
pub const MAX_BLOCKS: u32 = 1875;
pub const MAX_DURATION_US: u64 = 5_000_000;
pub const MAX_OPERATIONS: u32 = 100_000;
const FRAMES: usize = 128;

pub struct GeneratedStorage {
    input: [[i16; 2]; FRAMES],
    pcm: [TimedStereoFrame; FRAMES],
    render: [[i16; 2]; FRAMES],
    encoded: [u8; 119],
}
impl GeneratedStorage {
    pub const fn new() -> Self {
        Self {
            input: [[0; 2]; FRAMES],
            pcm: [TimedStereoFrame {
                position: 0,
                samples: [0; 2],
            }; FRAMES],
            render: [[0; 2]; FRAMES],
            encoded: [0; 119],
        }
    }
}
impl Default for GeneratedStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedConfig {
    pub parameters: SbcParameters,
    /// Actual host observation when this workload is admitted after encoder init.
    pub epoch_us: u64,
    pub duration_us: u64,
    pub max_blocks: u32,
    /// Counts active generate/encode/discard calls (including Waiting/Busy) and
    /// the one successful post-discard completion observation. Confirmed success
    /// needs at least three calls per block plus one final observation.
    pub max_operations: u32,
    pub max_lateness_us: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Complete,
    Stopped,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Completed,
    Requested,
    Failure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedFailure {
    ClockRegression,
    Deadline,
    OperationBudget,
    Overflow,
    Continuity,
    ExpiredOffer,
    SummaryMismatch,
    Audio(bt_audio::Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedError {
    InvalidConfig,
    NotComplete,
    Stopped,
    Failed(GeneratedFailure),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub progressed: bool,
    pub wake_at_us: Option<u64>,
    pub finished: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedSnapshot {
    pub phase: Phase,
    pub reason: Option<StopReason>,
    pub error: Option<GeneratedFailure>,
    pub operations: u32,
    /// Last checked host observation minus admission epoch; no acoustic timing.
    pub elapsed_us: u64,
    pub generated_blocks: u32,
    pub discarded_blocks: u32,
    pub discarded_bytes: u32,
    pub generation_backpressure: u32,
    pub staged_frames: u32,
    pub stopped_staged_frames: u32,
    pub prefix: u32,
    pub digest: u32,
    /// Pinned full 750-block profile only; None while partial or unpinned.
    pub summary_matches: Option<bool>,
    /// A fresh post-discard host observation verified completion strictly before
    /// the deadline. Complete frame counts alone never establish this evidence.
    pub completion_observed: bool,
    pub scheduler: bt_audio::Snapshot,
}

pub struct GeneratedWorkload<'storage> {
    config: GeneratedConfig,
    deadline: u64,
    clock: SampleClock,
    audio: AudioScheduler<'storage>,
    input: &'storage mut [[i16; 2]; FRAMES],
    prepared: bool,
    last_now: u64,
    phase: Phase,
    reason: Option<StopReason>,
    error: Option<GeneratedFailure>,
    operations: u32,
    generated_blocks: u32,
    discarded_blocks: u32,
    discarded_bytes: u32,
    generation_backpressure: u32,
    stopped_staged_frames: u32,
    prefix: u32,
    digest: u32,
    completion_observed: bool,
}
impl<'storage> GeneratedWorkload<'storage> {
    pub fn new(
        config: GeneratedConfig,
        storage: &'storage mut GeneratedStorage,
    ) -> Result<Self, GeneratedError> {
        let p = config.parameters;
        if p.sample_rate() != SbcSampleRate::Hz48000
            || !matches!(
                p.channel_mode(),
                SbcChannelMode::JointStereo | SbcChannelMode::Stereo
            )
            || p.blocks() != SbcBlocks::Sixteen
            || p.subbands() != SbcSubbands::Eight
            || p.allocation() != SbcAllocation::Loudness
            || p.bitpool() != 32
            || config.max_blocks == 0
            || config.max_blocks > MAX_BLOCKS
            || config.duration_us == 0
            || config.duration_us > MAX_DURATION_US
            || config.max_operations < config.max_blocks * 3
            || config.max_operations > MAX_OPERATIONS
            || config.max_lateness_us == 0
            || config.max_lateness_us > DEFAULT_LATENESS_US
        {
            return Err(GeneratedError::InvalidConfig);
        }
        let deadline = config
            .epoch_us
            .checked_add(config.duration_us)
            .ok_or(GeneratedError::InvalidConfig)?;
        let clock =
            SampleClock::new(48_000, config.epoch_us).map_err(|_| GeneratedError::InvalidConfig)?;
        let nominal_end = clock
            .deadline_us(u64::from(config.max_blocks) * FRAMES as u64)
            .map_err(|_| GeneratedError::InvalidConfig)?;
        if nominal_end > deadline {
            return Err(GeneratedError::InvalidConfig);
        }
        let audio = AudioScheduler::new(
            bt_audio::Config {
                parameters: p,
                epoch_us: config.epoch_us,
                max_lateness_us: config.max_lateness_us,
                max_concealed_frames: 0,
                max_rebuffer_us: config.max_lateness_us,
                deadline_us: Some(deadline),
            },
            &mut storage.pcm,
            &mut storage.render,
            &mut storage.encoded,
        )
        .map_err(|_| GeneratedError::InvalidConfig)?;
        storage.input.fill([0; 2]);
        Ok(Self {
            config,
            deadline,
            clock,
            audio,
            input: &mut storage.input,
            prepared: false,
            last_now: config.epoch_us,
            phase: Phase::Running,
            reason: None,
            error: None,
            operations: 0,
            generated_blocks: 0,
            discarded_blocks: 0,
            discarded_bytes: 0,
            generation_backpressure: 0,
            stopped_staged_frames: 0,
            prefix: 0,
            digest: 0x811c9dc5,
            completion_observed: false,
        })
    }
    pub fn snapshot(&self) -> GeneratedSnapshot {
        GeneratedSnapshot {
            phase: self.phase,
            reason: self.reason,
            error: self.error,
            operations: self.operations,
            elapsed_us: self.last_now - self.config.epoch_us,
            generated_blocks: self.generated_blocks,
            discarded_blocks: self.discarded_blocks,
            discarded_bytes: self.discarded_bytes,
            generation_backpressure: self.generation_backpressure,
            staged_frames: if self.prepared { FRAMES as u32 } else { 0 },
            stopped_staged_frames: self.stopped_staged_frames,
            prefix: self.prefix,
            digest: self.digest,
            summary_matches: if self.discarded_blocks == self.config.max_blocks {
                self.expected_summary()
                    .map(|(prefix, digest)| self.prefix == prefix && self.digest == digest)
            } else {
                None
            },
            completion_observed: self.completion_observed,
            scheduler: self.audio.snapshot(),
        }
    }
    fn expected_summary(&self) -> Option<(u32, u32)> {
        if self.config.max_blocks != DEFAULT_BLOCKS {
            return None;
        }
        match self.config.parameters.channel_mode() {
            SbcChannelMode::JointStereo => Some((0x9cfd20cb, 0x7784d1fb)),
            SbcChannelMode::Stereo => Some((0x9cf920f8, 0x1f2ed77b)),
            _ => None,
        }
    }
    fn progress(&self, progressed: bool, wake: Option<u64>) -> Progress {
        Progress {
            progressed,
            wake_at_us: if self.phase == Phase::Running {
                Some(wake.map_or(self.deadline, |due| due.min(self.deadline)))
            } else {
                None
            },
            finished: self.phase != Phase::Running,
        }
    }
    fn check(&mut self, now: u64) -> Result<bool, GeneratedError> {
        match self.phase {
            Phase::Complete => return Ok(false),
            Phase::Stopped => return Err(GeneratedError::Stopped),
            Phase::Failed => {
                return Err(GeneratedError::Failed(
                    self.error.unwrap_or(GeneratedFailure::Continuity),
                ));
            }
            Phase::Running => {}
        }
        if now < self.last_now {
            return self.fail(GeneratedFailure::ClockRegression);
        }
        self.last_now = now;
        if now >= self.deadline {
            return self.fail(GeneratedFailure::Deadline);
        }
        if self.operations == self.config.max_operations {
            return self.fail(GeneratedFailure::OperationBudget);
        }
        self.operations += 1;
        Ok(true)
    }
    fn fail<T>(&mut self, error: GeneratedFailure) -> Result<T, GeneratedError> {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.stop();
        self.phase = Phase::Failed;
        self.reason = Some(StopReason::Failure);
        Err(GeneratedError::Failed(self.error.unwrap_or(error)))
    }
    fn audio_error<T>(&mut self, error: bt_audio::Error) -> Result<T, GeneratedError> {
        self.fail(GeneratedFailure::Audio(error))
    }
    fn continuity(&mut self) -> Result<(), GeneratedError> {
        let stats = self.audio.snapshot().statistics;
        if stats.concealed_frames != 0
            || stats.expired_pcm_frames != 0
            || stats.skipped_sample_frames != 0
            || stats.expired_encoded_blocks != 0
            || stats.late_acceptances != 0
        {
            self.fail(GeneratedFailure::Continuity)
        } else {
            Ok(())
        }
    }
    /// At most one fixed 128-frame generation/queue offer. Real host now must
    /// reach its nominal sample deadline; no synthetic timestamp drives pacing.
    pub fn generate(&mut self, now: u64) -> Result<Progress, GeneratedError> {
        if !self.check(now)? {
            return Ok(self.progress(false, None));
        }
        if self.generated_blocks == self.config.max_blocks {
            return Ok(self.progress(false, None));
        }
        let sample = u64::from(self.generated_blocks) * FRAMES as u64;
        let due = match self.clock.deadline_us(sample) {
            Ok(due) => due,
            Err(_) => return self.fail(GeneratedFailure::Overflow),
        };
        if now < due {
            return Ok(self.progress(false, Some(due)));
        }
        if !self.prepared {
            for (offset, frame) in self.input.iter_mut().enumerate() {
                let value = (((sample + offset as u64) % 101) as i16 - 50) * 200;
                *frame = [value, -value];
            }
            self.prepared = true;
        }
        match self.audio.push_pcm(sample, self.input) {
            Ok(()) => {
                self.generated_blocks += 1;
                self.prepared = false;
                self.input.fill([0; 2]);
                Ok(self.progress(true, None))
            }
            Err(bt_audio::Error::Pcm(MediaError::Busy)) => {
                self.generation_backpressure += 1;
                Ok(self.progress(false, None))
            }
            Err(error) => self.audio_error(error),
        }
    }
    /// Borrow the factory's one persistent encoder for at most one codec call.
    /// A pending frame remains byte-for-byte retained; it is never regenerated.
    pub fn encode<E: BlockEncoder + ?Sized>(
        &mut self,
        now: u64,
        encoder: &mut E,
    ) -> Result<Progress, GeneratedError> {
        if !self.check(now)? {
            return Ok(self.progress(false, None));
        }
        let before = self.audio.snapshot().statistics.encoded_blocks;
        let action = match self.audio.pull(now, encoder) {
            Ok(action) => action,
            Err(error) => return self.audio_error(error),
        };
        self.continuity()?;
        let progressed = self.audio.snapshot().statistics.encoded_blocks != before;
        match action {
            Action::Ready(_) => Ok(self.progress(progressed, None)),
            Action::WaitUntil(due) => Ok(self.progress(false, Some(due))),
            Action::Rebuffer { deadline_us, .. } => Ok(self.progress(false, Some(deadline_us))),
            Action::Stopped => self.fail(GeneratedFailure::Continuity),
        }
    }
    /// Accept and hash only one fresh diagnostic-owned offer, then discard it.
    /// Acceptance here is neither network submission nor controller completion.
    pub fn discard(&mut self, now: u64) -> Result<Progress, GeneratedError> {
        if !self.check(now)? {
            return Ok(self.progress(false, None));
        }
        let Some(block) = self.audio.snapshot().pending else {
            return Ok(self.progress(false, None));
        };
        if now >= block.expires_us {
            return self.fail(GeneratedFailure::ExpiredOffer);
        }
        if block.first_sample != u64::from(self.discarded_blocks) * FRAMES as u64
            || block.source_frames != FRAMES
            || block.concealed_frames != 0
        {
            return self.fail(GeneratedFailure::Continuity);
        }
        let bytes = match self.audio.encoded_frame(block.ticket) {
            Ok(bytes) => bytes,
            Err(error) => return self.audio_error(error),
        };
        if self.discarded_blocks == 0 {
            self.prefix = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for byte in bytes {
            self.digest = (self.digest ^ u32::from(*byte)).wrapping_mul(0x01000193);
        }
        self.discarded_bytes += bytes.len() as u32;
        if let Err(error) = self.audio.confirm_accepted(now, block.ticket) {
            return self.audio_error(error);
        }
        self.discarded_blocks += 1;
        self.continuity()?;
        if self.discarded_blocks == self.config.max_blocks {
            let stats = self.audio.snapshot().statistics;
            if self.generated_blocks != self.config.max_blocks
                || stats.encoded_blocks != u64::from(self.config.max_blocks)
                || self.discarded_bytes
                    != self.config.max_blocks * self.config.parameters.frame_length_bytes() as u32
            {
                return self.fail(GeneratedFailure::SummaryMismatch);
            }
            if self
                .expected_summary()
                .is_some_and(|(prefix, digest)| self.prefix != prefix || self.digest != digest)
            {
                return self.fail(GeneratedFailure::SummaryMismatch);
            }
            self.audio.stop();
            self.phase = Phase::Complete;
            self.reason = Some(StopReason::Completed);
        }
        Ok(self.progress(true, None))
    }
    /// Confirm completion using a fresh host observation taken AFTER final
    /// discard/hash. The caller owns the fallible clock and must skip this step
    /// when Stop is already pending. Complete/summary alone is not elapsed-time
    /// evidence. This call never encodes, hashes, waits or finishes the encoder.
    /// Successful repeats preserve the first confirmed timestamp and consume no
    /// further budget; a rejected first confirmation is terminal and sticky.
    pub fn confirm_completion(&mut self, now: u64) -> Result<(), GeneratedError> {
        match self.phase {
            Phase::Running => return Err(GeneratedError::NotComplete),
            Phase::Stopped => return Err(GeneratedError::Stopped),
            Phase::Failed => {
                return Err(GeneratedError::Failed(
                    self.error.unwrap_or(GeneratedFailure::Continuity),
                ));
            }
            Phase::Complete => {}
        }
        if self.completion_observed {
            return Ok(());
        }
        if now < self.last_now {
            return self.fail(GeneratedFailure::ClockRegression);
        }
        self.last_now = now;
        if now >= self.deadline {
            return self.fail(GeneratedFailure::Deadline);
        }
        if self.operations == self.config.max_operations {
            return self.fail(GeneratedFailure::OperationBudget);
        }
        self.operations += 1;
        self.completion_observed = true;
        Ok(())
    }
    /// Clock-independent, idempotent stop. Only this diagnostic's queued PCM,
    /// staged block and wholly unaccepted SBC offer are discarded. The caller
    /// still owns encoder finish, HTTP shutdown and all native resource cleanup.
    pub fn stop(&mut self) {
        if self.prepared {
            self.stopped_staged_frames += FRAMES as u32;
            self.prepared = false;
        }
        self.input.fill([0; 2]);
        self.audio.stop();
        if self.phase == Phase::Running {
            self.phase = Phase::Stopped;
            self.reason = Some(StopReason::Requested);
        }
    }
}
impl Drop for GeneratedWorkload<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}
