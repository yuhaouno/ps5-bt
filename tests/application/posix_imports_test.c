/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Synthetic native exports only; reset models a fresh payload process. */
#include "../../platform/ps5/application/posix_imports.c"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static int fail_lookup, null_lookup, lookup_count, dirty_lookup, null_errno,
    nested;
static int opens, reads, closes, stat_failure, read_failure, close_failure,
    fake_errno;
static size_t position;
static int chunk;
static int cancel_calls, cancel_after;
static int32_t cancelled(void) { return ++cancel_calls >= cancel_after; }
static int *fake_error(void) { return null_errno ? NULL : &fake_errno; }
static int fake_open(const char *name, int flags, ...) {
  assert(strcmp(name, "/dev/random") == 0);
  assert((flags & (O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)) ==
         (O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC));
  ++opens;
  return 9;
}
static int fake_fstat(int fd, struct stat *out) {
  assert(fd == 9);
  memset(out, 0, sizeof *out);
  if (stat_failure) {
    out->st_mode = S_IFREG;
    return 0;
  }
  out->st_mode = S_IFCHR;
  return 0;
}
static ssize_t fake_read(int fd, void *out, size_t length) {
  assert(fd == 9 && length && length <= 16);
  ++reads;
  if (read_failure == 1) {
    fake_errno = EINTR;
    return -1;
  }
  if (read_failure == 2)
    return 0;
  if (read_failure == 3)
    return (ssize_t)length + 1;
  size_t count = chunk ? 1 : length;
  for (size_t i = 0; i < count; ++i)
    ((uint8_t *)out)[i] = (uint8_t)position++;
  return (ssize_t)count;
}
static int fake_close(int fd) {
  assert(fd == 9);
  ++closes;
  if (close_failure) {
    fake_errno = EINTR;
    return -1;
  }
  return 0;
}
#define SYMBOL(symbol, function)                                               \
  do {                                                                         \
    if (!strcmp(name, symbol)) {                                               \
      __typeof__(&(function)) ptr = &(function);                               \
      _Static_assert(sizeof ptr == sizeof *out, "callable ABI");               \
      memcpy(out, &ptr, sizeof ptr);                                           \
      goto done;                                                               \
    }                                                                          \
  } while (0)
