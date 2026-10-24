// SPDX-License-Identifier: GPL-3.0-or-later
//! Host-only deterministic simulation. No codec, radio or audio output.
use bt_core::media::{PcmQueue, RenderOutcome, SampleClock, TimedStereoFrame};
use std::ops::Range;

#[derive(Debug, Default)]
struct Counts {
    rendered: usize,
    source: usize,
    silence: usize,
    expired: usize,
    rebuffer: usize,
    skipped: u64,
    high_water: usize,
}

fn scenario(name: &str, blocked: Range<usize>, capture_gap: Range<usize>) -> Counts {
    const QUANTUM: usize = 128;
    // Simulation settings, not measured or recommended hardware parameters.
    const RETAIN_FRAMES: u64 = 1_920; // 40 ms at 48 kHz
    let clock = SampleClock::new(48_000, 0).unwrap();
    let mut storage = [TimedStereoFrame::default(); 4_096];
    let mut queue = PcmQueue::new(&mut storage, 2 * QUANTUM).unwrap();
    let mut counts = Counts::default();
    for tick in 0..180 {
        let first = tick as u64 * QUANTUM as u64;
        let now = clock.deadline_us(first).unwrap();
        assert_eq!(clock.frame_at(now).unwrap(), first);
        counts.expired += queue.discard_before(first.saturating_sub(RETAIN_FRAMES));
        if !capture_gap.contains(&tick) {
            let mut samples = [[0_i16; 2]; QUANTUM];
            for (index, sample) in samples.iter_mut().enumerate() {
                let value = (((first + index as u64) % 200) as i16 - 100) * 200;
                *sample = [value, -value];
            }
            queue.push(first, &samples).unwrap();
        }
        counts.high_water = counts.high_water.max(queue.queued_frames());
        if blocked.contains(&tick) {
            // Readiness is an explicit script input. It never grants HCI credit.
            continue;
        }
        let mut output = [[32_000_i16; 2]; QUANTUM];
        match queue.render(first, &mut output).unwrap() {
            RenderOutcome::Rendered(report) => {
                counts.rendered += output.len();
                counts.source += report.source_frames;
                counts.silence += report.silence_frames;
                counts.expired += report.expired_frames;
                counts.skipped += report.skipped_timeline_frames;
            }
            RenderOutcome::Rebuffer { .. } => {
                counts.rebuffer += 1;
                assert_eq!(output, [[32_000; 2]; QUANTUM]);
            }
        }
    }
    assert_eq!(counts.rendered, counts.source + counts.silence);
    assert_eq!(queue.queued_frames(), 0);
    println!("{name}: {counts:?}");
    counts
}

fn main() {
    println!("SYNTHETIC host PCM only: no HCI credits, codec, RF or audible-quality claim");
    let steady = scenario("steady", 0..0, 0..0);
    assert_eq!(steady.rendered, 180 * 128);
    assert_eq!((steady.silence, steady.expired, steady.rebuffer), (0, 0, 0));
    let short = scenario("capture gap 5.33 ms", 0..0, 90..92);
    assert_eq!((short.silence, short.rebuffer), (256, 0));
    let long = scenario("capture gap 21.33 ms", 0..0, 100..108);
    assert_eq!((long.silence, long.rebuffer), (256, 6));
    assert_eq!(long.skipped, 6 * 128);
    let stall = scenario("sender blocked 80 ms", 30..60, 0..0);
    assert_eq!(stall.expired, 30 * 128);
    assert_eq!(stall.skipped, 30 * 128);
    assert!(stall.high_water <= 2_048);
    assert_eq!(stall.silence, 0);
}
