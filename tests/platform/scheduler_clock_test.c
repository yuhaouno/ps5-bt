/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "scheduler_clock_fixture.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
static _Alignas(16) unsigned char storage[128];
static void init(void) {
  struct ps5_clock_wait_report r;
  memset(storage, 0, sizeof storage);
  assert(ps5_clock_wait_init(storage, sizeof storage, ps5_scheduler_test_lookup,
                             &r) == 0);
  assert(r.abi_version == PS5_CLOCK_WAIT_ABI && r.stage == 2);
}
int main(void) {
  struct ps5_clock_wait_report r;
  uint64_t now = 99;
  assert(ps5_clock_wait_abi_version() == PS5_CLOCK_WAIT_ABI);
  assert(ps5_clock_wait_storage_size() <= sizeof storage &&
         ps5_clock_wait_storage_alignment() <= 16);
  ps5_scheduler_test_reset();
  assert(ps5_clock_wait_init(storage + 1, sizeof storage - 1,
                             ps5_scheduler_test_lookup, &r) == -2);
  assert(ps5_clock_wait_init(storage, 1, ps5_scheduler_test_lookup, &r) == -2);
  assert(ps5_clock_wait_init(storage, sizeof storage, 0, &r) == -2);
  assert(ps5_clock_wait_init(storage, sizeof storage, ps5_scheduler_test_lookup,
                             (void *)storage) == -2);
  assert(ps5_scheduler_test_count(0) == 0);
  for (uint32_t which = 1; which <= 2; ++which) {
    for (uint32_t null = 0; null <= 1; ++null) {
      ps5_scheduler_test_reset();
      memset(storage, 0, sizeof storage);
      ps5_scheduler_test_lookup_failure(which, INT64_C(-0x100000123), null);
      assert(ps5_clock_wait_init(storage, sizeof storage,
                                 ps5_scheduler_test_lookup, &r) == -1);
      assert(r.raw_lookup == INT64_C(-0x100000123));
      assert(ps5_scheduler_test_count(0) == 1 &&
             ps5_scheduler_test_count(1) == (which == 2));
      assert(ps5_scheduler_test_count(2) == 0 &&
             ps5_scheduler_test_count(3) == 0);
      ps5_scheduler_test_lookup_failure(which, 0, 1);
      assert(ps5_clock_wait_init(storage, sizeof storage,
                                 ps5_scheduler_test_lookup, &r) == -1);
    }
  }
  ps5_scheduler_test_reset();
  init();
  assert(ps5_clock_wait_init(storage, sizeof storage, ps5_scheduler_test_lookup,
                             &r) == -3);
  ps5_scheduler_test_reenter(storage);
  ps5_scheduler_test_clock(0, 10, 123456789);
  assert(ps5_clock_wait_now(storage, &now, &r) == 0 && now == 10123456);
  assert(r.raw_seconds == 10 && r.raw_nanoseconds == 123456789);
  ps5_scheduler_test_clock(
      0, 10, 123456788); /* raw regression within same microsecond */
  assert(ps5_clock_wait_now(storage, &now, &r) == -8 && now == 0);
  ps5_scheduler_test_clock(-77, 999, 999);
  assert(ps5_clock_wait_now(storage, &now, &r) == -5 && now == 0 &&
         r.raw_status == -77);
  const int64_t invalid[][2] = {{-1, 0}, {0, -1}, {0, 1000000000}};
  for (size_t i = 0; i < sizeof invalid / sizeof invalid[0]; ++i) {
    ps5_scheduler_test_clock(0, invalid[i][0], invalid[i][1]);
    assert(ps5_clock_wait_now(storage, &now, &r) == -6 && now == 0);
  }
  ps5_scheduler_test_clock(0, INT64_MAX, 0);
  assert(ps5_clock_wait_now(storage, &now, &r) == -7 && now == 0);
  ps5_scheduler_test_clock(0, (int64_t)(UINT64_MAX / 1000000),
                           (int64_t)(UINT64_MAX % 1000000) * 1000);
  assert(ps5_clock_wait_now(storage, &now, &r) == 0 && now == UINT64_MAX);
  ps5_scheduler_test_clock(0, (int64_t)(UINT64_MAX / 1000000),
                           ((int64_t)(UINT64_MAX % 1000000) + 1) * 1000);
  assert(ps5_clock_wait_now(storage, &now, &r) == -7 && now == 0);
  assert(ps5_clock_wait_slice(storage, 0, &r) == -2);
  assert(ps5_clock_wait_slice(storage, 1001, &r) == -2);
  assert(ps5_scheduler_test_count(3) == 0);
  assert(ps5_clock_wait_slice(storage, 1, &r) == 0 && r.requested_us == 1);
  assert(ps5_clock_wait_slice(storage, 1000, &r) == 0 &&
         r.requested_us == 1000);
  ps5_scheduler_test_wait_status(-88);
  assert(ps5_clock_wait_slice(storage, 1000, &r) == -5 && r.raw_status == -88);
  assert(ps5_scheduler_test_count(3) == 3); /* no retries */
  assert(ps5_scheduler_test_count(0) == 1 &&
         ps5_scheduler_test_count(1) == 1); /* cached */
  assert(ps5_clock_wait_now(storage, (void *)storage, &r) == -2);
  assert(ps5_clock_wait_now(storage, (void *)&r, &r) == -2);
  assert(ps5_clock_wait_close(storage, &r) == 0 && r.stage == 5);
  assert(ps5_clock_wait_close(storage, &r) == -2);
  assert(ps5_clock_wait_now(storage, &now, &r) == -2 && now == 0);
  puts("PASS reviewed clock/wait ABI cache, native errors, bounds and "
       "monotonic conversion");
}
