/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_RAW_OPENAT_H
#define PS5_BT_RAW_OPENAT_H
#include "posix_bridge.h"
#define PS5_APP_RAW_OPENAT_ABI UINT32_C(0x00010000)
/* Pinned SDK libc/syscalls.c:1901 and sys/syscall.h:420; runtime compatibility
 * is the experiment's hypothesis, not demonstrated firmware support. */
#define PS5_APP_SOURCE_SYS_OPENAT 499
#define PS5_APP_RAW_OPENAT_REJECT_TYPE UINT32_C(1)
#define PS5_APP_RAW_OPENAT_REJECT_PARENT_OWNER UINT32_C(2)
#define PS5_APP_RAW_OPENAT_REJECT_PARENT_WRITE UINT32_C(4)
#define PS5_APP_RAW_OPENAT_MATCH_DEVICE UINT32_C(1)
#define PS5_APP_RAW_OPENAT_MATCH_INODE UINT32_C(2)
#define PS5_APP_RAW_OPENAT_MATCH_MODE UINT32_C(4)
#define PS5_APP_RAW_OPENAT_MATCH_UID UINT32_C(8)
#define PS5_APP_RAW_OPENAT_MATCH_GID UINT32_C(16)

struct ps5_app_raw_openat_report {
  uint32_t abi_version, stage;
  int32_t status, raw_errno;
  uint32_t raw_return_lo, raw_return_hi, failed_stage, operations,
      attempted_mask;
  uint32_t opened_mask, closed_mask, cleanup_failed_mask;
  int32_t cleanup_errno, cleanup_raw_return;
  uint32_t prior_cleanup_mask, observed_mask;
  uint32_t root_reject, imported_reject, checked_reject;
  int32_t imported_open_raw, imported_open_errno;
  int32_t imported_stat_raw, imported_stat_errno;
  uint32_t checked_open_lo, checked_open_hi, checked_open_kind;
  int32_t checked_stat_raw, checked_stat_errno;
  uint32_t data_match_mask, imported_same_device, checked_same_device;
  uint32_t raw_ownership_unknown, prior_raw_ownership_unknown, reserved;
};
/* Matching initialized current-thread POSIX imports and checked __crt_syscall
 * required throughout. Fixed root/data read-only calls only; no syscall-origin
 * mutation/custom syscall instruction. Normal calls <=7, reverse closes <=3.
 * Checked kind:0 unattempted,1 representable owned fd,2 negative raw error,
 * 3 positive unrepresentable result,4 held-fd alias. Kind3/4 or an imported
 * held-fd alias stops, closes only proven ownership,
 * and latches refusal; it never truncates or claims cleanup of an unknown fd.
 * Predicate/equality observations never accept native ABI or protected storage.
 */
int32_t ps5_app_raw_openat(ps5_app_cancel_fn,
                           struct ps5_app_raw_openat_report *);
#endif
