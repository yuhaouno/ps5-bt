/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "capture_probe.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>

static unsigned initialize_calls, open_calls, start_calls, read_calls;
static unsigned wait_calls, now_calls, stop_calls, close_calls, terminate_calls;
static int32_t reads[PS5_CAPTURE_PROBE_OBSERVE_MAX_READS];
static size_t read_count;
static uint64_t clocks[PS5_CAPTURE_PROBE_OBSERVE_MAX_READS + 1];
static size_t clock_count;
static int wait_fail_at;
static int now_fail_at;
static int stop_status, close_status, terminate_status;

static void store_function(void **out, const void *function, size_t size) {
  assert(out && !*out && size == sizeof *out);
  memcpy(out, function, size);
}

static int fake_initialize(void) {
  ++initialize_calls;
  return 0;
}

static int fake_terminate(void) {
  ++terminate_calls;
  return terminate_status;
}

static int fake_open(void **out, const void *param, uint8_t kind) {
  ++open_calls;
  assert(out && !*out && param && kind == 0);
  *out = (void *)(uintptr_t)0x1234;
  return 0;
}

static int fake_start(void *handle) {
  ++start_calls;
  assert(handle == (void *)(uintptr_t)0x1234);
  return 0;
}

static int fake_read(void *handle, void *buffer, size_t capacity,
                     void *raw_metadata, uint32_t flags) {
  unsigned char *metadata = raw_metadata;
  assert(handle == (void *)(uintptr_t)0x1234 && buffer && capacity == 8192);
  assert(metadata && metadata[0] == PS5_CAPTURE_PROBE_METADATA_BYTES);
  assert(flags == 1 && read_calls < read_count);
  int32_t raw = reads[read_calls++];
  if (raw > 0 && (size_t)raw <= capacity)
    memset(buffer, 0x5a, (size_t)raw);
  return raw;
}

static int fake_stop(void *handle) {
  ++stop_calls;
  assert(handle == (void *)(uintptr_t)0x1234);
  return stop_status;
}

static int fake_close(void *handle) {
  ++close_calls;
  assert(handle == (void *)(uintptr_t)0x1234);
  return close_status;
}

static int fake_load(const char *path, size_t argc, const void *argv,
                     uint32_t flags, void *options, int *secondary) {
  (void)path;
  assert(argc == 0 && !argv && flags == 0 && !options && secondary);
  *secondary = 0;
  return strstr(path, "Avcap2") ? 202 : 101;
}

static int64_t lookup_current(const char *name, void **out) {
  assert(strcmp(name, "sceKernelLoadStartModule") == 0);
  int (*function)(const char *, size_t, const void *, uint32_t, void *, int *) =
      fake_load;
  store_function(out, &function, sizeof function);
  return 0;
}

static int64_t lookup_module(int32_t module, const char *name, void **out) {
  assert(module == 202);
  if (!strcmp(name, "sceAvcap2Initialize")) {
    int (*function)(void) = fake_initialize;
    store_function(out, &function, sizeof function);
  } else if (!strcmp(name, "sceAvcap2Terminate")) {
    int (*function)(void) = fake_terminate;
    store_function(out, &function, sizeof function);
  } else if (!strcmp(name, "sceAvcap2OpenAudio")) {
    int (*function)(void **, const void *, uint8_t) = fake_open;
    store_function(out, &function, sizeof function);
  } else if (!strcmp(name, "sceAvcap2Close")) {
    int (*function)(void *) = fake_close;
    store_function(out, &function, sizeof function);
  } else if (!strcmp(name, "sceAvcap2Start")) {
    int (*function)(void *) = fake_start;
    store_function(out, &function, sizeof function);
  } else if (!strcmp(name, "sceAvcap2Stop")) {
    int (*function)(void *) = fake_stop;
    store_function(out, &function, sizeof function);
  } else {
    assert(!strcmp(name, "sceAvcap2ReadAudio"));
    int (*function)(void *, void *, size_t, void *, uint32_t) = fake_read;
    store_function(out, &function, sizeof function);
  }
  return 0;
}

static int fake_wait(uint32_t microseconds) {
  ++wait_calls;
  assert(microseconds == PS5_CAPTURE_PROBE_OBSERVE_INTERVAL_US);
  return wait_fail_at == (int)wait_calls ? -71 : 0;
}

static int fake_now(uint64_t *out) {
  ++now_calls;
  assert(out);
  if (now_fail_at == (int)now_calls)
    return -72;
  assert(now_calls <= clock_count);
  *out = clocks[now_calls - 1];
  return 0;
}

static const struct ps5_capture_probe_ops ops = {
    .lookup_current = lookup_current,
    .lookup_module = lookup_module,
    .trace = NULL,
    .wait_before_read = fake_wait,
    .now_us = fake_now,
};

static void reset(void) {
  initialize_calls = open_calls = start_calls = read_calls = 0;
  wait_calls = now_calls = stop_calls = close_calls = terminate_calls = 0;
  memset(reads, 0, sizeof reads);
  memset(clocks, 0, sizeof clocks);
  read_count = clock_count = 0;
  wait_fail_at = now_fail_at = 0;
  stop_status = close_status = terminate_status = 0;
}

static void assert_cleanup_once(void) {
  assert(initialize_calls == 1 && open_calls == 1 && start_calls == 1);
  assert(stop_calls == 1 && close_calls == 1 && terminate_calls == 1);
}

