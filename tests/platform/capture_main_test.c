/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static unsigned stage, sleeps, traces;
static int64_t lookup_status;
static int null_symbol, wait_status, encode_failure;
static enum ps5_capture_probe_status probe_status;

static int fake_sleep(uint32_t microseconds) {
  assert(microseconds == 100000);
  ++sleeps;
  return wait_status;
}

int64_t ps5_bootstrap_lookup(const char *name, void **out) {
  assert(!strcmp(name, "usleep") && out && !*out);
  int (*function)(uint32_t) = fake_sleep;
  _Static_assert(sizeof function == sizeof *out, "host callable pointer ABI");
  if (!null_symbol)
    memcpy(out, &function, sizeof function);
  return lookup_status;
}

int64_t ps5_bootstrap_module_lookup(int32_t module, const char *name,
                                    void **out) {
  (void)module;
  (void)name;
  (void)out;
  assert(!"main fixture must not perform module lookup");
  return -1;
}

int32_t ps5_rust_probe(void) {
  assert(stage == 0);
  stage = 1;
  return 2;
}

void ps5_platform_log(const uint8_t *data, uint32_t length) {
  assert(data && length && stage >= 1);
  if (stage == 1) {
    assert(length == sizeof("event=capture_probe_begin\n") - 1);
    stage = 2;
  } else {
    assert(stage == 3);
    if (length > 19 && memcmp(data, "event=capture_phase", 19) == 0)
      ++traces;
  }
}

static enum ps5_capture_probe_status
fake_probe(const struct ps5_capture_probe_ops *ops,
           struct ps5_capture_probe_result *out) {
  assert(stage == 2 && ops && out && ops->trace && ops->wait_before_read);
  assert(ops->lookup_current == ps5_bootstrap_lookup);
  assert(ops->lookup_module == ps5_bootstrap_module_lookup);
  stage = 3;
  ops->trace(PS5_CAPTURE_PHASE_READ, 0, 1, 0);
  ops->trace(PS5_CAPTURE_PHASE_READ, 0, 0, -1);
  *out = (struct ps5_capture_probe_result){.status = probe_status};
  return probe_status;
}

enum ps5_capture_probe_status
ps5_capture_probe_preflight(const struct ps5_capture_probe_ops *ops,
                            struct ps5_capture_probe_result *out) {
#ifndef PS5_BT_CAPTURE_PREFLIGHT
  assert(!"full mode unexpectedly selected preflight");
#endif
  return fake_probe(ops, out);
}

enum ps5_capture_probe_status
ps5_capture_probe_run(const struct ps5_capture_probe_ops *ops,
                      struct ps5_capture_probe_result *out) {
#ifdef PS5_BT_CAPTURE_PREFLIGHT
  assert(!"preflight unexpectedly selected full mode");
#endif
  return fake_probe(ops, out);
}

enum ps5_capture_probe_status
ps5_capture_probe_encode_report(const struct ps5_capture_probe_result *result,
                                char *data, size_t capacity, size_t *length) {
  assert(stage == 3 && result->status == probe_status && capacity >= 3);
  if (encode_failure)
    return PS5_CAPTURE_PROBE_CAPACITY;
  memcpy(data, "ok\n", 3);
  *length = 3;
  return PS5_CAPTURE_PROBE_OK;
}

int main(void) {
  assert(capture_wait_before_read(0) == PS5_BT_INVALID && sleeps == 0);
  assert(capture_wait_before_read(100000) == 0 && sleeps == 1);
  wait_status = -4;
  assert(capture_wait_before_read(100000) == -4 && sleeps == 2);
  lookup_status = -73; /* A dirty failure symbol must not be called. */
  assert(capture_wait_before_read(100000) == PS5_BT_IO && sleeps == 2);
  lookup_status = 0;
  null_symbol = 1;
  assert(capture_wait_before_read(100000) == PS5_BT_IO && sleeps == 2);
  assert(diagnostic_main() == 2 && stage == 3 && traces == 2);
  stage = traces = 0;
  probe_status = PS5_CAPTURE_PROBE_FAILED;
  assert(diagnostic_main() == 1 && traces == 2);
  stage = 0;
  probe_status = PS5_CAPTURE_PROBE_OK;
  encode_failure = 1;
  assert(diagnostic_main() == 1);
  puts("PASS synthetic capture main variant, trace, wait and failure paths");
  return 0;
}
