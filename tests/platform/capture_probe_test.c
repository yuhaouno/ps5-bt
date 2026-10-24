/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "capture_probe.h"

#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static unsigned lookup_calls, module_lookup_calls, load_calls;
static unsigned initialize_calls, open_calls, start_calls, wait_calls,
    read_calls;
static unsigned stop_calls, close_calls, terminate_calls;
static unsigned sequence;
static int64_t lookup_status, module_lookup_status;
static int fail_symbol_index;
static int ipmi_module, avcap2_module;
static int ipmi_start_result, avcap2_start_result;
static int initialize_status, open_status, start_status, wait_status,
    read_status;
static int stop_status, close_status, terminate_status;
static int return_null_handle, dirty_failed_handle, reenter_on_read;
static struct ps5_capture_probe_result nested_result;
static const struct ps5_capture_probe_ops *active_ops;

struct trace_event {
  enum ps5_capture_probe_phase phase;
  uint32_t ordinal;
  int entering;
  int64_t raw_status;
};

static struct trace_event trace_events[64];
static size_t trace_count;

static const char *const expected_symbols[] = {
    "sceAvcap2Initialize", "sceAvcap2Terminate", "sceAvcap2OpenAudio",
    "sceAvcap2Close",      "sceAvcap2Start",     "sceAvcap2Stop",
    "sceAvcap2ReadAudio",
};

static void store_function(void **out, const void *function, size_t size) {
  assert(out && *out == NULL && size == sizeof *out);
  memcpy(out, function, size);
}

static int fake_initialize(void) {
  ++initialize_calls;
  assert(++sequence == 1);
  return initialize_status;
}

static int fake_terminate(void) {
  ++terminate_calls;
  assert(++sequence == (read_calls    ? (wait_calls ? 8u : 7u)
                        : wait_calls  ? 7u
                        : start_calls ? 5u
                                      : 3u));
  return terminate_status;
}

static int fake_open(void **out, const void *raw_param, uint8_t kind) {
  const unsigned char *param = raw_param;
  uint64_t size, unknown_08;
  uint32_t source, mode, unknown_1c;
  ++open_calls;
  assert(++sequence == 2);
  assert(out && *out == NULL && raw_param && kind == 0);
  memcpy(&size, param, sizeof size);
  memcpy(&unknown_08, param + 8, sizeof unknown_08);
  memcpy(&source, param + 0x14, sizeof source);
  memcpy(&mode, param + 0x18, sizeof mode);
  memcpy(&unknown_1c, param + 0x1c, sizeof unknown_1c);
  assert(size == 0x38 && unknown_08 == 0x18 && source == 2 && mode == 0 &&
         unknown_1c == UINT32_MAX);
  if (!return_null_handle)
    *out = (void *)(uintptr_t)(dirty_failed_handle ? 0x2222 : 0x1111);
  return open_status;
}

static int fake_start(void *handle) {
  ++start_calls;
  assert(++sequence == 3 && handle == (void *)(uintptr_t)0x1111);
  return start_status;
}

static int fake_read(void *handle, void *buffer, size_t capacity,
                     void *raw_metadata, uint32_t flags) {
  unsigned char *bytes = buffer;
  const unsigned char *metadata = raw_metadata;
  ++read_calls;
  assert(++sequence == (wait_calls ? 5u : 4u) &&
         handle == (void *)(uintptr_t)0x1111);
  assert(buffer && capacity == PS5_CAPTURE_PROBE_READ_BYTES);
  assert(raw_metadata && metadata[0] == PS5_CAPTURE_PROBE_METADATA_BYTES);
  assert(flags == 1);
  if (read_status > 0 && (size_t)read_status <= capacity)
    memset(bytes, 0xa5, (size_t)read_status);
  if (reenter_on_read)
    assert(ps5_capture_probe_run(active_ops, &nested_result) ==
           PS5_CAPTURE_PROBE_BUSY);
  return read_status;
}

