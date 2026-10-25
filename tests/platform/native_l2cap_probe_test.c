/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "native_l2cap_probe.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static int lookup_calls, errno_calls, socket_calls, close_calls;
static int64_t lookup_status[3];
static int missing_symbol, null_errno, native_errno;
static int socket_results[2], socket_errors[2];
static int close_results[2], close_errors[2];
static enum ps5_native_l2cap_phase phases[16];
static int entries[16], trace_errno_valid[16], trace_errors[16];
static int64_t trace_results[16];
static size_t trace_count;
static int trace_enabled;

static int fake_socket(int domain, int type, int protocol) {
  assert(lookup_calls == 3 && errno_calls == 1 && socket_calls < 2);
  assert(native_errno == 0); /* Enter tracing may dirty errno. */
  if (socket_calls == 0) {
    assert(domain == 2 && type == 2 && protocol == 17);
    assert(close_calls == 0);
  } else {
    assert(domain == 36 && type == 5 && protocol == 135);
    assert(close_calls == 1 && close_results[0] == 0);
  }
  int index = socket_calls++;
  native_errno = socket_errors[index];
  return socket_results[index];
}

static int fake_close(int fd) {
  assert(close_calls < 2 && socket_calls == close_calls + 1);
  assert(fd == socket_results[socket_calls - 1] && fd >= 0);
  assert(native_errno == 0);
  int index = close_calls++;
  native_errno = close_errors[index];
  return close_results[index];
}

static int *fake_errno(void) {
  assert(lookup_calls == 3 && errno_calls == 0);
  ++errno_calls;
  return null_errno ? NULL : &native_errno;
}

static int64_t fake_lookup(const char *name, void **out) {
  static const char *const names[] = {"socket", "close", "__error"};
  assert(lookup_calls < 3 && out && *out == NULL);
  int index = lookup_calls++;
  assert(strcmp(name, names[index]) == 0);
  if (missing_symbol != index) {
    if (index == 0) {
      ps5_native_l2cap_socket_fn function = fake_socket;
      memcpy(out, &function, sizeof function);
    } else if (index == 1) {
      ps5_native_l2cap_close_fn function = fake_close;
      memcpy(out, &function, sizeof function);
    } else {
      ps5_native_l2cap_errno_fn function = fake_errno;
      memcpy(out, &function, sizeof function);
    }
  }
  return lookup_status[index]; /* Failure deliberately leaves dirty pointer. */
}

static void fake_trace(enum ps5_native_l2cap_phase phase, int entering,
                       int64_t raw_status, int errno_valid, int raw_errno) {
  assert(trace_count < 16);
  if (entering)
    assert(raw_status == 0 && errno_valid == 0 && raw_errno == 0);
  phases[trace_count] = phase;
  entries[trace_count] = entering;
  trace_results[trace_count] = raw_status;
  trace_errno_valid[trace_count] = errno_valid;
  trace_errors[trace_count++] = raw_errno;
  native_errno = 999; /* Logging must never corrupt captured native errno. */
}

static void reset(void) {
  lookup_calls = errno_calls = socket_calls = close_calls = 0;
  memset(lookup_status, 0, sizeof lookup_status);
  missing_symbol = -1;
  null_errno = 0;
  native_errno = 123;
  socket_results[0] = 0; /* fd zero is owned and must close. */
  socket_results[1] = INT_MAX;
  socket_errors[0] = socket_errors[1] = 12345;
  close_results[0] = close_results[1] = 0;
  close_errors[0] = close_errors[1] = 67890;
  trace_count = 0;
  trace_enabled = 1;
}

static enum ps5_native_l2cap_status run(struct ps5_native_l2cap_result *out) {
  return ps5_native_l2cap_probe_resolve(fake_lookup,
                                        trace_enabled ? fake_trace : NULL, out);
}

