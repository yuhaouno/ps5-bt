# Fixed-storage PS5 candidate resource factory

Status: concrete Rust constructors implemented and host-tested, with freestanding
checks of the external resource and target-codec paths. Native PS5 acquisition,
filesystem protection/durability, listener imports, capture format and scheduling
remain **Requires hardware**. No console operation is performed by development
checks. This factory does not provide a Bluetooth transport or playable audio.

## Concrete ownership and features

`ps5-app` contains an allocation-free `no_std` resource assembly owner. Its
`native-resources` feature selects the existing reviewed Rust owners and primitive
ABIs rather than inventing native layouts:

| Resource | Concrete constructor/storage | Acquisition and cleanup |
| --- | --- | --- |
| Clock | `ClockWait<128>` / `ClockWaitStorage<128>` | cache reviewed exports; consume close once |
| Protected store | `FileStore<1024>` / `StoreStorage<1024>` | validate/open an existing private directory and lock; close once |
| Listener | `Listener<256>` / `ListenerStorage<256>` | bounded nonblocking native bind; close owned client/listener once |
| Capture | `CaptureOwner<Avcap2Backend<9216>>` / `CaptureStorage<9216>` | explicit bounded Open/Start; consume Stop/Close/Terminate once |
| SBC | `SbcEncoder<8192>` / `EncoderStorage<8192>` plus 256 i16 scratch samples | persistent matching encoder; consume finish once |

Every backend queries/validates its actual private storage requirements. The
fixed capacities are policy bounds, not a duplicate of any native private
structure. A replacement codec requiring larger state fails cleanly; a query does
not authorize allocation or changing the bound implicitly. `NativeStorage`
contains only caller-owned stable buffers, no self-reference. Place it outside
the small loader stack and retain its exclusive borrow until all owners are gone.
No per-frame allocation is introduced. Neither directory bytes nor private native
handles are put into factory diagnostics; configuration intentionally lacks Debug.

Default features expose only configuration and ordered ownership logic. They
perform no native operation and have no new platform-link requirement.
`native-resources` enables the concrete clock/store/listener/capture constructors,
leaving matching target C objects/import resolution to the platform linker.
It does not enable a codec automatically. `target-libsbc` additionally selects the
reviewed external target archive through the existing `bt-codec` checks.
`host-candidate` selects actual host POSIX store/listener, real host libsbc and
synthetic reviewed clock/capture exports for development tests only; its transitive
host-only features reject cross compilation. Host and target codec features remain
mutually exclusive.

## Startup and reverse cleanup

`unsafe NativeFactory::create(&mut storage, config, resolvers)` validates the
configuration before any acquisition. Its unsafe preconditions are the existing
clock/capture resolver signature/export lifetime contracts and a source-reviewed,
matching store/listener target implementation whose native import table has
already initialized successfully. The established loader kernel resolver must
remain initialized for the whole borrow. This constructor does not acquire initial
kernel access, modify permissions or authorize an execution window.

Call `acquire(Clock, 0)` first, then `now_us()` to establish the actual fallible
monotonic domain. Acquire Store, Listener, Capture and Encoder in that order, using
that same clock. Optional disabled resources return without calling a backend and
remain marked `required=false, acquired=false`. Duplicate successful acquisitions
are idempotent; out-of-order acquisition and cleanup return local errors without
native calls. Clock acquisition itself needs no timestamp and performs no native
clock read. The later capture budget uses an absolute deadline in this same domain;
the caller must supply a currently valid deadline and finite read budget.

A failed acquisition consumes its own partially acquired native state according
to the existing backend contract, then the factory closes all earlier acquired
resources in reverse order. A successful factory reaches `FactoryPhase::Ready`:
that means its selected local resources initialized, never Bluetooth Link Ready,
command authority, capture-format verification or playback. Stop/failed startup
uses `shutdown()` or reverse `cleanup(resource)`. Acquired flags are consumed before
native cleanup; each result is retained and never retried. Cleanup failure does not
replace the primary startup error or prevent attempts to close earlier resources.
Drop is a fallback; explicitly retain `FactoryReport` to observe cleanup results.
A failed factory cannot silently acquire again or reset a recovery episode.

`ResourcePorts` is the ordered owner's injection boundary for fault tests, with
`NativePorts` implementing actual reviewed constructors, not empty production
callbacks. Reports retain the failed resource, primary native/project return,
full resolver return, native stage/flags, separate cleanup returns and raw clock
pair where applicable. They contain no path, pointer, key, request or PCM data.
The generic owner also enforces reverse cleanup before calling a port.

