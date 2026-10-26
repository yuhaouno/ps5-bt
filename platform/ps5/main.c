/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#ifdef PS5_BT_APPLICATION_CONTROL
#include "application/posix_bridge.h"
#ifdef PS5_BT_APPLICATION_SETUP
#include "application/setup_namespace.h"
#endif
#include "bootstrap/bootstrap.h"
static void application_field(const char *name, uint64_t value,
                              unsigned digits) {
  char line[128];
  uint32_t length = 0;
  while (*name && length < 90)
    line[length++] = *name++;
  for (unsigned bits = digits * 4; bits; bits -= 4)
    line[length++] = "0123456789abcdef"[(value >> (bits - 4)) & 15];
  line[length++] = '\n';
  ps5_platform_log((const uint8_t *)line, length);
}
static void application_import_report(const struct ps5_app_posix_report *r) {
  application_field("application_import_stage=0x", r->stage, 8);
  application_field("application_import_status_bits=0x", (uint32_t)r->status,
                    8);
  application_field("application_import_errno=0x", (uint32_t)r->raw_errno, 8);
  application_field("application_import_lookup_hi=0x",
                    (uint64_t)r->raw_lookup >> 32, 8);
  application_field("application_import_lookup_lo=0x", (uint32_t)r->raw_lookup,
                    8);
  application_field("application_import_return_hi=0x",
                    (uint64_t)r->raw_return >> 32, 8);
  application_field("application_import_return_lo=0x", (uint32_t)r->raw_return,
                    8);
  application_field("application_import_resolved=0x", r->resolved, 8);
  application_field("application_import_cleanup_flags=0x", r->cleanup_flags, 8);
  application_field("application_import_cleanup_errno=0x",
                    (uint32_t)r->cleanup_errno, 8);
}
#ifdef PS5_BT_APPLICATION_SETUP
static int application_setup(void) {
  const uint8_t begin[] =
      "event=application_setup_begin schema=2 namespace=fixed "
      "private_mode=0700 radio=Unavailable capture=Unavailable\n";
  const uint8_t complete[] = "event=application_setup_complete\n";
  const uint8_t failed[] = "event=application_setup_failed\n";
  const uint8_t cancelled[] = "event=application_setup_cancelled\n";
  struct ps5_app_posix_report imports;
  struct ps5_app_setup_report setup = {0};
  ps5_platform_log(begin, sizeof begin - 1);
  int32_t status = ps5_app_posix_init_cancel(
      ps5_bootstrap_lookup, ps5_rust_application_stop_requested, &imports);
  application_import_report(&imports);
  if (!status) {
    status = ps5_app_setup_namespace(
        ps5_bootstrap_lookup, ps5_rust_application_stop_requested, &setup);
#define FIELD(name, value)                                                     \
  application_field("application_setup_" name "=0x", (uint32_t)(value), 8)
    FIELD("stage", setup.stage);
    FIELD("status_bits", setup.status);
    FIELD("errno", setup.raw_errno);
    FIELD("lookup_hi", (uint64_t)setup.raw_lookup >> 32);
    FIELD("lookup_lo", setup.raw_lookup);
    FIELD("created_mask", setup.created_mask);
    FIELD("verified_mask", setup.verified_mask);
    FIELD("opened_mask", setup.opened_mask);
    FIELD("closed_mask", setup.closed_mask);
    FIELD("cleanup_failed_mask", setup.cleanup_failed_mask);
    FIELD("cleanup_errno", setup.cleanup_errno);
    FIELD("cleanup_raw_bits", setup.cleanup_raw_return);
    FIELD("prior_cleanup_mask", setup.prior_cleanup_mask);
    FIELD("operations", setup.operations);
    FIELD("raw_bits", setup.raw_return);
    FIELD("validation_failed_mask", setup.validation_failed_mask);
#undef FIELD
    int32_t shutdown = ps5_app_posix_shutdown(&imports);
    application_import_report(&imports);
    if (shutdown)
      status = -1;
    const uint8_t cleaned[] = "event=application_setup_imports_closed\n";
    if (!shutdown)
      ps5_platform_log(cleaned, sizeof cleaned - 1);
  }
  int clean_cancel =
      status == 1 && !setup.cleanup_failed_mask && !setup.prior_cleanup_mask;
  const uint8_t *event = status == 0    ? complete
                         : clean_cancel ? cancelled
                                        : failed;
  uint32_t length = status == 0    ? sizeof complete - 1
                    : clean_cancel ? sizeof cancelled - 1
                                   : sizeof failed - 1;
  ps5_platform_log(event, length);
  return status == 0 || clean_cancel ? 0 : 1;
}
#else
static int application_control(void) {
  const uint8_t begin[] = "event=application_preflight_begin schema=1 "
                          "radio=Unavailable capture=Unavailable\n";
  const uint8_t failed[] = "event=application_preflight_failed\n";
  const uint8_t cancelled[] = "event=application_preflight_cancelled\n";
  const uint8_t complete[] = "event=application_preflight_cleanup_complete\n";
  struct ps5_app_posix_report report;
  uint8_t secret[32] = {0};
  ps5_platform_log(begin, sizeof begin - 1);
  int32_t status = ps5_app_posix_init_cancel(
      ps5_bootstrap_lookup, ps5_rust_application_stop_requested, &report);
  application_import_report(&report);
  if (status) {
    ps5_platform_log(status == 1 ? cancelled : failed,
                     status == 1 ? sizeof cancelled - 1 : sizeof failed - 1);
    return status == 1 ? 0 : 1;
  }
  status = ps5_app_random_token_cancel(
      secret, ps5_rust_application_stop_requested, &report);
  application_import_report(&report);
  int result = 0;
  if (status) {
    result = (status == 1 && !report.cleanup_flags) ? 0 : 1;
    ps5_platform_log(status == 1 ? cancelled : failed,
                     status == 1 ? sizeof cancelled - 1 : sizeof failed - 1);
  } else
    result =
        ps5_rust_application_probe(ps5_bootstrap_lookup, secret, sizeof secret);
  ps5_app_scrub(secret, sizeof secret);
  int32_t cleanup = ps5_app_posix_shutdown(&report);
  application_import_report(&report);
  if (cleanup)
    result = 1;
  ps5_platform_log(complete, sizeof complete - 1);
  return result;
}
#endif /* separate setup versus application workload */
#endif
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
#if defined(PS5_BT_SBC_CONTINUOUS) || defined(PS5_BT_CAPTURE_STREAM)
int32_t ps5_continuous_now_us(uint64_t *out);
#endif
#ifdef PS5_BT_SBC_CONTINUOUS
int32_t ps5_rust_sbc_continuous(int32_t (*now_us)(uint64_t *out));
#endif
#ifdef PS5_BT_CAPTURE_STREAM
int32_t ps5_rust_capture_stream(int32_t (*now_us)(uint64_t *out));
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
#ifdef PS5_BT_DIAGNOSTIC_BUILD_ID
  static const uint8_t run_begin[] = "event=run_begin build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID " candidate=" PS5_BT_DIAGNOSTIC_CANDIDATE "\n";
  ps5_platform_log(run_begin, sizeof run_begin - 1);
#endif
  int result = (int)ps5_rust_probe();
#ifdef PS5_BT_APPLICATION_CONTROL
#ifdef PS5_BT_APPLICATION_SETUP
  if (application_setup() != 0)
#else
  if (application_control() != 0)
#endif
    result = 1;
#endif
#ifdef PS5_BT_CAPTURE_STREAM
  if (ps5_rust_capture_stream(ps5_continuous_now_us) != 0)
    result = 1;
#endif
#ifdef PS5_BT_SBC_CONTINUOUS
  /* Independent generated input; target transport remains Unsupported. */
  if (ps5_rust_sbc_continuous(ps5_continuous_now_us) != 0)
    result = 1;
#endif
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
#ifdef PS5_BT_DIAGNOSTIC_BUILD_ID
  static const uint8_t run_finish[] = "event=run_finish build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID "\n";
  ps5_platform_log(run_finish, sizeof run_finish - 1);
#endif
  return result;
}
