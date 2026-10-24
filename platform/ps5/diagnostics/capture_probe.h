/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_CAPTURE_PROBE_H
#define PS5_CAPTURE_PROBE_H

#include <stddef.h>
#include <stdint.h>

#define PS5_CAPTURE_PROBE_READ_BYTES 8192u
#define PS5_CAPTURE_PROBE_REQUESTED_FRAME_BYTES 8u
#define PS5_CAPTURE_PROBE_METADATA_BYTES 16u
#define PS5_CAPTURE_PROBE_SYMBOL_COUNT 7u
#define PS5_CAPTURE_PROBE_REPORT_CAPACITY 1536u
#define PS5_CAPTURE_PROBE_SETTLE_US 100000u

/* Adapter callbacks for the reviewed, current-process loader resolver.
 * Success is exactly zero and must return a non-NULL symbol. The module lookup
 * receives the nonnegative handle returned while loading libSceAvcap2. Neither
 * callback may retain pointers or inspect another process.
 */
typedef int64_t (*ps5_capture_current_lookup_fn)(const char *name, void **out);
typedef int64_t (*ps5_capture_module_lookup_fn)(int32_t module,
                                                const char *name, void **out);

enum ps5_capture_probe_status {
  PS5_CAPTURE_PROBE_OK = 0,
  PS5_CAPTURE_PROBE_UNAVAILABLE = -1,
  PS5_CAPTURE_PROBE_FAILED = -2,
  PS5_CAPTURE_PROBE_INVALID_LENGTH = -3,
  PS5_CAPTURE_PROBE_CLEANUP_FAILED = -4,
  PS5_CAPTURE_PROBE_BUSY = -5,
  PS5_CAPTURE_PROBE_INVALID = -6,
  PS5_CAPTURE_PROBE_CAPACITY = -7
};

enum ps5_capture_probe_phase {
  PS5_CAPTURE_PHASE_NONE = 0,
  PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE,
  PS5_CAPTURE_PHASE_LOAD_IPMI,
  PS5_CAPTURE_PHASE_LOAD_AVCAP2,
  PS5_CAPTURE_PHASE_RESOLVE_AVCAP2,
  PS5_CAPTURE_PHASE_INITIALIZE,
  PS5_CAPTURE_PHASE_OPEN,
  PS5_CAPTURE_PHASE_START,
  PS5_CAPTURE_PHASE_READ,
  PS5_CAPTURE_PHASE_STOP,
  PS5_CAPTURE_PHASE_CLOSE,
  PS5_CAPTURE_PHASE_TERMINATE,
  PS5_CAPTURE_PHASE_WAIT_BEFORE_READ,
  PS5_CAPTURE_PHASE_DONE
};

/* Optional durable phase tracing. ordinal is zero except for Avcap2 export
 * resolution, where it is 0..6 in the documented symbol order. entering is
 * exactly 1 before a call and 0 after it. Entry raw_status is zero; exit
 * raw_status is the resolver/native return. No handle, address or data is sent.
 */
typedef void (*ps5_capture_trace_fn)(enum ps5_capture_probe_phase phase,
                                     uint32_t ordinal, int entering,
                                     int64_t raw_status);
typedef int (*ps5_capture_wait_fn)(uint32_t microseconds);

struct ps5_capture_probe_ops {
  ps5_capture_current_lookup_fn lookup_current;
  ps5_capture_module_lookup_fn lookup_module;
  ps5_capture_trace_fn trace;
  /* Optional synchronous, non-reentrant settle hook. When present, run calls
   * it exactly once with PS5_CAPTURE_PROBE_SETTLE_US after successful Start and
   * before Read. A nonzero return skips Read and proceeds to owned cleanup.
   */
  ps5_capture_wait_fn wait_before_read;
};

enum ps5_capture_probe_read_kind {
  PS5_CAPTURE_READ_NOT_CALLED = 0,
  PS5_CAPTURE_READ_DATA,
  PS5_CAPTURE_READ_ZERO,
  PS5_CAPTURE_READ_EMPTY,
  PS5_CAPTURE_READ_OVERRUN,
  PS5_CAPTURE_READ_ENDED,
  PS5_CAPTURE_READ_INVALID,
  PS5_CAPTURE_READ_ERROR,
  PS5_CAPTURE_READ_INVALID_LENGTH
};

