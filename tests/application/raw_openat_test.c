/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Every native call uses synthetic snapshots/FDs, never the host namespace. */
#include "../../platform/ps5/application/posix_imports.c"
#include "../../platform/ps5/application/raw_openat.c"
#include <assert.h>
#include <stdarg.h>
#include <stdio.h>

#define OWNER ((uid_t)1001)
static struct stat views[3];
static int live[3], closed[3], close_order[3], close_count, fd_base;
static int actual_fd[3];
static int imported_alias, checked_override;
static int64_t checked_value;
static int native_raw[8], native_errno[8], fake_errno;
static int close_raw[3], close_errno[3];
static unsigned native_calls, attempted, trace[7], partial_views, reenter_stage;
static int cancel_at, cancel_calls, cancel_on_close, close_cancelled;
static int cancel_reentry_at, close_reentry;

static int64_t primary(const struct ps5_app_raw_openat_report *r) {
  uint64_t bits = ((uint64_t)r->raw_return_hi << 32) | r->raw_return_lo;
  int64_t value;
  memcpy(&value, &bits, sizeof value);
  return value;
}
static int64_t checked_result(const struct ps5_app_raw_openat_report *r) {
  uint64_t bits = ((uint64_t)r->checked_open_hi << 32) | r->checked_open_lo;
  int64_t value;
  memcpy(&value, &bits, sizeof value);
  return value;
}
static unsigned count_bits(unsigned mask) {
  unsigned count = 0;
  for (; mask; mask >>= 1)
    count += mask & 1;
  return count;
}
static void refused(const struct ps5_app_raw_openat_report *r, unsigned prior) {
  struct ps5_app_raw_openat_report expected = {0};
  expected.abi_version = PS5_APP_RAW_OPENAT_ABI;
  expected.stage = 1;
  expected.status = -3;
  expected.prior_cleanup_mask = prior;
  expected.cleanup_errno = raw_openat_cleanup_errno;
  expected.cleanup_raw_return = raw_openat_cleanup_return;
  expected.prior_raw_ownership_unknown = raw_openat_ownership_unknown;
  assert(!memcmp(r, &expected, sizeof expected));
}
static void reentry(void) {
  unsigned before = native_calls;
  struct ps5_app_raw_openat_report report;
  memset(&report, 0xa5, sizeof report);
  assert(ps5_app_raw_openat(NULL, &report) == -3);
  refused(&report, raw_openat_uncertain);
  assert(native_calls == before);
}
static int operation(unsigned stage) {
  assert(stage >= 1 && stage <= 7 && native_calls < 7);
  assert(!native_calls || trace[native_calls - 1] < stage);
  assert(!(attempted & (1u << (stage - 1))));
  trace[native_calls++] = stage;
  attempted |= 1u << (stage - 1);
  if (reenter_stage == stage)
    reentry();
  if (native_raw[stage])
    fake_errno = native_errno[stage];
  return native_raw[stage];
}
static unsigned slot(int fd) {
  for (unsigned index = 0; index < 3; ++index)
    if (live[index] && actual_fd[index] == fd)
      return index;
  assert(!"descriptor must have known owned identity");
  return 0;
}
static void check_flags(int flags) {
  assert(flags == (O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC));
}
static uid_t fake_owner(void) {
  for (unsigned index = 0; index < 3; ++index)
    actual_fd[index] = fd_base + (int)index;
  assert(!operation(1));
  return OWNER;
}
static int fake_open(const char *name, int flags, ...) {
  check_flags(flags);
  assert(!strcmp(name, "/"));
  int raw = operation(2);
  if (raw)
    return raw;
  assert(!live[0]);
  live[0] = 1;
  return fd_base;
}
long __crt_syscall(long number, ...) {
  assert(number == PS5_APP_SOURCE_SYS_OPENAT);
  va_list args;
  va_start(args, number);
  long root = va_arg(args, long), name = va_arg(args, long),
       flags = va_arg(args, long);
  assert(slot((int)root) == 0 &&
         !strcmp((const char *)(uintptr_t)name, "data"));
  check_flags((int)flags);
  assert(va_arg(args, long) == 0 && va_arg(args, long) == 0 &&
         va_arg(args, long) == 0);
  va_end(args);
  int raw = operation(6);
  fake_errno = 9876; /* The checked route must not read this errno slot. */
  if (checked_override) {
    if (checked_value >= 0 && checked_value <= INT32_MAX &&
        checked_value != actual_fd[0] &&
        (!live[1] || checked_value != actual_fd[1])) {
      actual_fd[2] = (int)checked_value;
      live[2] = 1;
    }
    return (long)checked_value;
  }
  if (raw)
    return raw;
  assert(!live[2]);
  live[2] = 1;
  return fd_base + 2;
}
static int fake_openat(int fd, const char *name, int flags, ...) {
  assert(slot(fd) == 0 && !strcmp(name, "data"));
  check_flags(flags);
  int raw = operation(4);
  if (raw)
    return raw;
  if (imported_alias)
    return fd_base;
  assert(!live[1]);
  live[1] = 1;
  return fd_base + 1;
}
static int fake_stat(int fd, struct stat *out) {
  unsigned index = slot(fd);
  unsigned stage = index == 0 ? 3 : index == 1 ? 5 : 7;
  assert(!out->st_mode && !out->st_uid && !out->st_gid);
  assert(!out->st_dev && !out->st_ino);
  int raw = operation(stage);
  if (raw)
    return raw;
  if (partial_views & (1u << index))
    out->st_uid = views[index].st_uid;
  else
    *out = views[index];
  return 0;
}
static int fake_statat(int fd, const char *name, struct stat *out, int flags) {
  (void)fd;
  (void)name;
  (void)out;
  (void)flags;
  assert(!"open comparison must not call fstatat");
  return -1;
}
static int fake_close(int fd) {
  unsigned index = slot(fd);
  assert(!closed[index] && close_count < 3);
  live[index] = 0;
  closed[index] = 1;
  close_order[close_count++] = (int)index;
  if (close_reentry)
    reentry();
  if (cancel_on_close)
    close_cancelled = 1;
  if (close_raw[index])
    fake_errno = close_errno[index];
  return close_raw[index];
}
static int32_t cancelled(void) {
  ++cancel_calls;
  if (cancel_calls == cancel_reentry_at)
    reentry();
  return close_cancelled || (cancel_at && cancel_calls >= cancel_at);
}
static void reset(void) {
  memset(views, 0, sizeof views);
  for (unsigned index = 0; index < 3; ++index) {
    views[index].st_mode = S_IFDIR | 0755;
    views[index].st_uid = OWNER;
    views[index].st_gid = 100;
    views[index].st_dev = 17;
    views[index].st_ino = index ? 101 : 100;
  }
  memset(live, 0, sizeof live);
  memset(closed, 0, sizeof closed);
  memset(close_order, 0, sizeof close_order);
  memset(native_raw, 0, sizeof native_raw);
  memset(native_errno, 0, sizeof native_errno);
  memset(close_raw, 0, sizeof close_raw);
  memset(close_errno, 0, sizeof close_errno);
  memset(trace, 0, sizeof trace);
  close_count = 0;
  fd_base = 40;
  native_calls = attempted = partial_views = reenter_stage = 0;
  fake_errno = 0;
  cancel_at = cancel_calls = cancel_on_close = close_cancelled = 0;
  cancel_reentry_at = close_reentry = 0;
  raw_openat_active = 0;
  raw_openat_uncertain = 0;
  raw_openat_ownership_unknown = 0;
  imported_alias = checked_override = 0;
  checked_value = 0;
  raw_openat_cleanup_errno = raw_openat_cleanup_return = 0;
  memset(&functions, 0, sizeof functions);
  functions.geteuid_call = fake_owner;
  functions.open_call = fake_open;
  functions.openat_call = fake_openat;
  functions.fstat_call = fake_stat;
  functions.fstatat_call = fake_statat;
  functions.close_call = fake_close;
  ready = 1;
  native_error = &fake_errno;
}
static struct ps5_app_raw_openat_report run(int expected, int use_cancel) {
  struct ps5_app_raw_openat_report report;
  memset(&report, 0xa5, sizeof report);
  assert(ps5_app_raw_openat(use_cancel ? cancelled : NULL, &report) ==
         expected);
  assert(report.status == expected &&
         report.abi_version == PS5_APP_RAW_OPENAT_ABI);
  assert(report.operations == native_calls && report.operations <= 7);
  assert(report.attempted_mask == attempted);
  assert(count_bits(attempted) == report.operations);
  assert(report.opened_mask == report.closed_mask && close_count <= 3);
  for (unsigned index = 0; index < 3; ++index) {
    assert(!live[index]);
    assert((unsigned)closed[index] == ((report.closed_mask >> index) & 1));
  }
  for (int index = 1; index < close_count; ++index)
    assert(close_order[index - 1] > close_order[index]);
  assert(!(report.root_reject & ~7u) && !(report.imported_reject & ~7u));
  assert(!(report.checked_reject & ~7u) && !(report.data_match_mask & ~31u));
  assert(!report.reserved);
  if (!(report.observed_mask & 1))
    assert(!report.root_reject);
  if (!(report.observed_mask & 2))
    assert(!report.imported_reject && !report.imported_same_device);
  if (!(report.observed_mask & 4))
    assert(!report.checked_reject && !report.checked_same_device);
  if ((report.observed_mask & 6) != 6)
    assert(!report.data_match_mask);
  if (!report.failed_stage)
    assert(!primary(&report)); /* Never a successful descriptor or UID. */
  return report;
}
static void next_clean_run(void) {
  assert(!raw_openat_active && !raw_openat_uncertain);
  for (unsigned index = 0; index < 3; ++index)
    assert(!live[index]);
  native_calls = attempted = 0;
  close_count = 0;
  memset(closed, 0, sizeof closed);
  memset(close_order, 0, sizeof close_order);
  memset(trace, 0, sizeof trace);
}

