/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SETUP_NAMESPACE_H
#define PS5_BT_SETUP_NAMESPACE_H
#include "posix_bridge.h"
#define PS5_APP_SETUP_ABI UINT32_C(0x00020000)
#define PS5_APP_SETUP_REJECT_TYPE UINT32_C(1)
#define PS5_APP_SETUP_REJECT_PARENT_OWNER UINT32_C(2)
#define PS5_APP_SETUP_REJECT_PARENT_WRITE UINT32_C(4)
#define PS5_APP_SETUP_REJECT_IDENTITY UINT32_C(8)
#define PS5_APP_SETUP_REJECT_PRIVATE_OWNER UINT32_C(16)
#define PS5_APP_SETUP_REJECT_PRIVATE_MODE UINT32_C(32)
/* Fixed /data/ps5-bt/control only; no caller-provided path or permission
 * values. */
struct ps5_app_setup_report {
  uint32_t abi_version, stage;
  int32_t status, raw_errno;
  int64_t raw_lookup;
  uint32_t created_mask, verified_mask, opened_mask, closed_mask;
  uint32_t cleanup_failed_mask;
  int32_t cleanup_errno;
  uint32_t operations;
  int32_t raw_return;
  int32_t cleanup_raw_return;
  uint32_t prior_cleanup_mask;
  /* Predicate results under the compiled SDK layout, not native stat/UID
   * attestation. Zero unless the primary outcome is policy rejection (-4).
   * No extra native observation or permission change is implied. */
  uint32_t validation_failed_mask, reserved;
};
/* Requires initialized matching POSIX imports and callable current-process
 * resolver. No reentry. Creates missing private components only. Existing paths
 * are never chmod/chown/unlinked. Owned descriptors close once even on failure;
 * created directories persist, with their creation explicitly reported. */
int32_t ps5_app_setup_namespace(ps5_app_lookup_fn, ps5_app_cancel_fn,
                                struct ps5_app_setup_report *);
#endif
