# First audible reference result — 2026-10-27

The user heard the generated tone on WH-1000XM4. Wireless DualSense operation
was reported normal before, during and after. The user confirmed that the
headset disconnected after the run. This is an isolated modified HearBridge
result on the user's Slim / 13.40, not PS5-BT production playback acceptance.

## Identity and authority

- Source/recipe commit: `980a9c6`; exact upstream pins and modifications are in
  [the reference recipe](../experiments/hearbridge-reference/README.md).
- ELF SHA-256: `21edbfce50f77292dad90fa245d37b56119975db83cc881dceef0d80df32d9c9`.
  Size 726,992 bytes. Remote FTP readback and schema-2 slot metadata matched.
- The user explicitly admitted this one experiment's competing USB reads,
  timed credit fallback, shared controller settings and SDK own-process
  permissions, and confirmed the current console window and headset readiness.
- PayloadManager 0.6.3 at `.111:8084` launched the exact reference through its
  inspected POST route. No earlier HearBridge process/listener was found.
- The on-hold raw-openat slot was removed. The only PS5-BT entry is now
  `local/ps5-bt-hearbridge-reference`; unrelated inventory records are unchanged.

## What actually happened

1. The fresh private directory, HTTP listener and USB transport opened.
   Marvell/NXP `1286:2059` was selected with 40 event and 20 ACL reads.
2. An explicit discovery request found one WH-1000XM4; the agent selected that
   device. Pairing, connection and AVDTP streaming followed.
3. The existing generated 1 kHz tone streamed for the bounded 30-second media
   window. Final counters: 1,410 media packets and 11,260 SBC frames; the
   sender reported zero media drops. Bitpool adapted from 35 to 53.
4. The user confirmed audible tone and normal controller operation. By the time
   that reply arrived, the media deadline had ended the payload. A subsequent
   read of the HTTP status failed with connection refused; no capture toggle
   was submitted. **Capture never initialized; game/system audio was not tested.**
5. The program requested shutdown and exited. The manager process inventory
   contained no reference process afterward. USB cleanup calls were acknowledged.
   AVDTP close and L2CAP disconnect responses were not observed, and HCI
   Disconnection Complete was not confirmed. The user's observed headset
   disconnection is retained separately from missing protocol acknowledgment.

The last ACL statistics were 1,460 submitted, 1,418 event credits, **37 timed
fallback credits**, five outstanding in the upstream six-slot accounting,
maximum local queue seven, and 19 recorded stalls. This demonstrates use of
the admitted heuristic, not real allowance ownership. Received ACL summaries
showed the selected handle `0x002`; missing/foreign traffic and final USB ring
loss counts remain Unknown. Zero sender drops is not a listening-quality,
latency or long-term coexistence measurement.

## Evidence and next concrete run

Private local evidence directory:
`build/reference/first-audio-candidate/remote/`. It contains `run.json`, operator
confirmation, upload/inventory/process records, and immediately collected
`hearbridge.log`, `diag.txt`, `status.txt`. No bond file was downloaded.
Final log: 27,755 bytes; SHA-256
`ac5d97ac4b3cddf945f2ed820cb1a9a2b7ab14f8b93beaa5bf09efb4f98d9007`.

The next useful experiment is capture-first using this same verified ELF:
preserve the completed reference state under a separate private remote name,
launch into its required fresh directory, set `/api/tone?on=0` **before**
discovery/connection, select the XM4, and observe at most 30 seconds of known
audible PS5 game/menu content. This avoids waiting for a chat reply inside the
short media budget. It requires a new explicit run decision because the last
authorization was for one experiment, and must retain the missing-disconnect
acknowledgment limitation. Do not silently relaunch or erase the existing state.

If capture fails, retain the first native failure and stop; do not return to
directory probes. After game audio is established, prioritize bounded shutdown
acknowledgment and reducing reliance on timed credits. No comparative advantage
over HearBridge is established by this reference result.
