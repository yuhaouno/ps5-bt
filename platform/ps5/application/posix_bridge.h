/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_APP_POSIX_BRIDGE_H
#define PS5_BT_APP_POSIX_BRIDGE_H
#include <stddef.h>
#include <stdint.h>
#define PS5_APP_POSIX_ABI UINT32_C(0x00010000)
struct ps5_app_posix_report {
  uint32_t abi_version, stage;
  int32_t status, raw_errno;
  int64_t raw_lookup, raw_return;
  uint32_t resolved, cleanup_flags;
  int32_t cleanup_errno;
  uint32_t reserved;
};
typedef int64_t (*ps5_app_lookup_fn)(const char *, void **);
typedef int32_t (*ps5_app_cancel_fn)(void);
/* Current-thread exports remain valid until all factory resources are consumed.
 * Init resolves all functions before any native resource is opened. It acquires
 * no descriptor, never changes credentials and must not race/reenter calls. */
int32_t ps5_app_posix_init_cancel(ps5_app_lookup_fn, ps5_app_cancel_fn,
                                  struct ps5_app_posix_report *);
int32_t ps5_app_posix_init(ps5_app_lookup_fn, struct ps5_app_posix_report *);
int32_t ps5_app_posix_shutdown(struct ps5_app_posix_report *);
/* One /dev/random open/stat, at most16 progress-making reads, one close.
 * Missing/unseeded/error/uncertain-close fails closed; no token is logged. */
int32_t ps5_app_random_token_cancel(uint8_t output[32], ps5_app_cancel_fn,
                                    struct ps5_app_posix_report *);
int32_t ps5_app_random_token(uint8_t output[32], struct ps5_app_posix_report *);
void ps5_app_scrub(void *, size_t);
#endif
