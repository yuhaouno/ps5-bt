/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#ifdef PS5_BT_NATIVE_QUERY
#include "bootstrap/bootstrap.h"
#include "diagnostics/native_query.h"
#endif
#ifdef PS5_BT_USB_METADATA
#include "diagnostics/usb_probe.h"
#endif
#ifdef PS5_BT_CAPTURE_PROBE
#include "bootstrap/bootstrap.h"
#include "diagnostics/capture_probe.h"
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
      "none",       "lookup_load", "load_ipmi", "load_avcap2", "resolve_avcap2",
      "initialize", "open",        "start",     "read",        "stop",
      "close",      "terminate",   "wait",      "done"};
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
  line[used++] = (char)('0' + ordinal % 10);
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
      ps5_bootstrap_lookup, ps5_bootstrap_module_lookup, capture_trace,
      capture_wait_before_read};
  char capture_report[PS5_CAPTURE_PROBE_REPORT_CAPACITY];
  size_t capture_length = 0;
  ps5_platform_log(capture_begin, (uint32_t)(sizeof capture_begin - 1));
#ifdef PS5_BT_CAPTURE_PREFLIGHT
  (void)ps5_capture_probe_preflight(&capture_ops, &capture);
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