Store initialization and persistence remain synchronous operations with the
bounds already documented in BOND-FILE-STORE.md. A factory does not make a native
filesystem call preemptible or establish its wall-clock bound. Keep these calls
out of the per-frame audio path; native stop latency and durability still need PS5
validation. No path is created automatically: integration must explicitly supply
an already provisioned private 0700 namespace with reviewed native protections.
A missing or unsafe path is a concrete acquisition error, not permission to relax
checks or open an arbitrary path supplied over LAN.

## Lending resources to the application and listener

`clock_mut`, `store_mut`, `listener_mut` and `parts_mut` expose exclusive borrows
of actual acquired owners. `parts_mut` splits the fields so a caller can service
the clock, listener and application without self-referential Rust storage. The
listener remains factory-owned; its managed client slot/HTTP state can service
one connection with fixed buffers and a finite deadline, and final listener close
still consumes owned sockets. HTTP/control startup is separate from Bluetooth
activation. Authentication, finite resources and protected/public redaction are
still required by the existing service contract.

When `Application` needs to own its store adapter, `take_store()` transfers the
actual `FileStore` into the external startup owner's `StoreSlot`. The factory
keeps the Store cleanup obligation pending. Destroy the application's `StoreLease`,
recover the actual store from its slot and call `restore_store` before Store
cleanup. Restore returns the unchanged store to its caller on misuse. A pending
external handoff defers Store cleanup and therefore earlier Clock cleanup; it is
reported as unresolved ownership, never guessed success or another close attempt.
The startup adapter should finish all native acquisitions before taking the store,
so a subsequent acquisition failure can unwind ordinary ownership directly.
After app-construction failure, return the store before final factory cleanup.
Do not drop the factory while retaining its store externally: Drop cannot reclaim
an owner that the caller explicitly transferred. Explicit shutdown/report and
restoration are required for an observable cleanup proof.

## Modes and activation gates

- `ControlOnly`: acquire clock, protected store and listener. Capture and encoder
  are absent. This is a usable candidate for the existing authenticated LAN
  status/control service while radio-dependent operations stay unavailable.
- `CaptureObserve`: additionally acquire/start the bounded capture owner, with
  `Format::UNKNOWN` required and no encoder. Reads expose bounded byte outcomes
  only; no normalization, sample rate or PCM timing is inferred.
- `GeneratedSbc`: additionally create the persistent real encoder, with no capture.
  Explicit generated PCM may test encoding independently of native audio format.
- `NativeAudio`: requires capture plus encoder configuration, a fully supported
  explicit format marked `HardwareVerified`, and exact encoder/sample-rate match.
  Unknown/Requested evidence fails; a merely Declared offline contract also fails
  native admission. The selected encoder subset remains 48 kHz stereo/joint,
  16 blocks, 8 subbands, loudness and bitpool <=53, within its negotiated range.

The factory verifies configuration consistency; it cannot manufacture the external
hardware evidence behind a `HardwareVerified` label. Current native capture evidence
is still Unknown, so there is no authorized captured-audio activation by default.
The LAN cannot choose a native format/evidence label or supply raw memory/config.
Even a future admitted NativeAudio factory creates local capture/codec resources,
not a connected stream; negotiated pipeline assembly and hardware acceptance remain
separate obligations.

There is deliberately no Transport resource, radio open, HCI reader, command
allowance, handle authority or ACL credit constructor in this factory. It cannot
create a Bluetooth Ready connection or an `AclCreditOwner`. Successful store,
listener, capture or codec setup supplies none of those facts. Unresolved native
routing/ownership/credits continue to gate all radio operations, independent of
control-only service readiness. No timer credit refill, competing reader, system
mutation, new privilege mechanism or guessed native layout is introduced.

## Integrated workload modes and scoped rebuilding

The native application variants now invoke these actual staged constructors:
`ps5-application-control` for local control, `ps5-application-generated` for
observed-clock generated PCM through persistent Rust/target SBC scheduling and
discard, and `ps5-application-observe` for bounded raw capture. All share protected
store/listener/clock setup and observable cleanup; none creates Bluetooth traffic
or verifies native PCM. The generated profile has 750 blocks over two seconds of
nominal sample positions and pinned joint-stereo summaries. Observation keeps
format/time Unknown, uses 100 ms settle and 10 ms interval and a two-second exclusive
boundary, with up to 190 scheduled polls and maximum 256. The standalone diagnostics remain
independent. See APPLICATION-STARTUP.md and its workload-specific contracts.

