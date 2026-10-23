# HW-1 hardware repair evidence

Date: 2026-10-24, Asia/Shanghai. User-reported PS5 Slim / firmware 13.40,
192.168.1.10, Payload Manager v0.6.3, existing elfldr and ftpsrv/2121.
The exact installed loader build remains Unknown; process name is not a pin.

## Observations and corrections

Sustained entry/argument probes stayed visible after several seconds.
A probe that read the first byte of the supplied function's code did not reach
its hold. Earlier instantaneous process-list sightings did not prove a hold
had been reached. Earlier claims proving a particular dlsym implementation or
denied file access were premature and are superseded by these observations.

Removing code-byte inspection and retaining the SDK syscall convention created
the first persistent log. Ordinary write lookup failed with signed result -1
and explicit final 122. This establishes a lookup failure; it does not identify
its exact errno or imply that file creation needs more privileges.

The explicit existing-loader kernel resolver then initialized with code 0,
resolved callable exports, entered Rust, closed its log and returned 2.
The loader does not provide usable stderr: this is now a recorded warning when
the persistent log is healthy. Both notification API calls returned success;
the user's visual confirmation and wireless DualSense check remain outstanding.

Baseline lifecycle:

```text
event=loader_kernel_init_begin
event=loader_kernel_init_end code=0
event=payload_start log=/data/ps5-bt-hw1.log
event=rust_entry abi=1 phase=bootstrap
event=adapter_unavailable status=unsupported
event=stream_log_unavailable fallback=persistent
event=payload_exit status=2 (expected Unsupported)
event=payload_return status=2 (expected Unsupported)
```

The optional native query also completed. sysctlbyname resolved successfully;
the one net.bluetooth.version call returned -1 with returned_length=4,
stack_version=unknown, controller=unknown, routing=unsupported and errno=unknown.
This neither proves absence of Bluetooth nor establishes a controller version.

## Reproduce

```sh
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-hw1
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config python3 tools/build_ps5.py --native-query --loader-kernel-resolver
```

Inspect the matching manifest, commit/source hashes and structural reports.
The baseline and native query occupy separate Payload Manager local slots.
Each run replaces /data/ps5-bt-hw1.log; save it before running the other variant.
Transfer success is never the completion criterion: require Rust and final
markers, then verify no new resident diagnostic process.

Temporary direct-syscall experiments (PIDs 249 and 250) remained listed after
termination requests; their exact state is Unknown. They are not normal
diagnostic daemons and no system/loader process was restarted to remove them.
Subsequent baseline runs returned without adding resident payload processes.

## Next milestone

Confirm wireless DualSense and visible notifications, then implement the
documented bounded read-only USB descriptor enumeration. HCI operations still
require native-traffic ownership and credit accounting. No codec, pairing,
tone, capture or audio capability is established here. See RUNTIME-REVIEW.md
for kernel-R/W scope, upstream provenance and unsafe invariants.
