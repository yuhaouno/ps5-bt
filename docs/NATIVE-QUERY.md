# Single read-only native presence query

Status: original C diagnostic and hostile host tests implemented, 2026-10-23.
**PS5 execution, firmware 13.40 symbol availability, OID availability and native
controller association are Untested/Unknown.** `BluetoothTransport` stays
Unsupported. This helper is separate from the transport ABI and creates no
session. See [native ownership research](NATIVE-HCI-RESEARCH.md) and the
[runtime review](RUNTIME-REVIEW.md).

## Source evidence and limits

The pinned [SDK header][sdk-header] declares a five-argument `sysctlbyname`
returning `int`, with name, old storage, `size_t` length pointer, **const** new
storage and `size_t` new length. The [SDK libkernel stub list][sdk-stub] names
the symbol. These establish a declared call signature and a source inventory;
neither establishes that the running PS5 exports it. The helper's typed callback
matches that declaration, with compile-time checks that `int` has signed 32-bit
range and storage size.

[FreeBSD 11.4 Bluetooth globals][fb-global] declare `net.bluetooth.version` as
`SYSCTL_INT` with `CTLFLAG_RD`. That source supplies the integer interpretation
for this one query. The helper does not perform a second metadata/type request;
successful return plus exactly four bytes is necessary for interpreting the
source-declared integer, but cannot prove the PS5 implementation or OID type.
The value is a stack interface version, not Bluetooth Core/HCI/LMP version,
controller identity, capabilities or working audio support. Proprietary PS5
behavior and safe command/event/ACL routing remain Unknown.

The SDK repository/revision is `ps5-payload-dev/sdk`,
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45). Its root GPL v3 license
and the Berkeley three-clause notice in the imported header were inspected.
The FreeBSD repository/revision is `freebsd/freebsd-src`,
`d106002967eff712894ab8b5337bbbff106ce5e0` (`releng/11.4`); the referenced
Bluetooth source has a BSD two-clause notice. These files were used as interface
and behavioral references; no upstream implementation, header or stub code was
copied, modified, compiled or linked by this diagnostic. Original diagnostic
code/tests carry the project's GPL-3.0-or-later identifier. Existing linked SDK
runtime inputs retain their separate provenance in [UPSTREAM.md](UPSTREAM.md)
and the runtime review.

## Operation and interface

`ps5_native_query_once(query, resolver_status, out)` clears the result, then:

1. A missing typed callback or nonzero resolver status returns
   `PS5_NATIVE_QUERY_UNAVAILABLE`, retaining the raw resolver status and making
   zero query calls.
2. Otherwise it initializes an `int32_t` value to zero and its `size_t` capacity
   to `sizeof(value)`, and calls exactly once:
   `query("net.bluetooth.version", &value, &length, NULL, 0)`.
3. Any nonzero query result is `PS5_NATIVE_QUERY_FAILED`, retaining the exact
   signed return value and returned length. No `errno` accessor is assumed.
4. Success with any other length is `PS5_NATIVE_QUERY_INVALID_LENGTH`. Only
   success with exactly four bytes exposes the signed value. All other values
   are cleared and rendered as `unknown`.

`ps5_native_query_resolve(lookup, out)` optionally performs exactly one lookup
of `sysctlbyname` before the same logic. Its lookup callback follows
`ps5_bootstrap_lookup`: zero and a non-NULL symbol on success. A failed lookup
cannot invoke a dirty returned symbol; zero with NULL is explicitly unavailable.
The bootstrap helper itself may try its two source-backed libkernel handles;
this diagnostic never retries the query or invokes a fallback interface.

The resolver adapter isolates a byte-copy from object-pointer representation to
the exact typed function pointer, with equal-size static checks. Equal size alone
does not make this ISO C portable: the selected PS5 x86-64 System V ABI must
supply the same representation and a callable symbol with the pinned signature.
The ARM64 host resolver injection exercises that host representation only.
Use the typed `once` function for hosts with another representation; no pointer
cast, guessed address, syscall number or instruction offset is introduced here.

Callbacks are synchronous trusted boundary functions and must obey output bounds;
post-call checks cannot undo an out-of-bounds write by a faulty implementation.
No callback or output pointer is retained. There is no global state, allocator,
TLS, `errno`, module loading, socket/Netgraph operation, ioctl, USB transfer,
configuration write, permission change, reset, HCI command, retry or cleanup
resource. The OS call's wall-clock duration is not bounded by this helper.

`ps5_native_query_encode_report` builds at most 384 bytes of ASCII, without a
trailing NUL. It emits fixed keys/name, status, exact resolver/query results,
returned length, valid stack value or `unknown`, plus `controller=unknown`,
`routing=unsupported` and `errno=unknown`. Uncalled query fields are `unknown`.
It accepts only fixed strings and numeric results; there are no addresses,
keys, serials, arbitrary external strings or raw dumps. Rendering performs no
queries. Capacity failure leaves the destination unchanged and returns the
required length; invalid input sets output length zero. Logging is an explicit
caller operation using that length, never `strlen` or a format string.

