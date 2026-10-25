# 2026-10-25 resumed hardware window

The user explicitly confirmed access from the PS5 LAN and authorized testing.
The prior recorded environment is Slim / firmware 13.40 / PayloadManager 0.6.3;
no newer firmware or loader version has been confirmed in this window.

## USB binding metadata

Source: clean `942eb9db1462a3a6110410e556e4ea1738d2de57` diagnostic kit.
ELF SHA-256:
`10e92ea284b874d0f79f4f8734397f3de01b123c02354c52c8d9074333542a34`.
The remote file was retrieved and hashed after upload, before manual launch.
The user confirmed completion and normal wireless controller behavior.
The [complete numeric log](evidence/2026-10-25-usb-binding.txt) was retrieved
before the next candidate could overwrite the per-run log.

Own read-only enumeration again found `1286:2059` with the same four-interface
configuration. Active configuration query returned 0 with index 0. Interface
driver queries returned 0 for each of indices 0 through 3, but the bounded
reported names were `Unknown`, classified as binding unknown. This does not
prove exclusive ownership, independent command domains or available credits.

Other nodes `ugen0.1`, `ugen1.1` and `ugen2.1` returned normalized -1 on open.
The acquired device and directory both closed with 0. Inventory remained
incomplete and final status was 1. Runtime, Rust entry and return markers were
complete. No HCI command, endpoint read, detach, reset, credential change or
audio operation was performed by this candidate.

## Settled capture read

Source: the same clean `942eb9d` kit. ELF SHA-256:
`e5d02ed02d3f2d12088eb5f3ffdc921efdfda579e28385b1c6d13f0efead1529`.
Remote readback matched that hash. The user manually ran the separate candidate
and confirmed completion with normal wireless controller behavior. The
[complete numeric log](evidence/2026-10-25-capture-settled.txt) was saved.

Initialize, OpenAudio, Start and the single 100,000 us wait all returned 0.
ReadAudio returned positive `0x2000`, exactly 8,192 bytes inside the supplied
capacity. Stop, Close and Terminate each returned 0; cleanup_uncertain=0. The
full runtime returned 2 for the independently Unsupported Bluetooth adapter.

This is the first observed nonempty capture result. The diagnostic reports
1,024 units at its requested 8-byte frame size; that is arithmetic against a
request, not confirmation of actual native PCM format, sample rate, channels,
sample type or timestamp units. No sample content was inspected or retained;
timestamp_present=0 and format remains Requires hardware. Neither captured
audio playback nor Bluetooth transport has been demonstrated.

## Generated-signal target SBC

Source: clean `34ea256` with matching corresponding-source/relink package.
ELF SHA-256:
`6a6b00051a5180516f438bfd65f30e522ad9a117ae55bd868ab01db7b85d66ab`.
Source-package SHA-256:
`d95a8344e20e6ff4bf51d67953e39f228e33361d9cafecf49be96d5f5907473e`.
Remote readback matched the ELF hash. The user confirmed manual completion and
normal wireless input. The [complete log](evidence/2026-10-25-sbc-selftest.txt)
records result=0, stage=6, codec=0, cleanup=0, storage=5648 and alignment=16.

Two fresh init/finish cycles encoded four generated 48 kHz joint-stereo frames:
1,024 interleaved i16 samples consumed, 77 bytes per frame, first prefix
`0x9cfd206f`, four-frame FNV-1a digest `0x14e0ec79`. These exactly match the host
test. This verifies the selected generic C codec/caller-storage ABI on this
environment. No hardware encode duration was measured. The diagnostic did not
capture PCM or send Bluetooth traffic, and final payload status remained 2 for
the independently Unsupported transport.

The archive was separately rebuilt and replacement-linked with identical ELF
bytes. The corresponding-source tarball was then extracted into an independent
temporary project, built using only its included SDK subset and pinned tools,
and produced the same `6a6b000...` ELF SHA-256. Those are local build checks,
separate from the actual console log.

## Bounded capture observation

Source: clean `34ea256`. ELF SHA-256:
`d4df718a28cf3dcd70df6eebcadbc4bfc2df1caa39a51533ed8f932610064293`.
Source-package SHA-256:
`ce610993380e47669e3a51c51e7b1987c81ca887d932907be841ad51691a3e0c`.
Remote readback matched; the user confirmed manual completion and normal
wireless input. The [complete log](evidence/2026-10-25-capture-observe.txt)
records one immediate Empty result, one successful 100,000 us wait, and one
8,192-byte result, then immediate owned cleanup.

The clock adapter resolved and called `clock_gettime` successfully twice.
Observation stopped on data after two reads: Empty=1, Overrun=0,
last-read-start elapsed=104,093 us, clock_status=0. Initialize/Open/Start and
Stop/Close/Terminate each returned 0; cleanup_uncertain=0; final payload status
2. This establishes the checked clock call path and bounded first-data path,
not clock accuracy, total native-read duration or sustained capture behavior.
The observation path divides by no assumed frame size, reports no timestamp
and retains no samples. Native PCM format/timing remain unknown.

## Remaining acceptance

All four named candidates completed with user-confirmed normal wireless input.
USB inventory remains partial and binding names Unknown; HCI ownership, command
routing and enforceable credits remain unresolved. No inquiry/pairing, radio
reset, controller firmware operation, ACL/audio send or headphone playback was
performed. Source-package rebuilds and local simulations do not promote those
unverified capabilities.
