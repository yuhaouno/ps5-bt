/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#ifdef PS5_BT_NATIVE_L2CAP_PROBE
#include "bootstrap/bootstrap.h"
#include "diagnostics/native_l2cap_probe.h"
static void native_l2cap_trace(enum ps5_native_l2cap_phase phase, int entering,
                               int64_t raw_status, int errno_valid,
                               int raw_errno) {
  static const char *const names[] = {
      "resolve_socket", "resolve_close", "resolve_errno", "errno",
      "control_socket", "control_close", "l2cap_socket",  "l2cap_close"};
  char line[192];
  size_t used = 0;
  const char *parts[] = {
      "event=native_l2cap_phase phase=",
      (unsigned)phase < sizeof names / sizeof names[0] ? names[phase]
                                                       : "unknown",
      entering ? " edge=begin raw_bits=0x" : " edge=end raw_bits=0x"};
  for (unsigned part = 0; part < 3; ++part)
    for (const char *value = parts[part]; *value; ++value)
      line[used++] = *value;
  for (unsigned shift = 64; shift; shift -= 4)
    line[used++] =
        "0123456789abcdef"[((uint64_t)raw_status >> (shift - 4)) & 15];
  const char suffix[] = " errno_valid=";
  for (size_t i = 0; i < sizeof suffix - 1; ++i)
    line[used++] = suffix[i];
  line[used++] = errno_valid ? '1' : '0';
  const char error_suffix[] = " errno_bits=0x";
  for (size_t i = 0; i < sizeof error_suffix - 1; ++i)
    line[used++] = error_suffix[i];
  for (unsigned shift = 32; shift; shift -= 4)
    line[used++] =
        "0123456789abcdef"[((uint32_t)raw_errno >> (shift - 4)) & 15];
  line[used++] = '\n';
  ps5_platform_log((const uint8_t *)line, (uint32_t)used);
}
#endif
#ifdef PS5_BT_NATIVE_QUERY
#include "bootstrap/bootstrap.h"
#include "diagnostics/native_query.h"
#endif
#ifdef PS5_BT_USB_METADATA
#include "diagnostics/usb_probe.h"
#endif
#ifdef PS5_BT_SBC_SELFTEST
#include "../codec/ps5_bt_sbc_selftest.h"
static _Alignas(64) uint8_t sbc_selftest_storage[8192];
#endif
#ifdef PS5_BT_CAPTURE_PROBE
#include "bootstrap/bootstrap.h"
#include "diagnostics/capture_probe.h"
#ifdef PS5_BT_CAPTURE_OBSERVE
struct target_timespec {
  int64_t seconds, nanoseconds;
};
static int capture_now_us(uint64_t *out) {
  /* Pinned SDK time.h declares int clock_gettime(int32 clockid, timespec*),
   * CLOCK_MONOTONIC=4; x86/_types.h and sys/_timespec.h use two signed 64-bit
   * fields. Runtime export availability and clock behavior require hardware. */
  typedef int (*clock_fn)(int32_t, struct target_timespec *);
  _Static_assert(sizeof(struct target_timespec) == 16, "target timespec ABI");
  _Static_assert(sizeof(clock_fn) == sizeof(void *), "target function ABI");
  if (!out)
    return PS5_BT_INVALID;
  *out = 0;
  void *symbol = 0;
  if (ps5_bootstrap_lookup("clock_gettime", &symbol) != 0 || !symbol)
    return PS5_BT_IO;
  clock_fn clock = 0;
  for (size_t index = 0; index < sizeof clock; ++index)
    ((unsigned char *)&clock)[index] = ((const unsigned char *)&symbol)[index];
  struct target_timespec time = {0};
  int raw = clock(4, &time);
  if (raw != 0)
    return raw;
  if (time.seconds < 0 || time.nanoseconds < 0 ||
      time.nanoseconds >= 1000000000)
    return PS5_BT_INVALID;
  uint64_t subseconds = (uint64_t)time.nanoseconds / 1000;
  if ((uint64_t)time.seconds > (UINT64_MAX - subseconds) / 1000000)
    return PS5_BT_INVALID;
  *out = (uint64_t)time.seconds * 1000000 + subseconds;
  return 0;
}
#endif
static int capture_wait_before_read(uint32_t microseconds) {
  /* SDK v0.45 unistd.h declares int usleep(useconds_t), with unsigned
   * 32-bit useconds_t. One explicit 100 ms diagnostic settle, never a loop. */
  typedef int (*sleep_fn)(uint32_t);
  _Static_assert(sizeof(sleep_fn) == sizeof(void *),
                 "PS5 callable pointer ABI");
  if (microseconds != 100000)
    return PS5_BT_INVALID;
  void *symbol = 0;
  if (ps5_bootstrap_lookup("usleep", &symbol) != 0 || !symbol)
    return PS5_BT_IO;
  sleep_fn sleep = 0;
  for (size_t i = 0; i < sizeof sleep; ++i)
    ((unsigned char *)&sleep)[i] = ((const unsigned char *)&symbol)[i];
  return sleep(microseconds);
}
static void capture_trace(enum ps5_capture_probe_phase phase, uint32_t ordinal,
                          int entering, int64_t raw_status) {
  static const char *const names[] = {
      "none",           "lookup_load", "load_ipmi",     "load_avcap2",
      "resolve_avcap2", "initialize",  "open",          "start",
      "read",           "stop",        "close",         "terminate",
      "wait",           "done",        "observe_clock", "observe_wait"};
  char line[128];
  size_t used = 0;
  const char *parts[] = {
      "event=capture_phase phase=",
      (unsigned)phase < sizeof names / sizeof names[0] ? names[phase]
                                                       : "unknown",
      entering ? " edge=begin ordinal=" : " edge=end ordinal="};
  for (unsigned part = 0; part < 3; ++part)
    for (const char *text = parts[part]; *text && used < sizeof line; ++text)
      line[used++] = *text;
  char ordinal_digits[10];
  size_t ordinal_count = 0;
  do {
    ordinal_digits[ordinal_count++] = (char)('0' + ordinal % 10);
    ordinal /= 10;
  } while (ordinal != 0);
  while (ordinal_count != 0)
    line[used++] = ordinal_digits[--ordinal_count];
  const char suffix[] = " raw_bits=0x";
  for (size_t i = 0; i < sizeof suffix - 1; ++i)
    line[used++] = suffix[i];
  for (unsigned shift = 64; shift; shift -= 4)
    line[used++] =
        "0123456789abcdef"[((uint64_t)raw_status >> (shift - 4)) & 15];
  line[used++] = '\n';
  ps5_platform_log((const uint8_t *)line, (uint32_t)used);
}
#endif