static int fake_wait(uint32_t microseconds) {
  ++wait_calls;
  assert(++sequence == 4);
  assert(microseconds == PS5_CAPTURE_PROBE_SETTLE_US);
  return wait_status;
}

static int fake_stop(void *handle) {
  ++stop_calls;
  assert(++sequence == (wait_calls ? (read_calls ? 6u : 5u) : 5u) &&
         handle == (void *)(uintptr_t)0x1111);
  return stop_status;
}

static int fake_close(void *handle) {
  ++close_calls;
  assert(handle == (void *)(uintptr_t)0x1111);
  assert(++sequence == (read_calls   ? (wait_calls ? 7u : 6u)
                        : wait_calls ? 6u
                                     : 4u));
  return close_status;
}

static int fake_load(const char *path, size_t argc, const void *argv,
                     uint32_t flags, void *options, int *start_result) {
  ++load_calls;
  assert(argc == 0 && argv == NULL && flags == 0 && options == NULL);
  assert(start_result && *start_result == 0);
  if (load_calls == 1) {
    assert(strcmp(path, "/system/common/lib/libSceIpmi.sprx") == 0);
    *start_result = ipmi_start_result;
    return ipmi_module;
  }
  assert(load_calls == 2);
  assert(strcmp(path, "/system/common/lib/libSceAvcap2.sprx") == 0);
  *start_result = avcap2_start_result;
  return avcap2_module;
}

static int64_t fake_lookup_current(const char *name, void **out) {
  ++lookup_calls;
  assert(lookup_calls == 1);
  assert(strcmp(name, "sceKernelLoadStartModule") == 0);
  assert(out && *out == NULL);
  if (lookup_status == 0) {
    int (*function)(const char *, size_t, const void *, uint32_t, void *,
                    int *) = fake_load;
    store_function(out, &function, sizeof function);
  } else {
    /* Dirty failure output must never be called. */
    *out = (void *)(uintptr_t)1;
  }
  return lookup_status;
}

static int64_t fake_lookup_module(int32_t module, const char *name,
                                  void **out) {
  unsigned index = module_lookup_calls++;
  assert(module == avcap2_module && index < PS5_CAPTURE_PROBE_SYMBOL_COUNT);
  assert(strcmp(name, expected_symbols[index]) == 0);
  assert(out && *out == NULL);
  if ((int)index == fail_symbol_index) {
    *out = (void *)(uintptr_t)1;
    return module_lookup_status;
  }
  switch (index) {
  case 0: {
    int (*function)(void) = fake_initialize;
    store_function(out, &function, sizeof function);
    break;
  }
  case 1: {
    int (*function)(void) = fake_terminate;
    store_function(out, &function, sizeof function);
    break;
  }
  case 2: {
    int (*function)(void **, const void *, uint8_t) = fake_open;
    store_function(out, &function, sizeof function);
    break;
  }
  case 3: {
    int (*function)(void *) = fake_close;
    store_function(out, &function, sizeof function);
    break;
  }
  case 4: {
    int (*function)(void *) = fake_start;
    store_function(out, &function, sizeof function);
    break;
  }
  case 5: {
    int (*function)(void *) = fake_stop;
    store_function(out, &function, sizeof function);
    break;
  }
  case 6: {
    int (*function)(void *, void *, size_t, void *, uint32_t) = fake_read;
    store_function(out, &function, sizeof function);
    break;
  }
  default:
    assert(0);
  }
  return 0;
}

static void fake_trace(enum ps5_capture_probe_phase phase, uint32_t ordinal,
                       int entering, int64_t raw_status) {
  assert(trace_count < sizeof trace_events / sizeof trace_events[0]);
  trace_events[trace_count++] =
      (struct trace_event){phase, ordinal, entering, raw_status};
}

