/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SCHEDULER_CLOCK_H
#define PS5_BT_SCHEDULER_CLOCK_H
#include <stdint.h>
#define PS5_CLOCK_WAIT_ABI UINT32_C(0x00010000)
#define PS5_CLOCK_WAIT_MAX_SLICE_US UINT32_C(1000)
struct ps5_clock_wait_report {
  uint32_t abi_version;
  uint32_t stage; /* 1 validation, 2 resolver, 3 now, 4 wait, 5 close */
  int32_t
      status; /* 0 OK, -1 unavailable, -2 invalid, -3 busy,
                -5 native error, -6 invalid time, -7 overflow, -8 regression */
  int32_t raw_status;
  int64_t raw_lookup;
  uint32_t requested_us;
  uint32_t reserved;
  int64_t raw_seconds;
  int64_t raw_nanoseconds;
};
typedef int64_t (*ps5_clock_wait_lookup_fn)(const char *, void **);
uint32_t ps5_clock_wait_abi_version(void);
uint32_t ps5_clock_wait_storage_size(void);
uint32_t ps5_clock_wait_storage_alignment(void);
/* Resolver callback must yield reviewed callable exports for context lifetime.
 * Storage is address-stable/exclusive until close. No native resource is
 * acquired and no callback/report/output pointer is retained. All calls
 * synchronous, single-threaded and non-reentrant. Init resolves once and makes
 * no clock/wait. */
int32_t ps5_clock_wait_init(void *, uint32_t, ps5_clock_wait_lookup_fn,
                            struct ps5_clock_wait_report *);
int32_t ps5_clock_wait_now(void *, uint64_t *, struct ps5_clock_wait_report *);
/* One usleep call, no retry. Request must be 1..1000 microseconds. A native
 * scheduler can oversleep; this bounds the request, not measured elapsed time.
 */
int32_t ps5_clock_wait_slice(void *, uint32_t, struct ps5_clock_wait_report *);
int32_t ps5_clock_wait_close(void *, struct ps5_clock_wait_report *);
#endif