int main(void) {
  int result = (int)ps5_rust_probe();
#ifdef PS5_BT_NATIVE_L2CAP_PROBE
  static const uint8_t begin[] = "event=native_l2cap_begin\n";
  static const uint8_t failed[] = "event=native_l2cap_report_failed\n";
  struct ps5_native_l2cap_result native;
  char line[PS5_NATIVE_L2CAP_REPORT_CAPACITY];
  size_t length = 0;
  ps5_platform_log(begin, sizeof begin - 1);
  enum ps5_native_l2cap_status status = ps5_native_l2cap_probe_resolve(
      ps5_bootstrap_lookup, native_l2cap_trace, &native);
  if (ps5_native_l2cap_probe_encode_report(
          &native, line, sizeof line, &length) == PS5_NATIVE_L2CAP_AVAILABLE) {
    ps5_platform_log((const uint8_t *)line, (uint32_t)length);
  } else {
    ps5_platform_log(failed, sizeof failed - 1);
    result = 1;
  }
  if (status < 0)
    result = 1;
#endif
#ifdef PS5_BT_SBC_SELFTEST
  static const uint8_t begin[] = "event=sbc_selftest_begin\n";
  static const uint8_t attribution[] =
      "codec=BlueZ_libsbc_2.2 license=LGPL-2.1-or-later generic=1\n";
  static const uint8_t encode_failed[] = "event=sbc_selftest_report_failed\n";
  struct ps5_bt_sbc_selftest_report sbc;
  char line[PS5_BT_SBC_SELFTEST_REPORT_CAPACITY];
  size_t length = 0;
  ps5_platform_log(begin, sizeof begin - 1);
  ps5_platform_log(attribution, sizeof attribution - 1);
  int32_t codec_result = ps5_bt_sbc_selftest_run(
      sbc_selftest_storage, sizeof sbc_selftest_storage, &sbc);
  if (ps5_bt_sbc_selftest_encode_report(&sbc, line, sizeof line, &length) ==
      PS5_BT_SBC_SELFTEST_OK) {
    ps5_platform_log((const uint8_t *)line, (uint32_t)length);
  } else {
    ps5_platform_log(encode_failed, sizeof encode_failed - 1);
    result = 1;
  }
  if (codec_result != PS5_BT_SBC_SELFTEST_OK)
    result = 1;
#endif
#ifdef PS5_BT_USB_METADATA
  /* Independent metadata diagnostic; never opens the live transport. */
  int metadata = ps5_usb_probe_run();
  if (metadata != 0)
    result = 1;
#endif
#ifdef PS5_BT_NATIVE_QUERY
  static const uint8_t begin[] = "event=native_query_begin\n";
  static const uint8_t encode_failed[] = "event=native_query_encode_failed\n";
  struct ps5_native_query_result native;
  char line[PS5_NATIVE_QUERY_REPORT_CAPACITY];
  size_t length = 0;
  ps5_platform_log(begin, (uint32_t)(sizeof begin - 1));
  (void)ps5_native_query_resolve(ps5_bootstrap_lookup, &native);
  if (ps5_native_query_encode_report(&native, line, sizeof line, &length) ==
      PS5_NATIVE_QUERY_OK) {
    ps5_platform_log((const uint8_t *)line, (uint32_t)length);
  } else {
    ps5_platform_log(encode_failed, (uint32_t)(sizeof encode_failed - 1));
  }
#endif
#ifdef PS5_BT_CAPTURE_PROBE
  static const uint8_t capture_begin[] = "event=capture_probe_begin\n";
  static const uint8_t capture_encode_failed[] =
      "event=capture_probe_encode_failed\n";
  struct ps5_capture_probe_result capture;
  const struct ps5_capture_probe_ops capture_ops = {
      .lookup_current = ps5_bootstrap_lookup,
      .lookup_module = ps5_bootstrap_module_lookup,
      .trace = capture_trace,
      .wait_before_read = capture_wait_before_read,
#ifdef PS5_BT_CAPTURE_OBSERVE
      .now_us = capture_now_us,
#endif
  };
  char capture_report[PS5_CAPTURE_PROBE_REPORT_CAPACITY];
  size_t capture_length = 0;
  ps5_platform_log(capture_begin, (uint32_t)(sizeof capture_begin - 1));
#ifdef PS5_BT_CAPTURE_PREFLIGHT
  (void)ps5_capture_probe_preflight(&capture_ops, &capture);
#elif defined(PS5_BT_CAPTURE_OBSERVE)
  (void)ps5_capture_probe_observe(&capture_ops, &capture);
#else
  (void)ps5_capture_probe_run(&capture_ops, &capture);
#endif
  if (ps5_capture_probe_encode_report(&capture, capture_report,
                                      sizeof capture_report, &capture_length) ==
      PS5_CAPTURE_PROBE_OK) {
    ps5_platform_log((const uint8_t *)capture_report, (uint32_t)capture_length);
  } else {
    ps5_platform_log(capture_encode_failed,
                     (uint32_t)(sizeof capture_encode_failed - 1));
    result = 1;
  }
  if (capture.status != PS5_CAPTURE_PROBE_OK)
    result = 1;
#endif
  return result;
}