static const struct ps5_capture_probe_ops ops = {
    .lookup_current = fake_lookup_current,
    .lookup_module = fake_lookup_module,
    .trace = fake_trace,
    .wait_before_read = fake_wait,
};

static const struct ps5_capture_probe_ops ops_no_trace = {
    .lookup_current = fake_lookup_current,
    .lookup_module = fake_lookup_module,
    .trace = NULL,
    .wait_before_read = NULL,
};

static const struct ps5_capture_probe_ops ops_no_wait = {
    .lookup_current = fake_lookup_current,
    .lookup_module = fake_lookup_module,
    .trace = fake_trace,
    .wait_before_read = NULL,
};

static void reset(void) {
  lookup_calls = module_lookup_calls = load_calls = 0;
  initialize_calls = open_calls = start_calls = wait_calls = read_calls = 0;
  stop_calls = close_calls = terminate_calls = 0;
  sequence = 0;
  lookup_status = module_lookup_status = 0;
  fail_symbol_index = -1;
  ipmi_module = 101;
  avcap2_module = 202;
  ipmi_start_result = avcap2_start_result = 0;
  initialize_status = open_status = start_status = wait_status = 0;
  read_status = 512;
  stop_status = close_status = terminate_status = 0;
  return_null_handle = dirty_failed_handle = reenter_on_read = 0;
  trace_count = 0;
  memset(&nested_result, 0xa5, sizeof nested_result);
  active_ops = &ops;
}

static void assert_trace_pair(size_t *index, enum ps5_capture_probe_phase phase,
                              uint32_t ordinal, int64_t raw_status) {
  assert(*index + 1 < trace_count);
  assert(trace_events[*index].phase == phase &&
         trace_events[*index].ordinal == ordinal &&
         trace_events[*index].entering == 1 &&
         trace_events[*index].raw_status == 0);
  ++*index;
  assert(trace_events[*index].phase == phase &&
         trace_events[*index].ordinal == ordinal &&
         trace_events[*index].entering == 0 &&
         trace_events[*index].raw_status == raw_status);
  ++*index;
}

static void assert_full_cleanup(void) {
  assert(initialize_calls == 1 && open_calls == 1 && start_calls == 1);
  assert(wait_calls == 1 && read_calls == 1 && stop_calls == 1 &&
         close_calls == 1 && terminate_calls == 1);
  assert(sequence == 8);
}

