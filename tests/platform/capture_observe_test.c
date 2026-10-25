/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "capture_observe.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>

struct scripted_reader {
  const int32_t *statuses;
  size_t count;
  size_t index;
  unsigned char fill;
};

static int32_t read_once(void *raw, void *buffer, size_t capacity) {
  struct scripted_reader *reader = raw;
  assert(reader && reader->index < reader->count);
  int32_t status = reader->statuses[reader->index++];
  if (status > 0 && (size_t)status <= capacity)
    memset(buffer, reader->fill, (size_t)status);
  return status;
}

static void test_empty_overrun_then_data(void) {
  const int32_t statuses[] = {
      (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW,
      (int32_t)PS5_CAPTURE_OBSERVE_OVERRUN_RAW,
      512,
  };
  struct scripted_reader reader = {statuses, 3, 0, 0xa5};
  struct ps5_capture_observe state = {0};
  unsigned char buffer[1024] = {0};

  assert(ps5_capture_observe_begin(&state, 100, 200, 4) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_take_cleanup(&state) ==
         PS5_CAPTURE_OBSERVE_NOT_DONE);
  assert(ps5_capture_observe_step(&state, 110, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(state.active && state.result.last_kind == PS5_CAPTURE_OBSERVE_EMPTY);
  assert(ps5_capture_observe_step(&state, 120, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(state.active && state.result.last_kind == PS5_CAPTURE_OBSERVE_OVERRUN);
  assert(ps5_capture_observe_step(&state, 130, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(!state.active && state.result.stop == PS5_CAPTURE_OBSERVE_STOP_DATA);
  assert(state.result.read_calls == 3 && state.result.empty_calls == 1 &&
         state.result.overrun_calls == 1 && state.result.data_bytes == 512 &&
         state.result.discontinuity == 1 && reader.index == 3);
  for (size_t i = 0; i < 512; ++i)
    assert(buffer[i] == 0xa5);
  assert(ps5_capture_observe_step(&state, 140, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(reader.index == 3);
  assert(ps5_capture_observe_take_cleanup(&state) == PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_take_cleanup(&state) ==
         PS5_CAPTURE_OBSERVE_CLEANUP_TAKEN);
}

static void test_call_limit_is_exact(void) {
  const int32_t statuses[] = {
      (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW,
      (int32_t)PS5_CAPTURE_OBSERVE_EMPTY_RAW,
  };
  struct scripted_reader reader = {statuses, 2, 0, 0};
  struct ps5_capture_observe state = {0};
  unsigned char buffer[8];

  assert(ps5_capture_observe_begin(&state, 0, 100, 2) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_step(&state, 1, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(state.active);
  assert(ps5_capture_observe_step(&state, 2, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(!state.active &&
         state.result.stop == PS5_CAPTURE_OBSERVE_STOP_CALL_LIMIT &&
         state.result.read_calls == 2 && state.result.empty_calls == 2 &&
         reader.index == 2);
  assert(ps5_capture_observe_take_cleanup(&state) == PS5_CAPTURE_OBSERVE_OK);
}

static void test_deadline_and_cancel_make_no_read(void) {
  struct ps5_capture_observe state = {0};
  assert(ps5_capture_observe_begin(&state, 10, 20, 2) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_step(&state, 20, 0, NULL, NULL, NULL, 0) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(!state.active && state.result.read_calls == 0 &&
         state.result.stop == PS5_CAPTURE_OBSERVE_STOP_DEADLINE);
  assert(ps5_capture_observe_take_cleanup(&state) == PS5_CAPTURE_OBSERVE_OK);

  assert(ps5_capture_observe_begin(&state, 10, 20, 2) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_step(&state, 11, 1, NULL, NULL, NULL, 0) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(!state.active && state.result.read_calls == 0 &&
         state.result.stop == PS5_CAPTURE_OBSERVE_STOP_CANCELLED);
  assert(ps5_capture_observe_take_cleanup(&state) == PS5_CAPTURE_OBSERVE_OK);
}

static void test_other_errors_and_invalid_lengths_stop(void) {
  const int32_t statuses[] = {0, (int32_t)UINT32_C(0x81950008), -77};
  for (size_t i = 0; i < sizeof statuses / sizeof statuses[0]; ++i) {
    struct scripted_reader reader = {&statuses[i], 1, 0, 0};
    struct ps5_capture_observe state = {0};
    unsigned char buffer[16];
    assert(ps5_capture_observe_begin(&state, 0, 10, 2) ==
           PS5_CAPTURE_OBSERVE_OK);
    assert(ps5_capture_observe_step(&state, 1, 0, read_once, &reader, buffer,
                                    sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
    assert(!state.active && state.result.read_calls == 1 &&
           state.result.last_kind == PS5_CAPTURE_OBSERVE_OTHER_ERROR &&
           state.result.stop == PS5_CAPTURE_OBSERVE_STOP_READ_ERROR &&
           state.result.raw_last_status == statuses[i]);
  }

  const int32_t too_long = 17;
  struct scripted_reader reader = {&too_long, 1, 0, 0};
  struct ps5_capture_observe state = {0};
  unsigned char buffer[16];
  assert(ps5_capture_observe_begin(&state, 0, 10, 1) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_step(&state, 1, 0, read_once, &reader, buffer,
                                  sizeof buffer) == PS5_CAPTURE_OBSERVE_OK);
  assert(!state.active &&
         state.result.stop == PS5_CAPTURE_OBSERVE_STOP_INVALID_LENGTH &&
         state.result.data_bytes == 0);
}

static void test_invalid_inputs_are_atomic(void) {
  struct ps5_capture_observe sentinel;
  struct ps5_capture_observe before;
  memset(&sentinel, 0xa5, sizeof sentinel);
  before = sentinel;
  assert(ps5_capture_observe_begin(&sentinel, 10, 10, 1) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&sentinel, &before, sizeof sentinel) == 0);
  assert(ps5_capture_observe_begin(&sentinel, 10, 20, 0) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&sentinel, &before, sizeof sentinel) == 0);
  assert(ps5_capture_observe_begin(&sentinel, 10, 20,
                                   PS5_CAPTURE_OBSERVE_MAX_CALLS + 1) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&sentinel, &before, sizeof sentinel) == 0);

  struct ps5_capture_observe state = {0};
  assert(ps5_capture_observe_begin(&state, 10, 20, 1) ==
         PS5_CAPTURE_OBSERVE_OK);
  before = state;
  assert(ps5_capture_observe_begin(&state, 10, 20, 1) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&state, &before, sizeof state) == 0);
  assert(ps5_capture_observe_step(&state, 9, 0, NULL, NULL, NULL, 0) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&state, &before, sizeof state) == 0);
  assert(ps5_capture_observe_step(&state, 11, 0, NULL, NULL, NULL, 0) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&state, &before, sizeof state) == 0);
  assert(ps5_capture_observe_step(&state, 11, 0, read_once, NULL, NULL,
                                  PS5_CAPTURE_OBSERVE_MAX_READ_BYTES + 1) ==
         PS5_CAPTURE_OBSERVE_INVALID);
  assert(memcmp(&state, &before, sizeof state) == 0);
  assert(ps5_capture_observe_step(&state, 11, 1, NULL, NULL, NULL, 0) ==
         PS5_CAPTURE_OBSERVE_OK);
  assert(ps5_capture_observe_take_cleanup(&state) == PS5_CAPTURE_OBSERVE_OK);
}

int main(void) {
  test_empty_overrun_then_data();
  test_call_limit_is_exact();
  test_deadline_and_cancel_make_no_read();
  test_other_errors_and_invalid_lengths_stop();
  test_invalid_inputs_are_atomic();
  puts("PASS bounded caller-driven capture observation and cleanup token");
  return 0;
}
