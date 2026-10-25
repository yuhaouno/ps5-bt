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

## Pending candidates

Generated-signal target SBC and bounded capture observation integration are in
progress. No execution or result is inferred from an uploaded file or build.
