/* SPDX-License-Identifier: GPL-3.0-only */
/* Avcap2 signatures/layout retained from the reviewed diagnostic; provenance in
 * docs/CAPTURE-STREAM.md. This is separate from the one-read baseline. */
#include "capture_stream.h"

typedef int (*load_fn)(const char *, size_t, const void *, uint32_t, void *, int *);
typedef int (*init_fn)(void);
typedef int (*open_fn)(void **, const void *, uint8_t);
typedef int (*handle_fn)(void *);
typedef int (*read_fn)(void *, void *, size_t, void *, uint32_t);
struct open_param {
  uint64_t size, unknown_08;
  uint32_t unknown_10, source, mode, unknown_1c;
  uint64_t unknown_20;
  uint32_t bitrate, padding;
  uint64_t unknown_30;
};
_Static_assert(sizeof(struct open_param) == 0x38, "reviewed OpenAudio size");
_Static_assert(offsetof(struct open_param, source) == 0x14, "source offset");
_Static_assert(offsetof(struct open_param, mode) == 0x18, "mode offset");
_Static_assert(offsetof(struct open_param, unknown_1c) == 0x1c, "unknown offset");
_Static_assert(sizeof(init_fn) == sizeof(void *), "function representation");
_Static_assert(sizeof(load_fn) == sizeof(void *), "function representation");
_Static_assert(sizeof(open_fn) == sizeof(void *), "function representation");
_Static_assert(sizeof(handle_fn) == sizeof(void *), "function representation");
_Static_assert(sizeof(read_fn) == sizeof(void *), "function representation");
_Static_assert(sizeof(struct ps5_capture_stream_read) == 36, "read ABI layout");
_Static_assert(sizeof(struct ps5_capture_stream_report) == 40, "report ABI layout");
struct context {
  init_fn initialize, terminate;
  open_fn open;
  handle_fn close, start, stop;
  read_fn read;
  void *handle;
  uint32_t initialized, opened, started, in_call;
  _Alignas(16) uint8_t scratch[PS5_CAPTURE_STREAM_MAX_BYTES];
};
static struct context *active;
static int opening;
static uint32_t uncertain_cleanup;
#ifdef PS5_CAPTURE_STREAM_HOST_TEST
/* Host fault fixtures model a fresh payload process between independent cases. */
void ps5_capture_stream_test_new_process(void) {
  if (!active && !opening) uncertain_cleanup = 0;
}
#endif
static void clear(void *pointer, size_t bytes) {
  uint8_t *p = pointer;
  for (size_t i = 0; i < bytes; ++i) p[i] = 0;
}
static void wipe(void *pointer, size_t bytes) {
  volatile uint8_t *p = pointer;
  for (size_t i = 0; i < bytes; ++i) p[i] = 0;
}
static void copy(void *destination, const void *source, size_t bytes) {
  uint8_t *d = destination;
  const uint8_t *s = source;
  for (size_t i = 0; i < bytes; ++i) d[i] = s[i];
}
static void report_init(struct ps5_capture_stream_report *r, uint32_t stage) {
  clear(r, sizeof *r);
  r->abi_version = PS5_CAPTURE_STREAM_ABI;
  r->stage = stage;
}
static int32_t result(struct ps5_capture_stream_report *r, int32_t status, int32_t raw) {
  r->status = status;
  r->raw_status = raw;
  return status;
}
static int overlaps(const void *left, size_t left_bytes, const void *right, size_t right_bytes) {
  uintptr_t l = (uintptr_t)left, r = (uintptr_t)right;
  if (left_bytes > UINTPTR_MAX - l || right_bytes > UINTPTR_MAX - r) return 1;
  return l < r + right_bytes && r < l + left_bytes;
}
static int valid_context(void *storage) {
  return storage && storage == active && (uintptr_t)storage % _Alignof(struct context) == 0;
}
static void cleanup(struct context *c, struct ps5_capture_stream_report *r) {
  if (c->started) {
    c->started = 0;
    r->raw_stop = c->stop(c->handle);
    if (r->raw_stop) r->cleanup_flags |= PS5_CAPTURE_STREAM_STOP_FAILED;
  }
  if (c->opened) {
    c->opened = 0;
    void *handle = c->handle;
    c->handle = 0;
    r->raw_close = c->close(handle);
    if (r->raw_close) r->cleanup_flags |= PS5_CAPTURE_STREAM_CLOSE_FAILED;
  }
  if (c->initialized) {
    c->initialized = 0;
    r->raw_terminate = c->terminate();
    if (r->raw_terminate) r->cleanup_flags |= PS5_CAPTURE_STREAM_TERMINATE_FAILED;
  }
  uncertain_cleanup |= r->cleanup_flags;
  wipe(c->scratch, sizeof c->scratch);
}
uint32_t ps5_capture_stream_abi_version(void) { return PS5_CAPTURE_STREAM_ABI; }
uint32_t ps5_capture_stream_storage_size(void) { return (uint32_t)sizeof(struct context); }
uint32_t ps5_capture_stream_storage_alignment(void) { return (uint32_t)_Alignof(struct context); }
int32_t ps5_capture_stream_open(void *storage, uint32_t capacity,
    const struct ps5_capture_stream_ops *ops, struct ps5_capture_stream_report *r) {
  if (!r) return -2;
  /* Validate disjointness before writing outputs into potentially live storage. */
  if ((storage && overlaps(storage, capacity, r, sizeof *r)) ||
      (ops && overlaps(ops, sizeof *ops, r, sizeof *r))) return -2;
  report_init(r, 1);
  if (!storage || capacity < sizeof(struct context) ||
      (uintptr_t)storage % _Alignof(struct context) || !ops ||
      ops->abi_version != PS5_CAPTURE_STREAM_ABI || ops->struct_bytes != sizeof *ops ||
      !ops->lookup_current || !ops->lookup_module ||
      overlaps(storage, capacity, ops, sizeof *ops)) return result(r, -2, 0);
  if (active || opening || uncertain_cleanup) {
    r->cleanup_flags = uncertain_cleanup;
    return result(r, -3, 0);
  }
  opening = 1;
  struct context *c = storage;
  clear(c, sizeof *c);
  c->in_call = 1;
  void *symbol = 0;
  r->stage = 2;
  int64_t lookup = ops->lookup_current("sceKernelLoadStartModule", &symbol);
  r->raw_lookup = lookup;
  if (lookup || !symbol) { result(r, -1, 0); goto failed; }
  load_fn load = 0;
  copy(&load, &symbol, sizeof load);
  int secondary = 0;
  int module = load("/system/common/lib/libSceIpmi.sprx", 0, 0, 0, 0, &secondary);
  if (module < 0 || secondary) { result(r, -1, module < 0 ? module : secondary); goto failed; }
  secondary = 0;
  module = load("/system/common/lib/libSceAvcap2.sprx", 0, 0, 0, 0, &secondary);
  if (module < 0 || secondary) { result(r, -1, module < 0 ? module : secondary); goto failed; }
#define RESOLVE(field, name) do { \
    symbol = 0; \
    lookup = ops->lookup_module(module, name, &symbol); \
    r->raw_lookup = lookup; \
    if (lookup || !symbol) { result(r, -1, 0); goto failed; } \
    copy(&c->field, &symbol, sizeof c->field); \
  } while (0)
  RESOLVE(initialize, "sceAvcap2Initialize");
  RESOLVE(terminate, "sceAvcap2Terminate");
  RESOLVE(open, "sceAvcap2OpenAudio");
  RESOLVE(close, "sceAvcap2Close");
  RESOLVE(start, "sceAvcap2Start");
  RESOLVE(stop, "sceAvcap2Stop");
  RESOLVE(read, "sceAvcap2ReadAudio");
