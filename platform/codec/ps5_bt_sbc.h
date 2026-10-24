/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_SBC_H
#define PS5_BT_SBC_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PS5_BT_SBC_OK INT32_C(0)
#define PS5_BT_SBC_UNSUPPORTED (-INT32_C(1))
#define PS5_BT_SBC_INVALID (-INT32_C(2))
#define PS5_BT_SBC_BUFFER_TOO_SMALL (-INT32_C(4))
#define PS5_BT_SBC_INTEGRITY_LOST (-INT32_C(7))

#define PS5_BT_SBC_MODE_STEREO UINT32_C(0)
#define PS5_BT_SBC_MODE_JOINT_STEREO UINT32_C(1)
#define PS5_BT_SBC_ALLOCATION_LOUDNESS UINT32_C(0)

/*
 * Project-owned, experimental codec ABI. These are not upstream libsbc symbols.
 * Storage is caller-owned, must have the queried alignment and remains borrowed
 * from a successful init until the single successful finish. No call retains
 * PCM or output pointers. One encoder is mutable and not thread-safe/reentrant.
 */
uint32_t ps5_bt_sbc_storage_size(void);
uint32_t ps5_bt_sbc_storage_alignment(void);

int32_t ps5_bt_sbc_init(void *storage, uint32_t storage_size,
                        uint32_t sample_rate_hz, uint32_t channel_mode,
                        uint32_t blocks, uint32_t subbands, uint32_t allocation,
                        uint32_t bitpool);

int32_t ps5_bt_sbc_encode(void *storage, uint32_t storage_size,
                          const int16_t *pcm, uint32_t pcm_samples,
                          uint8_t *output, uint32_t output_capacity,
                          uint32_t *out_pcm_samples, uint32_t *out_bytes);

int32_t ps5_bt_sbc_finish(void *storage, uint32_t storage_size);

#ifdef __cplusplus
}
#endif

#endif /* PS5_BT_SBC_H */
