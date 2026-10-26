# Scoped concrete native resource rebuilding

Status: implemented offline with actual native factory constructors, host POSIX
store/listener reopen checks and a freestanding compile check. This candidate
rebuilds local resources; it creates no Bluetooth transport, HCI reader, command
allowance, connection or ACL credit owner. A local resource epoch is explicitly
separate from any Bluetooth connection epoch. PS5 resource behavior, persistence,
permissions, timing and controller coexistence remain **Requires hardware**.
No hardware operation or deployment is authorized by these development checks.

## Why a scoped factory

`bt-runner::RebuildOwner` already defines finite cleanup, attempt, backoff and
stable-success behavior. Its factory trait returns an owned resource. Returning
`NativeFactory` while also owning the very `NativeStorage` it borrows would make
a self-reference; retaining its storage borrow across failed attempts would also
prevent safe reuse of the same backing buffers.

`ps5_app::rebuild::NativeRebuild` instead structurally borrows the existing
`bt_core::recovery::Recovery` for its entire lifetime. The caller owns one stable
`NativeStorage` outside it. `retire(old_factory, now, failure, prior_cleanup)`
consumes the actual old factory, closes its acquired resources once and retains
the complete report. Any handed-off store must first be returned. A pending
external handoff remains uncertain; there is no guessed cleanup or second close.
Earlier client-close uncertainty is supplied explicitly and retained independently
of a successful final factory close.

`poll(now)` can acknowledge only recorded successful local cleanup for the exact
current Cleanup safety ticket. Unknown cleanup blocks reopen permanently in this
coordinator. Reconciliation remains a visible blocked requirement: this candidate
does not implement a radio ownership/credit reconciliation mechanism or permit
elapsed time, a new handle, or resetting a ledger to stand in for one. Attempts,
episode deadline and capped backoff stay in the same Recovery. No method
constructs a replacement controller or automatically calls retry_exhausted.

After an actual Attempt offer, `with_attempt` accepts its exact ticket once,
increments the checked local resource epoch and creates a fresh NativeFactory
on the **same** backing storage. It acquires Clock, Store, Listener, optional
Capture and optional Encoder using the existing reviewed constructors and mode
admission checks. Stop is checked before and after every acquisition. A failed
partial acquisition uses the factory's existing reverse unwind and preserves
primary versus cleanup errors. There is no new backend, native layout or ABI.

A higher-ranked callback receives only a `LiveNativeAttempt`. Its native lifetime
cannot escape in the return value. The callback can perform bounded operations
through `operate`, explicitly attest working local resources with
`confirm_working`, and observe stable success with `poll`. Before returning to
its caller, with_attempt consumes the real factory and records every cleanup
result; only then can storage be lent to another attempt. There is no raw pointer,
lifetime extension, unsafe alias trick, self-reference, heap or per-frame allocation.
The one unsafe call uses the unchanged NativeFactory resolver/import contract.
A compile-fail check rejects an attempted store-handle escape.

## Budgets, stop and stable success

`NativeAttemptBudget` supplies an exclusive absolute microsecond deadline and a
nonzero maximum admitted operation count. Acquisition has five fixed stages;
active operation, working-attestation and stable-observation calls each consume
one operation before reading the native clock. Every checkpoint also advances
the persistent episode deadline. No method sleeps or retries internally. The
native call/operation itself must return with its already reviewed bounds;
a budget cannot interrupt a blocking callback. Callback code must itself be
finite and may not hide an unbounded loop after a rejected operation.

Stop during a waiting/backoff state rejects before creating a new factory or
spending another attempt. Stop during acquisition or a live scope consumes the
actual acquired subset and leaves Recovery stopped. Operation exhaustion,
exclusive attempt deadline, clock failure and episode exhaustion similarly
retain observable outcomes and clean native resources once. No cancellation
restores credits or proves some external Bluetooth owner reconciled.

A successful reopen alone leaves the policy InFlight. `confirm_working` is an
explicit observation beyond constructor success: the host check successfully
loads a synthetic absent bond from the real protected store and connects to,
accepts on and closes the real listener before attesting local service health.
This is local resource functionality, not audio/headset success.