int main(void) {
  struct ps5_app_raw_openat_report report;
  reset();
  report = run(0, 0);
  assert(sizeof report == 136 && report.stage == 8);
  assert(offsetof(struct ps5_app_raw_openat_report, failed_stage) == 24);
  assert(offsetof(struct ps5_app_raw_openat_report, imported_open_raw) == 76);
  assert(offsetof(struct ps5_app_raw_openat_report, data_match_mask) == 112);
  assert(offsetof(struct ps5_app_raw_openat_report, reserved) == 132);
  assert(report.operations == 7 && report.attempted_mask == 127);
  assert(report.opened_mask == 7 && report.observed_mask == 7);
  assert(report.data_match_mask == 31 && report.imported_same_device == 2);
  assert(report.checked_same_device == 2 && !report.failed_stage);
  assert(report.imported_open_raw == 41 && checked_result(&report) == 42);
  next_clean_run();
  views[1].st_mode = S_IFDIR | 0777;
  report = run(0, 0);
  assert(report.imported_reject == 4 && report.data_match_mask == 27);
  next_clean_run();
  views[1].st_mode = S_IFDIR | 0755;
  report = run(0, 0);
  assert(!report.imported_reject && report.data_match_mask == 31);
  reset();
  fd_base = 0;
  report = run(0, 0);
  assert(report.closed_mask == 7 && report.imported_open_raw == 1);

  /* All 64 combinations of normal native errors: prerequisite failure stops;
   * either later case fails independently, with no retry or invalid-fd stat. */
  for (unsigned faults = 0; faults < 64; ++faults) {
    reset();
    for (unsigned stage = 2; stage <= 7; ++stage) {
      if (faults & (1u << (stage - 2))) {
        native_raw[stage] = stage == 2 || stage == 4 || stage == 6
                                ? -(int)(100 + stage)
                                : (int)(100 + stage);
        native_errno[stage] = (int)(200 + stage);
      }
    }
    report = run(faults ? -1 : 0, 0);
    unsigned first = 0;
    for (unsigned index = 0; index < native_calls; ++index) {
      if (native_raw[trace[index]]) {
        first = trace[index];
        break;
      }
    }
    assert(report.failed_stage == first);
    if (first) {
      assert(primary(&report) == native_raw[first]);
      assert(report.raw_errno == (first == 6 ? 0 : native_errno[first]));
    }
    if (native_raw[2]) {
      assert(report.stage == 2 && report.attempted_mask == 3);
      assert(!report.opened_mask && !report.observed_mask);
    } else if (native_raw[3]) {
      assert(report.stage == 3 && report.attempted_mask == 7);
      assert(report.opened_mask == 1 && !report.observed_mask);
    } else {
      assert(report.stage == 8 && (report.attempted_mask & 40) == 40);
      assert(((report.attempted_mask & 16) != 0) == !native_raw[4]);
      assert(((report.attempted_mask & 64) != 0) == !native_raw[6]);
      assert(report.imported_open_raw ==
             (native_raw[4] ? native_raw[4] : fd_base + 1));
      assert(checked_result(&report) ==
             (native_raw[6] ? native_raw[6] : fd_base + 2));
      assert(report.imported_open_errno == native_errno[4]);
      assert(report.checked_open_kind == (native_raw[6] ? 2u : 1u));
      assert(report.imported_stat_raw == (native_raw[4] ? 0 : native_raw[5]));
      assert(report.imported_stat_errno ==
             (native_raw[4] ? 0 : native_errno[5]));
      assert(report.checked_stat_raw == (native_raw[6] ? 0 : native_raw[7]));
      assert(report.checked_stat_errno ==
             (native_raw[6] ? 0 : native_errno[7]));
    }
  }
  for (unsigned stage = 2; stage <= 7; ++stage) {
    reset();
    native_raw[stage] = -1;
    native_errno[stage] = EINTR;
    report = run(-1, 0);
    assert(report.failed_stage == stage &&
           report.raw_errno == (stage == 6 ? 0 : EINTR));
  }
  reset();
  native_raw[4] = -1;
  native_errno[4] = 0;
  report = run(-1, 0);
  assert(report.failed_stage == 4 && !report.raw_errno);
  assert(report.observed_mask == 5 && report.opened_mask == 5);

  /* Every cancellation point stops new normal operations. A prior case error
   * stays primary, but cannot suppress Stop before the independent case. */
  for (int point = 1; point <= 9; ++point) {
    reset();
    cancel_at = point;
    report = run(1, 1);
    unsigned completed = point <= 7 ? (unsigned)point - 1 : 7;
    assert(report.operations == completed);
    assert(report.stage == (point <= 7 ? (unsigned)point : 8));
    assert(report.attempted_mask == (1u << completed) - 1);
    assert(!report.failed_stage && !primary(&report) && !report.raw_errno);
  }
  for (int point = 5; point <= 8; ++point) {
    reset();
    native_raw[4] = -1;
    native_errno[4] = EPERM;
    cancel_at = point;
    report = run(-1, 1);
    assert(report.failed_stage == 4 && report.raw_errno == EPERM);
    assert(report.imported_open_raw == -1 && !report.imported_stat_raw);
    assert(!(report.attempted_mask & 16));
    assert(report.stage == (point == 5 ? 6u : point == 6 ? 7u : 8u));
    assert(report.operations == (point == 5 ? 4u : point == 6 ? 5u : 6u));
  }
  reset();
  cancel_on_close = 1;
  report = run(1, 1);
  assert(report.stage == 8 && report.closed_mask == 7);

  /* Full-width checked errors never read __error or narrow to a descriptor.
   * In-range boundary descriptors, including zero, are consumed once. */
  const int64_t negative[] = {-1, -123, INT64_MIN, -INT64_C(4294967297)};
  for (unsigned index = 0; index < sizeof negative / sizeof negative[0];
       ++index) {
    reset();
    checked_override = 1;
    checked_value = negative[index];
    report = run(-1, 0);
    assert(report.stage == 8 && report.failed_stage == 6);
    assert(primary(&report) == checked_value &&
           checked_result(&report) == checked_value);
    assert(!report.raw_errno && report.checked_open_kind == 2);
    assert(report.opened_mask == 3 && report.observed_mask == 3);
    assert(!(report.attempted_mask & 64) && !report.raw_ownership_unknown);
  }
  const int64_t valid_fds[] = {0, INT32_MAX};
  for (unsigned index = 0; index < sizeof valid_fds / sizeof valid_fds[0];
       ++index) {
    reset();
    checked_override = 1;
    checked_value = valid_fds[index];
    report = run(0, 0);
    assert(report.checked_open_kind == 1 && report.opened_mask == 7);
    assert(checked_result(&report) == checked_value && report.closed_mask == 7);
  }

  /* Unknown-width/aliased results do not confer new ownership. Clean only
   * known descriptors; do not stat, guess a cast, double close, or retry. */
  const int64_t anomalous[] = {INT64_C(2147483648), INT64_C(4294967295),
                               INT64_MAX, 40, 41};
  for (unsigned index = 0; index < sizeof anomalous / sizeof anomalous[0];
       ++index) {
    reset();
    checked_override = 1;
    checked_value = anomalous[index];
    report = run(-4, 0);
    assert(report.stage == 6 && report.failed_stage == 6);
    assert(primary(&report) == checked_value &&
           checked_result(&report) == checked_value);
    assert(report.checked_open_kind == (checked_value > INT32_MAX ? 3u : 4u));
    assert(report.raw_ownership_unknown == 1 && !report.raw_errno);
    assert(report.opened_mask == 3 && report.closed_mask == 3 &&
           close_count == 2);
    assert(!report.cleanup_failed_mask && report.observed_mask == 3);
    assert(report.operations == 6 && report.attempted_mask == 63);
    unsigned calls = native_calls;
    assert(ps5_app_raw_openat(NULL, &report) == -3);
    refused(&report, 0);
    assert(report.prior_raw_ownership_unknown == 1 && native_calls == calls);
  }
  reset();
  imported_alias = 1;
  report = run(-4, 0);
  assert(report.stage == 4 && report.failed_stage == 4);
  assert(primary(&report) == fd_base && report.imported_open_raw == fd_base);
  assert(report.raw_ownership_unknown == 1 && report.checked_open_kind == 0);
  assert(report.opened_mask == 1 && report.closed_mask == 1 &&
         close_count == 1);
  assert(report.operations == 4 && report.attempted_mask == 15);
  reset();
  native_raw[4] = -1;
  native_errno[4] = EPERM;
  checked_override = 1;
  checked_value = INT64_C(4294967297);
  close_raw[0] = -1;
  close_errno[0] = EINTR;
  report = run(-1, 0);
  assert(report.stage == 6 && report.failed_stage == 4);
  assert(primary(&report) == -1 && report.raw_errno == EPERM);
  assert(report.raw_ownership_unknown == 1 && report.cleanup_failed_mask == 1);
  assert(report.opened_mask == 1 && report.closed_mask == 1);
  assert(report.operations == 5 && report.attempted_mask == 47);
  unsigned calls = native_calls;
  assert(ps5_app_raw_openat(NULL, &report) == -3);
  refused(&report, 1);
  assert(report.prior_raw_ownership_unknown == 1 && native_calls == calls);

  /* Every close-failure combination consumes exactly once, retains the first
   * reverse-order result (even errno0/positive raw), and latches refusal. */
  for (unsigned mask = 1; mask <= 7; ++mask) {
    reset();
    for (unsigned index = 0; index < 3; ++index) {
      if (mask & (1u << index)) {
        close_raw[index] = index ? -(int)(10 + index) : 5;
        close_errno[index] = index ? EINTR : 0;
      }
    }
    report = run(-1, 0);
    unsigned first = mask & 4 ? 2 : mask & 2 ? 1 : 0;
    assert(report.cleanup_failed_mask == mask && report.stage == 8);
    assert(report.cleanup_errno == close_errno[first]);
    assert(report.cleanup_raw_return == close_raw[first]);
    assert(!report.failed_stage && !primary(&report));
    assert(report.raw_errno == close_errno[first]);
    unsigned calls = native_calls;
    assert(ps5_app_raw_openat(NULL, &report) == -3);
    refused(&report, mask);
    assert(native_calls == calls && close_count == 3);
  }
  reset();
  native_raw[4] = -1;
  native_errno[4] = EPERM;
  close_raw[2] = -1;
  close_errno[2] = EINTR;
  cancel_on_close = 1;
  report = run(-1, 1);
  assert(report.failed_stage == 4 && report.raw_errno == EPERM);
  assert(report.cleanup_failed_mask == 4 && report.cleanup_errno == EINTR);
  reset();
  cancel_at = 7;
  close_raw[2] = -1;
  close_errno[2] = EINTR;
  report = run(1, 1);
  assert(report.stage == 7 && !report.failed_stage && !report.raw_errno);
  assert(report.cleanup_failed_mask == 4 && report.cleanup_errno == EINTR);

  /* Rejections/mismatches are only observations, even when all are present. */
  for (unsigned index = 0; index < 3; ++index) {
    reset();
    views[index].st_mode = S_IFREG | 0777;
    views[index].st_uid = OWNER + 1;
    report = run(0, 0);
    const uint32_t rejects[] = {report.root_reject, report.imported_reject,
                                report.checked_reject};
    assert(rejects[index] == 7 && report.observed_mask == 7);
    reset();
    views[index].st_mode = S_IFDIR | 01777;
    views[index].st_uid = 0;
    report = run(0, 0);
    assert(!report.root_reject && !report.imported_reject &&
           !report.checked_reject);
  }
  for (unsigned field = 0; field < 5; ++field) {
    reset();
    if (field == 0)
      ++views[2].st_dev;
    else if (field == 1)
      ++views[2].st_ino;
    else if (field == 2)
      views[2].st_mode ^= 0100;
    else if (field == 3)
      ++views[2].st_uid;
    else
      ++views[2].st_gid;
    report = run(0, 0);
    assert(report.data_match_mask == (31u ^ (1u << field)));
    assert(report.imported_same_device == 2);
    assert(report.checked_same_device == (field == 0 ? 1u : 2u));
  }
  reset();
  partial_views = 7;
  report = run(0, 0);
  assert(report.root_reject == 1 && report.imported_reject == 1);
  assert(report.checked_reject == 1 && report.data_match_mask == 31);

  for (unsigned stage = 1; stage <= 7; ++stage) {
    reset();
    reenter_stage = stage;
    run(0, 0);
  }
  for (int point = 1; point <= 9; ++point) {
    reset();
    cancel_reentry_at = point;
    run(0, 1);
  }
  reset();
  close_reentry = 1;
  run(0, 0);
  reset();
  assert(ps5_app_raw_openat(NULL, NULL) == -2 && !native_calls);
  memset(&report, 0, sizeof report);
  report.operations = 7;
  assert(!route_checkpoint(NULL, &report, 7));
  assert(report.status == -2 && report.operations == 7 && !native_calls);
  memset(&report, 0, sizeof report);
  report.attempted_mask = 8;
  assert(!route_checkpoint(NULL, &report, 4));
  assert(report.status == -2 && !report.operations && !native_calls);
  puts("raw openat route synthetic checks passed");
  return 0;
}
