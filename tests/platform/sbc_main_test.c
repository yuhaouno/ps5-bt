/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static unsigned phase;
static int fail_codec, fail_report;

int32_t ps5_rust_probe(void) {
  assert(phase == 0);
  phase = 1;
  return 2;
}

void ps5_platform_log(const uint8_t *data, uint32_t length) {
  assert(data && length && phase >= 1);
  ++phase;
}

int32_t ps5_bt_sbc_selftest_run(void *storage, uint32_t capacity,
                                struct ps5_bt_sbc_selftest_report *report) {
  assert(phase == 3 && ((uintptr_t)storage & 63u) == 0 && capacity == 8192);
  *report = (struct ps5_bt_sbc_selftest_report){.schema = 1};
  ++phase;
  return fail_codec ? PS5_BT_SBC_SELFTEST_INIT_FAILED : 0;
}

int32_t ps5_bt_sbc_selftest_encode_report(
    const struct ps5_bt_sbc_selftest_report *report, char *data,
    size_t capacity, size_t *out_length) {
  assert(phase == 4 && report->schema == 1 && capacity >= 3);
  if (fail_report)
    return -2;
  memcpy(data, "ok\n", 3);
  *out_length = 3;
  return 0;
}

int main(void) {
  assert(diagnostic_main() == 2 && phase == 5);
  phase = 0;
  fail_codec = 1;
  assert(diagnostic_main() == 1 && phase == 5);
  phase = 0;
  fail_codec = 0;
  fail_report = 1;
  assert(diagnostic_main() == 1 && phase == 5);
  puts("PASS SBC self-test main ordering, alignment and failure propagation");
  return 0;
}
