/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "scheduler_clock_fixture.h"
#include <assert.h>
#include <string.h>
static uint32_t counts[4], fail_lookup, null_symbol, requested;
static int64_t lookup_raw, seconds, nanoseconds;
static int32_t clock_raw, wait_raw;
static void *reenter_storage;
struct target_timespec {
  int64_t seconds, nanoseconds;
};
void ps5_scheduler_test_reenter(void *storage) { reenter_storage = storage; }
void ps5_scheduler_test_reset(void) {
  reenter_storage = 0;
  memset(counts, 0, sizeof counts);
  fail_lookup = 0;
  null_symbol = 0;
  requested = 0;
  lookup_raw = 0;
  seconds = 0;
  nanoseconds = 0;
  clock_raw = 0;
  wait_raw = 0;
}
void ps5_scheduler_test_lookup_failure(uint32_t which, int64_t raw,
                                       uint32_t null) {
  fail_lookup = which;
  lookup_raw = raw;
  null_symbol = null;
}
void ps5_scheduler_test_clock(int32_t raw, int64_t sec, int64_t ns) {
  clock_raw = raw;
  seconds = sec;
  nanoseconds = ns;
}
void ps5_scheduler_test_wait_status(int32_t raw) { wait_raw = raw; }
uint32_t ps5_scheduler_test_count(uint32_t which) {
  assert(which < 4);
  return counts[which];
}
uint32_t ps5_scheduler_test_requested(void) { return requested; }
static int clock_call(int32_t clock_id, struct target_timespec *out) {
  assert(clock_id == 4);
  assert(out->seconds == 0 && out->nanoseconds == 0);
  ++counts[2];
  if (reenter_storage) {
    uint64_t nested_now = 7;
    struct ps5_clock_wait_report r;
    assert(ps5_clock_wait_now(reenter_storage, &nested_now, &r) == -3 &&
           nested_now == 0);
    assert(ps5_clock_wait_init(reenter_storage, 128, ps5_scheduler_test_lookup,
                               &r) == -3);
    reenter_storage = 0;
  }
  out->seconds = seconds;
  out->nanoseconds = nanoseconds;
  return clock_raw;
}
static int wait_call(uint32_t us) {
  assert(us > 0 && us <= 1000);
  ++counts[3];
  requested = us;
  return wait_raw;
}
int64_t ps5_scheduler_test_lookup(const char *name, void **out) {
  assert(out && *out == 0);
  uint32_t which = strcmp(name, "clock_gettime") == 0 ? 1 : 2;
  if (which == 2)
    assert(strcmp(name, "usleep") == 0);
  ++counts[which - 1];
  if (which == fail_lookup && null_symbol) {
    *out = 0;
    return lookup_raw;
  }
  if (which == 1) {
    int (*fn)(int32_t, struct target_timespec *) = clock_call;
    memcpy(out, &fn, sizeof fn);
  } else {
    int (*fn)(uint32_t) = wait_call;
    memcpy(out, &fn, sizeof fn);
  }
  return which == fail_lookup ? lookup_raw : 0;
}
