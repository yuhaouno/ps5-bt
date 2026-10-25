/* SPDX-License-Identifier: GPL-3.0-or-later */

#include "ps5_bt_sbc_selftest.h"

#include <stddef.h>
#include <stdint.h>

#include "ps5_bt_sbc.h"

#define SELFTEST_CYCLES UINT32_C(2)
#define SELFTEST_FRAMES_PER_CYCLE UINT32_C(2)
#define SELFTEST_PCM_SAMPLES UINT32_C(256)
#define SELFTEST_FRAME_BYTES UINT32_C(77)
#define SELFTEST_MAX_FRAME_BYTES UINT32_C(119)
#define FNV1A_OFFSET UINT32_C(2166136261)
#define FNV1A_PRIME UINT32_C(16777619)

static int range_end(const void *pointer, uint32_t length, uintptr_t *start,
                     uintptr_t *end) {
  *start = (uintptr_t)pointer;
  if ((uintptr_t)length > UINTPTR_MAX - *start)
    return 0;
  *end = *start + (uintptr_t)length;
  return 1;
}

static int ranges_overlap(const void *left, uint32_t left_length,
                          const void *right, uint32_t right_length) {
  uintptr_t left_start;
  uintptr_t left_end;
  uintptr_t right_start;
  uintptr_t right_end;

  if (!range_end(left, left_length, &left_start, &left_end) ||
      !range_end(right, right_length, &right_start, &right_end))
    return 1;
  return left_start < right_end && right_start < left_end;
}

static void clear_report(struct ps5_bt_sbc_selftest_report *report) {
  uint8_t *bytes = (uint8_t *)report;
  uint32_t index;

  for (index = 0; index < (uint32_t)sizeof(*report); ++index)
    bytes[index] = 0;
  report->schema = PS5_BT_SBC_SELFTEST_SCHEMA;
}

static void fill_pcm(int16_t pcm[SELFTEST_PCM_SAMPLES], uint32_t frame) {
  uint32_t index;

  for (index = 0; index < SELFTEST_PCM_SAMPLES; ++index) {
    uint32_t value =
        (index * UINT32_C(257) + frame * UINT32_C(811)) & UINT32_C(0xffff);
    pcm[index] = (int16_t)((int32_t)value - INT32_C(32768));
  }
}

static uint32_t digest_bytes(uint32_t digest, const uint8_t *bytes,
                             uint32_t length) {
  uint32_t index;

  for (index = 0; index < length; ++index) {
    digest ^= bytes[index];
    digest *= FNV1A_PRIME;
  }
  return digest;
}

static int bytes_equal(const uint8_t *left, const uint8_t *right,
                       uint32_t length) {
  uint32_t index;

  for (index = 0; index < length; ++index) {
    if (left[index] != right[index])
      return 0;
  }
  return 1;
}

static int32_t fail(struct ps5_bt_sbc_selftest_report *report, int32_t result,
                    uint32_t stage, int32_t codec_status) {
  report->result = result;
  report->stage = stage;
  report->codec_status = codec_status;
  return result;
}