static int64_t lookup(const char *name, void **out) {
  ++lookup_count;
  *out = NULL;
  if (nested) {
    struct ps5_app_posix_report report;
    nested = 0;
    assert(ps5_app_posix_init(lookup, &report) == -3);
  }
  if (fail_lookup == lookup_count) {
    if (dirty_lookup)
      *out = (void *)1;
    return INT64_C(0x100000007);
  }
  if (null_lookup == lookup_count)
    return 0;
  SYMBOL("__error", fake_error);
  SYMBOL("open", fake_open);
  SYMBOL("fstat", fake_fstat);
  SYMBOL("read", fake_read);
  SYMBOL("close", fake_close);
  SYMBOL("openat", openat);
  SYMBOL("fcntl", fcntl);
  SYMBOL("socket", socket);
  SYMBOL("bind", bind);
  SYMBOL("listen", listen);
  SYMBOL("accept", accept);
  SYMBOL("getsockname", getsockname);
  SYMBOL("setsockopt", setsockopt);
  SYMBOL("recv", recv);
  SYMBOL("send", send);
  SYMBOL("write", write);
  SYMBOL("fstatat", fstatat);
  SYMBOL("fchmod", fchmod);
  SYMBOL("flock", flock);
  SYMBOL("fsync", fsync);
  SYMBOL("renameat", renameat);
  SYMBOL("unlinkat", unlinkat);
  SYMBOL("geteuid", geteuid);
  assert(!"unexpected export");
done:
  return 0;
}
#undef SYMBOL
static void reset(void) {
  memset(&functions, 0, sizeof functions);
  ready = opening = ever_ready = fallback_error = 0;
  native_error = NULL;
  fail_lookup = null_lookup = lookup_count = dirty_lookup = null_errno =
      nested = 0;
  opens = reads = closes = stat_failure = read_failure = close_failure =
      fake_errno = 0;
  position = 0;
  chunk = 0;
  cancel_calls = cancel_after = 0;
}
static void zero(const uint8_t bytes[32]) {
  for (unsigned i = 0; i < 32; ++i)
    assert(bytes[i] == 0);
}
int main(void) {
  struct ps5_app_posix_report report;
  uint8_t token[32];
  for (int symbol = 1; symbol <= 23; ++symbol) {
    for (int dirty = 0; dirty <= 1; ++dirty) {
      reset();
      fail_lookup = symbol;
      dirty_lookup = dirty;
      assert(ps5_app_posix_init(lookup, &report) == -1);
      assert(report.raw_lookup == INT64_C(0x100000007) &&
             report.resolved == (uint32_t)symbol - 1);
      assert(opens == 0 && reads == 0 && closes == 0 && !ready);
    }
    reset();
    null_lookup = symbol;
    assert(ps5_app_posix_init(lookup, &report) == -1);
    assert(opens == 0 && !ready);
  }
  reset();
  null_errno = 1;
  assert(ps5_app_posix_init(lookup, &report) == -1 && report.stage == 3 &&
         !ready);
  reset();
  nested = 1;
  assert(ps5_app_posix_init(lookup, &report) == 0 && report.resolved == 23);
  assert(ps5_app_posix_init(lookup, &report) == -3);
  chunk = 1;
  assert(ps5_app_random_token(token, &report) == 0);
  assert(opens == 1 && reads == 16 && closes == 1);
  assert(!memcmp(token, "000102030405060708090a0b0c0d0e0f", 32));
  ps5_app_scrub(token, sizeof token);
  zero(token);
  assert(ps5_app_posix_shutdown(&report) == 0 && !ready);
  assert(ps5_app_read(9, token, 1) == -1 && reads == 16);
  assert(ps5_app_posix_init(lookup, &report) == -3);
  for (int fail = 1; fail <= 5; ++fail) {
    reset();
    assert(ps5_app_posix_init(lookup, &report) == 0);
    if (fail <= 3)
      read_failure = fail;
    else if (fail == 4)
      stat_failure = 1;
    else
      close_failure = 1;
    memset(token, 0xa5, sizeof token);
    assert(ps5_app_random_token(token, &report) < 0);
    zero(token);
    assert(opens == 1 && closes == 1 && reads <= (fail == 4 ? 0 : 1));
    if (fail == 5)
      assert(report.cleanup_flags == 1 && report.cleanup_errno == EINTR);
  }
  reset();
  assert(ps5_app_posix_init(lookup, &report) == 0);
  read_failure = 1;
  close_failure = 1;
  assert(ps5_app_random_token(token, &report) == -5 &&
         report.raw_errno == EINTR && report.cleanup_flags == 1);
  zero(token);
  assert(reads == 1 && closes == 1);
  reset();
  cancel_after = 1;
  assert(ps5_app_posix_init_cancel(lookup, cancelled, &report) == 1 &&
         lookup_count == 0 && !ready);
  reset();
  cancel_after = 5;
  assert(ps5_app_posix_init_cancel(lookup, cancelled, &report) == 1 &&
         lookup_count == 2 && !ready && opens == 0);
  for (int point = 1; point <= 4; ++point) {
    reset();
    assert(ps5_app_posix_init(lookup, &report) == 0);
    cancel_after = point;
    chunk = 1;
    assert(ps5_app_random_token_cancel(token, cancelled, &report) == 1);
    zero(token);
    assert(opens == (point > 1) && closes == opens &&
           reads == (point == 4 ? 1 : 0));
  }
  puts("PASS selected POSIX imports, atomic resolution, bounded token "
       "generation and consume-once cleanup");
  return 0;
}
