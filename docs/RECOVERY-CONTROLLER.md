# Caller-driven fault recovery controller

`bt_core::recovery` is a safe `no_std`, allocation-free policy component. It
makes per-fault attempt budgets, deadlines, backoff, cancellation and stable
success observable. It does not open capture, reconnect a headset, change a
connection epoch, send packets, sleep, close resources or modify HCI credits.
All PS5 behavior and audible continuity remain **Requires hardware**.

## Policy and state ownership

Construct `Recovery` with a `Policy` containing a nonzero maximum attempt count,
episode timeout, initial backoff, backoff cap and stable-success interval. All
times use caller-supplied monotonic microseconds; these are policy settings,
not measured/recommended PS5 parameters. The cap must be at least the initial
backoff. Checked deadline arithmetic and monotonic-clock checks fail closed.

The application retains one controller for the whole fault episode. Recreating
it after each failed open would bypass the policy. An episode starts at the first
`trigger`; subsequent reports retain its absolute deadline and attempts spent.
The first attempt can begin immediately after safety work. After an accepted
attempt fails, backoff is initial, doubled for each prior attempt, and capped.
All safety work and waiting consume the same episode timeout. The exclusive
boundary `now >= deadline` exhausts the episode, including an in-flight attempt.
The caller must give individual backend operations their own finite deadlines,
poll the controller and cancel/clean up when exhausted: this policy cannot
interrupt a blocking platform call.

The caller drives these steps:

1. `trigger(now, Failure::Retryable)` reports a recoverable owned-resource failure.
   `Failure::IntegrityLost` reports lost events, uncertain credits or ownership.
2. `poll(now)` returns `Action::AwaitSafety { ticket, requirement }`. Perform the
   requested work externally. `Requirement::Cleanup` means owned-resource cleanup
   is confirmed. `Reconciliation` additionally requires a verified mechanism
   establishing ownership/credit state; ordinary stop, elapsed time and resetting
   a local counter do not meet this requirement.
3. Only then call `confirm_safety(now, ticket, confirmation)`. Cleanup alone cannot
   acknowledge a reconciliation request. A new integrity fault can escalate an
   outstanding request; a later retryable fault cannot downgrade it.
4. Observe `WaitUntil`, then an `Attempt(ticket)` offer. Polling is side-effect
   free with respect to operations and attempts. `accept_attempt` spends exactly
   one attempt and changes to `InFlight`; duplicate acceptance is rejected.
5. Execute the bounded operation and correlate its outcome using `finish_attempt`.
   Failure requires new safety work. Success starts `StabilizingUntil`; the caller
   must mean a functioning recovered resource, not merely a successful open.
6. During stabilization, report any new fault. It retains the original budget.
   Only an uninterrupted configured success interval automatically returns to
   `Healthy` and resets the budget. A successful recovery may finish its stability
   observation after the episode deadline; that deadline bounds attempts, not a
   resource already reported successful. Polls do not themselves prove health.

Tickets are opaque and scoped to one controller instance. Do not exchange tickets
between controllers. Each new safety request and offered attempt has a new checked
identity. Old, duplicate, unaccepted or superseded results cannot complete a newer
attempt. A late result after timeout is rejected; its caller still owns any
required cleanup and must independently report newly discovered integrity loss.
No stale-result rejection is evidence that resources were released.

## Exhaustion and stop

`Snapshot` exposes attempts, the episode deadline, last reported failure and any
outstanding safety requirement, including after exhaustion or stop. An exhausted
controller remains visible and unavailable; repeated faults and polling never
create another budget. `retry_exhausted(now)` is only for an explicit user retry
or documented new external trigger. It starts a fresh episode, preserves unresolved
safety and invalidates old tickets. It must never be called from automatic polling,
a failure callback or a timer. If an attempt exhausted its budget, that does not
remove its cleanup obligation.

`stop()` is idempotent and requires no clock. It remains usable after clock
regression/overflow and invalidates pending tickets; the stopped controller stays
stopped. It cancels only policy, does not claim exactly-once resource cleanup, and
does not clear uncertainty. The application owns cleanup and session teardown.

A regressing clock or overflowing deadline/ticket identity enters an explicit
`Failed(error)` state. Ordinary invalid state, premature acceptance and stale
ticket errors reject the requested transition without granting new resources.
Safety/attempt deadlines can still expire during rejected caller operations.

## Development checks and remaining PS5 work

`cargo test -p bt-core --test recovery` checks bounded retries, capped backoff,
cleanup/reconciliation separation, stale and duplicate results, episode expiry,
stable-success reset, explicit operator retry, cancellation and clock arithmetic.
Synthetic microsecond values establish state-machine behavior only.

`bt-runner::RebuildOwner` now binds these actions to factory-owned resource
close/reopen, including a concrete capture/encoder pair. New epochs, partial
open cleanup and same-episode attempt budgets are checked offline; an already
accepted ticket can be handed to a rebuilt application without spending twice.
Native PS5 candidates still must establish actual backend cleanup,
connection epochs and verified transport reconciliation, and record each fault's
cause, attempt, waiting/exhausted outcome and cleanup. Recovery timing, Bluetooth
coexistence, audible gaps and improvement over another payload all require PS5
headset tests. This component supplies no transport authorization or experiment
admission; see [audio recovery rules](AUDIO-RECOVERY.md) and
[repository execution policy](../AGENTS.md#active-mvp-execution).
