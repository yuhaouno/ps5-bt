// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::media::{MediaError, PcmQueue, RenderOutcome, SampleClock, TimedStereoFrame};

fn rendered(outcome: RenderOutcome) -> bt_core::media::RenderReport {
    match outcome {
        RenderOutcome::Rendered(report) => report,
        RenderOutcome::Rebuffer { .. } => panic!("expected rendered block"),
    }
}

#[test]
fn absolute_sample_deadlines_do_not_accumulate_block_rounding() {
    for rate in [8_000, 32_000, 44_100, 48_000, 96_000] {
        let clock = SampleClock::new(rate, 17).unwrap();
        for block in 0..10_000_u64 {
            let frame = block * 128;
            let due = clock.deadline_us(frame).unwrap();
            assert_eq!(clock.frame_at(due), Ok(frame));
            if frame != 0 {
                assert!(clock.frame_at(due - 1).unwrap() < frame);
            }
        }
        let hour = u64::from(rate) * 3_600;
        assert_eq!(clock.deadline_us(hour), Ok(3_600_000_017));
    }
    let clock = SampleClock::new(48_000, 0).unwrap();
    assert_eq!(clock.deadline_us(128), Ok(2_667));
    assert_eq!(clock.deadline_us(375 * 128), Ok(1_000_000));
}

#[test]
fn timeline_edges_return_errors_without_wrapping() {
    assert!(matches!(SampleClock::new(0, 0), Err(MediaError::Invalid)));
    let clock = SampleClock::new(1, u64::MAX).unwrap();
    assert_eq!(clock.deadline_us(0), Ok(u64::MAX));
    assert_eq!(clock.deadline_us(1), Err(MediaError::Overflow));
    assert_eq!(clock.frame_at(0), Err(MediaError::Stale));
    assert_eq!(
        SampleClock::new(1, 0).unwrap().deadline_us(u64::MAX),
        Err(MediaError::Overflow)
    );
    assert_eq!(
        SampleClock::new(u32::MAX, 0).unwrap().frame_at(u64::MAX),
        Err(MediaError::Overflow)
    );
}

#[test]
fn full_queue_rejects_atomically_and_wrap_preserves_stereo_order() {
    let mut storage = [TimedStereoFrame::default(); 4];
    let mut queue = PcmQueue::new(&mut storage, 0).unwrap();
    queue.push(0, &[[1, -1], [2, -2], [3, -3]]).unwrap();
    assert_eq!(queue.push(3, &[[4, -4]; 2]), Err(MediaError::Busy));
    assert_eq!(queue.queued_frames(), 3);
    let mut output = [[99; 2]; 2];
    assert_eq!(
        rendered(queue.render(0, &mut output).unwrap()).source_frames,
        2
    );
    assert_eq!(output, [[1, -1], [2, -2]]);
    queue.push(3, &[[4, -4], [5, -5], [6, -6]]).unwrap();
    let mut all = [[99; 2]; 4];
    let report = rendered(queue.render(2, &mut all).unwrap());
    assert_eq!(all, [[3, -3], [4, -4], [5, -5], [6, -6]]);
    assert_eq!(
        (
            report.source_frames,
            report.silence_frames,
            report.expired_frames
        ),
        (4, 0, 0)
    );
    assert_eq!(queue.queued_frames(), 0);
}

#[test]
fn short_gaps_are_counted_but_long_gaps_request_rebuffer_atomically() {
    let mut storage = [TimedStereoFrame::default(); 8];
    let mut queue = PcmQueue::new(&mut storage, 2).unwrap();
    queue.push(0, &[[7, -7]; 2]).unwrap();
    queue.push(4, &[[8, -8]; 2]).unwrap();
    let mut output = [[99; 2]; 6];
    let report = rendered(queue.render(0, &mut output).unwrap());
    assert_eq!(output, [[7, -7], [7, -7], [0, 0], [0, 0], [8, -8], [8, -8]]);
    assert_eq!((report.source_frames, report.silence_frames), (4, 2));
    let mut missing = [[99; 2]; 2];
    assert_eq!(
        rendered(queue.render(6, &mut missing).unwrap()).silence_frames,
        2
    );
    let mut unchanged = [[99; 2]];
    for _ in 0..2 {
        assert_eq!(
            queue.render(8, &mut unchanged),
            Ok(RenderOutcome::Rebuffer {
                consecutive_missing_frames: 3
            })
        );
        assert_eq!(unchanged, [[99; 2]]);
    }
    queue.push(8, &[[9, -9]]).unwrap();
    assert_eq!(
        rendered(queue.render(8, &mut unchanged).unwrap()).source_frames,
        1
    );
    assert_eq!(unchanged, [[9, -9]]);
}

#[test]
fn rebuffer_does_not_consume_real_frames_or_expire_old_data() {
    let mut storage = [TimedStereoFrame::default(); 4];
    let mut queue = PcmQueue::new(&mut storage, 0).unwrap();
    queue.push(0, &[[1, -1], [2, -2]]).unwrap();
    let mut output = [[99; 2]; 2];
    assert!(matches!(
        queue.render(1, &mut output),
        Ok(RenderOutcome::Rebuffer { .. })
    ));
    assert_eq!(output, [[99; 2]; 2]);
    assert_eq!(queue.queued_frames(), 2);
    let report = rendered(queue.render(0, &mut output).unwrap());
    assert_eq!(report.expired_frames, 0);
    assert_eq!(output, [[1, -1], [2, -2]]);
}