int32_t ps5_bt_sbc_selftest_run(void *storage, uint32_t storage_capacity,
                                struct ps5_bt_sbc_selftest_report *report) {
  _Alignas(16) int16_t pcm[SELFTEST_PCM_SAMPLES];
  uint8_t output[SELFTEST_MAX_FRAME_BYTES];
  uint8_t reference[SELFTEST_FRAMES_PER_CYCLE][SELFTEST_MAX_FRAME_BYTES];
  uint32_t reference_lengths[SELFTEST_FRAMES_PER_CYCLE] = {0, 0};
  uint32_t required_storage;
  uint32_t required_alignment;
  uint32_t digest = FNV1A_OFFSET;
  uint32_t cycle;

  if (!report || ((uintptr_t)report &
                  (_Alignof(struct ps5_bt_sbc_selftest_report) - 1)) != 0)
    return PS5_BT_SBC_SELFTEST_INVALID;
  if (storage && ranges_overlap(storage, storage_capacity, report,
                                (uint32_t)sizeof(*report)))
    return PS5_BT_SBC_SELFTEST_INVALID;

  clear_report(report);
  report->stage = PS5_BT_SBC_SELFTEST_STAGE_QUERY;
  required_storage = ps5_bt_sbc_storage_size();
  required_alignment = ps5_bt_sbc_storage_alignment();
  report->required_storage = required_storage;
  report->required_alignment = required_alignment;
  if (!storage || required_storage == 0 || required_alignment == 0 ||
      (required_alignment & (required_alignment - 1)) != 0)
    return fail(report, PS5_BT_SBC_SELFTEST_QUERY_FAILED,
                PS5_BT_SBC_SELFTEST_STAGE_QUERY, PS5_BT_SBC_INVALID);
  if (storage_capacity < required_storage ||
      ((uintptr_t)storage & (required_alignment - 1)) != 0)
    return fail(report, PS5_BT_SBC_SELFTEST_INVALID,
                PS5_BT_SBC_SELFTEST_STAGE_QUERY, PS5_BT_SBC_INVALID);

  for (cycle = 0; cycle < SELFTEST_CYCLES; ++cycle) {
    uint32_t frame;
    int32_t codec_status;

    report->stage = PS5_BT_SBC_SELFTEST_STAGE_INIT;
    codec_status =
        ps5_bt_sbc_init(storage, required_storage, UINT32_C(48000),
                        PS5_BT_SBC_MODE_JOINT_STEREO, UINT32_C(16), UINT32_C(8),
                        PS5_BT_SBC_ALLOCATION_LOUDNESS, UINT32_C(32));
    if (codec_status != PS5_BT_SBC_OK)
      return fail(report, PS5_BT_SBC_SELFTEST_INIT_FAILED,
                  PS5_BT_SBC_SELFTEST_STAGE_INIT, codec_status);

    for (frame = 0; frame < SELFTEST_FRAMES_PER_CYCLE; ++frame) {
      uint32_t consumed = 0;
      uint32_t written = 0;
      uint32_t index;

      fill_pcm(pcm, frame);
      report->stage = PS5_BT_SBC_SELFTEST_STAGE_ENCODE;
      codec_status = ps5_bt_sbc_encode(
          storage, required_storage, pcm, SELFTEST_PCM_SAMPLES, output,
          SELFTEST_MAX_FRAME_BYTES, &consumed, &written);
      if (codec_status != PS5_BT_SBC_OK) {
        report->cleanup_status = ps5_bt_sbc_finish(storage, required_storage);
        return fail(report, PS5_BT_SBC_SELFTEST_ENCODE_FAILED,
                    PS5_BT_SBC_SELFTEST_STAGE_ENCODE, codec_status);
      }
      report->frame_bytes = written;
      if (report->frames_encoded == 0)
        report->first_frame_prefix =
            ((uint32_t)output[0] << 24) | ((uint32_t)output[1] << 16) |
            ((uint32_t)output[2] << 8) | (uint32_t)output[3];
      if (consumed != SELFTEST_PCM_SAMPLES || written != SELFTEST_FRAME_BYTES ||
          output[0] != UINT8_C(0x9c) || output[2] != UINT8_C(32)) {
        report->cleanup_status = ps5_bt_sbc_finish(storage, required_storage);
        return fail(report, PS5_BT_SBC_SELFTEST_VERIFY_FAILED,
                    PS5_BT_SBC_SELFTEST_STAGE_VERIFY,
                    PS5_BT_SBC_INTEGRITY_LOST);
      }

      report->frames_encoded += 1;
      report->pcm_samples_consumed += consumed;
      if (cycle == 0) {
        reference_lengths[frame] = written;
        for (index = 0; index < written; ++index)
          reference[frame][index] = output[index];
      } else if (reference_lengths[frame] != written ||
                 !bytes_equal(reference[frame], output, written)) {
        report->cleanup_status = ps5_bt_sbc_finish(storage, required_storage);
        return fail(report, PS5_BT_SBC_SELFTEST_VERIFY_FAILED,
                    PS5_BT_SBC_SELFTEST_STAGE_VERIFY,
                    PS5_BT_SBC_INTEGRITY_LOST);
      }
      digest = digest_bytes(digest, output, written);
    }

    report->stage = PS5_BT_SBC_SELFTEST_STAGE_FINISH;
    codec_status = ps5_bt_sbc_finish(storage, required_storage);
    if (codec_status != PS5_BT_SBC_OK)
      return fail(report, PS5_BT_SBC_SELFTEST_FINISH_FAILED,
                  PS5_BT_SBC_SELFTEST_STAGE_FINISH, codec_status);
    report->cycles_completed += 1;
  }

  report->frame_digest = digest;
  if (report->first_frame_prefix != PS5_BT_SBC_SELFTEST_EXPECTED_PREFIX ||
      report->frame_digest != PS5_BT_SBC_SELFTEST_EXPECTED_DIGEST)
    return fail(report, PS5_BT_SBC_SELFTEST_VERIFY_FAILED,
                PS5_BT_SBC_SELFTEST_STAGE_VERIFY, PS5_BT_SBC_INTEGRITY_LOST);
  report->stage = PS5_BT_SBC_SELFTEST_STAGE_COMPLETE;
  report->result = PS5_BT_SBC_SELFTEST_OK;
  report->codec_status = PS5_BT_SBC_OK;
  return PS5_BT_SBC_SELFTEST_OK;
}

