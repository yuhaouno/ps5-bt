/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Host-only setup orchestration; every native operation is a synthetic stub. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define PS5_BT_APPLICATION_CONTROL
#define PS5_BT_APPLICATION_SETUP
#define PS5_BT_DIAGNOSTIC_BUILD_ID                                             \
  "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"
#define PS5_BT_DIAGNOSTIC_CANDIDATE "ps5-bt-application-setup"
#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static struct {
  int init_status, setup_status, shutdown_status;
  uint32_t cleanup_mask, prior_mask;
  unsigned baseline, init, setup, shutdown, callback_phase, cancel_at, stops;
  char calls[16], log[16384];
  size_t call_count, log_length;
} state;
static void called(char call) {
  assert(state.call_count + 1 < sizeof state.calls);
  state.calls[state.call_count++] = call;
  state.calls[state.call_count] = 0;
}
int32_t ps5_rust_probe(void) {
  assert(state.baseline++ == 0 && state.init == 0);
  called('B');
  return 2; /* Existing finite baseline seam, not the Rust application. */
}
void ps5_platform_log(const uint8_t *bytes, uint32_t length) {
  assert(bytes && length && state.log_length + length < sizeof state.log);
  memcpy(state.log + state.log_length, bytes, length);
  state.log_length += length;
  state.log[state.log_length] = 0;
}
int64_t ps5_bootstrap_lookup(const char *name, void **out) {
  (void)name;
  (void)out;
  assert(!"orchestration stubs never resolve a native export");
  return -1;
}
int32_t ps5_rust_application_stop_requested(void) {
  ++state.stops;
  return state.cancel_at && state.cancel_at == state.callback_phase;
}
static void import_report(struct ps5_app_posix_report *out, uint32_t stage,
                          int status) {
  *out = (struct ps5_app_posix_report){
      .abi_version = PS5_APP_POSIX_ABI,
      .stage = stage,
      .status = status,
      .raw_errno = status < 0 ? 35 : 0,
      .raw_lookup = INT64_C(0x0123456789abcdef),
      .raw_return = stage == 8 ? INT64_MIN : -2,
      .resolved = stage == 2 ? 7 : 23,
  };
}
int32_t ps5_app_posix_init_cancel(ps5_app_lookup_fn lookup,
                                  ps5_app_cancel_fn cancelled,
                                  struct ps5_app_posix_report *out) {
  assert(state.init++ == 0 && state.setup == 0 && state.shutdown == 0);
  assert(lookup == ps5_bootstrap_lookup);
  assert(cancelled == ps5_rust_application_stop_requested);
  state.callback_phase = 1;
  assert(cancelled() == (state.cancel_at == 1));
  called('I');
  import_report(out, state.init_status ? 2 : 3, state.init_status);
  return state.init_status;
}
int32_t ps5_app_setup_namespace(ps5_app_lookup_fn lookup,
                                ps5_app_cancel_fn cancelled,
                                struct ps5_app_setup_report *out) {
  assert(state.init == 1 && state.init_status == 0 && state.setup++ == 0);
  assert(state.shutdown == 0 && lookup == ps5_bootstrap_lookup);
  assert(cancelled == ps5_rust_application_stop_requested);
  state.callback_phase = 2;
  assert(cancelled() == (state.cancel_at == 2));
  called('N');
  *out = (struct ps5_app_setup_report){
      .abi_version = PS5_APP_SETUP_ABI,
      .stage = state.setup_status < 0 ? 4 : 6,
      .status = state.setup_status,
      .raw_errno = state.setup_status < 0 ? 13 : 0,
      .raw_lookup = -INT64_C(0x100000007),
      .created_mask = 1,
      .verified_mask = 15,
      .opened_mask = 15,
      .closed_mask = 15,
      .cleanup_failed_mask = state.cleanup_mask,
      .cleanup_errno = state.cleanup_mask || state.prior_mask ? 4 : 0,
      .operations = 17,
      .raw_return = -7,
      .cleanup_raw_return = state.cleanup_mask || state.prior_mask ? -9 : 0,
      .prior_cleanup_mask = state.prior_mask,
      .validation_failed_mask =
          state.setup_status == -4 ? PS5_APP_SETUP_REJECT_PARENT_OWNER : 0,
  };
  return state.setup_status;
}
int32_t ps5_app_posix_shutdown(struct ps5_app_posix_report *out) {
  assert(state.init == 1 && !state.init_status && state.setup == 1);
  assert(state.shutdown++ == 0);
  called('D');
  import_report(out, 8, state.shutdown_status);
  return state.shutdown_status;
}
int32_t ps5_app_random_token_cancel(uint8_t output[32],
                                    ps5_app_cancel_fn cancelled,
                                    struct ps5_app_posix_report *out) {
  (void)output;
  (void)cancelled;
  (void)out;
  assert(!"setup must not acquire entropy");
  return -1;
}
int32_t ps5_rust_application_probe(int64_t (*lookup)(const char *, void **),
                                   const uint8_t *secret, uint32_t length) {
  (void)lookup;
  (void)secret;
  (void)length;
  assert(!"setup must not start the Rust application");
  return -1;
}
static size_t occurrences(const char *text) {
  size_t count = 0;
  const char *cursor = state.log;
  while ((cursor = strstr(cursor, text)) != NULL) {
    ++count;
    cursor += strlen(text);
  }
  return count;
}
static void checked(unsigned reports, int completed, int cancelled,
                    int failed) {
  assert(occurrences(
             "event=application_setup_begin schema=2 namespace=fixed "
             "private_mode=0700 radio=Unavailable capture=Unavailable\n") == 1);
  assert(occurrences("application_import_stage=") == reports);
  assert(occurrences("application_import_return_hi=") == reports);
  assert(occurrences("application_import_return_lo=") == reports);
  assert(occurrences("event=application_setup_complete\n") ==
         (unsigned)completed);
  assert(occurrences("event=application_setup_cancelled\n") ==
         (unsigned)cancelled);
  assert(occurrences("event=application_setup_failed\n") == (unsigned)failed);
  assert(occurrences("event=application_setup_imports_closed\n") ==
         (unsigned)(reports == 2 && !state.shutdown_status));
  assert(occurrences("application_setup_stage=") == state.setup);
  assert(occurrences("event=run_begin build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID
                     " candidate=ps5-bt-application-setup\n") == 1);
  assert(occurrences("event=run_finish build_id=" PS5_BT_DIAGNOSTIC_BUILD_ID
                     "\n") == 1);
  assert(strstr(state.log, "application_import_lookup_hi=0x01234567\n"));
  assert(strstr(state.log, "application_import_lookup_lo=0x89abcdef\n"));
  if (state.setup) {
    assert(strstr(state.log, "application_setup_lookup_hi=0xfffffffe\n"));
    assert(strstr(state.log, "application_setup_lookup_lo=0xfffffff9\n"));
    assert(strstr(state.log, "application_setup_raw_bits=0xfffffff9\n"));
    assert(strstr(state.log, "application_setup_created_mask=0x00000001\n"));
    assert(strstr(state.log, state.setup_status == -4
                                 ? "application_setup_validation_failed_mask="
                                   "0x00000002\n"
                                 : "application_setup_validation_failed_mask="
                                   "0x00000000\n"));
    assert(strstr(state.log, "application_import_return_hi=0x80000000\n"));
    assert(strstr(state.log, "application_import_return_lo=0x00000000\n"));
  }
  assert(!strstr(state.log, "application_preflight_begin"));
  assert(!strstr(state.log, "event=application_begin"));
}
static void reset(void) { memset(&state, 0, sizeof state); }
int main(void) {
  reset();
  assert(diagnostic_main() == 2 && !strcmp(state.calls, "BIND"));
  assert(state.stops == 2);
  checked(2, 1, 0, 0);
  for (unsigned cancel = 0; cancel < 2; ++cancel) {
    reset();
    state.init_status = cancel ? 1 : -17;
    state.cancel_at = cancel ? 1 : 0;
    assert(diagnostic_main() == (cancel ? 2 : 1));
    assert(!strcmp(state.calls, "BI") && state.stops == 1);
    assert(!state.setup && !state.shutdown);
    checked(1, 0, (int)cancel, !cancel);
  }
  for (int failure = -1; failure >= -4; --failure) {
    reset();
    state.setup_status = failure;
    assert(diagnostic_main() == 1 && !strcmp(state.calls, "BIND"));
    checked(2, 0, 0, 1);
  }
  reset();
  state.setup_status = 1;
  state.cancel_at = 2;
  assert(diagnostic_main() == 2 && !strcmp(state.calls, "BIND"));
  checked(2, 0, 1, 0);
  for (unsigned prior = 0; prior < 2; ++prior) {
    reset();
    state.setup_status = 1;
    state.cancel_at = 2;
    if (prior)
      state.prior_mask = 2;
    else
      state.cleanup_mask = 2;
    assert(diagnostic_main() == 1 && !strcmp(state.calls, "BIND"));
    assert(
        strstr(state.log, "application_setup_cleanup_raw_bits=0xfffffff7\n"));
    checked(2, 0, 0, 1);
  }
  for (unsigned cancel = 0; cancel < 2; ++cancel) {
    reset();
    state.setup_status = cancel ? 1 : 0;
    state.cancel_at = cancel ? 2 : 0;
    state.shutdown_status = -31;
    assert(diagnostic_main() == 1 && !strcmp(state.calls, "BIND"));
    checked(2, 0, 0, 1);
  }
  reset();
  state.setup_status = -4;
  state.shutdown_status = -31;
  assert(diagnostic_main() == 1 && !strcmp(state.calls, "BIND"));
  assert(strstr(state.log, "application_setup_status_bits=0xfffffffc\n"));
  checked(2, 0, 0, 1);
  puts("setup main orchestration: OK");
  return 0;
}
