/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SBC_SELFTEST_H
#define PS5_BT_SBC_SELFTEST_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PS5_BT_SBC_SELFTEST_SCHEMA UINT32_C(1)
#define PS5_BT_SBC_SELFTEST_EXPECTED_PREFIX UINT32_C(0x9cfd206f)
#define PS5_BT_SBC_SELFTEST_EXPECTED_DIGEST UINT32_C(0x14e0ec79)

#define PS5_BT_SBC_SELFTEST_OK INT32_C(0)
#define PS5_BT_SBC_SELFTEST_INVALID (-INT32_C(2))
#define PS5_BT_SBC_SELFTEST_QUERY_FAILED (-INT32_C(20))
#define PS5_BT_SBC_SELFTEST_INIT_FAILED (-INT32_C(21))
#define PS5_BT_SBC_SELFTEST_ENCODE_FAILED (-INT32_C(22))
#define PS5_BT_SBC_SELFTEST_FINISH_FAILED (-INT32_C(23))
#define PS5_BT_SBC_SELFTEST_VERIFY_FAILED (-INT32_C(24))

#define PS5_BT_SBC_SELFTEST_STAGE_NONE UINT32_C(0)
#define PS5_BT_SBC_SELFTEST_STAGE_QUERY UINT32_C(1)
#define PS5_BT_SBC_SELFTEST_STAGE_INIT UINT32_C(2)
#define PS5_BT_SBC_SELFTEST_STAGE_ENCODE UINT32_C(3)
#define PS5_BT_SBC_SELFTEST_STAGE_FINISH UINT32_C(4)
#define PS5_BT_SBC_SELFTEST_STAGE_VERIFY UINT32_C(5)
#define PS5_BT_SBC_SELFTEST_STAGE_COMPLETE UINT32_C(6)

/*
 * Identity-free deterministic result for the generated-signal codec test.
 * The caller owns `storage` and `report`; they must be disjoint. The report
 * contains no PCM or encoded frame bytes. `codec_status` preserves the raw
 * wrapper result for the failed phase, and `cleanup_status` preserves a
 * bounded cleanup failure after an encode/verification failure.
 */
struct ps5_bt_sbc_selftest_report {
  uint32_t schema;
  int32_t result;
  uint32_t stage;
  int32_t codec_status;
  int32_t cleanup_status;
  uint32_t required_storage;
  uint32_t required_alignment;
  uint32_t cycles_completed;
  uint32_t frames_encoded;
  uint32_t pcm_samples_consumed;
  uint32_t frame_bytes;
  uint32_t first_frame_prefix;
  uint32_t frame_digest;
};

/*
 * Runs two fresh init/finish cycles. Each cycle encodes the same two generated
 * 48 kHz joint-stereo, 16-block, 8-subband, loudness, bitpool-32 frames. The
 * second cycle must be byte-identical to the first. There is no allocator,
 * clock, I/O, retry, transport, or retained pointer.
 */
int32_t ps5_bt_sbc_selftest_run(void *storage, uint32_t storage_capacity,
                                struct ps5_bt_sbc_selftest_report *report);

#ifdef __cplusplus
}
#endif

#endif /* PS5_BT_SBC_SELFTEST_H */