static void test_immediate_data_and_report(void) {
  struct ps5_capture_probe_result result;
  char report[PS5_CAPTURE_PROBE_REPORT_CAPACITY + 1];
  size_t length;
  reset();
  reads[0] = 512;
  read_count = 1;
  clocks[0] = 1000;
  clock_count = 1;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.observation &&
         result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_DATA);
  assert(result.observation_read_calls == 1 &&
         result.observation_empty_calls == 0 &&
         result.observation_overrun_calls == 0 &&
         result.observation_last_read_start_elapsed_us == 0);
  assert(result.read_kind == PS5_CAPTURE_READ_DATA &&
         result.read_bytes == 512 && result.requested_frame_bytes == 0 &&
         result.requested_frames == 0 && !result.whole_requested_frames);
  assert(wait_calls == 0 && now_calls == 1);
  assert_cleanup_once();
  assert(ps5_capture_probe_encode_report(&result, report, sizeof report - 1,
                                         &length) == PS5_CAPTURE_PROBE_OK);
  report[length] = '\0';
  assert(strstr(report, "observation=1 observation_stop=data"));
  assert(strstr(report, "observation_reads=1 observation_empty=0"));
  assert(strstr(report, "observation_last_read_start_elapsed_us=0"));
  assert(strstr(report, "samples_stored=0"));
  assert(strstr(report, "5a5a") == NULL);
}

static void test_empty_overrun_then_data(void) {
  struct ps5_capture_probe_result result;
  reset();
  reads[0] = (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW;
  reads[1] = (int32_t)PS5_CAPTURE_OBSERVE_OVERRUN_RAW;
  reads[2] = 8192;
  read_count = 3;
  clocks[0] = 1000;
  clocks[1] = 101000;
  clocks[2] = 201000;
  clock_count = 3;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_DATA &&
         result.observation_read_calls == 3 &&
         result.observation_empty_calls == 1 &&
         result.observation_overrun_calls == 1 &&
         result.observation_last_read_start_elapsed_us == 200000 &&
         result.discontinuity);
  assert(read_calls == 3 && wait_calls == 2 && now_calls == 3);
  assert_cleanup_once();
}

static void test_call_limit_and_deadline(void) {
  struct ps5_capture_probe_result result;
  reset();
  for (size_t i = 0; i < PS5_CAPTURE_PROBE_OBSERVE_MAX_READS; ++i) {
    reads[i] = (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW;
    clocks[i] = UINT64_C(1000) + i * PS5_CAPTURE_PROBE_OBSERVE_INTERVAL_US;
  }
  read_count = clock_count = PS5_CAPTURE_PROBE_OBSERVE_MAX_READS;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_CALL_LIMIT &&
         result.observation_read_calls == 20 &&
         result.observation_empty_calls == 20 &&
         result.observation_last_read_start_elapsed_us == 1900000);
  assert(wait_calls == 19 && now_calls == 20);
  assert_cleanup_once();

  reset();
  reads[0] = (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW;
  read_count = 1;
  clocks[0] = 1000;
  clocks[1] = 2001000;
  clock_count = 2;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_OK);
  assert(result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_DEADLINE &&
         result.observation_read_calls == 1 &&
         result.observation_last_read_start_elapsed_us == 0);
  assert(read_calls == 1 && wait_calls == 1 && now_calls == 2);
  assert_cleanup_once();
}

static void test_wait_clock_read_and_cleanup_failures(void) {
  struct ps5_capture_probe_result result;

  reset();
  reads[0] = (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW;
  read_count = 1;
  clocks[0] = 1000;
  clock_count = 1;
  wait_fail_at = 1;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_OBSERVE_WAIT &&
         result.raw_wait_status == -71 &&
         result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_CANCELLED);
  assert_cleanup_once();

  reset();
  clocks[0] = 1000;
  clock_count = 1;
  now_fail_at = 1;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_OBSERVE_CLOCK &&
         result.raw_observation_clock_status == -72 && read_calls == 0);
  assert_cleanup_once();

  reset();
  reads[0] = (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW;
  read_count = 1;
  clocks[0] = 1000;
  clocks[1] = 999;
  clock_count = 2;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_OBSERVE_CLOCK &&
         result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_CANCELLED &&
         read_calls == 1 && wait_calls == 1 && now_calls == 2);
  assert_cleanup_once();

  reset();
  reads[0] = -77;
  read_count = 1;
  clocks[0] = 1000;
  clock_count = 1;
  assert(ps5_capture_probe_observe(&ops, &result) == PS5_CAPTURE_PROBE_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_READ &&
         result.observation_stop == PS5_CAPTURE_OBSERVE_STOP_READ_ERROR &&
         result.raw_read_status == -77);
  assert_cleanup_once();

  reset();
  reads[0] = 512;
  read_count = 1;
  clocks[0] = 1000;
  clock_count = 1;
  stop_status = -81;
  close_status = -82;
  terminate_status = -83;
  assert(ps5_capture_probe_observe(&ops, &result) ==
         PS5_CAPTURE_PROBE_CLEANUP_FAILED);
  assert(result.primary_phase == PS5_CAPTURE_PHASE_STOP &&
         result.cleanup_uncertain && result.raw_stop_status == -81 &&
         result.raw_close_status == -82 && result.raw_terminate_status == -83);
  assert_cleanup_once();
}

int main(void) {
  test_immediate_data_and_report();
  test_empty_overrun_then_data();
  test_call_limit_and_deadline();
  test_wait_clock_read_and_cleanup_failures();
  puts("PASS bounded capture observation runner and exactly-once cleanup");
  return 0;
}
