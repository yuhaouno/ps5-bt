/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_NATIVE_L2CAP_PROBE_H
#define PS5_NATIVE_L2CAP_PROBE_H

#include <stddef.h>
#include <stdint.h>

#define PS5_NATIVE_L2CAP_REPORT_CAPACITY 1024u
#define PS5_NATIVE_L2CAP_SYMBOL_COUNT 3u

/* SDK v0.45 sys/socket.h, unistd.h and sys/errno.h integer/pointer ABI.
 * Symbol inventories and imported headers do not prove PS5 implementation.
 */
typedef int (*ps5_native_l2cap_socket_fn)(int, int, int);
typedef int (*ps5_native_l2cap_close_fn)(int);
typedef int *(*ps5_native_l2cap_errno_fn)(void);
typedef int64_t (*ps5_native_l2cap_lookup_fn)(const char *, void **);

enum ps5_native_l2cap_phase {
  PS5_NATIVE_L2CAP_PHASE_RESOLVE_SOCKET = 0,
  PS5_NATIVE_L2CAP_PHASE_RESOLVE_CLOSE = 1,
  PS5_NATIVE_L2CAP_PHASE_RESOLVE_ERRNO = 2,
  PS5_NATIVE_L2CAP_PHASE_ERRNO = 3,
  PS5_NATIVE_L2CAP_PHASE_CONTROL_SOCKET = 4,
  PS5_NATIVE_L2CAP_PHASE_CONTROL_CLOSE = 5,
  PS5_NATIVE_L2CAP_PHASE_L2CAP_SOCKET = 6,
  PS5_NATIVE_L2CAP_PHASE_L2CAP_CLOSE = 7
};

/* Optional synchronous trace: entering=1 before a call, entering=0 after.
 * Enter has raw_status/errno_valid/raw_errno zero. Exit has the saved result;
 * errno is saved before tracing and valid only on socket/close failure.
 * ERRNO exit raw_status is 1 for non-NULL storage, 0 for NULL, never an
 * address. Trace must not reenter the probe or retain borrowed pointers. It may
 * change native errno: the probe clears it again immediately before each native
 * call.
 */
typedef void (*ps5_native_l2cap_trace_fn)(enum ps5_native_l2cap_phase phase,
                                          int entering, int64_t raw_status,
                                          int errno_valid, int raw_errno);

enum ps5_native_l2cap_status {
  PS5_NATIVE_L2CAP_AVAILABLE = 0,
  PS5_NATIVE_L2CAP_NEGATIVE = 1,
  PS5_NATIVE_L2CAP_RESOLVE_FAILED = -1,
  PS5_NATIVE_L2CAP_ERRNO_UNAVAILABLE = -2,
  PS5_NATIVE_L2CAP_CONTROL_FAILED = -3,
  PS5_NATIVE_L2CAP_CLEANUP_UNCERTAIN = -4,
  PS5_NATIVE_L2CAP_INVALID = -5,
  PS5_NATIVE_L2CAP_CAPACITY = -6
};

/* Symbols are indexed socket=0, close=1, __error=2. Raw resolver status is
 * meaningful only if called; available requires status 0 AND non-NULL symbol.
 */
struct ps5_native_l2cap_symbol_result {
  int called;
  int64_t raw_status;
  int available;
};

/* raw_errno is captured immediately, only on native failure. Successful calls
 * leave errno_valid/raw_errno zero, regardless of native stale errno contents.
 * raw_status for socket is its descriptor or negative failure, never a token.
 */
struct ps5_native_l2cap_call_result {
  int called;
  int raw_status;
  int errno_valid;
  int raw_errno;
};

struct ps5_native_l2cap_result {
  enum ps5_native_l2cap_status status;
  struct ps5_native_l2cap_symbol_result symbols[PS5_NATIVE_L2CAP_SYMBOL_COUNT];
  int errno_called;
  int errno_available;
  struct ps5_native_l2cap_call_result control_socket;
  struct ps5_native_l2cap_call_result control_close;
  struct ps5_native_l2cap_call_result l2cap_socket;
  struct ps5_native_l2cap_call_result l2cap_close;
};

/* Experimental, finite local allocation test, not a transport initializer.
 * Resolve all three exports before action, then call __error once on the same
 * thread. NULL errno storage prevents every socket call. First socket(2,2,17),
 * then exactly one close if fd >= 0, including fd 0. Only after clean control
 * cleanup call socket(36,5,135) and close its nonnegative descriptor once.
 * Consume ownership before close; never retry even on EINTR or ambiguous error.
 * No bind/connect/listen/send/receive/ioctl, flags, fallback, module load or
 * permission change. At most one owned descriptor exists at a time.
 * Lookup failure ignores dirty symbol output. All callbacks are synchronous,
 * obey the declared ABI and retain no borrowed pointers. Resolver conversion
 * assumes matching PS5 x86-64 System V code/data pointer representations.
 * Socket/close have no network wait here, but no hard kernel-call deadline is
 * claimed. Caller supplies an external observation ceiling and durable markers.
 * AVAILABLE means only local allocation/close accepted; NEGATIVE means the
 * Bluetooth allocation returned negative after successful control cleanup.
 * NULL out returns INVALID with no callback calls.
 */
enum ps5_native_l2cap_status
ps5_native_l2cap_probe_resolve(ps5_native_l2cap_lookup_fn lookup,
                               ps5_native_l2cap_trace_fn trace,
                               struct ps5_native_l2cap_result *out);

/* Atomic ASCII v1 report, no NUL, <= REPORT_CAPACITY bytes. Requires an
 * unmodified result from resolve. Returns AVAILABLE on encoding success only;
 * the result's diagnostic status remains in the text. Invalid => out_len 0;
 * short capacity => required length with data unchanged. NULL,0 queries size.
 * Input, output and out_len must be distinct valid objects; nothing retained.
 */
enum ps5_native_l2cap_status ps5_native_l2cap_probe_encode_report(
    const struct ps5_native_l2cap_result *result, char *data, size_t capacity,
    size_t *out_len);

#endif
