# Prepare the six PayloadManager local slots

Status: source-reviewed procedure for the reported PayloadManager **0.6.3**.
The procedure below has not been executed against the next console window.
No console was contacted during this audit. Installed-version identity, actual
manager URL, native filesystem behavior and successful import remain **Requires
hardware**. Slot preparation uploads files; it does not launch a payload.

## Pinned source and scope

The reviewed upstream is `itsPLK/ps5-payload-manager`, tag `v0.6.3`, commit
`3f881e8e569eba3054d1784a1962439b2eee92c2` (tag and commit independently resolved
through Git). The [immutable version/route definitions](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/include/pldmgr.h#L1-L48)
define manager HTTP port **8084**, ELF-loader port 9021, version `0.6.3`, and
`/manage:upload`. The historical service observed at 9180 is not evidence that
it implements this manager API. Neither 8084 nor any other port is assumed live.
Use the URL of the actual installed manager UI and verify its `/version` response
before issuing an upload.

The [upstream HTTP access contract](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/DEVELOPMENT.md#L317-L349)
and [request-origin checks](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/http_server.c#L75-L94)
require POST for mutations. A matching Origin/Referer is accepted; with neither
present, the terminal-client marker `X-PLDMGR-Request: 1` is accepted. A foreign
Origin/Referer or cross-site request is rejected even with that marker. These are
CSRF checks, not secret authentication. Use only the confirmed trusted LAN URL.

This audit read source only. Upstream's [GPL-3.0 license](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/LICENSE)
and existing notices were inspected; no upstream implementation, binary, loader,
SDK runtime, exploit or installation code is copied or bundled here. Selected
source SHA-256 values provide a repeatable audit anchor:

| Selected file | SHA-256 |
| --- | --- |
| `src/slot.c` | `c78782878c3d9b22b001343e628c191c8d5c12e3d2cdd2362d2dd9bc9c621d44` |
| `src/payload_mgr.c` | `c01d826119644926cef0064cc4d6ead865e36589da52d413ca0f578b67723baf` |
| `src/http_server.c` | `9e8d6a8bc8ac5cb15937b4201a63b731cd27ada9fde1e76ad6ebc2792f4932b5` |
| `frontend/src/App.jsx` | `992930baabb76b1845cd2b49cb680148fd36dcd8dc4c390b796fd758b2938615` |
| `frontend/src/components/views/StorageHub.jsx` | `6457e0034e7e62506ba3e2c793f3090b7ddac24777fcc32f1a605e0fd2ea8a6d` |

## Exact kit files and slot names

Use one validated clean-commit kit. The current six-item plan is
`tools/window_test_plan.json`. With kit root
`build/ps5/next-window-kit-20261026`, each local ELF is at
`<kit-root>/<candidate-id>/<filename>`. The manager's slot root is
`/data/pldmgr/payloads/local/`:

| Candidate ID | Exact slot directory below `local/` | Exact filename inside slot |
| --- | --- | --- |
| `application-setup` | `ps5-bt-application-setup` | `ps5-bt-application-setup.elf` |
| `application-control` | `ps5-bt-application-control` | `ps5-bt-application-control.elf` |
| `application-generated` | `ps5-bt-application-generated` | `ps5-bt-application-generated.elf` |
| `application-observe` | `ps5-bt-application-observe` | `ps5-bt-application-observe.elf` |
| `sbc-continuous` | `ps5-bt-sbc-continuous` | `ps5-bt-sbc-continuous.elf` |
| `capture-stream` | `ps5-bt-capture-stream` | `ps5-bt-capture-stream.elf` |

The manager [derives its local key from the filename](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/slot.c#L368-L420):
remove `.elf`, then apply its version/slug normalization. These six lowercase,
version-free filenames retain exactly the intended stems. Do not rename them,
append a build version or use **Keep both**, which allocates a suffixed slot.

## Supported UI procedure

After a newly confirmed hardware window and upload authorization:

1. From a computer or phone, open the actual PayloadManager dashboard and verify
   its installed version. The [upload control is hidden in the PS5-browser view](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/frontend/src/components/views/StorageHub.jsx#L492-L501).
2. Use **Upload ELF Payload** in the storage view and select one exact kit ELF
   from the table. The [frontend checks for a filename conflict, then uploads raw
   bytes](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/frontend/src/App.jsx#L461-L515)
   using `mode=new` when there is none.
3. If a conflict dialog appears, choose **Cancel** and inspect the exact existing
   slot/filename/hash first. An explicitly reviewed replacement uses **Replace**;
   do not choose **Keep both**. The [dialog maps those choices directly to
   replace/keep_both modes](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/frontend/src/App.jsx#L745-L779).
4. Repeat for the remaining five files, stopping on an error or ambiguous upload.
   Do not launch an item during preparation. Do not alter autoload configuration.
5. Run the read-only FTP metadata/hash preflight below. A UI success notification
   alone is not an independent ELF identity check.

The upload handler imports the file and refreshes the list; it does not call the
separate launch action. The manager itself creates the slot and complete metadata,
using its [serialized staging/journal/publication path](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/slot.c#L221-L290).
This is the supported first-slot route. Existing FTP upload helpers can replace
an ELF in an already prepared, validated slot; they cannot create a first slot.

## Equivalent explicit terminal upload

The following commands are a future operator recipe, not an instruction already
executed by this audit. Use them only for a currently confirmed manager instance
and authorized upload. Set `PM_BASE` from the actual manager UI URL, with no
trailing slash or path. The shown address/port is the pinned source's example,
not a live-service assertion:

```sh
PM_BASE='http://192.168.1.111:8084'
KIT='build/ps5/next-window-kit-20261026'
ID='application-setup'
FILE="ps5-bt-${ID}.elf"

curl --disable --noproxy '*' --fail --show-error --silent \
  --connect-timeout 3 --max-time 10 --retry 0 \
  "$PM_BASE/version"

curl --disable --noproxy '*' --fail --show-error --silent \
  --connect-timeout 3 --max-time 10 --retry 0 \
  "$PM_BASE/manage:check?filename=$FILE"
```

The version must identify `0.6.3`; another version needs its own source review.
The check response's derived key must equal `ps5-bt-application-setup` for this
example. Inspect any reported `existing.slot` and `existing.filename`; do not
silently replace a conflict. The check is advisory: a concurrent change or an
empty existing directory can still cause the subsequent `new` commit to fail.

For a first import, after reviewing the check and validated kit file:

```sh
curl --disable --noproxy '*' --fail --show-error --silent \
  --connect-timeout 3 --max-time 20 --retry 0 \
  --request POST \
  --header 'X-PLDMGR-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --data-binary "@$KIT/$ID/$FILE" \
  "$PM_BASE/manage:upload?filename=$FILE&mode=new"
```

This is a raw file body, not multipart/form-data or a JSON wrapper. The query,
content type and body match the frontend. The [server validates the filename,
streams to an owned temporary file and commits through the import owner](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/http_server.c#L297-L337),
then [finishes the upload without launching](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/http_server.c#L529-L576).
The terminal command sends no Origin or Referer, so the explicit marker follows
the reviewed guard. It is not a secret token or an authentication bypass.

Change only `ID` to another ID from the table, recompute `FILE`, and perform at
most one reviewed upload per candidate. There are six planned imports, no hidden
retry or catch-up loop. The shown requests have finite connection/overall limits
and retries disabled. An upload timeout/failure can leave an unknown publication
state: stop, inspect the exact slot, and do not retry or delete files blindly.
The source `new` mode refuses an existing target; it does not allocate a suffix.
`keep_both` is the only suffix-allocating mode. `replace` is available only for
an exact, already reviewed slot after explicit replacement authorization; it
publishes a complete slot transaction rather than merely editing `meta.json`.
See [mode/conflict handling](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/slot.c#L243-L271).

## Metadata contract and final verification

For a local web upload, the [import owner initializes](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/payload_mgr.c#L346-L384)
and the [metadata writer emits](https://github.com/itsPLK/ps5-payload-manager/blob/3f881e8e569eba3054d1784a1962439b2eee92c2/src/slot.c#L572-L644)
these six fields. This illustration is not a file to upload manually:

```json
{
  "schema": 2,
  "kind": "local",
  "filename": "ps5-bt-application-setup.elf",
  "installed_at": "<manager-generated local date/time and timezone offset>",
  "origin": "web_upload",
  "origin_detail": ""
}
```

`installed_at` uses the manager's local clock/offset format
`%Y-%m-%dT%H:%M:%S%z`; the payload's original build time is not substituted.
Repository-only fields such as `id` and `source_url` are not part of this local
writer branch. The reader tolerates missing metadata and can discover a payload
file, but that fallback does not make a handcrafted three-field document the
canonical import procedure. Our FTP preflight's schema/kind/filename checks are
a narrow acceptance guard, not a complete slot-creation schema. Let the reviewed
manager produce metadata, timestamps and its transaction files.

After importing all six, with separately authorized read-only FTP access:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 \
  --preflight --execute --verify-hash
```

The helper checks exact existing slots, bounded metadata, file sizes and SHA-256
against the kit. FTP is the separate confirmed service on `.111:2121`; this does
not discover or validate the manager's HTTP port. Save the successful preflight
alongside the exact kit identity. A failure stops preparation until resolved;
neither helper creates a substitute slot, bypasses manager origin checks,
updates the manager/loader, or launches a test.

The fixed private runtime directory `/data/ps5-bt/control` is separate from these
PayloadManager slots. Only the separately selected setup candidate addresses
that prerequisite; uploading its ELF does not execute it or establish directory
protection. Slot import, native setup, application tests and controller checks
remain separate observable steps. Manager transaction/fsync source review does
not establish PS5 power-loss persistence, firmware compatibility, stable audio,
Bluetooth ownership or usable PCM format.
