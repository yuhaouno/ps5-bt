/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define PS5_BT_SBC_CONTINUOUS
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main
#include "../../platform/ps5/diagnostics/continuous_clock.c"

static unsigned phase;
static int codec_status, clock_status, lookup_status, null_symbol;
static int64_t seconds, nanoseconds;
static int fake_clock(int32_t id, struct target_timespec *out) {
  assert(id == 4 && out);
  *out = (struct target_timespec){seconds, nanoseconds};
  return clock_status;
}
int64_t ps5_bootstrap_lookup(const char *name, void **out) {
  assert(strcmp(name, "clock_gettime") == 0);
  int (*clock)(int32_t, struct target_timespec *) = fake_clock;
  *out = 0;
  if (!null_symbol)
    memcpy(out, &clock, sizeof clock);
  return lookup_status;
}
int32_t ps5_rust_probe(void) {
  assert(phase++ == 0);
  return 2;
}
int32_t ps5_rust_sbc_continuous(int32_t (*now)(uint64_t *)) {
  assert(phase++ == 1 && now == ps5_continuous_now_us);
  return codec_status;
}
int main(void) {
  assert(diagnostic_main() == 2 && phase == 2);
  phase = 0;
  codec_status = -1;
  assert(diagnostic_main() == 1 && phase == 2);
  uint64_t out = 99;
  assert(ps5_continuous_now_us(0) == PS5_BT_INVALID);
  seconds = 7;
  nanoseconds = 123456789;
  assert(ps5_continuous_now_us(&out) == 0 && out == 7123456);
  lookup_status = -9;
  assert(ps5_continuous_now_us(&out) == PS5_BT_IO && out == 0);
  lookup_status = 0;
  null_symbol = 1;
  assert(ps5_continuous_now_us(&out) == PS5_BT_IO && out == 0);
  null_symbol = 0;
  clock_status = 42;
  assert(ps5_continuous_now_us(&out) == 42 && out == 0);
  clock_status = 0;
  seconds = -1;
  assert(ps5_continuous_now_us(&out) == PS5_BT_INVALID && out == 0);
  seconds = 0;
  nanoseconds = -1;
  assert(ps5_continuous_now_us(&out) == PS5_BT_INVALID && out == 0);
  nanoseconds = 1000000000;
  assert(ps5_continuous_now_us(&out) == PS5_BT_INVALID && out == 0);
  seconds = INT64_MAX;
  nanoseconds = 0;
  assert(ps5_continuous_now_us(&out) == PS5_BT_INVALID && out == 0);
  puts("PASS continuous SBC main ordering and pinned clock boundary failures");
  return 0;
}
