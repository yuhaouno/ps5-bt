/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_OPEN_COMPARE_H
#define PS5_BT_OPEN_COMPARE_H
#include "posix_bridge.h"

#define PS5_APP_OPEN_COMPARE_ABI UINT32_C(0x00010000)
#define PS5_APP_OPEN_COMPARE_REJECT_TYPE UINT32_C(1)
#define PS5_APP_OPEN_COMPARE_REJECT_PARENT_OWNER UINT32_C(2)
#define PS5_APP_OPEN_COMPARE_REJECT_PARENT_WRITE UINT32_C(4)
#define PS5_APP_OPEN_COMPARE_MATCH_DEVICE UINT32_C(1)
#define PS5_APP_OPEN_COMPARE_MATCH_INODE UINT32_C(2)
#define PS5_APP_OPEN_COMPARE_MATCH_MODE UINT32_C(4)
#define PS5_APP_OPEN_COMPARE_MATCH_UID UINT32_C(8)
#define PS5_APP_OPEN_COMPARE_MATCH_GID UINT32_C(16)

/* Primitive observations under the compiled SDK layout, never native ABI,
 * mount, permission or store acceptance. UIDs/GIDs, paths and raw stat bytes
 * remain local. Per-case successful open returns can retain descriptors.
 */
struct ps5_app_open_compare_report {
  uint32_t abi_version, stage;
  int32_t status, raw_errno, raw_return;
  uint32_t failed_stage, operations, attempted_mask;
  uint32_t opened_mask, closed_mask, cleanup_failed_mask;
  int32_t cleanup_errno, cleanup_raw_return;
  uint32_t prior_cleanup_mask, observed_mask;
  uint32_t root_reject, relative_reject, absolute_reject;
  int32_t relative_open_raw, relative_open_errno;
  int32_t relative_stat_raw, relative_stat_errno;
  int32_t absolute_open_raw, absolute_open_errno;
  int32_t absolute_stat_raw, absolute_stat_errno;
  uint32_t data_match_mask, relative_same_device, absolute_same_device,
      reserved;
};

/* Requires serialized matching initialized imports/current-thread errno through
 * cleanup. Fixed root/data paths only, seven normal calls at most, no retries.
 * Root acquisition/stat errors stop; each later case retains its own error and
 * may proceed to the independent case. Failed opens skip their stat. Stop
 * always cancels further calls even when an earlier error remains primary. At
 * most three reverse consume-once closes; uncertainty latches refusal of later
 * calls.
 */
int32_t ps5_app_open_compare(ps5_app_cancel_fn,
                             struct ps5_app_open_compare_report *);
#endif
