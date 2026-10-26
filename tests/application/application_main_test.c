/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Host-only orchestration faults; no resolver, entropy or PS5 call is
 * performed. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define PS5_BT_APPLICATION_CONTROL
#define PS5_BT_DIAGNOSTIC_BUILD_ID                                             \
  "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"
#define PS5_BT_DIAGNOSTIC_CANDIDATE "ps5-bt-application-control"
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static const uint8_t fixture_secret[] = "00112233445566778899aabbccddeeff";
static struct {
  int init_status, entropy_status, rust_status, shutdown_status;
  uint32_t entropy_cleanup_flags;
  unsigned baseline, init, entropy, rust, scrub, shutdown, stop, lookup;
  unsigned cancel_at, callback_phase;
  uint8_t *secret;
  char calls[32], log[16384];
  size_t call_count, log_length;
} state;

static void called(char call) {
  assert(state.call_count + 1 < sizeof state.calls);
  state.calls[state.call_count++] = call;
  state.calls[state.call_count] = '\0';
}
static void report(struct ps5_app_posix_report *out, uint32_t stage,
                   int32_t status, int64_t native_return) {
  assert(out);
  *out = (struct ps5_app_posix_report){
      .abi_version = PS5_APP_POSIX_ABI,
      .stage = stage,
      .status = status,
      .raw_errno = status < 0 ? 35 : 0,
      .raw_lookup = -1,
      .raw_return = native_return,
      .resolved = stage == 3 ? 23 : 0,
  };
}
int32_t ps5_rust_probe(void) {
  assert(state.baseline++ == 0 && state.init == 0);
  called('B');
  return 2;
}
void ps5_platform_log(const uint8_t *data, uint32_t bytes) {
  assert(data && bytes && state.log_length + bytes < sizeof state.log);
  memcpy(state.log + state.log_length, data, bytes);
  state.log_length += bytes;
  state.log[state.log_length] = '\0';
}
int64_t ps5_bootstrap_lookup(const char *name, void **out) {
  static uint8_t marker;
  assert(name && !strcmp(name, "fixture") && out && !*out);
  ++state.lookup;
  *out = &marker;
  return 0;
}
int32_t ps5_rust_application_stop_requested(void) {
  ++state.stop;
  return state.callback_phase == state.cancel_at && state.cancel_at != 0;
}
int32_t ps5_app_posix_init_cancel(ps5_app_lookup_fn lookup,
                                  ps5_app_cancel_fn cancelled,
                                  struct ps5_app_posix_report *out) {
  assert(state.init++ == 0 && state.entropy == 0 && state.rust == 0);
  assert(lookup == ps5_bootstrap_lookup);
  assert(cancelled == ps5_rust_application_stop_requested);
  state.callback_phase = 1;
  assert(cancelled() == (state.cancel_at == 1));
  called('I');
  void *symbol = NULL;
  assert(lookup("fixture", &symbol) == 0 && symbol);
  report(out, state.init_status == 0 ? 3 : 2, state.init_status,
         INT64_C(0x0123456789abcdef));
  return state.init_status;
}
int32_t ps5_app_random_token_cancel(uint8_t output[32],
                                    ps5_app_cancel_fn cancelled,
                                    struct ps5_app_posix_report *out) {
  assert(state.init == 1 && state.init_status == 0 && state.entropy++ == 0);
  assert(state.rust == 0 && state.scrub == 0 && state.shutdown == 0);
  assert(output && cancelled == ps5_rust_application_stop_requested);
  state.callback_phase = 2;
  assert(cancelled() == (state.cancel_at == 2));
  for (size_t i = 0; i < 32; ++i)
    assert(output[i] == 0);
  state.secret = output;
  /* Even a failing/cancelled backend may have partially filled the secret. */
  memcpy(output, fixture_secret, 32);
  called('E');
  report(out, 6, state.entropy_status, -2);
  out->cleanup_flags = state.entropy_cleanup_flags;
  out->cleanup_errno = state.entropy_cleanup_flags ? 35 : 0;
  return state.entropy_status;
}
int32_t ps5_rust_application_probe(int64_t (*lookup)(const char *, void **),
                                   const uint8_t *secret, uint32_t length) {
  assert(state.init == 1 && state.entropy == 1 && state.entropy_status == 0);
  assert(state.rust++ == 0 && state.scrub == 0 && state.shutdown == 0);
  assert(lookup == ps5_bootstrap_lookup && secret == state.secret &&
         length == 32);
  assert(!memcmp(secret, fixture_secret, 32));
  void *symbol = NULL;
  assert(lookup("fixture", &symbol) == 0 && symbol);
  called('R');
  return state.rust_status;
}
void ps5_app_scrub(void *memory, size_t bytes) {
  assert(state.entropy == 1 && state.scrub++ == 0 && state.shutdown == 0);
  assert(memory == state.secret && bytes == 32);
  volatile uint8_t *out = memory;
  for (size_t i = 0; i < bytes; ++i)
    out[i] = 0;
  called('S');
}
int32_t ps5_app_posix_shutdown(struct ps5_app_posix_report *out) {
  assert(state.init == 1 && state.init_status == 0 && state.entropy == 1);
  assert(state.scrub == 1 && state.shutdown++ == 0);
  for (size_t i = 0; i < 32; ++i)
    assert(state.secret[i] ==
           0); /* Still inside application_control lifetime. */
  state.secret = NULL;
  called('D');
  report(out, 8, state.shutdown_status, INT64_MIN);
  return state.shutdown_status;
}
static void reset(void) { memset(&state, 0, sizeof state); }
static size_t occurrences(const char *text) {
  size_t count = 0;
  const char *cursor = state.log;
  while ((cursor = strstr(cursor, text)) != NULL) {
    ++count;
    cursor += strlen(text);
  }
  return count;
}
static void checked_log(unsigned reports, int completed, int cancelled,
                        int failed) {
  assert(!strstr(state.log, (const char *)fixture_secret));
  assert(occurrences("event=application_preflight_begin ") == 1);
  assert(occurrences("application_import_stage=") == reports);
  assert(occurrences("application_import_return_hi=") == reports);
  assert(occurrences("application_import_return_lo=") == reports);
  assert(strstr(state.log, "application_import_return_hi=0x01234567\n"));
  assert(strstr(state.log, "application_import_return_lo=0x89abcdef\n"));
  if (reports == 3) {
    assert(strstr(state.log, "application_import_return_hi=0xffffffff\n"));
    assert(strstr(state.log, "application_import_return_lo=0xfffffffe\n"));
    assert(strstr(state.log, "application_import_return_hi=0x80000000\n"));
    assert(strstr(state.log, "application_import_return_lo=0x00000000\n"));
  }
  assert(occurrences("event=application_preflight_cleanup_complete\n") ==
         (unsigned)completed);
  assert(occurrences("event=application_preflight_cancelled\n") ==
         (unsigned)cancelled);
  assert(occurrences("event=application_preflight_failed\n") ==
         (unsigned)failed);
}
int main(void) {
  reset();
  assert(diagnostic_main() == 2);
  assert(!strcmp(state.calls, "BIERSD"));
  assert(state.stop == 2 && state.lookup == 2);
  assert(occurrences("event=run_begin build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID
                     " candidate=ps5-bt-application-control\n") == 1);
  assert(occurrences("event=run_finish build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID
                     "\n") == 1);
  checked_log(3, 1, 0, 0);

  for (unsigned cancelled = 0; cancelled < 2; ++cancelled) {
    reset();
    state.init_status = cancelled ? 1 : -17;
    state.cancel_at = cancelled ? 1 : 0;
    assert(diagnostic_main() == (cancelled ? 2 : 1));
    assert(!strcmp(state.calls, "BI"));
    assert(state.entropy == 0 && state.rust == 0 && state.scrub == 0 &&
           state.shutdown == 0 && state.stop == 1 && state.lookup == 1);
    checked_log(1, 0, cancelled, !cancelled);
  }
  for (unsigned scenario = 0; scenario < 4; ++scenario) {
    reset();
    state.entropy_status = scenario < 2 ? 1 : -19;
    state.entropy_cleanup_flags = scenario % 2;
    state.cancel_at = scenario < 2 ? 2 : 0;
    assert(diagnostic_main() == (scenario == 0 ? 2 : 1));
    assert(!strcmp(state.calls, "BIESD"));
    assert(state.rust == 0 && state.scrub == 1 && state.shutdown == 1);
    checked_log(3, 1, scenario < 2, scenario >= 2);
    if (state.entropy_cleanup_flags)
      assert(
          strstr(state.log, "application_import_cleanup_flags=0x00000001\n"));
  }
  for (unsigned scenario = 0; scenario < 3; ++scenario) {
    reset();
    state.rust_status = scenario == 0 ? 1 : -23;
    state.shutdown_status = scenario == 2 ? -31 : 0;
    assert(application_control() == (scenario == 2 ? 1 : state.rust_status));
    assert(!strcmp(state.calls, "IERSD"));
    assert(state.scrub == 1 && state.shutdown == 1);
    checked_log(3, 1, 0, 0);
    reset();
    state.rust_status = -23;
    state.shutdown_status = scenario == 2 ? -31 : 0;
    assert(diagnostic_main() == 1);
    assert(!strcmp(state.calls, "BIERSD"));
    checked_log(3, 1, 0, 0);
  }
  reset();
  state.shutdown_status = -31;
  assert(diagnostic_main() ==
         1); /* Clean Rust result never hides cleanup failure. */
  assert(!strcmp(state.calls, "BIERSD"));
  checked_log(3, 1, 0, 0);
  puts("PASS application main preflight, callback wiring, secret cleanup and "
       "raw returns");
  return 0;
}
