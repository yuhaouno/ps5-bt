# Bounded Classic Inquiry device search

Status: implemented portable candidate, **Requires PS5 hardware**. `bt-discovery`
is original safe, allocation-free `no_std` Rust using `bt-hci` framing and
`bt-link::Peer`. It encodes real Classic Inquiry and Inquiry Cancel commands and
parses their events. It does not perform radio I/O, configure inquiry/scan mode,
change event masks/filters, reset a controller, allocate transport ownership,
pair a device or infer headset compatibility. Existing PS5 transport and event
routing evidence remains unresolved.

## Owner API and result selection

The caller supplies `1..=256` `ResultSlot`s, a nonzero scan epoch, monotonic
microseconds and finite `Config` budgets to `Inquiry::new`. An example control
boundary is:

```rust
use bt_discovery::{Config, Inquiry, ResultSlot};
let mut slots = [ResultSlot::empty(); 16];
let mut search = Inquiry::new(1, 0, Config {
    inquiry_length: 4, max_responses: 32,
    scan_timeout_us: 6_000_000, cleanup_timeout_us: 1_000_000,
    max_result_events: 64,
}, &mut slots).unwrap();
let mut command = [0u8; 8];
let ticket = search.prepare_command(0, &mut command).unwrap().unwrap();
assert_eq!(ticket.len(), 8);
// Only an externally authorized owner may copy command[..ticket.len()] into
// owned HCI storage. Invoke confirm_accepted(ticket) only after full acceptance.
```

GIAC is fixed to `0x9E8B33`. `Inquiry_Length` is the real `1..=0x30` field in
1.28-second units, and `max_responses` is finite `1..=255`; zero/unlimited is
excluded. The host scan budget is independent of any controller
Extended_Inquiry_Length setting, which this candidate neither reads nor changes.
Host expiry explicitly requests cancellation; elapsed time never proves inquiry
has stopped. If the precondition for issued command allowance/routing is absent,
the platform must keep Search unavailable, not run a competing reader.

`prepare_command(now, output)` returns a private-field ticket carrying exact
opcode/length. Short output is unchanged; Busy retains the same ticket/bytes.
`confirm_accepted` records exactly one full-command transport acceptance before
an exact controller Status/Complete acknowledgement can advance the scan. At
most one command is outstanding. Command credits returned by the controller are
raw observations; the external owner must enforce real command allowance.
Unknown HCI Command status 0x01 permits either acknowledgement kind per Core
section 4.5. Wrong-kind success and wrong opcode never consume pending state.

`receive(epoch, now, packet)` processes one complete HCI event without H4.
Foreign events/opcodes, stale scan epochs, early acknowledgements and malformed
packets are returned to the caller; their borrowed input remains available for
routing/diagnostics. There is no silent consume/drop policy. All rows of a batch
and the complete EIR structure are validated before any result-storage mutation.
A dispatcher must preserve system/foreign traffic, retain bounded pending input
where appropriate, and report actual queue/framing loss.

Results deduplicate by peer address without eviction. Their opaque
`DeviceId { scan epoch, ordinal }` remains stable across RSSI/name/metadata updates.
The integer parts can be serialized for protected authenticated management;
`from_parts` does not grant authority or allow selecting an arbitrary address.
`get(id)` and `results()` return protected metadata. `connection_candidate(id)`
also requires confirmed inquiry quiescence and no scan failure, so Select cannot
race the still-active Inquiry/Cancel command. Use its peer and most recently
observed page-scan repetition for the connection request. Discovery identity is
not an authentication result; normal pairing/confirmation remains required.

A new scan uses `reset` with a strictly newer epoch and only after quiescence;
old IDs and command tickets are rejected. HCI carries no scan epoch. The enclosing
owner must establish the procedure's event routing, quarantine old traffic and
never relabel a system/old inquiry event with the new epoch. Creating a new
Inquiry with the same epoch/old event stream bypasses that contract and is forbidden.

## Parsed wire subset and protected metadata

The Core 6.0 baseline uses interleaved per-response entries:

| Event | Code | Exact parameter layout |
| --- | --- | --- |
| Inquiry Complete | `0x01` | one Status octet |
| Inquiry Result | `0x02` | count + count × 14 octets: address6, repetition1, reserved2, class3, clock2 |
| Inquiry Result with RSSI | `0x22` | count + count × 14 octets: address6, repetition1, reserved1, class3, clock2, RSSI1 |
| Extended Inquiry Result | `0x2f` | count=1 + one 254-octet entry: RSSI layout + EIR240 |

The count must be nonzero and match exact event framing. A standard/RSSI event
can fit at most 18 such entries in HCI's 255-octet parameter limit. The historical
15-octet RSSI-with-extra-paging-mode layout is deliberately unsupported, never
guessed from field values. Reserved paging fields are ignored; the clock offset
retains only specified bits 0..14. Page-scan repetition is checked as R0/R1/R2;
RSSI is checked in the specified -127..+20 dBm range. No special unavailable-RSSI
value is invented. Class of Device is retained as the raw three-octet observation,
not interpreted as proof of audio services or headset compatibility.

