/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_NAMESPACE_PROBE_H
#define PS5_BT_NAMESPACE_PROBE_H
#include "posix_bridge.h"

#define PS5_APP_NAMESPACE_ABI UINT32_C(0x00010000)
#define PS5_APP_NAMESPACE_REJECT_TYPE UINT32_C(1)
#define PS5_APP_NAMESPACE_REJECT_PARENT_OWNER UINT32_C(2)
#define PS5_APP_NAMESPACE_REJECT_PARENT_WRITE UINT32_C(4)
#define PS5_APP_NAMESPACE_MATCH_DEVICE UINT32_C(1)
#define PS5_APP_NAMESPACE_MATCH_INODE UINT32_C(2)
#define PS5_APP_NAMESPACE_MATCH_MODE UINT32_C(4)
#define PS5_APP_NAMESPACE_MATCH_UID UINT32_C(8)
#define PS5_APP_NAMESPACE_MATCH_GID UINT32_C(16)

/* Primitive predicate/equality observations under the compiled SDK layout.
 * These are not native-layout, mount, permission or protected-store acceptance.
 * No UID/GID, path, native stat bytes or arbitrary input enters this report.
 */
struct ps5_app_namespace_report {
  uint32_t abi_version, stage;
  int32_t status, raw_errno, raw_return;
  uint32_t operations, opened_mask, closed_mask, cleanup_failed_mask;
  int32_t cleanup_errno, cleanup_raw_return;
  uint32_t prior_cleanup_mask, observed_mask;
  uint32_t root_fd_reject, root_path_reject, data_path_reject, data_fd_reject;
  uint32_t root_match_mask, data_match_mask, same_device;
};

/* Requires serialized, initialized matching POSIX imports/current-thread errno
 * for the complete call including cleanup. Only fixed / and data are read.
 * Seven normal calls, no retries, and up to two reverse consume-once closes.
 * Rejections/mismatches remain observations; the protected-store gate is never
 * cleared. Native error/cancellation stops normal work. An uncertain close
 * latches refusal of later calls; restarting a payload is not cleanup proof.
 */
int32_t ps5_app_namespace_probe(ps5_app_cancel_fn,
                                struct ps5_app_namespace_report *);
#endif