static void test_success_and_report(void) {
  struct ps5_capture_probe_result result;
  char report[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 2];
  size_t length = 0;
  reset();
  reenter_on_read = 1;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.status == PS5_CAPTURE_PROBE_OK &&
         result.primary_phase == PS5_CAPTURE_PHASE_DONE);
  assert(result.resolved_mask == 0x7f && module_lookup_calls == 7);
  assert(result.called_mask & PS5_CAPTURE_CALL_WAIT_BEFORE_READ);
  assert(result.read_kind == PS5_CAPTURE_READ_DATA &&
         result.raw_read_status == 512 && result.read_bytes == 512);
  assert(result.read_capacity_bytes == 8192 &&
         result.requested_frame_bytes == 8 && result.requested_frames == 64 &&
         result.whole_requested_frames == 1);
  assert(!result.timestamp_present && !result.cleanup_uncertain);
  assert(nested_result.status == PS5_CAPTURE_PROBE_BUSY &&
         nested_result.called_mask == 0);
  assert_full_cleanup();
  size_t trace_index = 0;
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_LOAD_IPMI, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_LOAD_AVCAP2, 0, 0);
  for (uint32_t ordinal = 0; ordinal < PS5_CAPTURE_PROBE_SYMBOL_COUNT;
       ++ordinal)
    assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_RESOLVE_AVCAP2, ordinal,
                      0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_INITIALIZE, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_OPEN, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_START, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_WAIT_BEFORE_READ, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_READ, 0, 512);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_STOP, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_CLOSE, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_TERMINATE, 0, 0);
  assert(trace_index == trace_count);

  memset(report, 'X', sizeof report);
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &length) == PS5_CAPTURE_PROBE_OK);
  assert(length > 0 && length <= PS5_CAPTURE_PROBE_REPORT_CAPACITY);
  assert(report[length] == 'X' && report[length - 1] == '\n');
  report[length] = '\0';
  assert(strstr(report, "capture_probe_v1 status=ok phase=done"));
  assert(strstr(report, "capture_invoked=1"));
  assert(strstr(report, "ipmi_module=101"));
  assert(strstr(report, "avcap2_module=202"));
  assert(strstr(report, "wait_status=0"));
  assert(strstr(report, "read_kind=data read_raw=0x00000200"));
  assert(strstr(report, "read_capacity_bytes=8192 read_bytes=512"));
  assert(strstr(report, "requested_frame_bytes=8 requested_frames=64"));
  assert(strstr(report, "format=requires_hardware samples_stored=0"));
  assert(strstr(report, "cleanup_uncertain=0"));
  assert(strstr(report, "a5a5") == NULL); /* No capture bytes. */

  for (size_t capacity = 0; capacity < length; ++capacity) {
    char guarded[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 2];
    size_t required = 0;
    memset(guarded, 'X', sizeof guarded);
    assert(ps5_capture_probe_encode_report(&result, guarded + 1, capacity,
                                           &required) ==
           PS5_CAPTURE_PROBE_CAPACITY);
    assert(required == length && guarded[0] == 'X');
    for (size_t i = 1; i < sizeof guarded; ++i)
      assert(guarded[i] == 'X');
  }
}

static void test_preflight(void) {
  struct ps5_capture_probe_result result;
  char report[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 1];
  size_t length;
  reset();
  assert(ps5_capture_probe_preflight(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.status == PS5_CAPTURE_PROBE_OK &&
         result.primary_phase == PS5_CAPTURE_PHASE_DONE);
  assert(result.called_mask ==
         (PS5_CAPTURE_CALL_LOOKUP_LOAD_MODULE | PS5_CAPTURE_CALL_LOAD_IPMI |
          PS5_CAPTURE_CALL_LOAD_AVCAP2 | PS5_CAPTURE_CALL_RESOLVE_AVCAP2));
  assert(result.resolved_mask == 0x7f && module_lookup_calls == 7);
  assert(result.read_kind == PS5_CAPTURE_READ_NOT_CALLED);
  assert(initialize_calls == 0 && open_calls == 0 && start_calls == 0 &&
         wait_calls == 0 && read_calls == 0 && stop_calls == 0 &&
         close_calls == 0 && terminate_calls == 0);
  assert(trace_count == 20);
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &length) == PS5_CAPTURE_PROBE_OK);
  report[length] = '\0';
  assert(strstr(report, "capture_invoked=0"));
  assert(strstr(report, "read_kind=not_called read_raw=na"));

  reset();
  assert(ps5_capture_probe_preflight(&ops_no_trace, &result) ==
         PS5_CAPTURE_PROBE_OK);
  assert(trace_count == 0);
}