EIR is a bounded sequence of length/type/value structures followed by all-zero
padding. Only Shortened/Complete Local Name types `0x08/0x09` are retained.
Unknown types are length-checked then ignored; raw EIR, OOB pairing material and
other opaque payloads are never stored or exposed through the result table.
Names must be valid UTF-8 without control characters. Duplicate name tags,
malformed structures, invalid UTF-8 and nonzero padding after the zero terminator
are explicit errors; this is a documented strict UI subset. An empty complete
name and an all-zero EIR are valid. Names are at most 64 stored bytes, cut only
at a UTF-8 boundary, with independent source-complete and local-truncated flags.
A complete source name that was locally truncated is not a complete stored name.
An observed complete name is not downgraded by a later shortened report.

Device/name/result/event objects intentionally have no Debug implementation;
Peer Debug is redacted. Protected metadata contains address, class, name,
page-scan repetition, low clock bits, most recent RSSI observation time, last-seen
time and report count. `Snapshot`/`Notice` contain no addresses or names. Display
names as escaped text in protected management, never HTML or a log-format string.
Even a validated name is an untrusted peer claim and may contain personal data.

## Bounds, full lists and cancellation races

`Snapshot` distinguishes occupied capacity (`list_full`) from an actually
incomplete list (`truncated`). A full table still accepts updates for existing
peers; extra peers produce exact `unretained_reports` increments without replacing
IDs. That counter counts reports, not provably distinct discarded devices.
`max_result_events` counts duplicates as well; on its boundary the scan queues
Cancel. Further owned results while cancellation is pending are explicitly
reported as unretained and do not grow the table or refresh the budget.

Scan time/event expiry and user cancel begin one finite cleanup episode. Repeat
cancel never refreshes its deadline. A never-accepted Inquiry cancels locally.
An accepted Inquiry waits for its Status before Inquiry Cancel may be offered.
A successful Inquiry Cancel Command Complete ends the procedure: **no later
Inquiry Complete is required or expected** for that cancelled inquiry.

If natural Inquiry Complete races an unsent Cancel, the local Cancel is removed.
If Cancel was accepted, the natural complete status is retained but state stays
Stopping until that exact Cancel acknowledgement arrives. A nonzero Cancel status
after the original completion is observable `CancelRace`; the original event
proves the radio procedure ended, without erasing the raw Cancel status. A failed
Cancel without that proof remains failure/cleanup-required. Timeout or clock
failure never establishes quiescence. Known late cleanup acknowledgements may
establish quiescence but retain the original failure; no failed scan is selectable.

`fail_transport` records the raw non-Busy error and uncertain acceptance, forbids
replay/guessed Cancel, and prevents reset until external reconciliation. This
primitive provides no way to manufacture that proof. Cancellation is bounded and
caller-driven; it never starts another Inquiry, changes radio settings, closes
native resources or synthesizes credits. Runtime receive work is bounded by 18
rows × 256 slots plus the fixed 240-octet EIR parser; no operation blocks or loops
on I/O. The external owner must schedule finite waits and call `tick` regularly.

## Primary source baseline and validation

Original implementation; no upstream source copied or adapted, no new third-party
dependency. The following sources were inspected on 2026-10-26:

- [Bluetooth Core Specification 6.0, Vol 4 Part E](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html),
  sections 4.5 (command errors), 5.4.1/5.4.4 (wire framing), 7.1.1–7.1.2
  (Inquiry/Cancel and absence of a cancelled Inquiry Complete), 7.7.1–7.7.2,
  7.7.33 and 7.7.38 (the four result/completion events).
- [Core 6.0, Vol 3 Part C](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/generic-access-profile.html),
  sections 3.2.2, 6.4 and 8: device names/discovery and the EIR format/padding.
- [Core Supplement v11, Part A](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/CSS_v11/out/en/supplement-to-the-bluetooth-core-specification/data-types-specification.html),
  section 1.2; [Assigned Numbers](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Assigned_Numbers/out/en/index-en.html),
  sections 2.2 and 2.3: GIAC and name data-type assignments.
- Linux repository `torvalds/linux`, tag v6.12, peeled commit
  `adc218676eef25575469234709c2d87185ca223a`:
  [HCI declarations](https://github.com/torvalds/linux/blob/adc218676eef25575469234709c2d87185ca223a/include/net/bluetooth/hci.h)
  and [event handlers](https://github.com/torvalds/linux/blob/adc218676eef25575469234709c2d87185ca223a/net/bluetooth/hci_event.c)
  independently corroborate the repeated-entry layout. Reference-only inspection,
  no Linux code imported; current Core layout remains the supported contract.

Checks: `cargo test -p bt-discovery`, `cargo clippy -p bt-discovery --all-targets
-- -D warnings`, and `cargo check -p bt-discovery --target x86_64-unknown-none`.
Synthetic tests cover exact command bytes, all event layouts, malformed suffix
atomicity, 18-result batches, every truncation/trailing byte, name truncation,
reserved-field handling, stable IDs, capacity/event/time bounds, acknowledgement
correlation, cancel races, epoch reset and uncertain send.

PS5 Search/Select acceptance still requires an authorized hardware window, a
usable reviewed transport with owned command routing, real nearby devices,
finite run/cleanup identity and controller checks before/during/after. Neither
Inquiry success nor a class/name observation proves pairing, audio playback,
headset support, wireless coexistence, scan duration or performance on PS5.