enum ps5_capture_probe_call {
  PS5_CAPTURE_CALL_LOOKUP_LOAD_MODULE = UINT32_C(1) << 0,
  PS5_CAPTURE_CALL_LOAD_IPMI = UINT32_C(1) << 1,
  PS5_CAPTURE_CALL_LOAD_AVCAP2 = UINT32_C(1) << 2,
  PS5_CAPTURE_CALL_INITIALIZE = UINT32_C(1) << 3,
  PS5_CAPTURE_CALL_OPEN = UINT32_C(1) << 4,
  PS5_CAPTURE_CALL_START = UINT32_C(1) << 5,
  PS5_CAPTURE_CALL_READ = UINT32_C(1) << 6,
  PS5_CAPTURE_CALL_STOP = UINT32_C(1) << 7,
  PS5_CAPTURE_CALL_CLOSE = UINT32_C(1) << 8,
  PS5_CAPTURE_CALL_TERMINATE = UINT32_C(1) << 9,
  PS5_CAPTURE_CALL_WAIT_BEFORE_READ = UINT32_C(1) << 10,
  PS5_CAPTURE_CALL_RESOLVE_AVCAP2 = UINT32_C(1) << 11
};

/* Metadata only. No native handle, private parameter block, sample bytes or
 * metadata bytes leave the call. Raw fields are meaningful only when their
 * corresponding called_mask bit is present. raw_module_lookup_status records
 * the first failed module lookup, or zero after all seven resolve.
 */
struct ps5_capture_probe_result {
  enum ps5_capture_probe_status status;
  enum ps5_capture_probe_phase primary_phase;
  enum ps5_capture_probe_read_kind read_kind;
  uint32_t called_mask;
  uint32_t resolved_mask;
  int64_t raw_load_lookup_status;
  int64_t raw_module_lookup_status;
  int32_t raw_ipmi_module;
  int32_t raw_ipmi_start_result;
  int32_t raw_avcap2_module;
  int32_t raw_avcap2_start_result;
  int32_t raw_initialize_status;
  int32_t raw_open_status;
  int32_t raw_start_status;
  int32_t raw_wait_status;
  int32_t raw_read_status;
  int32_t raw_stop_status;
  int32_t raw_close_status;
  int32_t raw_terminate_status;
  uint32_t read_capacity_bytes;
  uint32_t read_bytes;
  uint32_t requested_frame_bytes;
  uint32_t requested_frames;
  uint8_t whole_requested_frames;
  uint8_t discontinuity;
  uint8_t timestamp_present;
  uint8_t cleanup_uncertain;
};

/* One synchronous, non-reentrant diagnostic cycle:
 * lookup LoadStartModule; load Ipmi then Avcap2, requiring nonnegative module
 * returns and exact-zero secondary start results; resolve seven Avcap2 exports;
 * Initialize/Open/Start; optionally call one injected 100,000 us settle hook;
 * perform one READ_NOWAIT read; consume each owned resource once with
 * Stop/Close/Terminate. There is no retry, polling loop, allocation, credential
 * change, module unload, sample inspection or retained pointer. Internal audio
 * and 16-byte native metadata scratch are wiped before return. A callback that
 * violates its native write bounds can still corrupt memory; post-call checks
 * cannot repair that native ABI violation.
 */
enum ps5_capture_probe_status
ps5_capture_probe_run(const struct ps5_capture_probe_ops *ops,
                      struct ps5_capture_probe_result *out);

/* Safer first-hardware gate: perform lookup, ordered module loads and all seven
 * export resolutions, then return without Initialize/Open/Start/Read or any
 * Avcap2 cleanup call. Modules remain process-scoped as in both pinned sources.
 */
enum ps5_capture_probe_status
ps5_capture_probe_preflight(const struct ps5_capture_probe_ops *ops,
                            struct ps5_capture_probe_result *out);

/* Deterministic ASCII v1 metadata report without a trailing NUL. On short
 * capacity, data is unchanged and *out_len receives the required length.
 * data=NULL, capacity=0 is the supported length query. No sample or native
 * metadata byte is encoded.
 */
enum ps5_capture_probe_status
ps5_capture_probe_encode_report(const struct ps5_capture_probe_result *result,
                                char *data, size_t capacity, size_t *out_len);

#endif
