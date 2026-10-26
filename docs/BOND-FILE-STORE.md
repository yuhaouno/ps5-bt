# Protected local bond file store candidate

`bt-store` and `platform/store/bond_file_store.{h,c}` provide an actual filesystem
implementation of `bt-device::BondStore`. This is original project code, using
host system POSIX headers/functions rather than copying a native `stat` layout
into Rust. It is not wired into the PS5 diagnostic. PS5 export availability,
filesystem protection, durability and reboot behavior remain **Requires hardware**.
All development fixtures contain synthetic keys in temporary test directories.

## Features and usable boundary

- Default: safe `no_std` ABI definitions, no C build, filesystem operation or
  allocator. Freestanding checking remains possible.
- `external-posix`: the safe Rust `FileStore` owner and versioned extern C ABI,
  with no host compiler invocation. It can be checked for the freestanding
  target; a separately reviewed matching target C implementation must be linked
  before execution. This feature does not resolve PS5 imports or guess syscalls.
- `native-posix`: includes `external-posix` and builds the concrete C implementation
  with the host `CC`/`AR`. It requires `HOST == TARGET` and a Unix host, so host
  headers cannot silently become the PS5 ABI.

`StoreStorage<N>` supplies zero-initialized, 64-byte-aligned caller-owned memory.
`FileStore::open(&mut storage, absolute_directory_bytes)` queries the C ABI version,
private storage size and alignment before opening anything. ABI v1 contains only
fixed-width integers, pointers and the six-field `Diagnostic`; native descriptor,
uid, offset, inode and `stat` types remain private to the C translation unit.
The owner holds the storage borrow until exactly-once `close`/Drop, retains no
borrowed path/record buffer and is neither Send nor Sync. All operations are
serialized; there is no per-audio-frame storage access or allocation.

Every C operation receives a fresh invalid sentinel report; unwritten or dirty
success reports cannot reuse a previous successful operation. A successful open
return establishes ownership even if its report is malformed: the adapter attempts
exactly one matching close and returns the original report with any cleanup
uncertainty. A nonzero open return must retain no context by the C ABI contract.
Malformed mutation reports return durability unknown and poison further operations.
Explicit close validates both its return and its fresh report; `CloseFailure`
retains both values, and Drop never retries a failed or malformed close.

The caller securely provisions an existing private directory. Initialization
walks its absolute path one component at a time with descriptor-relative
`openat`, `O_NOFOLLOW`, `O_DIRECTORY` and `O_CLOEXEC`. Embedded NUL, empty/dot/dot-dot
components, trailing slash and paths over 512 bytes are rejected. Symlinks in
any component are rejected, not resolved. Parents must belong to root or the
current effective user and cannot be group/other writable unless sticky; the
final directory must belong to the effective user and have exact mode 0700.
Test callers canonicalize only their synthetic fixture root before this check.

A mode-0600, one-link, owner-matching `.p5bt.lock` file and nonblocking exclusive
`flock` prevent cooperating processes/contexts from writing concurrently. The
lock descriptor remains owned until close. Each operation rechecks directory
protection and the lock's descriptor/path inode identity. Changing effective uid
requires a new appropriately reviewed store, not continued use of an old owner.
No platform path, filename supplied by the LAN, privilege mutation or protected
key import is introduced.

## Files, atomic replacement and deletion

Only a fixed peer-derived basename (`p5bd-` + twelve lowercase hexadecimal HCI
address digits + `.bond`) reaches file operations. Device identity is protected
local metadata and never enters diagnostics. Files must be regular, mode 0600,
owned by the effective user and have exactly one hard link. `O_NONBLOCK` prevents
opening a FIFO from hanging before its type can be rejected. Load additionally
requires exactly 32 bytes and validates the existing v1 record metadata/peer.
It reads into a local buffer and copies to Rust only after successful reads,
revalidation and descriptor close; absence and errors leave output zeroed.
Rust/`DeviceManager` still applies its stronger configured key provenance policy.

Save validates the fixed v1 input record, checks an existing destination and
creates an exclusive, no-follow mode-0600 temporary file. Its name contains an
instance counter, never key data. It completes bounded short writes, fsyncs the
file, rechecks private-directory protection and temp/destination inode identity,
closes the temp exactly once, renames atomically and fsyncs the directory. Valid
replacements publish the old or new whole record, never a reported-success
partial record. A failed precommit replacement preserves the old destination;
only an owned temporary inode is eligible for cleanup. A substituted symlink or
foreign inode is never unlinked as if it were the owned temp.

Delete checks the same file ownership/type/link/mode protection, rechecks its
identity, unlinks and fsyncs the directory. An already absent file still requires
a successful directory sync before durable absence is reported. Explicit forget
may delete a protected corrupt-size/content record: load/save rejection must not
make forgetting an owned device impossible. Symlink, hardlink, wrong-owner or
unsafe-mode entries remain errors and are not deleted.

