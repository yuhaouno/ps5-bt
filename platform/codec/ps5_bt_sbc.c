/* SPDX-License-Identifier: GPL-3.0-or-later */

#include "ps5_bt_sbc.h"

#include <limits.h>
#include <stddef.h>
#include <stdint.h>

#include "sbc/sbc.h"

#if !defined(__BYTE_ORDER__) || __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error "the initial PS5-BT SBC wrapper supports little-endian PCM only"
#endif

#define PS5_BT_SBC_MAGIC UINT32_C(0x53424331)
#define PS5_BT_SBC_PCM_SAMPLES UINT32_C(256)
#define PS5_BT_SBC_PCM_BYTES UINT32_C(512)
#define PS5_BT_SBC_MAX_FRAME_BYTES UINT32_C(119)

struct ps5_bt_sbc_state {
  uint32_t magic;
  uint32_t poisoned;
  sbc_t codec;
};

static int checked_u32(size_t value, uint32_t *out) {
  if (value > UINT32_MAX)
    return 0;
  *out = (uint32_t)value;
  return 1;
}

static size_t align_up(size_t value, size_t alignment) {
  return (value + alignment - 1) & ~(alignment - 1);
}

static size_t private_offset(void) {
  return align_up(sizeof(struct ps5_bt_sbc_state), sbc_get_static_alignment());
}

static int storage_requirements(size_t *size, size_t *alignment) {
  size_t upstream_alignment = sbc_get_static_alignment();
  size_t upstream_size = sbc_get_static_size();
  size_t state_alignment = _Alignof(struct ps5_bt_sbc_state);
  size_t offset;

  if (upstream_alignment == 0 ||
      (upstream_alignment & (upstream_alignment - 1)) != 0)
    return 0;
  if (sizeof(struct ps5_bt_sbc_state) > SIZE_MAX - upstream_alignment + 1)
    return 0;
  offset = private_offset();
  if (upstream_size > SIZE_MAX - offset)
    return 0;

  *size = offset + upstream_size;
  *alignment = upstream_alignment > state_alignment ? upstream_alignment
                                                    : state_alignment;
  return 1;
}

static int valid_storage(void *storage, uint32_t storage_size,
                         struct ps5_bt_sbc_state **out_state,
                         void **out_private, size_t *out_private_size) {
  size_t needed;
  size_t alignment;
  size_t offset;

  if (!storage || !storage_requirements(&needed, &alignment) ||
      storage_size < needed || ((uintptr_t)storage & (alignment - 1)) != 0)
    return 0;

  offset = private_offset();
  *out_state = storage;
  *out_private = (uint8_t *)storage + offset;
  *out_private_size = needed - offset;
  return 1;
}

static int range_end(const void *ptr, size_t len, uintptr_t *start,
                     uintptr_t *end) {
  *start = (uintptr_t)ptr;
  if (len > UINTPTR_MAX - *start)
    return 0;
  *end = *start + len;
  return 1;
}

static int ranges_overlap(const void *left, size_t left_len, const void *right,
                          size_t right_len) {
  uintptr_t left_start;
  uintptr_t left_end;
  uintptr_t right_start;
  uintptr_t right_end;

  if (!range_end(left, left_len, &left_start, &left_end) ||
      !range_end(right, right_len, &right_start, &right_end))
    return 1;
  return left_start < right_end && right_start < left_end;
}

uint32_t ps5_bt_sbc_storage_size(void) {
  size_t size;
  size_t alignment;
  uint32_t result;

  if (!storage_requirements(&size, &alignment) || !checked_u32(size, &result))
    return 0;
  return result;
}

uint32_t ps5_bt_sbc_storage_alignment(void) {
  size_t size;
  size_t alignment;
  uint32_t result;

  if (!storage_requirements(&size, &alignment) ||
      !checked_u32(alignment, &result))
    return 0;
  return result;
}

