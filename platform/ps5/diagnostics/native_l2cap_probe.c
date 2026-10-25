/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "native_l2cap_probe.h"
#include <limits.h>

_Static_assert(sizeof(int) == sizeof(int32_t) && INT_MAX == INT32_MAX,
               "native socket integer ABI requires signed int32");
_Static_assert(sizeof(ps5_native_l2cap_socket_fn) == sizeof(void *),
               "socket resolver pointer representation");
_Static_assert(sizeof(ps5_native_l2cap_close_fn) == sizeof(void *),
               "close resolver pointer representation");
_Static_assert(sizeof(ps5_native_l2cap_errno_fn) == sizeof(void *),
               "errno resolver pointer representation");

static void copy_pointer(void *destination, const void *source, size_t size) {
  unsigned char *dst = destination;
  const unsigned char *src = source;
  for (size_t i = 0; i < size; ++i)
    dst[i] = src[i];
}

static int allocate_once(ps5_native_l2cap_socket_fn socket_fn, int *error,
                         int domain, int type, int protocol,
                         ps5_native_l2cap_trace_fn trace,
                         enum ps5_native_l2cap_phase phase,
                         struct ps5_native_l2cap_call_result *out) {
  out->called = 1;
  if (trace)
    trace(phase, 1, 0, 0, 0);
  *error = 0;
  out->raw_status = socket_fn(domain, type, protocol);
  if (out->raw_status < 0) {
    out->raw_errno = *error;
    out->errno_valid = 1;
  }
  if (trace)
    trace(phase, 0, out->raw_status, out->errno_valid, out->raw_errno);
  return out->raw_status;
}

static int close_once(ps5_native_l2cap_close_fn close_fn, int *error, int fd,
                      ps5_native_l2cap_trace_fn trace,
                      enum ps5_native_l2cap_phase phase,
                      struct ps5_native_l2cap_call_result *out) {
  /* No retained owner or fallback path can close this consumed fd again. */
  out->called = 1;
  if (trace)
    trace(phase, 1, 0, 0, 0);
  *error = 0;
  out->raw_status = close_fn(fd);
  if (out->raw_status != 0) {
    out->raw_errno = *error;
    out->errno_valid = 1;
  }
  if (trace)
    trace(phase, 0, out->raw_status, out->errno_valid, out->raw_errno);
  return out->raw_status;
}

enum ps5_native_l2cap_status
ps5_native_l2cap_probe_resolve(ps5_native_l2cap_lookup_fn lookup,
                               ps5_native_l2cap_trace_fn trace,
                               struct ps5_native_l2cap_result *out) {
  if (!out)
    return PS5_NATIVE_L2CAP_INVALID;
  *out = (struct ps5_native_l2cap_result){.status =
                                              PS5_NATIVE_L2CAP_RESOLVE_FAILED};
  if (!lookup)
    return out->status;

  static const char *const names[PS5_NATIVE_L2CAP_SYMBOL_COUNT] = {
      "socket", "close", "__error"};
  void *symbols[PS5_NATIVE_L2CAP_SYMBOL_COUNT] = {NULL, NULL, NULL};
  int all_available = 1;
  for (size_t i = 0; i < PS5_NATIVE_L2CAP_SYMBOL_COUNT; ++i) {
    out->symbols[i].called = 1;
    enum ps5_native_l2cap_phase phase = (enum ps5_native_l2cap_phase)i;
    if (trace)
      trace(phase, 1, 0, 0, 0);
    out->symbols[i].raw_status = lookup(names[i], &symbols[i]);
    out->symbols[i].available =
        out->symbols[i].raw_status == 0 && symbols[i] != NULL;
    if (!out->symbols[i].available)
      all_available = 0;
    if (trace)
      trace(phase, 0, out->symbols[i].raw_status, 0, 0);
  }
  if (!all_available)
    return out->status;

  ps5_native_l2cap_socket_fn socket_fn = NULL;
  ps5_native_l2cap_close_fn close_fn = NULL;
  ps5_native_l2cap_errno_fn errno_fn = NULL;
  copy_pointer(&socket_fn, &symbols[0], sizeof socket_fn);
  copy_pointer(&close_fn, &symbols[1], sizeof close_fn);
  copy_pointer(&errno_fn, &symbols[2], sizeof errno_fn);
  out->errno_called = 1;
  if (trace)
    trace(PS5_NATIVE_L2CAP_PHASE_ERRNO, 1, 0, 0, 0);
  int *error = errno_fn();
  if (trace)
    trace(PS5_NATIVE_L2CAP_PHASE_ERRNO, 0, error != NULL, 0, 0);
  if (!error) {
    out->status = PS5_NATIVE_L2CAP_ERRNO_UNAVAILABLE;
    return out->status;
  }
  out->errno_available = 1;

  int fd = allocate_once(socket_fn, error, 2, 2, 17, trace,
                         PS5_NATIVE_L2CAP_PHASE_CONTROL_SOCKET,
                         &out->control_socket);
  if (fd < 0) {
    out->status = PS5_NATIVE_L2CAP_CONTROL_FAILED;
    return out->status;
  }
  if (close_once(close_fn, error, fd, trace,
                 PS5_NATIVE_L2CAP_PHASE_CONTROL_CLOSE,
                 &out->control_close) != 0) {
    out->status = PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN;
    return out->status;
  }
  fd = allocate_once(socket_fn, error, 36, 5, 135, trace,
                     PS5_NATIVE_L2CAP_PHASE_L2CAP_SOCKET, &out->l2cap_socket);
  if (fd < 0) {
    out->status = PS5_NATIVE_L2CAP_NEGATIVE;
    return out->status;
  }
  if (close_once(close_fn, error, fd, trace, PS5_NATIVE_L2CAP_PHASE_L2CAP_CLOSE,
                 &out->l2cap_close) != 0) {
    out->status = PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN;
    return out->status;
  }
  out->status = PS5_NATIVE_L2CAP_AVAILABLE;
  return out->status;
}

