# Bounded capture observation

Status: portable C state machine implemented and host-tested. It is not yet a
linked PS5 diagnostic and has not observed console audio.

The existing `capture-once` diagnostic deliberately performs one settled,
nonblocking read. `capture_observe` is a separate helper for a later named
candidate when that read is still Empty. It never opens, starts, stops or closes
capture itself. The caller must already own a started handle and retains all
native callbacks, waiting and timekeeping.

One `step` performs at most one nonblocking read. The caller supplies an
absolute monotonic deadline, a finite call count no greater than 256 and a
buffer no larger than 64 KiB. Empty and Overrun are counted while budget
remains. The first Data, other native result, invalid byte count, cancellation,
deadline or exact call limit terminates observation. Sample bytes are neither
retained nor interpreted, and an Overrun is recorded as a discontinuity.

A successful `begin` creates one cleanup obligation. Once observation stops,
`take_cleanup` transfers that obligation exactly once to the integrator, which
must guard the existing Stop/Close/Terminate sequence. Reuse before taking the
old token is rejected. Local validation and clock-regression failures leave the
state unchanged.

Host tests cover the classifications and every stop boundary under strict C11
and ASan/UBSan. The core also compiles freestanding for `x86_64-sie-ps5`
without extra runtime imports. Hardware integration still needs a source-backed
monotonic clock, bounded wait adapter, native READ_NOWAIT bridge, durable report
and the existing exactly-once lifecycle cleanup.
