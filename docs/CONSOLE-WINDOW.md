# Offline-default console window helper

`tools/console_window.py` validates the existing kit and reduces manual slot/log
checks for the next console window. Both actions default to **offline dry-run**.
Implementing or testing this helper does not connect to the console, deploy an
artifact or authorize execution. Actual launch remains a manual, separately
confirmed PayloadManager operation.

The helper supports exactly six identity-bearing candidates: `application-setup`,
`application-control`, `application-generated`, `application-observe`,
`sbc-continuous` and `capture-stream`. It calls the existing
`mvp_test_kit.load_kit` validation before any network operation. The default kit
is `build/ps5/next-window-kit-20261026`; select another validated kit directory
with `--kit`. Target, FTP port, slot root and diagnostic log remain fixed:

| Item | Value |
| --- | --- |
| Console FTP | `192.168.1.111:2121` |
| Existing local slots | Exact `ps5-bt-<selected-id>` slots below `/data/pldmgr/payloads/local` |
| Only collected remote log | `/data/ps5-bt-hw1.log` |
| Application runtime prerequisite | Existing `/data/ps5-bt/control`, mode `0700`, owned by the payload identity |

Slot IDs/ELF filenames are pinned to the six supported candidate IDs.
The helper provides no arbitrary remote path, upload, create, rename, chmod,
delete, credential mutation or launch operation. FTP visibility does not prove
the payload's private-directory ownership/protection. That prerequisite always
remains Unknown here and must be checked by the native candidate. The separately
selected setup artifact may create its permitted missing private components;
this helper never creates them or treats FTP visibility as native validation.

## Preflight

Review the selected existing slots, expected ELF hashes/sizes and prerequisite
without opening an FTP client:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 --preflight
```

With no `--candidate`, the helper first validates the actual kit and selects its
supported items in plan order. An existing three-application kit still selects
exactly those three; it does not require setup or either standalone audio test.
A six-item window kit selects its six items. Unsupported legacy diagnostic IDs
are skipped by automatic selection; a kit with no supported item fails locally.
Repeat `--candidate` for an explicit subset, in requested order. Empty, duplicate,
unsupported, more-than-six or supported-but-absent selections fail before FTP.
After the console window is confirmed, explicit read-only checks are available:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 --preflight --execute \
  --candidate application-generated --verify-hash
```

Execute mode reads only each existing slot's bounded `meta.json`, requires schema
2 / kind local / its exact ELF filename, switches the FTP transfer type to binary
and compares SIZE with the validated kit. `--verify-hash` additionally streams
that existing ELF through SHA-256 with a four-MiB ceiling and exact expected
length; no ELF copy is saved. Without it, the remote hash result remains Unknown,
not inferred from SIZE. A missing slot, malformed metadata, wrong size/hash or
cleanup failure is an explicit error. No failed preflight is repaired by writes.

## Collect one manually selected run

The next launch truncates the fixed diagnostic log. Save the previous run before
launching another candidate. First review collection offline:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-generated
```

Then, when explicitly authorized to read the console, collect that fixed log and
correlate it with this candidate's matching kit manifest:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-generated --execute \
  --controller-before normal --controller-during Unknown --controller-after normal \
  --run-observation observed
```

Omit observations that were not actually made. All controller phases default to
Unknown. `--run-observation` defaults to Unknown; `observed` records only the
operator's explicit statement that they observed the manual run. The helper
never launches or claims to have witnessed a payload. A retrieved log can belong
to a prior run of the same build; build identity correlation is not proof of
freshness, and `ps5_log_freshness` remains Unknown. Unknown observations are never
filled in as normal operation or fabricated into another run.

Execute collection retrieves only `/data/ps5-bt-hw1.log`, with a one-MiB ceiling.
It saves a unique mode-0600 raw log plus a mode-0600 JSON result under
`build/ps5/console-window`. Use `--output-dir DIR` for a different existing/new
private directory; the leaf directory must be owned by the current host user
and have mode 0700. Raw logs and results are private local evidence, not files
to commit. Dry-run creates no directory or output file.

The existing diagnostic collector checks the exact embedded build/candidate
identity, profile, numerical counters, cleanup and outer lifecycle, including the
C-only setup profile's two import groups and no entropy/runtime entry. Setup
cancellation remains incomplete; setup success certifies only the bounded fixed
namespace checks and reported cleanup. The standalone SBC/capture candidates use
their original distinct bounds. This helper
adds transfer completion/error, FTP cleanup outcome and explicit operator-run
observation. No raw FTP response, metadata secret, payload token or arbitrary
server text is copied into the JSON result or error output. The raw diagnostic
log remains the original private input; the normal collector's whitelist keeps
its arbitrary contents out of the JSON.

A partial transfer is preserved as a unique `.partial.log` plus evidence JSON.
Its result cannot be complete_success even if the received prefix contains
success-like markers. A connection failure with no bytes is `collection_failed`,
not a claim that the payload failed to enter Rust. A failed session cleanup after
a complete read is retained separately and prevents overall success. An invalid
or non-text saved log is preserved privately with incomplete_evidence. A stale
or mismatched identity remains a diagnostic mismatch, without being rewritten
to match the selected candidate.

## Bounds, local durability and outcomes

Metadata is bounded to 16 KiB, ELF readback to four MiB, and the log to one MiB.
At most six fixed candidates are checked. FTP control/data sockets use a
five-second timeout, with a shared 45-second monotonic operation/transfer budget
checked between calls and on every received block. The cleanup request still
gets its finite socket timeout. These are helper bounds, not a PS5 payload run
ceiling or a measured network completion guarantee.

Logs are written to exclusive private temporary files, flushed and fsynced,
then published by exclusive hard link and parent-directory fsync. Existing
results are never overwritten. The JSON uses the diagnostic collector's same
private atomic no-clobber persistence protocol. A failed write/fsync/publication
is explicit; a private partial or published file may remain after that failure
and must not be treated as confirmed durable. No host fsync establishes PS5
power-loss log persistence, audio acceptance or controller coexistence.

Process exit 0 means a successful dry-run/read-only preflight, or a saved
complete_success collection. Exit 2 means collection evidence was saved with a
non-success/partial outcome. Exit 1 means validation, FTP preflight or local
write/durability failed; fixed bounded error categories avoid echoing private
server responses. Inspect the saved result and original private log before the
next launch. Neither preflight nor collection changes native capability gates.

Run the no-network tests with:

```sh
python3 tests/tools/test_console_window.py
```

The tests use actual local kit/collector validation and fake FTP clients only.
They cover old three-item and new six-item automatic selection in plan order,
explicit subsets/negative selections, all six actual result-collector profiles,
default zero-FTP behavior, existing-slot metadata/size/hash checks,
rejection/size/operation bounds, interrupted and mismatched logs, private unique
output, explicit Unknown/changed observations, server/cleanup failures and local
fsync failure. No console connection or hardware result is involved.

For first-slot creation use the version-pinned manager UI/raw-upload procedure in
[PAYLOADMANAGER-SLOTS.md](PAYLOADMANAGER-SLOTS.md), then this helper's read-back
hash preflight. The manager import creates complete metadata and uploads the ELF;
it does not launch it. A second FTP replacement is unnecessary after that import.