struct report_buffer {
  char data[PS5_NATIVE_L2CAP_REPORT_CAPACITY];
  size_t length;
  int overflow;
};

static void byte(struct report_buffer *out, char value) {
  if (out->length == sizeof out->data)
    out->overflow = 1;
  else
    out->data[out->length++] = value;
}

static void text(struct report_buffer *out, const char *value) {
  while (*value)
    byte(out, *value++);
}

static void number(struct report_buffer *out, int64_t value) {
  uint64_t magnitude = (uint64_t)value;
  if (value < 0) {
    byte(out, '-');
    magnitude = (uint64_t)0 - magnitude;
  }
  char digits[20];
  size_t count = 0;
  do {
    digits[count++] = (char)('0' + magnitude % 10);
    magnitude /= 10;
  } while (magnitude);
  while (count)
    byte(out, digits[--count]);
}

static int boolean(int value) { return value == 0 || value == 1; }

static const char *status_name(enum ps5_native_l2cap_status status) {
  switch (status) {
  case PS5_NATIVE_L2CAP_AVAILABLE:
    return "local_socket_available";
  case PS5_NATIVE_L2CAP_NEGATIVE:
    return "completed_negative";
  case PS5_NATIVE_L2CAP_RESOLVE_FAILED:
    return "resolve_failed";
  case PS5_NATIVE_L2CAP_ERRNO_UNAVAILABLE:
    return "errno_unavailable";
  case PS5_NATIVE_L2CAP_CONTROL_FAILED:
    return "control_failed";
  case PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN:
    return "cleanup_uncertain";
  default:
    return NULL;
  }
}

static int call_report(struct report_buffer *out, const char *name,
                       const struct ps5_native_l2cap_call_result *call) {
  if (!boolean(call->called) || !boolean(call->errno_valid))
    return 0;
  text(out, " ");
  text(out, name);
  text(out, "=");
  if (call->called)
    number(out, call->raw_status);
  else
    text(out, "not_called");
  text(out, " errno=");
  if (call->errno_valid)
    number(out, call->raw_errno);
  else
    text(out, "unknown");
  return 1;
}

enum ps5_native_l2cap_status ps5_native_l2cap_probe_encode_report(
    const struct ps5_native_l2cap_result *result, char *data, size_t capacity,
    size_t *out_len) {
  if (!out_len)
    return PS5_NATIVE_L2CAP_INVALID;
  *out_len = 0;
  if (!result || (!data && capacity))
    return PS5_NATIVE_L2CAP_INVALID;
  const char *status = status_name(result->status);
  if (!status || !boolean(result->errno_called) ||
      !boolean(result->errno_available))
    return PS5_NATIVE_L2CAP_INVALID;
  struct report_buffer buffer = {.length = 0, .overflow = 0};
  text(&buffer, "native_l2cap_v1 status=");
  text(&buffer, status);
  for (size_t i = 0; i < PS5_NATIVE_L2CAP_SYMBOL_COUNT; ++i) {
    const struct ps5_native_l2cap_symbol_result *symbol = &result->symbols[i];
    if (!boolean(symbol->called) || !boolean(symbol->available))
      return PS5_NATIVE_L2CAP_INVALID;
    text(&buffer, " resolve");
    number(&buffer, (int64_t)i);
    text(&buffer, "=");
    if (symbol->called)
      number(&buffer, symbol->raw_status);
    else
      text(&buffer, "not_called");
    text(&buffer, " available=");
    number(&buffer, symbol->available);
  }
  text(&buffer, " errno_called=");
  number(&buffer, result->errno_called);
  text(&buffer, " errno_available=");
  number(&buffer, result->errno_available);
  if (!call_report(&buffer, "control_socket(2,2,17)",
                   &result->control_socket) ||
      !call_report(&buffer, "control_close", &result->control_close) ||
      !call_report(&buffer, "l2cap_socket(36,5,135)", &result->l2cap_socket) ||
      !call_report(&buffer, "l2cap_close", &result->l2cap_close))
    return PS5_NATIVE_L2CAP_INVALID;
  text(&buffer, " transport=unsupported controller=unknown"
                " scope=local_socket_only\n");
  if (buffer.overflow)
    return PS5_NATIVE_L2CAP_INVALID;
  *out_len = buffer.length;
  if (!data || capacity < buffer.length)
    return PS5_NATIVE_L2CAP_CAPACITY;
  for (size_t i = 0; i < buffer.length; ++i)
    data[i] = buffer.data[i];
  return PS5_NATIVE_L2CAP_AVAILABLE;
}