#undef RESOLVE
  r->stage = 3;
  int raw = c->initialize();
  if (raw) { result(r, -5, raw); goto failed; }
  c->initialized = 1;
  struct open_param param = {0};
  param.size = sizeof param;
  param.unknown_08 = 0x18;
  param.source = 2;
  param.mode = 0;
  param.unknown_1c = UINT32_MAX;
  r->stage = 4;
  raw = c->open(&c->handle, &param, 0);
  if (raw || !c->handle) { c->handle = 0; result(r, -5, raw); goto failed; }
  c->opened = 1;
  c->in_call = 0;
  active = c;
  opening = 0;
  return 0;
failed:
  cleanup(c, r);
  wipe(c, sizeof *c);
  opening = 0;
  return r->status;
}
static int32_t begin(void *storage, struct ps5_capture_stream_report *r, uint32_t stage) {
  if (!r || (storage && overlaps(storage, sizeof(struct context), r, sizeof *r))) return -2;
  report_init(r, stage);
  if (!valid_context(storage)) return result(r, -2, 0);
  struct context *c = storage;
  if (c->in_call) return result(r, -3, 0);
  c->in_call = 1;
  return 0;
}
int32_t ps5_capture_stream_start(void *storage, struct ps5_capture_stream_report *r) {
  int32_t status = begin(storage, r, 5);
  if (status) return status;
  struct context *c = storage;
  if (!c->opened || c->started) { c->in_call = 0; return result(r, -2, 0); }
  int raw = c->start(c->handle);
  if (!raw) c->started = 1;
  c->in_call = 0;
  return result(r, raw ? -5 : 0, raw);
}
int32_t ps5_capture_stream_read_once(void *storage, uint8_t *output, uint32_t capacity,
    struct ps5_capture_stream_read *read, struct ps5_capture_stream_report *r) {
  if (!r || !read || !output || capacity == 0 || capacity > PS5_CAPTURE_STREAM_MAX_BYTES ||
      overlaps(read, sizeof *read, r, sizeof *r) ||
      overlaps(output, capacity, read, sizeof *read) ||
      overlaps(output, capacity, r, sizeof *r) ||
      (storage && (overlaps(storage, sizeof(struct context), read, sizeof *read) ||
                   overlaps(storage, sizeof(struct context), output, capacity)))) return -2;
  clear(read, sizeof *read);
  read->abi_version = PS5_CAPTURE_STREAM_ABI;
  int32_t status = begin(storage, r, 6);
  if (status) return status;
  struct context *c = storage;
  if (!c->started) { c->in_call = 0; return result(r, -2, 0); }
  _Alignas(16) uint8_t metadata[16] = {0};
  metadata[0] = 16;
  int raw = c->read(c->handle, c->scratch, capacity, metadata, 1);
  read->raw_status = raw;
  copy(read->metadata, metadata, sizeof metadata);
  if (raw > 0) {
    if ((uint32_t)raw > capacity) { read->kind = 6; status = -5; }
    else { read->kind = 1; read->bytes = (uint32_t)raw; copy(output, c->scratch, read->bytes); }
  } else if (raw == 0) read->kind = 2;
  else if ((uint32_t)raw == UINT32_C(0x81950002)) read->kind = 3;
  else if ((uint32_t)raw == UINT32_C(0x81950004)) { read->kind = 4; read->flags = PS5_CAPTURE_STREAM_DISCONTINUITY; }
  else if ((uint32_t)raw == UINT32_C(0x81950008)) read->kind = 5;
  else read->kind = 6;
  wipe(metadata, sizeof metadata);
  wipe(c->scratch, sizeof c->scratch);
  c->in_call = 0;
  return result(r, status, raw);
}
int32_t ps5_capture_stream_stop(void *storage, struct ps5_capture_stream_report *r) {
  int32_t status = begin(storage, r, 7);
  if (status) return status;
  struct context *c = storage;
  if (!c->started) { c->in_call = 0; return result(r, -2, 0); }
  c->started = 0;
  r->raw_stop = c->stop(c->handle);
  if (r->raw_stop) r->cleanup_flags |= PS5_CAPTURE_STREAM_STOP_FAILED;
  uncertain_cleanup |= r->cleanup_flags;
  c->in_call = 0;
  return result(r, r->raw_stop ? -5 : 0, r->raw_stop);
}
int32_t ps5_capture_stream_close(void *storage, struct ps5_capture_stream_report *r) {
  int32_t status = begin(storage, r, 8);
  if (status) return status;
  struct context *c = storage;
  cleanup(c, r);
  active = 0;
  wipe(c, sizeof *c);
  return result(r, r->cleanup_flags ? -5 : 0,
      r->raw_stop ? r->raw_stop : (r->raw_close ? r->raw_close : r->raw_terminate));
}
