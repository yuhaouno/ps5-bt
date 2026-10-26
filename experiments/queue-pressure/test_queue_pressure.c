/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5bt_queue_pressure.h"

#include <assert.h>
#include <limits.h>
#include <stdio.h>

static ps5bt_queue_pressure_result sample(ps5bt_queue_pressure *state,
                                          int64_t now, unsigned queue) {
  return ps5bt_queue_pressure_step(state, now, queue, 8, 4, 41, 22, 1);
}

static void transient_and_sustained(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 0);
  assert(!sample(&state, 0, 4).suppress_up);
  out = sample(&state, 10, 5);
  assert(out.suppress_up && !out.qualified && !out.reduce_by);
  assert(!sample(&state, 20, 6).reduce_by);
  assert(!sample(&state, 50, 7).reduce_by); /* 30 ms burst ignored. */
  assert(!sample(&state, 51, 5).reduce_by); /* Break persistence. */
  assert(!sample(&state, 60, 6).reduce_by);
  assert(!sample(&state, 99, 8).reduce_by);
  out = sample(&state, 100, 6);
  assert(out.qualified && out.reduce_by == 4 && state.trigger_count == 1);
}

static void cooldown_and_eligibility(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 0);
  (void)sample(&state, 0, 6);
  out = ps5bt_queue_pressure_step(&state, 40, 8, 8, 4, 41, 22, 0);
  assert(out.qualified && !out.reduce_by && state.trigger_count == 0);
  assert(sample(&state, 41, 8).reduce_by == 4);
  assert(!sample(&state, 42, 0).reduce_by);
  assert(!sample(&state, 50, 8).reduce_by);
  assert(!sample(&state, 340, 8).reduce_by);
  assert(sample(&state, 341, 8).reduce_by == 4);
  assert(state.trigger_count == 2);
}

static void floor_and_counter(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 0);
  (void)sample(&state, 0, 8);
  out = ps5bt_queue_pressure_step(&state, 40, 8, 8, 4, 24, 22, 1);
  assert(out.reduce_by == 2);
  out = ps5bt_queue_pressure_step(&state, 340, 8, 8, 4, 22, 22, 1);
  assert(out.qualified && !out.reduce_by && state.trigger_count == 1);
  state.trigger_count = UINT32_MAX;
  assert(sample(&state, 341, 8).reduce_by == 4);
  assert(state.trigger_count == UINT32_MAX);
}

static void geometry_and_boundaries(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 0);
  (void)sample(&state, 0, 6);
  out = ps5bt_queue_pressure_step(&state, 40, 6, 7, 4, 41, 22, 1);
  assert(!out.qualified && !out.reduce_by); /* Capacity change resets. */
  out = ps5bt_queue_pressure_step(&state, 80, 6, 7, 5, 41, 22, 1);
  assert(!out.qualified && !out.reduce_by); /* Slack change resets. */
  out = ps5bt_queue_pressure_step(&state, 120, 5, 7, 5, 41, 22, 1);
  assert(!out.suppress_up && !out.reduce_by);
  out = ps5bt_queue_pressure_step(&state, 160, 3, 5, 0, 41, 22, 1);
  assert(out.suppress_up && !out.qualified); /* ceil(75% of 5) = 4. */
  (void)ps5bt_queue_pressure_step(&state, 200, 4, 5, 0, 41, 22, 1);
  assert(ps5bt_queue_pressure_step(&state, 240, 4, 5, 0, 41, 22, 1).reduce_by ==
         4);
  ps5bt_queue_pressure_init(&state, 0);
  (void)ps5bt_queue_pressure_step(&state, 0, UINT_MAX, UINT_MAX, UINT_MAX - 1,
                                  41, 22, 1);
  assert(ps5bt_queue_pressure_step(&state, 40, UINT_MAX, UINT_MAX, UINT_MAX - 1,
                                   41, 22, 1)
             .reduce_by == 4); /* Threshold arithmetic cannot overflow. */
}

static void rollback_and_clock_failure(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 100);
  (void)sample(&state, 100, 8);
  assert(sample(&state, 140, 8).reduce_by == 4);
  out = sample(&state, 130, 8);
  assert(out.clock_reset && out.suppress_up && !out.reduce_by);
  (void)sample(&state, 131, 8);
  assert(!sample(&state, 429, 8).reduce_by);
  assert(sample(&state, 430, 8).reduce_by == 4);
  out = sample(&state, -1, 0);
  assert(out.clock_reset && out.suppress_up && !out.reduce_by);
  assert(sample(&state, 0, 8).clock_reset);
  (void)sample(&state, 1, 8);
  assert(!sample(&state, 299, 8).reduce_by);
  assert(sample(&state, 300, 8).reduce_by == 4);
  ps5bt_queue_pressure_init(&state, -1);
  assert(sample(&state, INT64_MAX - 300, 8).clock_reset);
  (void)sample(&state, INT64_MAX - 299, 8);
  assert(sample(&state, INT64_MAX, 8).reduce_by == 4);
}

static void invalid_input_and_reset(void) {
  ps5bt_queue_pressure state;
  ps5bt_queue_pressure_result out;
  ps5bt_queue_pressure_init(&state, 0);
  (void)sample(&state, 0, 8);
  out = ps5bt_queue_pressure_step(&state, 40, 9, 8, 4, 41, 22, 1);
  assert(out.invalid_input && out.suppress_up && !out.reduce_by);
  assert(!sample(&state, 41, 8).qualified);
  assert(sample(&state, 81, 8).reduce_by == 4);
  out = ps5bt_queue_pressure_step(&state, 82, 0, 0, 0, 41, 22, 1);
  assert(out.invalid_input && out.suppress_up);
  assert(
      ps5bt_queue_pressure_step(&state, 83, 0, 8, 9, 41, 22, 1).invalid_input);
  assert(
      ps5bt_queue_pressure_step(&state, 84, 0, 8, 4, 21, 22, 1).invalid_input);
  assert(
      ps5bt_queue_pressure_step(&state, 85, 0, 8, 4, 41, -1, 1).invalid_input);
  assert(sample(NULL, 86, 0).invalid_input);
  ps5bt_queue_pressure_init(&state, 100);
  assert(state.trigger_count == 0 && !state.pressure_active);
  state.abi_version = 0;
  assert(sample(&state, 100, 8).invalid_input);
}

int main(void) {
  transient_and_sustained();
  cooldown_and_eligibility();
  floor_and_counter();
  geometry_and_boundaries();
  rollback_and_clock_failure();
  invalid_input_and_reset();
  puts("queue-pressure: 6 focused checks passed");
  return 0;
}