static void bounded_report(const struct ps5_native_l2cap_result *result) {
  char reference[PS5_NATIVE_L2CAP_REPORT_CAPACITY + 1];
  char guarded[PS5_NATIVE_L2CAP_REPORT_CAPACITY + 2];
  size_t length;
  memset(reference, 'X', sizeof reference);
  assert(ps5_native_l2cap_probe_encode_report(result, reference,
                                              sizeof reference - 1, &length) ==
         PS5_NATIVE_L2CAP_AVAILABLE);
  assert(length && length <= PS5_NATIVE_L2CAP_REPORT_CAPACITY);
  assert(reference[length] == 'X' && reference[length - 1] == '\n');
  for (size_t i = 0; i + 1 < length; ++i)
    assert(reference[i] >= ' ' && reference[i] <= '~');
  reference[length] = '\0';
  assert(strstr(reference, "transport=unsupported controller=unknown"));
  for (size_t capacity = 0; capacity <= length + 1; ++capacity) {
    memset(guarded, 'X', sizeof guarded);
    size_t actual = 999;
    enum ps5_native_l2cap_status status = ps5_native_l2cap_probe_encode_report(
        result, guarded + 1, capacity, &actual);
    assert(actual == length && guarded[0] == 'X');
    if (capacity < length) {
      assert(status == PS5_NATIVE_L2CAP_CAPACITY);
      for (size_t i = 0; i < sizeof guarded; ++i)
        assert(guarded[i] == 'X');
    } else {
      assert(status == PS5_NATIVE_L2CAP_AVAILABLE);
      assert(memcmp(guarded + 1, reference, length) == 0);
      for (size_t i = length + 1; i < sizeof guarded; ++i)
        assert(guarded[i] == 'X');
    }
  }
  size_t required = 0;
  assert(ps5_native_l2cap_probe_encode_report(result, NULL, 0, &required) ==
         PS5_NATIVE_L2CAP_CAPACITY);
  assert(required == length);
}

static void test_resolution(void) {
  struct ps5_native_l2cap_result result;
  reset();
  assert(ps5_native_l2cap_probe_resolve(fake_lookup, fake_trace, NULL) ==
         PS5_NATIVE_L2CAP_INVALID);
  assert(lookup_calls == 0 && trace_count == 0);
  memset(&result, 0xa5, sizeof result);
  assert(ps5_native_l2cap_probe_resolve(NULL, fake_trace, &result) ==
         PS5_NATIVE_L2CAP_RESOLVE_FAILED);
  assert(!result.symbols[0].called && !result.control_socket.called);
  bounded_report(&result);

  const int64_t failures[] = {-1, 13, INT64_MIN, INT64_MAX};
  for (int symbol = 0; symbol < 3; ++symbol) {
    for (size_t i = 0; i < sizeof failures / sizeof failures[0]; ++i) {
      reset();
      lookup_status[symbol] = failures[i];
      assert(run(&result) == PS5_NATIVE_L2CAP_RESOLVE_FAILED);
      assert(lookup_calls == 3 && !errno_calls && !socket_calls);
      assert(result.symbols[symbol].raw_status == failures[i]);
      assert(!result.symbols[symbol].available && trace_count == 6);
      bounded_report(&result);
    }
    reset();
    missing_symbol = symbol;
    assert(run(&result) == PS5_NATIVE_L2CAP_RESOLVE_FAILED);
    assert(lookup_calls == 3 && !errno_calls && !socket_calls);
    assert(!result.symbols[symbol].available);
  }
  reset();
  null_errno = 1;
  assert(run(&result) == PS5_NATIVE_L2CAP_ERRNO_UNAVAILABLE);
  assert(errno_calls == 1 && !socket_calls && !close_calls);
  assert(result.errno_called && !result.errno_available);
  assert(trace_count == 8 && trace_results[7] == 0);
  bounded_report(&result);
}

