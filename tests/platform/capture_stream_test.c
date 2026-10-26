/* SPDX-License-Identifier: GPL-3.0-only */
#include "capture_stream_fixture.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
static _Alignas(16) unsigned char storage[9216];
static const struct ps5_capture_stream_ops ops = {PS5_CAPTURE_STREAM_ABI, sizeof ops, ps5_capture_test_current, ps5_capture_test_module};
static void open_start(void) {
  struct ps5_capture_stream_report r;
  assert(ps5_capture_stream_open(storage, sizeof storage, &ops, &r) == 0);
  assert(r.stage == 4 && r.abi_version == PS5_CAPTURE_STREAM_ABI);
  assert(ps5_capture_stream_start(storage, &r) == 0);
}
int main(void) {
  struct ps5_capture_stream_report r;
  struct ps5_capture_stream_read read;
  unsigned char output[32];
  assert(ps5_capture_stream_abi_version() == PS5_CAPTURE_STREAM_ABI);
  assert(ps5_capture_stream_storage_size() <= sizeof storage);
  assert(ps5_capture_stream_storage_alignment() == 16);
  ps5_capture_test_reset();
  assert(ps5_capture_stream_open(storage + 1, sizeof storage - 1, &ops, &r) == -2);
  assert(ps5_capture_stream_open(storage, 1, &ops, &r) == -2);
  assert(ps5_capture_stream_open(storage, sizeof storage, &ops, (void *)storage) == -2);
  assert(ps5_capture_test_calls(10) == 0);
  for (uint32_t phase = 1; phase <= 11; ++phase) {
    if (phase == 4) continue;
    ps5_capture_test_reset(); ps5_capture_test_fail(phase, -44);
    int status = ps5_capture_stream_open(storage, sizeof storage, &ops, &r);
    if (phase == 1 || phase == 2 || phase >= 8) {
      assert(status != 0);
      assert(ps5_capture_test_calls(6) == 0);
      assert(ps5_capture_test_calls(7) == (phase == 2 ? 1u : 0u));
      if (phase == 10) assert(r.raw_lookup == INT64_C(0x100000000) - 44);
      continue;
    }
    assert(status == 0);
    status = ps5_capture_stream_start(storage, &r);
    if (phase == 3) assert(status == -5); else assert(status == 0);
    if (phase != 3) {
      status = ps5_capture_stream_stop(storage, &r);
      if (phase == 5) { assert(status == -5 && r.cleanup_flags == 1); }
      else assert(status == 0);
    }
    status = ps5_capture_stream_close(storage, &r);
    if (phase == 6 || phase == 7) assert(status == -5 && r.cleanup_flags != 0);
    else assert(status == 0);
    assert(ps5_capture_stream_close(storage, &r) == -2);
    if (phase >= 5 && phase <= 7) {
      assert(ps5_capture_stream_open(storage, sizeof storage, &ops, &r) == -3);
      assert(r.cleanup_flags != 0);
    }
    assert(ps5_capture_test_calls(5) == (phase == 3 ? 0u : 1u));
    assert(ps5_capture_test_calls(6) == 1 && ps5_capture_test_calls(7) == 1);
  }
  const int32_t statuses[] = {8, 0, (int32_t)UINT32_C(0x81950002), (int32_t)UINT32_C(0x81950004), (int32_t)UINT32_C(0x81950008), -99};
  for (uint32_t i = 0; i < sizeof statuses / sizeof statuses[0]; ++i) {
    ps5_capture_test_reset(); open_start(); ps5_capture_test_read(statuses[i]);
    memset(output, 0x77, sizeof output);
    assert(ps5_capture_stream_read_once(storage, output, sizeof output, &read, &r) == 0);
    assert(read.kind == i + 1 && read.raw_status == statuses[i] && r.raw_status == statuses[i]);
    assert(read.bytes == (i == 0 ? 8u : 0u));
    assert(read.flags == (i == 3 ? 1u : 0u));
    for (uint32_t j = 0; j < 16; ++j) assert(read.metadata[j] == 0xa5);
    if (i == 0) for (uint32_t j = 0; j < 8; ++j) assert(output[j] == 0x3c);
    assert(ps5_capture_stream_close(storage, &r) == 0);
    assert(ps5_capture_test_calls(5) == 1 && ps5_capture_test_calls(6) == 1 && ps5_capture_test_calls(7) == 1);
  }
  ps5_capture_test_reset(); open_start();
  assert(ps5_capture_stream_open(storage, sizeof storage, &ops, &r) == -3);
  assert(ps5_capture_stream_read_once(storage, storage, 32, &read, &r) == -2);
  assert(ps5_capture_stream_read_once(storage, output, 0, &read, &r) == -2);
  assert(ps5_capture_test_calls(4) == 0);
  ps5_capture_test_read(33);
  assert(ps5_capture_stream_read_once(storage, output, sizeof output, &read, &r) == -5);
  assert(read.bytes == 0 && r.raw_status == 33 && read.kind == 6);
  assert(ps5_capture_stream_close(storage, &r) == 0);
  puts("PASS experimental continuous capture ABI lifecycle, bounds and consume-once cleanup");
}
