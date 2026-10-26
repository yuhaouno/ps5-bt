/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Synthetic snapshots/exports only. Never opens the host's root or /data. */
#include "../../platform/ps5/application/namespace_probe.c"
#include "../../platform/ps5/application/posix_imports.c"
#include <assert.h>
#include <stdio.h>

#define OWNER ((uid_t)1001)
static struct stat views[4];
static int live[2], closed[2], close_order[2], close_count, fd_base;
static unsigned native_calls, fail_stage, partial_views, reenter_stage;
static int fake_errno, fail_errno, fail_raw;
static int close_raw[2], close_errno[2];
static int cancel_at, cancel_calls, cancel_on_close, close_cancelled;
static int cancel_reentry_at, close_reentry;

static void refused_report(const struct ps5_app_namespace_report *r,
                           unsigned prior) {
  assert(r->abi_version == PS5_APP_NAMESPACE_ABI && r->stage == 1);
  assert(r->status == -3 && r->prior_cleanup_mask == prior);
  assert(!r->operations && !r->opened_mask && !r->closed_mask);
  assert(!r->raw_errno && !r->raw_return && !r->cleanup_failed_mask);
  assert(!r->observed_mask && !r->root_fd_reject && !r->root_path_reject);
  assert(!r->data_path_reject && !r->data_fd_reject);
  assert(!r->root_match_mask && !r->data_match_mask && !r->same_device);
}
static void reentry(void) {
  unsigned before = native_calls;
  struct ps5_app_namespace_report report;
  memset(&report, 0xa5, sizeof report);
  assert(ps5_app_namespace_probe(NULL, &report) == -3);
  refused_report(&report, namespace_uncertain);
  assert(native_calls == before);
}
static int operation(unsigned stage) {
  assert(++native_calls == stage && native_calls <= 7);
  if (reenter_stage == stage)
    reentry();
  if (fail_stage == stage) {
    fake_errno = fail_errno;
    return 1;
  }
  return 0;
}
static unsigned slot(int fd) {
  assert(fd >= fd_base && fd < fd_base + 2);
  unsigned index = (unsigned)(fd - fd_base);
  assert(live[index]);
  return index;
}
static void check_flags(int flags) {
  assert(flags == (O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC));
}
static uid_t fake_owner(void) {
  assert(!operation(1)); /* geteuid has no error return. */
  return OWNER;
}
static int fake_open(const char *name, int flags, ...) {
  assert(!strcmp(name, "/"));
  check_flags(flags);
  if (operation(2))
    return fail_raw;
  assert(!live[0]);
  live[0] = 1;
  return fd_base;
}
static int fake_openat(int fd, const char *name, int flags, ...) {
  assert(slot(fd) == 0 && !strcmp(name, "data"));
  check_flags(flags);
  if (operation(6))
    return fail_raw;
  assert(!live[1]);
  live[1] = 1;
  return fd_base + 1;
}
static void snapshot(unsigned index, struct stat *out) {
  /* All accessed fields start zero even if a broken native fixture reports
   * success with partial output. This does not certify any native layout. */
  assert(!out->st_mode && !out->st_uid && !out->st_gid);
  assert(!out->st_dev && !out->st_ino);
  if (partial_views & (1u << index))
    out->st_uid = views[index].st_uid;
  else
    *out = views[index];
}
static int fake_stat(int fd, struct stat *out) {
  unsigned index = slot(fd);
  if (operation(index ? 7 : 3))
    return fail_raw;
  snapshot(index ? 3 : 0, out);
  return 0;
}
static int fake_statat(int fd, const char *name, struct stat *out, int flags) {
  assert(slot(fd) == 0 && flags == AT_SYMLINK_NOFOLLOW);
  unsigned index = !strcmp(name, ".") ? 1 : 2;
  assert(index == 1 || !strcmp(name, "data"));
  if (operation(index == 1 ? 4 : 5))
    return fail_raw;
  snapshot(index, out);
  return 0;
}
static int fake_close(int fd) {
  unsigned index = slot(fd);
  assert(!closed[index] && close_count < 2);
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
  for (unsigned index = 0; index < 4; ++index) {
    views[index].st_mode = S_IFDIR | 0755;
    views[index].st_uid = OWNER;
    views[index].st_gid = 100;
    views[index].st_dev = 17;
    views[index].st_ino = index < 2 ? 100 : 101;
  }
  memset(live, 0, sizeof live);
  memset(closed, 0, sizeof closed);
  memset(close_order, 0, sizeof close_order);
  memset(close_raw, 0, sizeof close_raw);
  memset(close_errno, 0, sizeof close_errno);
  close_count = 0;
  fd_base = 40;
  native_calls = fail_stage = partial_views = reenter_stage = 0;
  fake_errno = 0;
  fail_errno = EIO;
  fail_raw = -1;
  cancel_at = cancel_calls = cancel_on_close = close_cancelled = 0;
  cancel_reentry_at = close_reentry = 0;
  namespace_active = 0;
  namespace_uncertain = 0;
  namespace_cleanup_errno = namespace_cleanup_return = 0;
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
static struct ps5_app_namespace_report run(int expected, int use_cancel) {
  struct ps5_app_namespace_report report;
  memset(&report, 0xa5, sizeof report);
  assert(ps5_app_namespace_probe(use_cancel ? cancelled : NULL, &report) ==
         expected);
  assert(report.status == expected &&
         report.abi_version == PS5_APP_NAMESPACE_ABI);
  assert(report.operations == native_calls && report.operations <= 7);
  assert(report.opened_mask == report.closed_mask && close_count <= 2);
  for (unsigned index = 0; index < 2; ++index) {
    assert(!live[index]);
    assert((unsigned)closed[index] == ((report.closed_mask >> index) & 1));
  }
  if (close_count == 2)
    assert(close_order[0] == 1 && close_order[1] == 0);
  assert(!(report.root_fd_reject & ~7u) && !(report.root_path_reject & ~7u));
  assert(!(report.data_path_reject & ~7u) && !(report.data_fd_reject & ~7u));
  assert(!(report.root_match_mask & ~31u) && !(report.data_match_mask & ~31u));
  if ((report.observed_mask & 3) != 3)
    assert(!report.root_match_mask);
  if ((report.observed_mask & 12) != 12)
    assert(!report.data_match_mask);
  if ((report.observed_mask & 9) != 9)
    assert(!report.same_device);
  return report;
}
static void next_clean_run(void) {
  /* Reset only the fake observations, never the implementation's guards. */
  assert(!namespace_active && !namespace_uncertain && !live[0] && !live[1]);
  native_calls = 0;
  close_count = 0;
  memset(closed, 0, sizeof closed);
  memset(close_order, 0, sizeof close_order);
}

int main(void) {
  struct ps5_app_namespace_report report;
  reset();
  report = run(0, 0);
  assert(sizeof report == 80 && report.stage == 8 && report.operations == 7);
  assert(offsetof(struct ps5_app_namespace_report, status) == 8);
  assert(offsetof(struct ps5_app_namespace_report, observed_mask) == 48);
  assert(offsetof(struct ps5_app_namespace_report, same_device) == 76);
  assert(report.opened_mask == 3 && report.observed_mask == 15);
  assert(report.root_match_mask == 31 && report.data_match_mask == 31);
  assert(report.same_device == 2 && !report.raw_return && !report.raw_errno);
  next_clean_run();
  views[0].st_mode = S_IFDIR | 0777;
  report = run(0, 0);
  assert(report.root_fd_reject == 4 && report.root_match_mask == 27);
  next_clean_run();
  views[0].st_mode = S_IFDIR | 0755;
  report = run(0, 0);
  assert(!report.root_fd_reject && report.root_match_mask == 31);
  reset();
  fd_base = 0;
  report = run(0, 0);
  assert(report.closed_mask == 3); /* Descriptor zero is owned too. */

  /* Predicate rejection is observation only, never protected-store acceptance.
   * Read-only work continues even for type/owner/writability inconsistencies.
   */
  for (unsigned index = 0; index < 4; ++index) {
    reset();
    views[index].st_mode = S_IFREG | 0777;
    views[index].st_uid = OWNER + 1;
    report = run(0, 0);
    const uint32_t rejects[] = {report.root_fd_reject, report.root_path_reject,
                                report.data_path_reject, report.data_fd_reject};
    assert(rejects[index] == 7 && report.observed_mask == 15);
    reset();
    views[index].st_mode = S_IFDIR | 01777;
    report = run(0, 0);
    assert(!report.root_fd_reject && !report.root_path_reject);
    assert(!report.data_path_reject && !report.data_fd_reject);
    reset();
    views[index].st_uid = 0;
    report = run(0, 0);
    assert(!report.root_fd_reject && !report.root_path_reject);
    assert(!report.data_path_reject && !report.data_fd_reject);
  }
  for (unsigned field = 0; field < 5; ++field) {
    for (unsigned index = 1; index <= 3; index += 2) {
      reset();
      if (field == 0)
        ++views[index].st_dev;
      else if (field == 1)
        ++views[index].st_ino;
      else if (field == 2)
        views[index].st_mode ^= 0100;
      else if (field == 3)
        ++views[index].st_uid;
      else
        ++views[index].st_gid;
      report = run(0, 0);
      assert((index == 1 ? report.root_match_mask : report.data_match_mask) ==
             (31u ^ (1u << field)));
      assert(report.same_device == (index == 3 && field == 0 ? 1u : 2u));
    }
  }
  reset();
  partial_views = 15;
  report = run(0, 0);
  assert(report.root_fd_reject == 1 && report.root_path_reject == 1);
  assert(report.data_path_reject == 1 && report.data_fd_reject == 1);
  assert(report.root_match_mask == 31 && report.data_match_mask == 31);

  /* Every fallible normal call fails without retry; native errno and even
   * nonstandard nonzero returns survive. geteuid has no error return. */
  const unsigned opened_before[] = {0, 0, 0, 1, 1, 1, 1, 3};
  const unsigned observed_before[] = {0, 0, 0, 0, 1, 3, 7, 7};
  for (unsigned stage = 2; stage <= 7; ++stage) {
    for (unsigned interrupted = 0; interrupted < 2; ++interrupted) {
      reset();
      fail_stage = stage;
      fail_errno = interrupted ? EINTR : EIO;
      report = run(-1, 0);
      assert(report.stage == stage && report.operations == stage);
      assert(report.raw_return == -1 && report.raw_errno == fail_errno);
      assert(report.opened_mask == opened_before[stage]);
      assert(report.observed_mask == observed_before[stage]);
      assert(!report.cleanup_failed_mask);
    }
  }
  reset();
  fail_stage = 4;
  fail_raw = 5;
  fail_errno = 0;
  report = run(-1, 0);
  assert(report.raw_return == 5 && !report.raw_errno);

  /* Every cancellation check, including final and post-cleanup, preserves
   * completed observations and closes all acquired descriptors once. */
  for (int point = 1; point <= 9; ++point) {
    reset();
    cancel_at = point;
    report = run(1, 1);
    unsigned completed = point <= 7 ? (unsigned)point - 1 : 7;
    assert(report.operations == completed);
    assert(report.stage == (point <= 7 ? (unsigned)point : 8));
    assert(report.opened_mask == (completed < 2   ? 0u
                                  : completed < 6 ? 1u
                                                  : 3u));
    assert(report.observed_mask == (completed < 3   ? 0u
                                    : completed < 4 ? 1u
                                    : completed < 5 ? 3u
                                    : completed < 7 ? 7u
                                                    : 15u));
    if (completed <= 1)
      assert(!report.raw_return); /* Native UID never enters the report. */
  }
  reset();
  cancel_on_close = 1;
  report = run(1, 1);
  assert(report.stage == 8 && report.closed_mask == 3);

  /* Each uncertain close is consumed; retain the first reverse-order close
   * failure and refuse later invocations without any native operation. */
  for (unsigned mask = 1; mask <= 3; ++mask) {
    reset();
    for (unsigned index = 0; index < 2; ++index) {
      if (mask & (1u << index)) {
        close_raw[index] = index ? -17 : 5;
        close_errno[index] = index ? EINTR : 0;
      }
    }
    report = run(-1, 0);
    unsigned first = mask & 2 ? 1 : 0;
    assert(report.cleanup_failed_mask == mask && report.stage == 8);
    assert(report.cleanup_errno == close_errno[first]);
    assert(report.cleanup_raw_return == close_raw[first]);
    unsigned calls = native_calls;
    memset(&report, 0xa5, sizeof report);
    assert(ps5_app_namespace_probe(NULL, &report) == -3);
    refused_report(&report, mask);
    assert(report.cleanup_errno == close_errno[first]);
    assert(report.cleanup_raw_return == close_raw[first]);
    assert(native_calls == calls && close_count == 2);
  }
  reset();
  fail_stage = 7;
  fail_errno = EPERM;
  close_raw[1] = -1;
  close_errno[1] = EINTR;
  cancel_on_close = 1;
  report = run(-1, 1);
  assert(report.stage == 7 && report.raw_errno == EPERM);
  assert(report.cleanup_failed_mask == 2 && report.cleanup_errno == EINTR);
  reset();
  cancel_at = 7;
  close_raw[1] = -1;
  close_errno[1] = EINTR;
  report = run(1, 1);
  assert(report.stage == 7 && !report.raw_errno);
  assert(report.cleanup_failed_mask == 2 && report.cleanup_errno == EINTR);

  /* Reentry from every normal native call, a cancellation callback and close
   * cannot alter ownership or leak a previous report. */
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
  assert(ps5_app_namespace_probe(NULL, NULL) == -2 && !native_calls);
  memset(&report, 0, sizeof report);
  report.operations = 7;
  assert(!namespace_checkpoint(NULL, &report, 7));
  assert(report.status == -2 && report.operations == 7 && !native_calls);
  puts("namespace probe synthetic checks passed");
  return 0;
}
