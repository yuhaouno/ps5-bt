/* SPDX-License-Identifier: GPL-3.0-only */
#include "capture_stream_fixture.h"
#include <assert.h>
#include <string.h>
#ifdef PS5_CAPTURE_STREAM_HOST_TEST
void ps5_capture_stream_test_new_process(void);
#else
#error "capture fixture requires the explicit host-test macro"
#endif
static uint32_t calls[16], fail_phase;
static int32_t failure, read_raw;
static int handle;
void ps5_capture_test_reset(void) { ps5_capture_stream_test_new_process(); memset(calls, 0, sizeof calls); fail_phase = 0; failure = -44; read_raw = 8; }
void ps5_capture_test_fail(uint32_t phase, int32_t raw) { fail_phase = phase; failure = raw; }
void ps5_capture_test_read(int32_t raw) { read_raw = raw; }
uint32_t ps5_capture_test_calls(uint32_t phase) { assert(phase < 16); return calls[phase]; }
static int call(uint32_t phase) { ++calls[phase]; return phase == fail_phase ? failure : 0; }
static int initialize(void) { return call(1); }
static int terminate(void) { return call(7); }
static int open_audio(void **out, const void *param, uint8_t kind) {
  assert(kind == 0);
  unsigned char expected[0x38] = {0};
  expected[0] = 0x38; expected[8] = 0x18; expected[0x14] = 2;
  memset(expected + 0x1c, 255, 4);
  assert(memcmp(param, expected, sizeof expected) == 0);
  *out = &handle; /* dirty failure handle must never become owned */
  return call(2);
}
static int start(void *h) { assert(h == &handle); return call(3); }
static int stop(void *h) { assert(h == &handle); return call(5); }
static int close_audio(void *h) { assert(h == &handle); return call(6); }
static int read_audio(void *h, void *output, size_t capacity, void *metadata, uint32_t flags) {
  assert(h == &handle && flags == 1 && capacity > 0 && capacity <= 8192);
  assert((uintptr_t)output % 16 == 0 && (uintptr_t)metadata % 16 == 0);
  unsigned char *m = metadata;
  assert(m[0] == 16);
  for (unsigned i = 1; i < 16; ++i) assert(m[i] == 0);
  int failure_result = call(4);
  if (failure_result) return failure_result;
  memset(metadata, 0xa5, 16); /* preserved as opaque bytes */
  if (read_raw > 0) memset(output, 0x3c, (size_t)read_raw < capacity ? (size_t)read_raw : capacity);
  return read_raw;
}
static int load_module(const char *path, size_t bytes, const void *args, uint32_t flags, void *options, int *secondary) {
  assert(bytes == 0 && args == 0 && flags == 0 && options == 0 && *secondary == 0);
  uint32_t phase = strcmp(path, "/system/common/lib/libSceIpmi.sprx") == 0 ? 8 : 9;
  if (phase == 9) assert(strcmp(path, "/system/common/lib/libSceAvcap2.sprx") == 0);
  int result = call(phase);
  return result ? result : (int)phase;
}
int64_t ps5_capture_test_current(const char *name, void **out) {
  assert(strcmp(name, "sceKernelLoadStartModule") == 0);
  int (*fn)(const char *, size_t, const void *, uint32_t, void *, int *) = load_module;
  memcpy(out, &fn, sizeof fn);
  int result = call(10);
  return result ? INT64_C(0x100000000) + result : 0;
}
int64_t ps5_capture_test_module(int32_t module, const char *name, void **out) {
  assert(module == 9);
  int result = call(11);
  if (result) { *out = 0; return result; }
#define SYMBOL(n, f, t) if (strcmp(name, n) == 0) { t fn = f; memcpy(out, &fn, sizeof fn); return 0; }
  typedef int (*init_fn)(void);
  typedef int (*open_fn)(void **, const void *, uint8_t);
  typedef int (*handle_fn)(void *);
  typedef int (*read_fn)(void *, void *, size_t, void *, uint32_t);
  SYMBOL("sceAvcap2Initialize", initialize, init_fn)
  SYMBOL("sceAvcap2Terminate", terminate, init_fn)
  SYMBOL("sceAvcap2OpenAudio", open_audio, open_fn)
  SYMBOL("sceAvcap2Close", close_audio, handle_fn)
  SYMBOL("sceAvcap2Start", start, handle_fn)
  SYMBOL("sceAvcap2Stop", stop, handle_fn)
  SYMBOL("sceAvcap2ReadAudio", read_audio, read_fn)
#undef SYMBOL
  assert(0); return -1;
}
