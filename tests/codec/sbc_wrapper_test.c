/* SPDX-License-Identifier: GPL-3.0-or-later */

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "ps5_bt_sbc.h"

#define TEST_STORAGE_CAPACITY 8192U

struct aligned_storage {
  _Alignas(64) uint8_t bytes[TEST_STORAGE_CAPACITY];
};

static void fill_pcm(int16_t pcm[256], uint32_t salt) {
  uint32_t i;

  for (i = 0; i < 256; ++i)
    pcm[i] =
        (int16_t)((i * UINT32_C(257) + salt * UINT32_C(811)) ^ (i << (i & 3)));
}

static void init_encoder(struct aligned_storage *storage, uint32_t mode,
                         uint32_t bitpool) {
  memset(storage->bytes, 0, sizeof(storage->bytes));
  assert(ps5_bt_sbc_init(storage->bytes, ps5_bt_sbc_storage_size(), 48000, mode,
                         16, 8, PS5_BT_SBC_ALLOCATION_LOUDNESS,
                         bitpool) == PS5_BT_SBC_OK);
}

static void test_queries_and_invalid_init(void) {
  struct aligned_storage storage;
  uint32_t size = ps5_bt_sbc_storage_size();
  uint32_t alignment = ps5_bt_sbc_storage_alignment();

  assert(size > 0 && size <= TEST_STORAGE_CAPACITY);
  assert(alignment >= 16 && (alignment & (alignment - 1)) == 0);
  assert(ps5_bt_sbc_init(NULL, size, 48000, PS5_BT_SBC_MODE_STEREO, 16, 8, 0,
                         32) == PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes, size - 1, 48000, PS5_BT_SBC_MODE_STEREO,
                         16, 8, 0, 32) == PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes + 1, size, 48000, PS5_BT_SBC_MODE_STEREO,
                         16, 8, 0, 32) == PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes, size, 12345, PS5_BT_SBC_MODE_STEREO, 16,
                         8, 0, 32) == PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes, size, 48000, 9, 16, 8, 0, 32) ==
         PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes, size, 48000, PS5_BT_SBC_MODE_STEREO, 16,
                         8, 0, 1) == PS5_BT_SBC_INVALID);
  assert(ps5_bt_sbc_init(storage.bytes, size, 44100, PS5_BT_SBC_MODE_STEREO, 16,
                         8, 0, 32) == PS5_BT_SBC_UNSUPPORTED);
  assert(ps5_bt_sbc_init(storage.bytes, size, 48000, PS5_BT_SBC_MODE_STEREO, 12,
                         8, 0, 32) == PS5_BT_SBC_UNSUPPORTED);
  assert(ps5_bt_sbc_init(storage.bytes, size, 48000, PS5_BT_SBC_MODE_STEREO, 16,
                         8, 0, 54) == PS5_BT_SBC_UNSUPPORTED);
}

static void test_lengths_bounds_and_cleanup(void) {
  static const uint32_t bitpools[] = {2, 32, 53};
  struct aligned_storage storage;
  int16_t pcm[256];
  uint32_t mode;
  uint32_t index;

  fill_pcm(pcm, 1);
  for (mode = PS5_BT_SBC_MODE_STEREO; mode <= PS5_BT_SBC_MODE_JOINT_STEREO;
       ++mode) {
    for (index = 0; index < sizeof(bitpools) / sizeof(bitpools[0]); ++index) {
      uint8_t guarded[121];
      uint32_t consumed = UINT32_MAX;
      uint32_t written = UINT32_MAX;
      uint32_t expected =
          (mode == PS5_BT_SBC_MODE_STEREO ? 12U : 13U) + 2U * bitpools[index];

      memset(guarded, 0xa5, sizeof(guarded));
      init_encoder(&storage, mode, bitpools[index]);
      assert(ps5_bt_sbc_encode(storage.bytes, ps5_bt_sbc_storage_size(), pcm,
                               256, guarded + 1, 119, &consumed,
                               &written) == PS5_BT_SBC_OK);
      assert(consumed == 256 && written == expected);
      assert(guarded[0] == 0xa5 && guarded[120] == 0xa5);
      assert(guarded[1] == 0x9c);
      assert(guarded[3] == bitpools[index]);
      while (written < 119)
        assert(guarded[1 + written++] == 0xa5);
      assert(ps5_bt_sbc_finish(storage.bytes, ps5_bt_sbc_storage_size()) ==
             PS5_BT_SBC_OK);
      assert(ps5_bt_sbc_finish(storage.bytes, ps5_bt_sbc_storage_size()) ==
             PS5_BT_SBC_INVALID);
    }
  }
}

static void test_rejections_do_not_advance(void) {
  struct aligned_storage rejected;
  struct aligned_storage fresh;
  int16_t pcm[256];
  uint8_t rejected_frame[119];
  uint8_t fresh_frame[119];
  uint32_t consumed;
  uint32_t written;
  uint32_t fresh_consumed;
  uint32_t fresh_written;

  fill_pcm(pcm, 2);
  init_encoder(&rejected, PS5_BT_SBC_MODE_JOINT_STEREO, 32);
  init_encoder(&fresh, PS5_BT_SBC_MODE_JOINT_STEREO, 32);

  consumed = written = UINT32_MAX;
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           rejected_frame, 76, &consumed,
                           &written) == PS5_BT_SBC_BUFFER_TOO_SMALL);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 255,
                           rejected_frame, sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 257,
                           rejected_frame, sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), NULL, 256,
                           rejected_frame, sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           NULL, sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size() - 1, pcm,
                           256, rejected_frame, sizeof(rejected_frame),
                           &consumed, &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           rejected.bytes, 119, &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(),
                           (const int16_t *)rejected.bytes, 256, rejected_frame,
                           sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_INVALID);
  assert(consumed == 0 && written == 0);
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           rejected_frame, sizeof(rejected_frame), NULL,
                           &written) == PS5_BT_SBC_INVALID);
  assert(written == 0);
  consumed = UINT32_MAX;
  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           rejected_frame, sizeof(rejected_frame), &consumed,
                           NULL) == PS5_BT_SBC_INVALID);
  assert(consumed == 0);

  assert(ps5_bt_sbc_encode(rejected.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           rejected_frame, sizeof(rejected_frame), &consumed,
                           &written) == PS5_BT_SBC_OK);
  assert(ps5_bt_sbc_encode(fresh.bytes, ps5_bt_sbc_storage_size(), pcm, 256,
                           fresh_frame, sizeof(fresh_frame), &fresh_consumed,
                           &fresh_written) == PS5_BT_SBC_OK);
  assert(consumed == fresh_consumed && written == fresh_written);
  assert(memcmp(rejected_frame, fresh_frame, written) == 0);
  assert(ps5_bt_sbc_finish(rejected.bytes, ps5_bt_sbc_storage_size()) == 0);
  assert(ps5_bt_sbc_finish(fresh.bytes, ps5_bt_sbc_storage_size()) == 0);
}

int main(void) {
  test_queries_and_invalid_init();
  test_lengths_bounds_and_cleanup();
  test_rejections_do_not_advance();
  puts("SBC wrapper bounds and lifecycle tests passed.");
  return 0;
}