## Separate optional diagnostic artifact

After the [documented toolchain setup](TOOLCHAIN.md), build explicitly:

```sh
python3 tools/build_ps5.py --native-query
```

This produces `build/ps5/ps5-bt-native-query.elf` plus matching `.map`,
`.manifest.json`, `.inspect.txt` and `.disasm.txt`. The optional build defines
`PS5_BT_NATIVE_QUERY`, links `platform/ps5/diagnostics/native_query.c`, hashes its
source/header in the manifest, and records `native_query: true`. No new library,
SDK header, import stub or Rust/C transport symbol is needed.

The default `python3 tools/build_ps5.py` still produces `ps5-bt.elf` and its
matching reports, records `native_query: false`, and does not link the query.
Object names also include the selected stem. A build failure invalidates all
five selected artifact/report files while preserving the other variant.

Both variants call the existing Rust probe first. Only the optional build then
logs `event=native_query_begin`, resolves `sysctlbyname`, makes the single named
query if available, and logs its length-delimited report. An encoding failure
logs `event=native_query_encode_failed` without another query. Main preserves
the Rust result, currently 2 for Unsupported, even when the presence query
succeeds. Bootstrap logging already treats short/failed writes as terminal and
returns its existing log-failure status; the helper never retries them. Desktop
main builds without the flag retain their existing Rust-only path.

## Host validation

From the repository root:

```sh
mkdir -p build/host
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -Iplatform/ps5/diagnostics platform/ps5/diagnostics/native_query.c \
  tests/platform/native_query_test.c -o build/host/native_query_test
build/host/native_query_test
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -fsanitize=address,undefined -fno-omit-frame-pointer -g \
  -Iplatform/ps5/diagnostics platform/ps5/diagnostics/native_query.c \
  tests/platform/native_query_test.c -o build/host/native_query_test_sanitized
build/host/native_query_test_sanitized
cc -std=c11 -Wall -Wextra -Werror -pedantic -Iplatform/ps5/include \
  tests/platform/native_main_test.c -o build/host/native_main_baseline_test
build/host/native_main_baseline_test
cc -std=c11 -Wall -Wextra -Werror -pedantic -DPS5_BT_NATIVE_QUERY \
  -Iplatform/ps5/include platform/ps5/diagnostics/native_query.c \
  tests/platform/native_main_test.c -o build/host/native_main_test
build/host/native_main_test
cc -std=c11 -Wall -Wextra -Werror -pedantic -DPS5_BT_NATIVE_QUERY \
  -fsanitize=address,undefined -fno-omit-frame-pointer -g \
  -Iplatform/ps5/include platform/ps5/diagnostics/native_query.c \
  tests/platform/native_main_test.c -o build/host/native_main_test_sanitized
build/host/native_main_test_sanitized
```

Both passed with Apple Clang 21.0.0 on ARM64 macOS. Cases cover missing callback
and symbol, NULL outputs, raw 64-bit resolver extrema, dirty failure symbols,
zero/negative/extreme successful values, all returned lengths 0..16 and SIZE_MAX,
positive/negative/extreme query failures, initialized storage, exact name,
`newp == NULL`/`newlen == 0`, one query/no retries, all short report capacities,
guard bytes, deterministic exact output, malformed report arguments and ASCII
limits. ASan/UBSan exercise those host cases, not PS5/libkernel behavior.

The real main hook is also compiled into an injected host fixture. Strict and
sanitized runs verify Rust-before-marker-before-lookup ordering, one query,
preserved result 2, missing/dirty symbols, rendered resolver failure status,
query failure and invalid length. The baseline fixture links without the query
source or any resolver/logger definition. No fixture invokes PS5 code.

The diagnostic also compiled as a freestanding `x86_64-sie-ps5` object with
LLVM 18.1.8, strict C11 and warnings denied. Its only undefined object symbols
were compiler-lowered `memcpy`/`memset`, already supplied by the reviewed minimal
runtime. This object check does not substitute for the integrated ELF audit.

Both complete ELF variants passed the existing final dependency/relocation
inspector using LLVM 18.1.8 and Rust 1.90.0. Baseline contains no query symbol or
query name; the optional manifest includes the query source/header and marks the
variant explicitly. Variant-specific invalidation is covered by the builder's
self-test. These are build/host results only.

Hardware must separately establish the actual existing loader and stub
contract, clean load/log/return, the raw query results, and wireless DualSense
operation before/during/after. No controller ownership or audio gate is closed
by any possible result of this query.

[sdk-header]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/sysctl.h
[sdk-stub]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/sce_stubs/libkernel.c
[fb-global]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/common/ng_bluetooth.c