`NativeRebuild` adds scoped actual old-factory consumption and new-factory
construction on the same caller-owned NativeStorage. A higher-ranked callback
prevents owner lifetimes escaping; the persistent Recovery is borrowed
structurally, retaining attempts, deadlines, Stop and cleanup uncertainty.
Explicit stable success is observed while actual resources remain acquired.
Host checks reopen real FileStore/Listener resources after close, fail an actual
occupied bind, and gate uncertain capture/client cleanup. No self-reference or
unsafe lifetime extension is used. Local resource epochs grant no Bluetooth
connection epoch/credits. This library is not installed as automatic application
reconnect and creates no additional hardware payload; see
NATIVE-RESOURCE-REBUILD.md.

## Existing source, layout and licensing review

No new upstream source or native API is imported by this Rust crate. It calls the
already reviewed project-owned boundaries documented in:

- [PLATFORM-CLOCK.md](PLATFORM-CLOCK.md): pinned SDK
  `a7685ce8e97ce1f8a4f38253a41d54241c9124f1` clock/usleep declarations and
  layout; fallible cached clock and <=1 ms finite requested wait slices.
- [BOND-FILE-STORE.md](BOND-FILE-STORE.md) and
  [CONTROL-LISTENER.md](CONTROL-LISTENER.md): original GPL-3.0-or-later POSIX
  C owners with private native layouts, protected records and consume-once
  descriptors. Target headers/import closure must be reviewed separately;
  host headers are never substituted for a PS5 ABI.
- [CAPTURE-STREAM.md](CAPTURE-STREAM.md): independent experimental Avcap2 ABI,
  full-width raw resolver/return metadata, unknown format/time, conservative
  GPL-3.0-only C adaptation from the exact recorded PlayPods/HearBridge revisions.
- [SBC-IMPLEMENTATION.md](SBC-IMPLEMENTATION.md),
  [SBC-TARGET.md](SBC-TARGET.md), and `third_party/sbc/VENDORED.md`: the existing
  selected LGPL codec source/revision, reviewed patches and replaceable archive.
  Any distributed codec-enabled artifact still needs matching source, notices
  and working replacement/relink materials.

Those components retain their file-specific obligations and unsafe invariants;
this factory does not change them. Its new Rust implementation is original project
code under GPL-3.0-or-later. Source/license/layout review and target import linkage
do not transfer any upstream hardware result to this payload.

## Development checks and remaining hardware work

```sh
cargo test -p ps5-app --locked
cargo test -p ps5-app --features host-candidate --locked
cargo clippy -p ps5-app --all-targets --features host-candidate --locked -- -D warnings
cargo check -p ps5-app --target x86_64-unknown-none --features native-resources --locked
# With the separately built/reviewed target codec archive and LLVM18 llvm-nm:
PS5_BT_SBC_ARCHIVE=/absolute/path/libps5_bt_sbc.a \
PS5_BT_SBC_NM=/absolute/path/llvm-nm \
  cargo check -p ps5-app --target x86_64-unknown-none --features target-libsbc --locked
```

Five portable tests cover every acquisition-stage failure and exact reverse
cleanup, primary-versus-cleanup errors, duplicate/disabled/out-of-order operations,
Drop, unknown/unverified format gates and deferred store handoff. The native host
fixture uses actual protected-store locks, a real loopback listener, and a real
persistent encoder producing a 77-byte frame. Separately it uses synthetic
reviewed capture exports with UNKNOWN format and no encoder, then injects actual
C capture Start failure and checks Close/Terminate once. It also checks resolver
failure, a real missing-directory failure, an occupied loopback bind failure and
release of earlier resources, including reopening the protected store lock.
Clock/capture fixtures do not execute native PS5 code and generated SBC does not
establish captured audio.

The separately selected application candidates initialize reviewed native imports,
entropy, authentication, the supplied private path and finite startup/HTTP/work
budgets, recording artifact identity and cleanup while retaining unavailable
radio capabilities. Building them is distinct from successful native execution.
The existing private 0700 directory and matching native owner remain manual
prerequisites; no kit or console helper creates or relaxes them.
Native export behavior, filesystem protection/durability, clock/wait behavior,
listener permissions and controller coexistence require a current authorized
bounded PS5 window. No console was contacted, and no playback or continuity
improvement is claimed by these checks.
