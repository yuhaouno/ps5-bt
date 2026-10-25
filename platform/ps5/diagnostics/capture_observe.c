/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "capture_observe.h"

static void clear_bytes(void *storage, size_t size) {
  unsigned char *bytes = storage;
  for (size_t i = 0; i < size; ++i)
    bytes[i] = 0;
}

enum ps5_capture_observe_status
ps5_capture_observe_begin(struct ps5_capture_observe *state,
                          uint64_t start_tick, uint64_t deadline_tick,
                          uint32_t max_calls) {
  if (!state || deadline_tick <= start_tick || max_calls == 0 ||
      max_calls > PS5_CAPTURE_OBSERVE_MAX_CALLS)
    return PS5_CAPTURE_OBSERVE_INVALID;
  if (state->cleanup_available && !state->cleanup_taken)
    return PS5_CAPTURE_OBSERVE_INVALID;

  struct ps5_capture_observe next;
  clear_bytes(&next, sizeof next);
  next.deadline_tick = deadline_tick;
  next.last_tick = start_tick;
  next.max_calls = max_calls;
  next.result.stop = PS5_CAPTURE_OBSERVE_RUNNING;
  next.active = 1;
  next.cleanup_available = 1;
  *state = next;
  return PS5_CAPTURE_OBSERVE_OK;
}

static void stop(struct ps5_capture_observe *state,
                 enum ps5_capture_observe_stop reason) {
  state->result.stop = reason;
  state->active = 0;
}

enum ps5_capture_observe_status
ps5_capture_observe_step(struct ps5_capture_observe *state, uint64_t now_tick,
                         int cancelled,
                         ps5_capture_observe_read_fn read_once, void *context,
                         void *buffer, size_t capacity) {
  if (!state || !state->cleanup_available)
    return PS5_CAPTURE_OBSERVE_INVALID;
  if (!state->active)
    return PS5_CAPTURE_OBSERVE_OK;
  if (now_tick < state->last_tick)
    return PS5_CAPTURE_OBSERVE_INVALID;

  struct ps5_capture_observe next = *state;
  next.last_tick = now_tick;
  if (cancelled) {
    stop(&next, PS5_CAPTURE_OBSERVE_STOP_CANCELLED);
    *state = next;
    return PS5_CAPTURE_OBSERVE_OK;
  }
  if (now_tick >= next.deadline_tick) {
    stop(&next, PS5_CAPTURE_OBSERVE_STOP_DEADLINE);
    *state = next;
    return PS5_CAPTURE_OBSERVE_OK;
  }
  if (next.result.read_calls >= next.max_calls) {
    stop(&next, PS5_CAPTURE_OBSERVE_STOP_CALL_LIMIT);
    *state = next;
    return PS5_CAPTURE_OBSERVE_OK;
  }
  if (!read_once || !buffer || capacity == 0 ||
      capacity > PS5_CAPTURE_OBSERVE_MAX_READ_BYTES)
    return PS5_CAPTURE_OBSERVE_INVALID;

  int32_t raw = read_once(context, buffer, capacity);
  next.result.raw_last_status = raw;
  ++next.result.read_calls;

  if (raw > 0) {
    if ((uint32_t)raw > capacity) {
      next.result.last_kind = PS5_CAPTURE_OBSERVE_OTHER_ERROR;
      stop(&next, PS5_CAPTURE_OBSERVE_STOP_INVALID_LENGTH);
    } else {
      next.result.last_kind = PS5_CAPTURE_OBSERVE_DATA;
      next.result.data_bytes = (uint32_t)raw;
      stop(&next, PS5_CAPTURE_OBSERVE_STOP_DATA);
    }
  } else if ((uint32_t)raw == PS5_CAPTURE_OBSERVE_EMPTY_RAW) {
    next.result.last_kind = PS5_CAPTURE_OBSERVE_EMPTY;
    ++next.result.empty_calls;
  } else if ((uint32_t)raw == PS5_CAPTURE_OBSERVE_OVERRUN_RAW) {
    next.result.last_kind = PS5_CAPTURE_OBSERVE_OVERRUN;
    ++next.result.overrun_calls;
    next.result.discontinuity = 1;
  } else {
    next.result.last_kind = PS5_CAPTURE_OBSERVE_OTHER_ERROR;
    stop(&next, PS5_CAPTURE_OBSERVE_STOP_READ_ERROR);
  }

  if (next.active && next.result.read_calls == next.max_calls)
    stop(&next, PS5_CAPTURE_OBSERVE_STOP_CALL_LIMIT);
  *state = next;
  return PS5_CAPTURE_OBSERVE_OK;
}

enum ps5_capture_observe_status
ps5_capture_observe_take_cleanup(struct ps5_capture_observe *state) {
  if (!state || !state->cleanup_available)
    return PS5_CAPTURE_OBSERVE_INVALID;
  if (state->active)
    return PS5_CAPTURE_OBSERVE_NOT_DONE;
  if (state->cleanup_taken)
    return PS5_CAPTURE_OBSERVE_CLEANUP_TAKEN;
  state->cleanup_taken = 1;
  return PS5_CAPTURE_OBSERVE_OK;
}