static void test_optional_wait_and_failure(void) {
  struct ps5_capture_probe_result result;
  char report[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 1];
  size_t length;

  reset();
  assert(ps5_capture_probe_run(&ops_no_wait, &result) == PS5_CAPTURE_PROBE_OK);
  assert(wait_calls == 0 && read_calls == 1 && sequence == 7);
  assert(!(result.called_mask & PS5_CAPTURE_CALL_WAIT_BEFORE_READ));
  assert(result.raw_wait_status == 0);
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &length) == PS5_CAPTURE_PROBE_OK);
  report[length] = '\0';
  assert(strstr(report, "wait_status=na"));

  reset();
  wait_status = -54;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_WAIT_BEFORE_READ);
  assert(result.called_mask & PS5_CAPTURE_CALL_WAIT_BEFORE_READ);
  assert(!(result.called_mask & PS5_CAPTURE_CALL_READ));
  assert(result.raw_wait_status == -54);
  assert(wait_calls == 1 && read_calls == 0 && stop_calls == 1 &&
         close_calls == 1 && terminate_calls == 1 && sequence == 7);
  size_t trace_index = 26;
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_WAIT_BEFORE_READ, 0, -54);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_STOP, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_CLOSE, 0, 0);
  assert_trace_pair(&trace_index, PS5_CAPTURE_PHASE_TERMINATE, 0, 0);
  assert(trace_index == trace_count);
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &length) == PS5_CAPTURE_PROBE_OK);
  report[length] = '\0';
  assert(strstr(report, "status=failed phase=wait_before_read"));
  assert(strstr(report, "wait_status=-54"));
  assert(strstr(report, "read_kind=not_called read_raw=na"));
}

static void test_read_classes(void) {
  struct ps5_capture_probe_result result;
  const struct {
    uint32_t raw;
    enum ps5_capture_probe_status status;
    enum ps5_capture_probe_read_kind kind;
    uint8_t discontinuity;
  } cases[] = {
      {0, PS5_CAPTURE_PROBE_OK, PS5_CAPTURE_READ_ZERO, 0},
      {UINT32_C(0x81950002), PS5_CAPTURE_PROBE_OK, PS5_CAPTURE_READ_EMPTY, 0},
      {UINT32_C(0x81950004), PS5_CAPTURE_PROBE_OK, PS5_CAPTURE_READ_OVERRUN, 1},
      {UINT32_C(0x81950008), PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_READ_ENDED,
       0},
      {UINT32_C(0x81950001), PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_READ_INVALID,
       0},
      {UINT32_C(0x81234567), PS5_CAPTURE_PROBE_FAILED, PS5_CAPTURE_READ_ERROR,
       0},
  };
  for (size_t i = 0; i < sizeof cases / sizeof cases[0]; ++i) {
    reset();
    read_status = (int32_t)cases[i].raw;
    assert(ps5_capture_probe_run(&ops, &result) == cases[i].status);
    assert(result.read_kind == cases[i].kind);
    assert((uint32_t)result.raw_read_status == cases[i].raw);
    assert(result.discontinuity == cases[i].discontinuity);
    assert(result.read_bytes == 0 && result.requested_frames == 0);
    assert_full_cleanup();
  }

  const int invalid_lengths[] = {1, 7, 9, 8191, 8193, INT_MAX};
  for (size_t i = 0; i < sizeof invalid_lengths / sizeof invalid_lengths[0];
       ++i) {
    reset();
    read_status = invalid_lengths[i];
    assert(ps5_capture_probe_run(&ops, &result) ==
           PS5_CAPTURE_PROBE_INVALID_LENGTH);
    assert(result.read_kind == PS5_CAPTURE_READ_INVALID_LENGTH &&
           result.primary_phase == PS5_CAPTURE_PHASE_READ);
    assert(result.read_bytes == 0 && !result.whole_requested_frames);
    assert_full_cleanup();
  }
}