#[test]
fn concealment_limit_survives_block_boundaries_and_discard() {
    let mut storage = [TimedStereoFrame::default(); 8];
    let mut queue = PcmQueue::new(&mut storage, 3).unwrap();
    let mut output = [[99; 2]; 2];
    rendered(queue.render(0, &mut output).unwrap());
    assert_eq!(queue.discard_before(2), 0);
    assert_eq!(
        queue.render(2, &mut output),
        Ok(RenderOutcome::Rebuffer {
            consecutive_missing_frames: 4
        })
    );
    queue.push(3, &[[7, -7]]).unwrap();
    let report = rendered(queue.render(2, &mut output).unwrap());
    assert_eq!((report.silence_frames, report.source_frames), (1, 1));
    assert_eq!(output, [[0, 0], [7, -7]]);
    assert_eq!(
        rendered(queue.render(4, &mut output).unwrap()).silence_frames,
        2
    );
}

#[test]
fn future_pcm_is_retained_when_rendering_an_earlier_gap() {
    let mut storage = [TimedStereoFrame::default(); 4];
    let mut queue = PcmQueue::new(&mut storage, 2).unwrap();
    queue.push(2, &[[5, -5]; 2]).unwrap();
    let mut output = [[99; 2]; 2];
    assert_eq!(
        rendered(queue.render(0, &mut output).unwrap()).silence_frames,
        2
    );
    assert_eq!(queue.queued_frames(), 2);
    assert_eq!(output, [[0; 2]; 2]);
    assert_eq!(
        rendered(queue.render(2, &mut output).unwrap()).source_frames,
        2
    );
    assert_eq!(output, [[5, -5]; 2]);
}

#[test]
fn late_pcm_is_explicitly_discarded_and_catchup_is_reported() {
    let mut storage = [TimedStereoFrame::default(); 8];
    let mut queue = PcmQueue::new(&mut storage, 0).unwrap();
    queue.push(0, &[[1, -1]; 8]).unwrap();
    let mut output = [[99; 2]; 2];
    rendered(queue.render(0, &mut output).unwrap());
    assert_eq!(queue.discard_before(4), 2);
    assert_eq!(queue.discard_before(4), 0);
    let report = rendered(queue.render(6, &mut output).unwrap());
    assert_eq!(report.skipped_timeline_frames, 4);
    assert_eq!(report.expired_frames, 2);
    assert_eq!(report.first_frame, 6);
    assert_eq!(queue.push(7, &[[2; 2]]), Err(MediaError::Stale));
    assert_eq!(queue.render(7, &mut output), Err(MediaError::Stale));
    queue.push(8, &[[2; 2]; 2]).unwrap();
    assert_eq!(
        rendered(queue.render(8, &mut output).unwrap()).skipped_timeline_frames,
        0
    );
}

#[test]
fn invalid_requests_leave_storage_and_output_unchanged() {
    let mut empty = [];
    assert!(matches!(
        PcmQueue::new(&mut empty, 0),
        Err(MediaError::Invalid)
    ));
    let mut storage = [TimedStereoFrame::default(); 2];
    let mut queue = PcmQueue::new(&mut storage, 0).unwrap();
    assert_eq!(queue.push(0, &[]), Err(MediaError::Invalid));
    assert_eq!(queue.push(u64::MAX, &[[1; 2]]), Err(MediaError::Overflow));
    queue.push(0, &[[1; 2]; 2]).unwrap();
    assert_eq!(queue.push(1, &[[1; 2]]), Err(MediaError::Stale));
    let mut output = [[99; 2]; 3];
    assert_eq!(queue.render(0, &mut output), Err(MediaError::Invalid));
    assert_eq!(
        queue.render(u64::MAX, &mut output[..1]),
        Err(MediaError::Overflow)
    );
    assert_eq!(queue.queued_frames(), 2);
    assert_eq!(output, [[99; 2]; 3]);
    let report = rendered(queue.render(0, &mut output[..2]).unwrap());
    assert_eq!(report.source_frames, 2);
}

#[test]
fn deterministic_wrap_stress_matches_a_reference_queue() {
    use std::collections::VecDeque;
    let mut reference = VecDeque::new();
    let mut storage = [TimedStereoFrame::default(); 17];
    let mut queue = PcmQueue::new(&mut storage, 0).unwrap();
    let mut position = 0_u64;
    let mut state = 13_u32;
    for _ in 0..10_000 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let requested = ((state >> 16) % 5 + 1) as usize;
        let count = requested.min(queue.capacity_frames() - queue.queued_frames());
        if count > 0 {
            let samples: Vec<_> = (0..count)
                .map(|i| [(position + i as u64) as i16; 2])
                .collect();
            queue.push(position, &samples).unwrap();
            for (i, sample) in samples.into_iter().enumerate() {
                reference.push_back((position + i as u64, sample));
            }
            position += count as u64;
        }
        let count = ((state >> 8) as usize % 7).min(reference.len());
        if count > 0 {
            let first = reference.front().unwrap().0;
            let mut output = vec![[99; 2]; count];
            let report = rendered(queue.render(first, &mut output).unwrap());
            for sample in output {
                assert_eq!(sample, reference.pop_front().unwrap().1);
            }
            assert_eq!(report.silence_frames, 0);
        }
        assert_eq!(queue.queued_frames(), reference.len());
    }
}
