/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5bt_queue_pressure.h"

#include <limits.h>
#include <string.h>

void ps5bt_queue_pressure_init(ps5bt_queue_pressure *state, int64_t now_ms) {
  if (!state)
    return;
  memset(state, 0, sizeof *state);
  state->abi_version = PS5BT_QUEUE_PRESSURE_ABI_VERSION;
  state->clock_known = now_ms >= 0;
  state->last_ms = now_ms >= 0 ? now_ms : 0;
}

ps5bt_queue_pressure_result
ps5bt_queue_pressure_step(ps5bt_queue_pressure *state, int64_t now_ms,
                          unsigned queue, unsigned capacity, unsigned slack,
                          int bitpool, int floor, int down_eligible) {
  ps5bt_queue_pressure_result result = {0};
  if (!state || state->abi_version != PS5BT_QUEUE_PRESSURE_ABI_VERSION) {
    result.suppress_up = result.invalid_input = 1;
    return result;
  }

  if (now_ms < 0) {
    state->clock_known = state->pressure_active = 0;
    result.suppress_up = result.clock_reset = 1;
    return result;
  }
  if (!state->clock_known || now_ms < state->last_ms) {
    state->clock_known = 1;
    state->pressure_active = 0;
    state->last_ms = now_ms;
    state->last_down_ms = now_ms;
    state->down_known = 1;
    result.suppress_up = result.clock_reset = 1;
    return result;
  }
  state->last_ms = now_ms;

  if (capacity == 0 || queue > capacity || slack > capacity || floor < 0 ||
      bitpool < floor) {
    state->pressure_active = 0;
    result.suppress_up = result.invalid_input = 1;
    return result;
  }
  if (!state->geometry_known || capacity != state->capacity ||
      slack != state->slack) {
    state->geometry_known = 1;
    state->capacity = capacity;
    state->slack = slack;
    state->pressure_active = 0;
  }

  result.suppress_up = queue > slack;
  /* capacity - capacity/4 == ceil(3*capacity/4), without overflow. */
  if (!result.suppress_up || queue < capacity - capacity / 4) {
    state->pressure_active = 0;
    return result;
  }
  if (!state->pressure_active) {
    state->pressure_active = 1;
    state->pressure_since_ms = now_ms;
  }
  result.qualified =
      now_ms - state->pressure_since_ms >= PS5BT_QUEUE_PRESSURE_PERSIST_MS;
  if (!result.qualified || !down_eligible || bitpool == floor ||
      (state->down_known &&
       now_ms - state->last_down_ms < PS5BT_QUEUE_PRESSURE_COOLDOWN_MS))
    return result;

  result.reduce_by = bitpool - floor < PS5BT_QUEUE_PRESSURE_STEP
                         ? bitpool - floor
                         : PS5BT_QUEUE_PRESSURE_STEP;
  state->last_down_ms = now_ms;
  state->down_known = 1;
  if (state->trigger_count != UINT32_MAX)
    ++state->trigger_count;
  return result;
}
