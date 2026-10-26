# PS5-BT queue-pressure policy v1

Original PS5-BT C policy, GPL-3.0-or-later. This module is independent of the
upstream protocol implementation. Its input is observed local queue occupancy;
it does not read, infer, grant or return HCI credits and does not change buffer
capacity. The experimental adapter remains a separate HearBridge reference,
with the original license, pin, shared-reader behavior and execution limits.

## Hypothesis and bounds

The captured-audio baseline had two sender drops and approximately two very
brief audible glitches. At the first drop, queue occupancy reached eight and
the upstream controller subsequently reduced bitpool 41 → 37 → 22. Its normal
large-queue response waits 300 ms; two remaining slots at roughly 24 ms per
packet provide much less headroom. This is a hypothesis about rate adjustment,
not proof of the glitch cause or real controller capacity.

- Queue above the upstream four-packet slack suppresses quality increases.
- Consecutive observations at or above ceil(75% of capacity), beyond that
  slack, for 40 ms may request a four-point bitpool reduction, clamped to the
  existing floor. Both the existing and new 300 ms cooldowns must permit it.
- Actual drop/recovery handling takes precedence; never apply both reductions
  on the same observation. An early reduction sets the existing temporary
  ceiling to the new bitpool for the existing 30-second hold, avoiding immediate
  upward re-probing. Other upstream adaptation is retained.
- Capacity/slack changes reset pressure persistence. Invalid inputs and
  clock failures suppress increases; rollback restarts a cooldown. Counters
  saturate. Each stream explicitly initializes its own ABI-versioned state.
- No allocation, I/O, pointers to native transport state, threads or timers.
  The caller supplies monotonic time and owns state exclusively. Decisions
  are applied once between complete media packets; existing queued PDUs remain
  under upstream transport ownership.

Forty milliseconds may also catch benign bursts (upstream comments mention
96 ms completion gaps). This is an explicit bitrate/continuity tradeoff to
measure, not a claim that all such bursts are congestion. Initial buffer target,
codec negotiation, packet limits, capture and radio operations are unchanged.

## Build and checks

The opt-in adapter is `../hearbridge-reference/queue-pressure.patch`; it changes
the five upstream build/rate/logging files listed in that patch. The original
bounded patch and original tested ELF remain unchanged.

```sh
python3 experiments/hearbridge-reference/build.py \
  --hearbridge-source build/reference/hearbridge \
  --sdk-source build/reference/sdk-v043-source \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config \
  --queue-pressure --output build/reference/queue-pressure-candidate
```

Focused tests are `test_queue_pressure.c` (six groups, strict C11 and sanitizers)
and `test_rate_adapter.c` (real upstream controller interaction). The existing
upstream `test_rate.c` runs unchanged against the adapter. Initial integration
failed its end-rate assertion despite zero simulated drops; holding the new
bitpool as the temporary ceiling fixed that issue. The final suite passes.
These host models are not hardware measurements.

## Current hardware experiment

The user authorized the next optimization comparison and confirmed devices
available on 2026-10-27. Use the already admitted reference transport/runtime
mechanisms, a verified separately named artifact, fresh private state with the
old state preserved, and capture-first operation. Keep the same 180-second
cooperative overall limit and 30-second media limit. No radio reset, driver
change, expanded traffic or longer run is introduced. Observe controller input
before/during/after, preserve logs immediately, and stop on anomalies/uncertainty.

Compare audible glitches, sender drops, actual pressure interventions, bitpool
trajectory and buffer targets against the saved baseline. A different audio
segment or radio conditions limit the comparison. Reduced bitrate or a single
clean run alone does not prove superior quality, latency or stability.

The first completed comparison reported one audible dropout and one sender drop,
with three policy interventions and normal operator-reported controller operation.
The previous baseline reported approximately two audible dropouts and two sender
drops. See the [current evidence and next hypothesis](../../docs/TEMP-PS5-TEST-HANDOFF.md#queue-pressure-comparison-result--2026-10-27):
the retained upstream capacity adjustment tightened the queue while occupied.
This run demonstrates policy execution on PS5, not proven comparative superiority.

## Bounded queue shrinking candidate

The separately selected `--queue-drain` candidate retains pressure v1 and adds
the original GPL-3.0-or-later `ps5bt_queue_drain.h` (ABI v1). The three-file
`queue-drain.patch` applies **after** `queue-pressure.patch`: it names the ELF,
adds a focused host check, and adapts the pinned upstream `src/a2dp/btlink.c`.
The baseline pressure patch and tested artifact are preserved. All reused code
keeps the HearBridge revision/license/attribution recorded in the reference recipe.

The last run requested cap 10 → 8 at backlog 8; the next 235 ms contained one
sender drop. Upstream enqueue discards the oldest whole queued media packet at
capacity. The new policy delays a smaller requested cap until media occupancy
is at least two packets below that cap, or **500 ms** elapses. It does not itself
remove, resend or edit any queued packet. The setter preserves the requested
target; pump progress and the flush before each enqueue evaluate the policy.

- Retain at most the previous cap during the wait; do not allocate or expand it.
  Cap increases explicitly requested by existing tuning still apply immediately.
- Repeated or changed shrink requests keep the first wait's deadline. Applying
  the target, or an explicit non-shrink request cancelling it, ends that episode.
- On expiry, apply the requested cap even if occupied; the existing admission
  drop behavior remains possible and is reported. A bad/regressing clock applies
  the target immediately. There is no credit restoration or recovery loop.
- Log wait start, apply, expiry, cancellation and clock failure, with actual cap,
  target, media occupancy and elapsed wait. ABI and invalid-input failures are
  observable. State is initialized per link and used only by its existing owner.
- The deadline is cooperative: enforced on the next pump/admission after native
  calls return. No promise of interrupting blocked native calls is introduced.
  Temporarily retaining more packets can increase queue residence time; 500 ms
  is the deferral bound, **not** an end-to-end latency bound. Measure listening
  continuity and queue delay rather than claiming unchanged latency.

Build with the same source/SDK/LLVM arguments above, replacing `--queue-pressure`
with `--queue-drain` and using a fresh output such as
`build/reference/queue-drain-candidate`. The same 30-second capture-first media
and 180-second overall bounds, stop conditions, cleanup and controller checks
apply; no additional radio/runtime mechanism is introduced.

`make -C build/reference/queue-drain-candidate/hearbridge test-queue-drain`
uses the actual patched `btlink.c` against a fake transport under ASan/UBSan.
It reproduces the old oldest-packet loss, checks the fix preserves packet order
until known allowance drains the queue, and checks no policy credit mutation.
It also verifies stalled expiry (including retained drop behavior), unchanged
deadlines across requests, cancellation, clock regression and integer bounds.
These tests establish local behavior; hardware improvement remains unverified.
