# USB metadata and capture lifecycle evidence

Date: 2026-10-24, Asia/Shanghai. Recorded environment: user-reported Slim
CFI-2200 B01, firmware 13.400.002, PayloadManager 0.6.3. Target .111.
These were project-built diagnostics, not readings copied from HearBridge.
No privilege/credential change or active HCI operation was used.

The builds honestly recorded a dirty shared source tree. The hashes below
identify the actually tested ELF bytes, not an assertion that a later clean
commit is byte-identical. Rebuilt artifacts need their own manifest and test.

## Read-only USB enumeration

Tested ELF SHA-256:
`f5abd0e7e8d119a5d67fda0b1dc82adc9d872d8fc5d23c41cf2d9958794b2ab9`.
The [complete numeric log](evidence/2026-10-24-usb-first.txt) contains no device
strings, serials, Bluetooth addresses, keys or PCM. The user confirmed normal
wireless controller operation after running it.

`/dev/ugen0.2` opened read-only. Device/configuration reads and parser returned
0: VID:PID `1286:2059` (Marvell/NXP), configuration value 1, total length
`0x0167`, four interfaces. Two interface groups declare class `e0/01/01`:

| Interface | Alternate | Endpoint descriptors |
| --- | --- | --- |
| 0 | 0 | 81 bulk IN / 01 bulk OUT, 512 bytes; 82 interrupt IN, 64 bytes; 02 bulk OUT, 512 bytes |
| 2 | 0 | 84 bulk IN / 04 bulk OUT, 512 bytes; 85 interrupt IN, 64 bytes; 05 bulk OUT, 512 bytes |
| 1 and 3 | 0..5 | respective isochronous pairs 83/03 and 86/06; max-packet values 0, 9, 17, 25, 33, 49 |

This corrects the earlier incomplete upstream summary: our complete descriptor
inventory includes a second HCI-class group. It does **not** prove a separate
radio, independent controller state, native-driver ownership, supported HCI
commands or an enforceable credit budget. No endpoint was selected or read.

Other enumerated nodes ugen0.1, ugen1.1 and ugen2.1 returned normalized -1 from
open. The acquired device and directory closed with 0. The inventory therefore
reported incomplete and main returned final 1 with a complete lifecycle. This
is a visible partial diagnostic result, not startup failure.

A subsequent driver-binding build was deployed with SHA-256
`1cf0c2395ff219ebadc7584c01789c49ddfdc3f38f0b98798d4fdb22646f71fb`.
Its execution/result was not confirmed before the usage interruption. Do not
infer anything about interface ownership from an installed file.

## Capture module preflight

Tested ELF SHA-256:
`8b200c3771c3ffb9b783a3fc3aeb3563f5ebaa928de494ba52d6fbdb26a58a3c`.
The [complete preflight log](evidence/2026-10-24-capture-preflight.txt) records
successful LoadStartModule lookup, Ipmi then Avcap2 loading (secondary results
0), all seven Avcap2 exports resolved, no capture lifecycle call, and final 2
for the still-Unsupported Bluetooth adapter. The user confirmed the run.

## One immediate nonblocking capture read

Tested ELF SHA-256:
`ced89c5c3c9dcedd0a5a453e4ae129386d5cd2bac97a4ca3f6ca805a02c711bb`.
The [complete capture log](evidence/2026-10-24-capture-once.txt) records:

- Initialize, OpenAudio and Start: each 0.
- One immediate ReadAudio: raw `0x81950002` (upstream Empty classification),
  zero bytes. No PCM data was acquired or interpreted.
- Stop, Close and Terminate: each 0; cleanup_uncertain=0; full final return 2.
- The user reported no hang and normal controller operation.

This establishes the recorded lifecycle, not captured PCM format, sample rate,
channel count, clock, sustained availability or audible Bluetooth playback.
The later optional 100 ms settling change was completed and host-tested after
the interruption, but was never hardware-tested. It requires a new explicit
test window.

## Remaining hardware work

When the user next makes the console available, verify matching artifacts and
run the driver-binding metadata probe and settled capture read separately,
saving each durable log before the next launch. Confirm wireless input and
notifications again. A successful nonempty read still needs independent format
and timing validation; a second interface description is not a routing proof.
No PS5 access is needed for the current offline codec/packetization work.