static size_t append_text(char *buffer, size_t used, const char *text) {
  while (*text)
    buffer[used++] = *text++;
  return used;
}

static size_t append_u32(char *buffer, size_t used, uint32_t value) {
  char digits[10];
  size_t count = 0;
  do {
    digits[count++] = (char)('0' + value % 10u);
    value /= 10u;
  } while (value != 0);
  while (count != 0)
    buffer[used++] = digits[--count];
  return used;
}

static size_t append_i32(char *buffer, size_t used, int32_t value) {
  uint32_t magnitude;
  if (value < 0) {
    buffer[used++] = '-';
    magnitude = 0u - (uint32_t)value;
  } else {
    magnitude = (uint32_t)value;
  }
  return append_u32(buffer, used, magnitude);
}

static size_t append_hex32(char *buffer, size_t used, uint32_t value) {
  static const char digits[] = "0123456789abcdef";
  buffer[used++] = '0';
  buffer[used++] = 'x';
  for (uint32_t shift = 32; shift != 0; shift -= 4)
    buffer[used++] = digits[(value >> (shift - 4)) & 15u];
  return used;
}

int32_t ps5_bt_sbc_selftest_encode_report(
    const struct ps5_bt_sbc_selftest_report *report, char *data,
    size_t capacity, size_t *out_length) {
  char local[PS5_BT_SBC_SELFTEST_REPORT_CAPACITY];
  size_t used = 0;

  if (!report || !out_length || (data == NULL && capacity != 0) ||
      report->schema != PS5_BT_SBC_SELFTEST_SCHEMA)
    return PS5_BT_SBC_SELFTEST_INVALID;
  used = append_text(local, used, "ps5_bt_sbc_selftest_v1 result=");
  used = append_i32(local, used, report->result);
  used = append_text(local, used, " stage=");
  used = append_u32(local, used, report->stage);
  used = append_text(local, used, " codec=");
  used = append_i32(local, used, report->codec_status);
  used = append_text(local, used, " cleanup=");
  used = append_i32(local, used, report->cleanup_status);
  used = append_text(local, used, " storage=");
  used = append_u32(local, used, report->required_storage);
  used = append_text(local, used, " alignment=");
  used = append_u32(local, used, report->required_alignment);
  used = append_text(local, used, " cycles=");
  used = append_u32(local, used, report->cycles_completed);
  used = append_text(local, used, " frames=");
  used = append_u32(local, used, report->frames_encoded);
  used = append_text(local, used, " samples=");
  used = append_u32(local, used, report->pcm_samples_consumed);
  used = append_text(local, used, " frame_bytes=");
  used = append_u32(local, used, report->frame_bytes);
  used = append_text(local, used, " prefix=");
  used = append_hex32(local, used, report->first_frame_prefix);
  used = append_text(local, used, " digest=");
  used = append_hex32(local, used, report->frame_digest);
  local[used++] = '\n';

  *out_length = used;
  if (data == NULL && capacity == 0)
    return PS5_BT_SBC_SELFTEST_OK;
  if (capacity < used)
    return PS5_BT_SBC_SELFTEST_INVALID;
  for (size_t index = 0; index < used; ++index)
    data[index] = local[index];
  return PS5_BT_SBC_SELFTEST_OK;
}
