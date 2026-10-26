/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Same pinned clock ABI as capture-observe; availability requires hardware. */
#include "../bootstrap/bootstrap.h"
#include "ps5_bt.h"
#include <stddef.h>

struct target_timespec {
  int64_t seconds, nanoseconds;
};
int32_t ps5_continuous_now_us(uint64_t *out) {
  /* Pinned SDK time.h declares int clock_gettime(int32 clockid, timespec*),
   * CLOCK_MONOTONIC=4; x86/_types.h and sys/_timespec.h use two signed 64-bit
   * fields. Runtime export availability and clock behavior require hardware. */
  typedef int (*clock_fn)(int32_t, struct target_timespec *);
  _Static_assert(sizeof(struct target_timespec) == 16, "target timespec ABI");
  _Static_assert(sizeof(clock_fn) == sizeof(void *), "target function ABI");
  if (!out)
    return PS5_BT_INVALID;
  *out = 0;
  void *symbol = 0;
  if (ps5_bootstrap_lookup("clock_gettime", &symbol) != 0 || !symbol)
    return PS5_BT_IO;
  clock_fn clock = 0;
  for (size_t index = 0; index < sizeof clock; ++index)
    ((unsigned char *)&clock)[index] = ((const unsigned char *)&symbol)[index];
  struct target_timespec time = {0};
  int raw = clock(4, &time);
  if (raw != 0)
    return raw;
  if (time.seconds < 0 || time.nanoseconds < 0 ||
      time.nanoseconds >= 1000000000)
    return PS5_BT_INVALID;
  uint64_t subseconds = (uint64_t)time.nanoseconds / 1000;
  if ((uint64_t)time.seconds > (UINT64_MAX - subseconds) / 1000000)
    return PS5_BT_INVALID;
  *out = (uint64_t)time.seconds * 1000000 + subseconds;
  return 0;
}
