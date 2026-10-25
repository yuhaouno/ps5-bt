/* SPDX-License-Identifier: GPL-3.0-or-later */

#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "ps5_bt_sbc.h"
#include "ps5_bt_sbc_selftest.h"

#define TEST_STORAGE_CAPACITY UINT32_C(8192)

struct aligned_storage {
  _Alignas(64) uint8_t bytes[TEST_STORAGE_CAPACITY];
};

static int contains(const char *data, size_t length, const char *needle) {
  size_t needle_length = strlen(needle);
  if (needle_length > length)
    return 0;
  for (size_t offset = 0; offset + needle_length <= length; ++offset) {
    if (memcmp(data + offset, needle, needle_length) == 0)
      return 1;
  }
  return 0;
}

static void test_deterministic_repeated_lifecycle(void) {
  struct aligned_storage storage;
  struct ps5_bt_sbc_selftest_report first;
  struct ps5_bt_sbc_selftest_report second;
  int32_t result;

  result =
      ps5_bt_sbc_selftest_run(storage.bytes, sizeof(storage.bytes), &first);
  if (result != PS5_BT_SBC_SELFTEST_OK)
    fprintf(stderr,
            "self-test failed: result=%d stage=%u codec=%d cleanup=%d "
            "frames=%u bytes=%u prefix=%08x\n",
            result, first.stage, first.codec_status, first.cleanup_status,
            first.frames_encoded, first.frame_bytes, first.first_frame_prefix);
  assert(result == PS5_BT_SBC_SELFTEST_OK);
  assert(first.schema == PS5_BT_SBC_SELFTEST_SCHEMA);
  assert(first.result == PS5_BT_SBC_SELFTEST_OK);
  assert(first.stage == PS5_BT_SBC_SELFTEST_STAGE_COMPLETE);
  assert(first.codec_status == PS5_BT_SBC_OK);
  assert(first.cleanup_status == PS5_BT_SBC_OK);
  assert(first.required_storage > 0 &&
         first.required_storage <= sizeof(storage.bytes));
  assert(first.required_alignment > 0 && first.required_alignment <= 64);
  assert(first.cycles_completed == 2);
  assert(first.frames_encoded == 4);
  assert(first.pcm_samples_consumed == 1024);
  assert(first.frame_bytes == 77);
  assert((first.first_frame_prefix >> 24) == 0x9c);
  assert(((first.first_frame_prefix >> 8) & 0xff) == 32);
  assert(first.first_frame_prefix == PS5_BT_SBC_SELFTEST_EXPECTED_PREFIX);
  assert(first.frame_digest == PS5_BT_SBC_SELFTEST_EXPECTED_DIGEST);

  char report_line[PS5_BT_SBC_SELFTEST_REPORT_CAPACITY];
  size_t report_length = 0;
  assert(ps5_bt_sbc_selftest_encode_report(&first, NULL, 0,
                                            &report_length) ==
         PS5_BT_SBC_SELFTEST_OK);
  assert(report_length < sizeof report_line);
  memset(report_line, 0xa5, sizeof report_line);
  assert(ps5_bt_sbc_selftest_encode_report(&first, report_line, report_length,
                                            &report_length) ==
         PS5_BT_SBC_SELFTEST_OK);
  assert(contains(report_line, report_length, "result=0"));
  assert(contains(report_line, report_length, "prefix=0x9cfd206f"));
  assert(contains(report_line, report_length, "digest=0x14e0ec79"));
  char short_report[8];
  memset(short_report, 0xa5, sizeof short_report);
  assert(ps5_bt_sbc_selftest_encode_report(&first, short_report,
                                            sizeof short_report,
                                            &report_length) ==
         PS5_BT_SBC_SELFTEST_INVALID);
  for (size_t index = 0; index < sizeof short_report; ++index)
    assert((unsigned char)short_report[index] == 0xa5);
  printf("SBC self-test prefix=%08x digest=%08x\n", first.first_frame_prefix,
         first.frame_digest);

  assert(ps5_bt_sbc_selftest_run(storage.bytes, sizeof(storage.bytes),
                                 &second) == PS5_BT_SBC_SELFTEST_OK);
  assert(second.first_frame_prefix == first.first_frame_prefix);
  assert(second.frame_digest == first.frame_digest);
}

static void test_invalid_inputs_are_bounded(void) {
  struct aligned_storage storage;
  struct ps5_bt_sbc_selftest_report report;
  uint32_t required;

  assert(ps5_bt_sbc_selftest_run(storage.bytes, sizeof(storage.bytes), NULL) ==
         PS5_BT_SBC_SELFTEST_INVALID);
  assert(ps5_bt_sbc_selftest_run(NULL, 0, &report) ==
         PS5_BT_SBC_SELFTEST_QUERY_FAILED);
  assert(report.result == PS5_BT_SBC_SELFTEST_QUERY_FAILED);
  assert(report.stage == PS5_BT_SBC_SELFTEST_STAGE_QUERY);
  assert(report.codec_status == PS5_BT_SBC_INVALID);

  required = report.required_storage;
  assert(required > 0);
  assert(ps5_bt_sbc_selftest_run(storage.bytes, required - 1, &report) ==
         PS5_BT_SBC_SELFTEST_INVALID);
  assert(report.result == PS5_BT_SBC_SELFTEST_INVALID);
  assert(report.stage == PS5_BT_SBC_SELFTEST_STAGE_QUERY);
  assert(report.cycles_completed == 0 && report.frames_encoded == 0);

  assert(ps5_bt_sbc_selftest_run(storage.bytes + 1, required, &report) ==
         PS5_BT_SBC_SELFTEST_INVALID);
  assert(report.result == PS5_BT_SBC_SELFTEST_INVALID);
  assert(report.cycles_completed == 0 && report.frames_encoded == 0);

  assert(ps5_bt_sbc_selftest_run(
             storage.bytes, sizeof(storage.bytes),
             (struct ps5_bt_sbc_selftest_report *)storage.bytes) ==
         PS5_BT_SBC_SELFTEST_INVALID);
}

int main(void) {
  test_deterministic_repeated_lifecycle();
  test_invalid_inputs_are_bounded();
  puts("SBC generated-signal self-test passed.");
  return 0;
}
