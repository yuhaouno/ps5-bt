/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_CAPTURE_OBSERVE_H
#define PS5_CAPTURE_OBSERVE_H

#include <stddef.h>
#include <stdint.h>

/* This helper is entered only after the caller owns a started capture handle.
 * It makes at most one nonblocking read per step. Time and waiting remain with
 * the caller, so there is no hidden polling or sleep loop.
 */
#define PS5_CAPTURE_OBSERVE_MAX_CALLS 256u
#define PS5_CAPTURE_OBSERVE_MAX_READ_BYTES 65536u

#define PS5_CAPTURE_OBSERVE_EMPTY_RAW UINT32_C(0x81950002)
#define PS5_CAPTURE_OBSERVE_OVERRUN_RAW UINT32_C(0x81950004)

enum ps5_capture_observe_status {
  PS5_CAPTURE_OBSERVE_OK = 0,
  PS5_CAPTURE_OBSERVE_INVALID = -1,
  PS5_CAPTURE_OBSERVE_NOT_DONE = -2,
  PS5_CAPTURE_OBSERVE_CLEANUP_TAKEN = -3
};

enum ps5_capture_observe_kind {
  PS5_CAPTURE_OBSERVE_NOT_READ = 0,
  PS5_CAPTURE_OBSERVE_EMPTY,
  PS5_CAPTURE_OBSERVE_DATA,
  PS5_CAPTURE_OBSERVE_OVERRUN,
  PS5_CAPTURE_OBSERVE_OTHER_ERROR
};

enum ps5_capture_observe_stop {
  PS5_CAPTURE_OBSERVE_RUNNING = 0,
  PS5_CAPTURE_OBSERVE_STOP_DATA,
  PS5_CAPTURE_OBSERVE_STOP_CALL_LIMIT,
  PS5_CAPTURE_OBSERVE_STOP_DEADLINE,
  PS5_CAPTURE_OBSERVE_STOP_CANCELLED,
  PS5_CAPTURE_OBSERVE_STOP_READ_ERROR,
  PS5_CAPTURE_OBSERVE_STOP_INVALID_LENGTH
};

typedef int32_t (*ps5_capture_observe_read_fn)(void *context, void *buffer,
                                               size_t capacity);

struct ps5_capture_observe_result {
  enum ps5_capture_observe_kind last_kind;
  enum ps5_capture_observe_stop stop;
  int32_t raw_last_status;
  uint32_t read_calls;
  uint32_t empty_calls;
  uint32_t overrun_calls;
  uint32_t data_bytes;
  uint8_t discontinuity;
};

struct ps5_capture_observe {
  uint64_t deadline_tick;
  uint64_t last_tick;
  uint32_t max_calls;
  struct ps5_capture_observe_result result;
  uint8_t active;
  uint8_t cleanup_available;
  uint8_t cleanup_taken;
};

/* `state` must initially be zeroed; it may be reused only after cleanup was
 * taken. `start_tick` and `deadline_tick` use one caller-owned monotonic time
 * domain. The deadline is exclusive and must be greater than start. A
 * successful begin records one cleanup obligation corresponding to the
 * already-started capture resource. Invalid input leaves `state` byte-for-byte
 * unchanged.
 */
enum ps5_capture_observe_status
ps5_capture_observe_begin(struct ps5_capture_observe *state,
                          uint64_t start_tick, uint64_t deadline_tick,
                          uint32_t max_calls);

/* Perform zero or one reads. `cancelled` may be nonzero with NULL read/buffer
 * arguments and transitions directly to STOP_CANCELLED. The same is true once
 * the deadline is reached. Otherwise read/context/buffer are caller-owned; the
 * callback must be nonblocking and may write only within capacity. Positive native results are
 * byte counts and stop on the first valid data result. Empty and Overrun may
 * continue only while both deadline and call budget remain. Zero and every
 * other native error stop as OTHER_ERROR. No sample/format inference occurs.
 * Invalid local arguments or a regressing clock leave state unchanged.
 */
enum ps5_capture_observe_status
ps5_capture_observe_step(struct ps5_capture_observe *state, uint64_t now_tick,
                         int cancelled,
                         ps5_capture_observe_read_fn read_once, void *context,
                         void *buffer, size_t capacity);

/* Transfer the single cleanup obligation after observation stops. Exactly one
 * successful take is possible. The integrator must use it to guard its existing
 * Stop/Close/Terminate cleanup path; this helper never owns native callbacks.
 */
enum ps5_capture_observe_status
ps5_capture_observe_take_cleanup(struct ps5_capture_observe *state);

#endif
