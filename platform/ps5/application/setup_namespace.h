/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SETUP_NAMESPACE_H
#define PS5_BT_SETUP_NAMESPACE_H
#include "posix_bridge.h"
#define PS5_APP_SETUP_ABI UINT32_C(0x00010000)
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
};
/* Requires initialized matching POSIX imports and callable current-process
 * resolver. No reentry. Creates missing private components only. Existing paths
 * are never chmod/chown/unlinked. Owned descriptors close once even on failure;
 * created directories persist, with their creation explicitly reported. */
int32_t ps5_app_setup_namespace(ps5_app_lookup_fn, ps5_app_cancel_fn,
                                struct ps5_app_setup_report *);
#endif
