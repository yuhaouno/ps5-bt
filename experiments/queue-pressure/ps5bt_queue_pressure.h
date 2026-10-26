/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5BT_QUEUE_PRESSURE_H
#define PS5BT_QUEUE_PRESSURE_H

#include <stdint.h>

#define PS5BT_QUEUE_PRESSURE_ABI_VERSION 1u
#define PS5BT_QUEUE_PRESSURE_PERSIST_MS 40
#define PS5BT_QUEUE_PRESSURE_COOLDOWN_MS 300
#define PS5BT_QUEUE_PRESSURE_STEP 4

/* Single-owner state. No queue, clock, credit or bitpool ownership is assumed.
 */
typedef struct {
  uint32_t abi_version;
  uint32_t trigger_count; /* Saturating count of emitted reductions. */
  int64_t last_ms, pressure_since_ms, last_down_ms;
  unsigned capacity, slack;
  int clock_known, pressure_active, down_known, geometry_known;
} ps5bt_queue_pressure;

typedef struct {
  int suppress_up;
  int qualified;
  int reduce_by; /* Apply once, update upstream decrease/calm/cap state. */
  int clock_reset;
  int invalid_input;
} ps5bt_queue_pressure_result;

/* Initialize once per stream; now_ms is monotonic milliseconds, nonnegative.
 * Negative/rollback time fails closed: suppress rises, discard persistence,
 * and require a fresh 300 ms cooldown after a usable clock is established. */
void ps5bt_queue_pressure_init(ps5bt_queue_pressure *state, int64_t now_ms);

/* Call at queue observations. Persistent means consecutive observed samples;
 * this does not prove what happened between samples. Capacity/slack changes
 * restart persistence. Pressure > slack suppresses rises immediately. At
 * >= ceil(3*capacity/4), 40 ms pressure can request -4, clamped to floor.
 * down_eligible must reflect the upstream controller's current 300 ms guard.
 * The caller must apply an emitted reduction before calling again; existing
 * drop/recovery actions take precedence, with down_eligible false in that call.
 * Invalid input suppresses rises and resets persistence without requesting a
 * reduction. This experimental policy changes neither queues nor credits. */
ps5bt_queue_pressure_result
ps5bt_queue_pressure_step(ps5bt_queue_pressure *state, int64_t now_ms,
                          unsigned queue, unsigned capacity, unsigned slack,
                          int bitpool, int floor, int down_eligible);

#endif
