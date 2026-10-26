// SPDX-License-Identifier: GPL-3.0-or-later
//! Finite generated-PCM wiring check. Nominal sample time is synthetic; the
//! separate caller clock only bounds elapsed execution. No capture or transport.
use crate::{Action, AudioScheduler, Config, SbcEncoder};
use bt_codec::{Encoder, EncoderStorage};
use bt_core::{
    Error, SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
    media::{SampleClock, TimedStereoFrame},
};

pub const MAX_BLOCKS: u32 = 750;
pub const MAX_ELAPSED_US: u64 = 2_000_000;
pub const EXPECTED_PREFIX: u32 = 0x9cfd20cb;
pub const EXPECTED_DIGEST: u32 = 0x7784d1fb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Outcome {
    Complete = 0,
    InvalidBudget = 1,
    Clock = 2,
    ClockRegressed = 3,
    Deadline = 4,
    Codec = 5,
    Scheduler = 6,
    Cleanup = 7,
    Overflow = 8,
    BitstreamMismatch = 9,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub outcome: Outcome,
    pub backend_error: Option<Error>,
    pub cleanup_error: Option<Error>,
    pub scheduler_error: Option<crate::Error>,
    pub cleanup_attempted: bool,
    pub blocks: u32,
    pub pcm_frames: u64,
    pub encoded_bytes: u64,
    pub elapsed_us: u64,
    pub digest: u32,
    pub prefix: u32,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            outcome: Outcome::Complete,
            backend_error: None,
            cleanup_error: None,
            scheduler_error: None,
            cleanup_attempted: false,
            blocks: 0,
            pcm_frames: 0,
            encoded_bytes: 0,
            elapsed_us: 0,
            digest: 0x811c9dc5,
            prefix: 0,
        }
    }
}

fn observe(
    now: &mut impl FnMut() -> Result<u64, Error>,
    start: u64,
    previous: &mut u64,
    budget: u64,
    report: &mut Report,
) -> bool {
    let value = match now() {
        Ok(value) => value,
        Err(error) => {
            report.outcome = Outcome::Clock;
            report.backend_error = Some(error);
            return false;
        }
    };
    if value < *previous {
        report.outcome = Outcome::ClockRegressed;
        return false;
    }
    *previous = value;
    report.elapsed_us = value - start;
    if report.elapsed_us >= budget {
        report.outcome = Outcome::Deadline;
        return false;
    }
    true
}

/// At most 750 blocks and two seconds of observed elapsed execution. A stalled
/// clock cannot extend the operation count. The callback must itself return;
/// this synchronous function cannot preempt a hung native call. Completed SBC
/// blocks are hashed then accepted by a discard sink, never sent over Bluetooth.
pub fn run(
    mut now: impl FnMut() -> Result<u64, Error>,
    blocks: u32,
    elapsed_budget_us: u64,
) -> Report {
    let mut report = Report::default();
    if blocks == 0
        || blocks > MAX_BLOCKS
        || elapsed_budget_us == 0
        || elapsed_budget_us > MAX_ELAPSED_US
    {
        report.outcome = Outcome::InvalidBudget;
        return report;
    }
    let start = match now() {
        Ok(value) => value,
        Err(error) => {
            report.outcome = Outcome::Clock;
            report.backend_error = Some(error);
            return report;
        }
    };
    if start.checked_add(elapsed_budget_us).is_none() {
        report.outcome = Outcome::Overflow;
        return report;
    }
    let parameters = SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .expect("fixed reviewed SBC parameters");
    let mut storage = EncoderStorage::<8192>::new();
    let codec = match Encoder::new(
        &mut storage,
        parameters,
        SbcBitpoolRange::new(2, 53).expect("fixed bitpool"),
    ) {
        Ok(codec) => codec,
        Err(error) => {
            report.outcome = Outcome::Codec;
            report.backend_error = Some(error);
            return report;
        }
    };
    let mut interleaved = [0_i16; 256];
    // The fixed storage has exactly the checked 256 interleaved samples. A failed
    // adapter construction would drop the codec; this branch is unreachable for
    // the admitted fixed configuration above.
    let mut encoder =
        SbcEncoder::new(codec, &mut interleaved).expect("fixed interleaving capacity");
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0_i16; 2]; 128];
    let mut output = [0_u8; 119];
    let mut scheduler = AudioScheduler::new(
        Config {
            parameters,
            epoch_us: 0,
            max_lateness_us: 1_000,
            max_concealed_frames: 0,
            max_rebuffer_us: 10_000,
            deadline_us: None,
        },
        &mut pcm,
        &mut render,
        &mut output,
    )
    .expect("fixed scheduler capacities");
    let sample_clock = SampleClock::new(48_000, 0).expect("fixed sample rate");
    let mut previous = start;
    for block in 0..blocks {
        if !observe(
            &mut now,
            start,
            &mut previous,
            elapsed_budget_us,
            &mut report,
        ) {
            break;
        }
        let first_sample = u64::from(block) * 128;
        let input = core::array::from_fn::<_, 128, _>(|i| {
            let value = (((first_sample + i as u64) % 101) as i16 - 50) * 200;
            [value, -value]
        });
        let nominal = sample_clock
            .deadline_us(first_sample)
            .expect("bounded sample positions");
        let result = scheduler
            .push_pcm(first_sample, &input)
            .and_then(|()| scheduler.pull(nominal, &mut encoder));
        let ready = match result {
            Ok(Action::Ready(ready)) => ready,
            _ => {
                report.outcome = Outcome::Scheduler;
                if let Err(error) = result {
                    report.scheduler_error = Some(error);
                    if let crate::Error::Failed(crate::Failure::Encoder(error)) = error {
                        report.backend_error = Some(error);
                    }
                }
                break;
            }
        };
        let bytes = scheduler
            .encoded_frame(ready.ticket)
            .expect("fresh retained ticket");
        if report.blocks == 0 {
            report.prefix = u32::from_be_bytes(bytes[..4].try_into().expect("complete SBC header"));
        }
        for byte in bytes {
            report.digest = (report.digest ^ u32::from(*byte)).wrapping_mul(0x01000193);
        }
        report.blocks += 1;
        report.pcm_frames += 128;
        report.encoded_bytes += bytes.len() as u64;
        if let Err(error) = scheduler.confirm_accepted(nominal, ready.ticket) {
            report.outcome = Outcome::Scheduler;
            report.scheduler_error = Some(error);
            break;
        }
        if !observe(
            &mut now,
            start,
            &mut previous,
            elapsed_budget_us,
            &mut report,
        ) {
            break;
        }
    }
    scheduler.stop();
    if report.outcome == Outcome::Complete
        && blocks == MAX_BLOCKS
        && (report.prefix != EXPECTED_PREFIX || report.digest != EXPECTED_DIGEST)
    {
        report.outcome = Outcome::BitstreamMismatch;
    }
    report.cleanup_attempted = true;
    if let Err(error) = encoder.finish() {
        report.cleanup_error = Some(error);
        if report.outcome == Outcome::Complete {
            report.outcome = Outcome::Cleanup;
        }
    }
    report
}