Successful save/delete means the specified file/directory fsync boundary was
acknowledged by the native OS. Calls at or after attempted rename/unlink that
fail return `StoreError::DurabilityUnknown`; a failed directory sync never becomes
success merely because the new file or absence is presently visible. The
nonsecret diagnostic retains the raw native errno, failing stage and flags for
commit attempted/committed and cleanup uncertainty. Validation/security failures
have a project status and zero raw errno; they do not fabricate a native errno.

## Bounds, cleanup and trust scope

Reads/writes and EINTR fsync/stat retries have finite 64-call bounds; exclusive
temp creation probes at most 64 names. Path length bounds traversal. Interrupted
`close` is attempted **once**, never retried on a potentially reused descriptor.
Cleanup uncertainty is retained separately from the primary error and poisons
further use when an owned file descriptor/temp cannot be safely reconciled.
Explicit close returns its diagnostic; Drop can only perform best-effort close.
No error path logs records, paths, peer identity or command buffers.

These are syscall-count/resource bounds, not a claim that a kernel filesystem
operation has a bounded wall-clock duration. File I/O must remain outside the
real-time audio path; production PS5 integration still needs a measured bounded
execution/stop arrangement. There is no hidden retry thread, sleep or network I/O.

Protection assumes a reviewed private namespace, ordinary POSIX DAC semantics
and the documented serialized owner. Privileged or same-uid hostile processes
can defeat those permissions/locks; inherited ACLs, unusual network filesystems,
mount changes and storage hardware flush semantics require platform review.
Mode checks do not certify absence of extra ACL grants. The record itself is
not encrypted or authenticated at rest; its confidentiality/integrity relies on
that protected store. Raw native filesystem APIs are not a substitute for this
reviewed platform assumption.

Process-crash checkpoints demonstrate whole-file publication, not actual power
loss. A crash before rename may leave a protected temporary file. Initialization
does not scan/delete arbitrary leftovers; repeated collisions fail after 64
probes with an explicit limit status. Future orphan recovery needs its own
bounded identity/protection review. No stale temp can be loaded as a bond because
its basename is outside the fixed peer namespace.

## PS5 API/source review

The inspected SDK revision is `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`.
Its [fcntl header](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/fcntl.h)
declares `openat`/`flock` and the needed no-follow/directory/close-on-exec flags.
Its [stat header](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/stat.h)
declares `fstat`, `fstatat`, `fchmod` and a native `struct stat`; that structure is
not copied into Rust or frozen as a claimed PS5 runtime layout.
The [unistd header](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/unistd.h)
declares read/write/close/fsync/unlinkat/geteuid, and
[stdio](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/stdio.h)
declares renameat. These are source declarations, not evidence of console exports,
syscall-origin permission, actual filesystem semantics or runtime layout safety.

Reviewed header SHA-256 identities:

| SDK path | SHA-256 |
| --- | --- |
| include/freebsd/sys/fcntl.h | `d41a582c3b731d1a4faffe12325f552df9be867b6bfbd6817e32d51eafd9927f` |
| include/freebsd/sys/stat.h | `33b19ac8d547c631f656b707fe2d6f737813884176aca1f71d593a35f3fd8365` |
| include/freebsd/unistd.h | `a69fe86ee9267af53ededa0b3013dd2e62bc835ca5a6cbde151f78389063f20b` |
| include/freebsd/stdio.h | `1e0b639eb07c7098f6618b782df76cef37356f1d8d3e1fb7fdeebae9f36b6618` |

The existing `platform/ps5/bootstrap/runtime.c` resolves selected symbols through
the reviewed loader/syscall path; `crt/syscall.c` at that SDK revision supplies
the bridge, not a blanket filesystem-import implementation. This change does
not add a guessed resolver, syscall number, module import or stat layout. The
external feature makes the Rust owner linkable when a concrete target path is
reviewed; live PS5 storage remains unavailable until then. No SDK header/source
was copied or vendored; the shim is original and uses native system declarations.

## Development verification

```sh
cargo test --locked --offline -p bt-store --features native-posix
cargo test --locked --offline -p bt-store --features external-posix
cargo clippy --locked --offline -p bt-store --all-targets --features native-posix -- -D warnings
cargo check --locked --offline -p bt-store --target x86_64-unknown-none
cargo check --locked --offline -p bt-store --features external-posix --target x86_64-unknown-none
python3 tests/store/run.py --sanitize
```

Five native Rust tests exercise actual temporary-filesystem replacement/reopen,
protected metadata/path rejection, hardlinks/symlinks, exclusive owner lifetime,
corrupt-record forget and storage bounds, plus a compile-fail single-owner check.
A separate external-ABI fake regression checks dirty/unwritten reports, acquired
open cleanup, stale success rejection, mutation uncertainty, poisoned reuse and
close-return/report disagreement without double close.
The C harness injects short reads/writes, bounded repeated EINTR, file/directory
sync and commit failures, inode/temp substitution races, permission changes and
interrupted exactly-once close. Fork checkpoints exit between file sync, rename
and unlink, then reopen only synthetic records. ASan/UBSan runs the same real
filesystem scenarios. No PS5, real local bonds or user files are accessed.