Stable-success time is observed only while the selected actual native owners
remain acquired. Checkpoints validate both factory acquired flags and concrete
owner presence, so closing/taking a required owner cannot leave a false Ready
attestation. A later fault before the stable interval preserves the original
attempt count, episode deadline and next backoff. An uninterrupted configured
stable interval returns Healthy and resets the policy only while resources are
still live. Closing an unstable scope is a new fault in the same episode; no
later poll can grant stable time to already closed resources.

The callback returns `NativeSessionExit::Fault { value, failure }` or Stop. A fault
is recorded after consume-once native cleanup. If stable success already reset
the policy, a subsequent fault legitimately starts a new episode; otherwise its
previous budgets remain. A Stop exit ends automatic recovery. Actual cleanup
uncertainty overrides a retryable classification and remains sticky. Earlier
cleanup error records are preserved in snapshots and attempt reports.

Host callback unwinding leaves scope_incomplete set. A later poll stops the
policy **before** advancing stable-success timers and returns CleanupUnknown;
Drop cleanup cannot be retroactively inferred as confirmed. Target panic behavior
still follows the existing abort path, with durable missing-final classification
owned by the surrounding diagnostic. This module does not invent panic recovery.

## API outline

```text
NativeRebuild::new(&mut persistent_recovery, initial_local_epoch)
retire(old_native_factory, actual_now, fault, earlier_cleanup_uncertainty)
poll(actual_now) -> AwaitSafety / WaitUntil / Attempt / Exhausted / Stopped
unsafe with_attempt(
    &mut same_native_storage, config, reviewed_resolvers, actual_now,
    NativeAttemptBudget { deadline_us, max_operations }, stop_callback,
    |live| {
        live.operate(one_bounded_native_operation)
        live.confirm_working() // only after actual functional observation
        live.poll()            // bounded observations while native owners live
        NativeSessionExit::Fault { value, failure }
    })
```

The config remains local protected configuration. Existing NativeAudio mode still
requires fully supported HardwareVerified capture format and matching target SBC
parameters; Declared/Unknown format cannot acquire that mode. CaptureObserve
keeps UNKNOWN format and no encoder. GeneratedSbc never implies native PCM.
Cleanup uncertainty in the existing capture ABI remains process-sticky; this
coordinator provides no reset of it. The module can also rebuild ControlOnly
without acquiring capture or codec at all.

## Verification and remaining admission

```sh
cargo test -p ps5-app --features host-candidate --test rebuild --locked
cargo test -p ps5-app --features native-resources --doc --locked
cargo clippy -p ps5-app --all-targets --features host-candidate --locked -- -D warnings
cargo check -p ps5-app --target x86_64-unknown-none --features native-resources --locked
```

The serialized host integration scenarios use actual private-directory FileStore
locks and a real loopback listener across repeated scopes of the same NativeStorage.
Synthetic static exports supply reviewed clock/capture signatures only; they are
not native PS5 execution. Cases cover functional reopen, explicit live stable
success, unchanged budgets after open-only success, actual occupied-bind partial
failure followed by successful reuse, exhaustion, Stop during backoff and partial
Store acquisition, finite work/episode budgets, earlier cleanup uncertainty and
unwinding. A separate case injects an actual C capture Stop failure, verifies
Stop/Close/Terminate once and blocks every further reopen despite successful
later store/listener/clock cleanup. The host accept probe has an explicit1000
nonblocking-call ceiling; it is a development fixture, not a native scheduling
policy or PS5 timing claim.

All source/layout/license obligations remain those of PS5-RESOURCE-FACTORY.md,
PLATFORM-CLOCK.md, BOND-FILE-STORE.md, CONTROL-LISTENER.md, CAPTURE-STREAM.md and
SBC-TARGET.md. This module imports no additional upstream source or native API.
It contains no system mutation, credential expansion, competing reader, radio
reset or exploit.

A separate bounded console artifact may exercise local close/reopen after fresh
review, exact build/run identity, durable outcomes, operation/time ceilings,
current execution authorization and controller checks before/during/after.
This library does not deploy or register one automatically. Bluetooth routing,
actual credits, connection recovery, audio continuity and headset acceptance
remain independent unresolved hardware gates.