static void test_calls(void) {
  struct ps5_native_l2cap_result result;
  reset();
  assert(run(&result) == PS5_NATIVE_L2CAP_AVAILABLE);
  assert(socket_calls == 2 && close_calls == 2 && trace_count == 16);
  assert(result.control_socket.raw_status == 0);
  assert(result.l2cap_socket.raw_status == INT_MAX);
  assert(!result.control_socket.errno_valid &&
         !result.control_close.errno_valid &&
         !result.l2cap_socket.errno_valid && !result.l2cap_close.errno_valid);
  for (size_t i = 0; i < 16; ++i) {
    assert(phases[i] == (enum ps5_native_l2cap_phase)(i / 2));
    assert(entries[i] == (i % 2 == 0));
  }
  bounded_report(&result);
  reset();
  trace_enabled = 0;
  assert(run(&result) == PS5_NATIVE_L2CAP_AVAILABLE && trace_count == 0);

  const int errors[] = {0, 1, 4, 13, 22, 43, 47, -1, INT_MIN, INT_MAX};
  for (size_t i = 0; i < sizeof errors / sizeof errors[0]; ++i) {
    for (int index = 0; index < 2; ++index) {
      reset();
      socket_results[index] = i % 2 ? -1 : INT_MIN;
      socket_errors[index] = errors[i];
      assert(run(&result) == (index ? PS5_NATIVE_L2CAP_NEGATIVE
                                    : PS5_NATIVE_L2CAP_CONTROL_FAILED));
      assert(socket_calls == index + 1 && close_calls == index);
      const struct ps5_native_l2cap_call_result *call =
          index ? &result.l2cap_socket : &result.control_socket;
      assert(call->errno_valid && call->raw_errno == errors[i]);
      assert(trace_errno_valid[trace_count - 1] == 1);
      assert(trace_errors[trace_count - 1] == errors[i]);
      bounded_report(&result);

      reset();
      close_results[index] = i % 2 ? -1 : INT_MIN;
      close_errors[index] = errors[i];
      assert(run(&result) == PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN);
      assert(socket_calls == index + 1 && close_calls == index + 1);
      call = index ? &result.l2cap_close : &result.control_close;
      assert(call->errno_valid && call->raw_errno == errors[i]);
      assert(trace_errors[trace_count - 1] == errors[i]);
      assert(index || !result.l2cap_socket.called);
      bounded_report(&result); /* Rendering does not retry close. */
      assert(close_calls == index + 1);
    }
  }
  reset();
  close_results[0] = 1; /* Unexpected positive status also fails closed. */
  assert(run(&result) == PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN);
  assert(socket_calls == 1 && close_calls == 1);
}

static void test_invalid_reports(void) {
  struct ps5_native_l2cap_result result;
  reset();
  assert(run(&result) == PS5_NATIVE_L2CAP_AVAILABLE);
  char data[PS5_NATIVE_L2CAP_REPORT_CAPACITY];
  memset(data, 'X', sizeof data);
  size_t length = 123;
  assert(ps5_native_l2cap_probe_encode_report(
             NULL, data, sizeof data, &length) == PS5_NATIVE_L2CAP_INVALID);
  assert(length == 0);
  assert(ps5_native_l2cap_probe_encode_report(&result, NULL, 1, &length) ==
         PS5_NATIVE_L2CAP_INVALID);
  assert(ps5_native_l2cap_probe_encode_report(
             &result, data, sizeof data, NULL) == PS5_NATIVE_L2CAP_INVALID);
  result.status = (enum ps5_native_l2cap_status)123;
  assert(ps5_native_l2cap_probe_encode_report(
             &result, data, sizeof data, &length) == PS5_NATIVE_L2CAP_INVALID);
  result.status = PS5_NATIVE_L2CAP_AVAILABLE;
  result.symbols[2].available = -1;
  assert(ps5_native_l2cap_probe_encode_report(
             &result, data, sizeof data, &length) == PS5_NATIVE_L2CAP_INVALID);
  result.symbols[2].available = 1;
  result.l2cap_close.errno_valid = 2;
  assert(ps5_native_l2cap_probe_encode_report(
             &result, data, sizeof data, &length) == PS5_NATIVE_L2CAP_INVALID);
  assert(length == 0);
  for (size_t i = 0; i < sizeof data; ++i)
    assert(data[i] == 'X');
  assert(socket_calls == 2 && close_calls == 2);
}

int main(void) {
  test_resolution();
  test_calls();
  test_invalid_reports();
  puts("PASS synthetic native L2CAP allocation, ordered trace, errno, "
       "consume-once close and atomic reports");
  return 0;
}
