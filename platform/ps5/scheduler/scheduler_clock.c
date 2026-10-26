/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Original narrow adapter; inspected SDK signatures/layout/provenance
 * documented in docs/PLATFORM-CLOCK.md. No SDK header or implementation is
 * copied. */
#include "scheduler_clock.h"
#include <stddef.h>
struct target_timespec {
  int64_t seconds, nanoseconds;
};
typedef int (*clock_fn)(int32_t, struct target_timespec *);
typedef int (*wait_fn)(uint32_t);
_Static_assert(sizeof(struct target_timespec) == 16,
               "reviewed target timespec");
_Static_assert(offsetof(struct target_timespec, nanoseconds) == 8,
               "nanosecond offset");
_Static_assert(sizeof(clock_fn) == sizeof(void *),
               "target function representation");
_Static_assert(sizeof(wait_fn) == sizeof(void *),
               "target function representation");
_Static_assert(sizeof(struct ps5_clock_wait_report) == 48, "report ABI");
struct clock_context {
  clock_fn clock;
  wait_fn wait;
  int64_t last_seconds, last_nanoseconds;
  uint32_t ready, busy, have_now;
};
static void clear(void *p, size_t bytes) {
  unsigned char *out = p;
  for (size_t i = 0; i < bytes; ++i)
    out[i] = 0;
}
static int overlaps(const void *left, size_t ln, const void *right, size_t rn) {
  uintptr_t l = (uintptr_t)left, r = (uintptr_t)right;
  return ln > UINTPTR_MAX - l || rn > UINTPTR_MAX - r ||
         (l < r + rn && r < l + ln);
}
static void report(struct ps5_clock_wait_report *r, uint32_t stage) {
  clear(r, sizeof *r);
  r->abi_version = PS5_CLOCK_WAIT_ABI;
  r->stage = stage;
}
static int32_t result(struct ps5_clock_wait_report *r, int32_t status,
                      int32_t raw) {
  r->status = status;
  r->raw_status = raw;
  return status;
}
static int32_t begin(void *storage, struct ps5_clock_wait_report *r,
                     uint32_t stage) {
  if (!r || (storage &&
             overlaps(storage, sizeof(struct clock_context), r, sizeof *r)))
    return -2;
  report(r, stage);
  if (!storage || (uintptr_t)storage % _Alignof(struct clock_context))
    return result(r, -2, 0);
  struct clock_context *c = storage;
  if (!c->ready)
    return result(r, -2, 0);
  if (c->busy)
    return result(r, -3, 0);
  c->busy = 1;
  return 0;
}
uint32_t ps5_clock_wait_abi_version(void) { return PS5_CLOCK_WAIT_ABI; }
uint32_t ps5_clock_wait_storage_size(void) {
  return (uint32_t)sizeof(struct clock_context);
}
uint32_t ps5_clock_wait_storage_alignment(void) {
  return (uint32_t) _Alignof(struct clock_context);
}
int32_t ps5_clock_wait_init(void *storage, uint32_t capacity,
                            ps5_clock_wait_lookup_fn lookup,
                            struct ps5_clock_wait_report *r) {
  if (!r || (storage && overlaps(storage, capacity, r, sizeof *r)))
    return -2;
  report(r, 1);
  if (!storage || capacity < sizeof(struct clock_context) ||
      (uintptr_t)storage % _Alignof(struct clock_context) || !lookup)
    return result(r, -2, 0);
  struct clock_context *c = storage;
  /* Init is legal only for fresh zeroed/closed storage, preventing a nested
   * init from replacing cached function pointers while a native call is in
   * flight. */
  if (c->ready || c->busy)
    return result(r, -3, 0);
  clear(c, sizeof *c);
  c->busy = 1;
  void *symbol = 0;
  r->stage = 2;
  r->raw_lookup = lookup("clock_gettime", &symbol);
  if (r->raw_lookup || !symbol) {
    clear(c, sizeof *c);
    return result(r, -1, 0);
  }
  for (size_t i = 0; i < sizeof c->clock; ++i)
    ((unsigned char *)&c->clock)[i] = ((unsigned char *)&symbol)[i];
  symbol = 0;
  r->raw_lookup = lookup("usleep", &symbol);
  if (r->raw_lookup || !symbol) {
    clear(c, sizeof *c);
    return result(r, -1, 0);
  }
  for (size_t i = 0; i < sizeof c->wait; ++i)
    ((unsigned char *)&c->wait)[i] = ((unsigned char *)&symbol)[i];
  c->ready = 1;
  c->busy = 0;
  return 0;
}
int32_t ps5_clock_wait_now(void *storage, uint64_t *out_us,
                           struct ps5_clock_wait_report *r) {
  if (!out_us || !r || overlaps(out_us, sizeof *out_us, r, sizeof *r) ||
      (storage &&
       overlaps(storage, sizeof(struct clock_context), out_us, sizeof *out_us)))
    return -2;
  *out_us = 0;
  int32_t status = begin(storage, r, 3);
  if (status)
    return status;
  struct clock_context *c = storage;
  struct target_timespec time = {0};
  int raw = c->clock(4, &time); /* CLOCK_MONOTONIC from pinned SDK time.h. */
  r->raw_seconds = time.seconds;
  r->raw_nanoseconds = time.nanoseconds;
  if (raw)
    status = result(r, -5, raw);
  else if (time.seconds < 0 || time.nanoseconds < 0 ||
           time.nanoseconds >= INT64_C(1000000000))
    status = result(r, -6, 0);
  else {
    uint64_t fraction = (uint64_t)time.nanoseconds / 1000;
    if ((uint64_t)time.seconds > (UINT64_MAX - fraction) / 1000000)
      status = result(r, -7, 0);
    else if (c->have_now && (time.seconds < c->last_seconds ||
                             (time.seconds == c->last_seconds &&
                              time.nanoseconds < c->last_nanoseconds)))
      status = result(r, -8, 0);
    else {
      c->last_seconds = time.seconds;
      c->last_nanoseconds = time.nanoseconds;
      c->have_now = 1;
      *out_us = (uint64_t)time.seconds * 1000000 + fraction;
    }
  }
  c->busy = 0;
  return status;
}
int32_t ps5_clock_wait_slice(void *storage, uint32_t requested_us,
                             struct ps5_clock_wait_report *r) {
  int32_t status = begin(storage, r, 4);
  if (status)
    return status;
  struct clock_context *c = storage;
  if (requested_us == 0 || requested_us > PS5_CLOCK_WAIT_MAX_SLICE_US) {
    c->busy = 0;
    return result(r, -2, 0);
  }
  r->requested_us = requested_us;
  int raw = c->wait(requested_us);
  c->busy = 0;
  return result(r, raw ? -5 : 0, raw);
}
int32_t ps5_clock_wait_close(void *storage, struct ps5_clock_wait_report *r) {
  int32_t status = begin(storage, r, 5);
  if (status)
    return status;
  clear(storage, sizeof(struct clock_context));
  return 0;
}