int32_t ps5_bt_sbc_init(void *storage, uint32_t storage_size,
                        uint32_t sample_rate_hz, uint32_t channel_mode,
                        uint32_t blocks, uint32_t subbands, uint32_t allocation,
                        uint32_t bitpool) {
  struct ps5_bt_sbc_state *state;
  void *private_storage;
  size_t private_size;
  size_t expected_frame_length;
  int result;

  if (!valid_storage(storage, storage_size, &state, &private_storage,
                     &private_size))
    return PS5_BT_SBC_INVALID;
  if ((sample_rate_hz != 16000 && sample_rate_hz != 32000 &&
       sample_rate_hz != 44100 && sample_rate_hz != 48000) ||
      channel_mode > PS5_BT_SBC_MODE_JOINT_STEREO ||
      (blocks != 4 && blocks != 8 && blocks != 12 && blocks != 16) ||
      (subbands != 4 && subbands != 8) || allocation > 1 || bitpool < 2 ||
      bitpool > 250)
    return PS5_BT_SBC_INVALID;
  if (sample_rate_hz != 48000 || blocks != 16 || subbands != 8 ||
      allocation != PS5_BT_SBC_ALLOCATION_LOUDNESS || bitpool > 53)
    return PS5_BT_SBC_UNSUPPORTED;

  state->magic = 0;
  state->poisoned = 0;
  result = sbc_init_static(&state->codec, 0, private_storage, private_size);
  if (result < 0)
    return result;

  state->codec.frequency = SBC_FREQ_48000;
  state->codec.mode = channel_mode == PS5_BT_SBC_MODE_STEREO
                          ? SBC_MODE_STEREO
                          : SBC_MODE_JOINT_STEREO;
  state->codec.blocks = SBC_BLK_16;
  state->codec.subbands = SBC_SB_8;
  state->codec.allocation = SBC_AM_LOUDNESS;
  state->codec.bitpool = (uint8_t)bitpool;
  state->codec.endian = SBC_LE;
  expected_frame_length =
      (channel_mode == PS5_BT_SBC_MODE_STEREO ? 12U : 13U) + 2U * bitpool;
  if (sbc_get_codesize(&state->codec) != PS5_BT_SBC_PCM_BYTES ||
      sbc_get_frame_length(&state->codec) != expected_frame_length ||
      expected_frame_length > PS5_BT_SBC_MAX_FRAME_BYTES) {
    sbc_finish_static(&state->codec);
    return PS5_BT_SBC_INTEGRITY_LOST;
  }

  state->magic = PS5_BT_SBC_MAGIC;
  return PS5_BT_SBC_OK;
}

int32_t ps5_bt_sbc_encode(void *storage, uint32_t storage_size,
                          const int16_t *pcm, uint32_t pcm_samples,
                          uint8_t *output, uint32_t output_capacity,
                          uint32_t *out_pcm_samples, uint32_t *out_bytes) {
  struct ps5_bt_sbc_state *state;
  void *private_storage;
  size_t private_size;
  size_t expected_frame_length;
  ssize_t written = 0;
  ssize_t consumed;

  if (out_pcm_samples)
    *out_pcm_samples = 0;
  if (out_bytes)
    *out_bytes = 0;
  if (!out_pcm_samples || !out_bytes || !pcm || !output ||
      pcm_samples != PS5_BT_SBC_PCM_SAMPLES ||
      !valid_storage(storage, storage_size, &state, &private_storage,
                     &private_size) ||
      state->magic != PS5_BT_SBC_MAGIC || state->poisoned)
    return PS5_BT_SBC_INVALID;

  expected_frame_length = sbc_get_frame_length(&state->codec);
  if (expected_frame_length > output_capacity)
    return PS5_BT_SBC_BUFFER_TOO_SMALL;
  if (ranges_overlap(storage, storage_size, pcm, PS5_BT_SBC_PCM_BYTES) ||
      ranges_overlap(storage, storage_size, output, output_capacity) ||
      ranges_overlap(pcm, PS5_BT_SBC_PCM_BYTES, output, output_capacity))
    return PS5_BT_SBC_INVALID;

  consumed = sbc_encode(&state->codec, pcm, PS5_BT_SBC_PCM_BYTES, output,
                        output_capacity, &written);
  if (consumed < (ssize_t)INT32_MIN) {
    state->poisoned = 1;
    return PS5_BT_SBC_INTEGRITY_LOST;
  }
  if (consumed < 0)
    return (int32_t)consumed;
  if (consumed != PS5_BT_SBC_PCM_BYTES || written < 0 ||
      (size_t)written != expected_frame_length) {
    state->poisoned = 1;
    return PS5_BT_SBC_INTEGRITY_LOST;
  }

  *out_pcm_samples = PS5_BT_SBC_PCM_SAMPLES;
  *out_bytes = (uint32_t)written;
  return PS5_BT_SBC_OK;
}

int32_t ps5_bt_sbc_finish(void *storage, uint32_t storage_size) {
  struct ps5_bt_sbc_state *state;
  void *private_storage;
  size_t private_size;

  if (!valid_storage(storage, storage_size, &state, &private_storage,
                     &private_size) ||
      state->magic != PS5_BT_SBC_MAGIC)
    return PS5_BT_SBC_INVALID;

  state->magic = 0;
  state->poisoned = 0;
  sbc_finish_static(&state->codec);
  return PS5_BT_SBC_OK;
}