static void test_setup_failures(void) {
  struct ps5_capture_probe_result result;
  char report[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 1];
  size_t report_length;
  reset();
  assert(ps5_capture_probe_run(NULL, &result) == PS5_CAPTURE_PROBE_INVALID);
  assert(ps5_capture_probe_run(&ops, NULL) == PS5_CAPTURE_PROBE_INVALID);

  reset();
  lookup_status = INT64_MIN;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_UNAVAILABLE);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_LOOKUP_LOAD_MODULE &&
         result.raw_load_lookup_status == INT64_MIN);
  assert(load_calls == 0 && initialize_calls == 0);

  reset();
  ipmi_module = -31;
  ipmi_start_result = 17;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_UNAVAILABLE);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_LOAD_IPMI &&
         result.raw_ipmi_module == -31 && result.raw_ipmi_start_result == 17);
  assert(load_calls == 1 && module_lookup_calls == 0);

  reset();
  ipmi_start_result = 17;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_UNAVAILABLE);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_LOAD_IPMI &&
         result.raw_ipmi_module == 101 && result.raw_ipmi_start_result == 17);
  assert(load_calls == 1 && module_lookup_calls == 0 && initialize_calls == 0 &&
         stop_calls == 0 && close_calls == 0 && terminate_calls == 0);
  assert(trace_count == 4 && trace_events[3].raw_status == 17);

  reset();
  avcap2_module = -41;
  avcap2_start_result = 23;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_UNAVAILABLE);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_LOAD_AVCAP2 &&
         result.raw_avcap2_module == -41 &&
         result.raw_avcap2_start_result == 23);
  assert(load_calls == 2 && module_lookup_calls == 0);

  reset();
  avcap2_start_result = 23;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_UNAVAILABLE);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_LOAD_AVCAP2 &&
         result.raw_avcap2_module == 202 &&
         result.raw_avcap2_start_result == 23);
  assert(load_calls == 2 && module_lookup_calls == 0 && initialize_calls == 0 &&
         stop_calls == 0 && close_calls == 0 && terminate_calls == 0);
  assert(trace_count == 6 && trace_events[5].raw_status == 23);
  assert(!(result.called_mask & PS5_CAPTURE_CALL_RESOLVE_AVCAP2));
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &report_length) ==
         PS5_CAPTURE_PROBE_OK);
  report[report_length] = '\0';
  assert(strstr(report, "module_lookup_status=na"));

  for (int i = 0; i < (int)PS5_CAPTURE_PROBE_SYMBOL_COUNT; ++i) {
    reset();
    fail_symbol_index = i;
    module_lookup_status = -100 - i;
    assert(ps5_capture_probe_run(&ops, &result) ==
           PS5_CAPTURE_PROBE_UNAVAILABLE);
    assert(result.primary_phase == PS5_CAPTURE_PHASE_RESOLVE_AVCAP2 &&
           result.raw_module_lookup_status == -100 - i);
    assert(module_lookup_calls == (unsigned)i + 1 && initialize_calls == 0);
  }
}

static void test_lifecycle_and_cleanup_failures(void) {
  struct ps5_capture_probe_result result;
  reset();
  initialize_status = -51;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_INITIALIZE);
  assert(open_calls == 0 && terminate_calls == 0);

  reset();
  open_status = -52;
  dirty_failed_handle = 1;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_OPEN);
  assert(close_calls == 0 && terminate_calls == 1);

  reset();
  return_null_handle = 1;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.raw_open_status == 0 && close_calls == 0 &&
         terminate_calls == 1);

  reset();
  start_status = -53;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_START);
  assert(read_calls == 0 && stop_calls == 0 && close_calls == 1 &&
         terminate_calls == 1);

  reset();
  stop_status = -61;
  close_status = -62;
  terminate_status = -63;
  assert(ps5_capture_probe_run(&ops, &result) ==
         PS5_CAPTURE_PROBE_CLEANUP_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_STOP &&
         result.raw_stop_status == -61 && result.raw_close_status == -62 &&
         result.raw_terminate_status == -63 && result.cleanup_uncertain);
  assert_full_cleanup();

  reset();
  read_status = (int32_t)UINT32_C(0x81950008);
  close_status = -62;
  assert(ps5_capture_probe_run(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_READ &&
         result.cleanup_uncertain && result.raw_close_status == -62);
  assert_full_cleanup();
}

int main(void) {
  test_success_and_report();
  test_preflight();
  test_optional_wait_and_failure();
  test_read_classes();
  test_setup_failures();
  test_lifecycle_and_cleanup_failures();
  puts("PASS bounded metadata-only capture probe lifecycle and cleanup");
  return 0;
}
