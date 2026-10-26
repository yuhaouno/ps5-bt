/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "rate.h"
#include <assert.h>
#include <stdio.h>

int main(void) {
  hb_rate rate;
  hb_rate_init(&rate, 2, 53, 41, 0);
  assert(hb_rate_update(&rate, 0, 6, 8, 0) == 41);
  assert(hb_rate_update(&rate, 39, 7, 8, 0) == 41);
  assert(hb_rate_update(&rate, 40, 7, 8, 0) == 37);
  assert(rate.pressure.trigger_count == 1 && rate.cap == 37);
  /* Drain succeeds: do not immediately re-probe the known pressure rate. */
  assert(hb_rate_update(&rate, 100, 0, 8, 0) == 37);
  assert(hb_rate_update(&rate, 5000, 0, 8, 0) == 37);
  assert(hb_rate_update(&rate, 31000, 0, 8, 0) == 39);

  hb_rate_init(&rate, 2, 53, 41, 0);
  (void)hb_rate_update(&rate, 0, 6, 8, 0);
  /* A real drop takes precedence, without an extra policy reduction. */
  assert(hb_rate_update(&rate, 40, 8, 8, 1) == 37);
  assert(rate.pressure.trigger_count == 0);
  assert(hb_rate_update(&rate, 100, 8, 8, 1) == 37);
  assert(hb_rate_update(&rate, 340, 7, 8, 1) == 33);
  assert(rate.pressure.trigger_count == 1);
  puts("rate adapter: early response, recovery ceiling and drop precedence passed");
  return 0;
}
