/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Synthetic nodes and descriptors only. No host /data access or native calls.
 */
#include "../../platform/ps5/application/posix_imports.c"
#include "../../platform/ps5/application/setup_namespace.c"
#include <assert.h>
#include <stdio.h>

#define OWNER ((uid_t)1001)
#define FD_BASE 40
struct node {
  int exists;
  struct stat info;
};
static struct node nodes[4];
static int live[4], closed[4], close_order[4], close_count;
static unsigned native_calls, fail_at, mkdir_calls, open_calls, sync_calls;
static int fake_errno, fail_errno, close_fail_slot, cancel_at, cancel_calls;
static int lookup_null, lookup_dirty, lookup_nested, substitute_slot;
static int substitute_device;
static unsigned close_fail_mask;
static int close_raw_return, close_errno, cancel_on_close, close_cancelled;
static int cancel_reentry_at;
static int64_t lookup(const char *, void **);
static int64_t lookup_failure;
static int mkdir_side_effect_failure;
static mode_t created_mode;
static uid_t created_owner;
static unsigned trace[32];
enum operation { LOOKUP = 1, OWNER_CALL, OPEN, STAT, STAT_AT, MKDIR, SYNC };

static int operation(unsigned kind) {
  assert(native_calls < 32);
  trace[native_calls++] = kind;
  if (native_calls == fail_at) {
    fake_errno = fail_errno;
    return 1;
  }
  return 0;
}
static unsigned slot(int fd) {
  assert(fd >= FD_BASE && fd < FD_BASE + 4);
  unsigned index = (unsigned)(fd - FD_BASE);
  assert(live[index]);
  return index;
}
static unsigned child(int fd, const char *name) {
  unsigned parent = slot(fd);
  static const char *const names[] = {"data", "ps5-bt", "control"};
  assert(parent < 3 && !strcmp(name, names[parent]));
  return parent + 1;
}
static void flags(int value) {
  assert(value == (O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC));
}
static int fake_open(const char *name, int value, ...) {
  assert(!strcmp(name, "/"));
  flags(value);
  ++open_calls;
  if (operation(OPEN))
    return -1;
  if (!nodes[0].exists) {
    fake_errno = ENOENT;
    return -1;
  }
  assert(!live[0]);
  live[0] = 1;
  return FD_BASE;
}
static int fake_openat(int fd, const char *name, int value, ...) {
  unsigned index = child(fd, name);
  flags(value);
  ++open_calls;
  if (operation(OPEN))
    return -1;
  if (!nodes[index].exists) {
    fake_errno = ENOENT;
    return -1;
  }
  assert(!live[index]);
  live[index] = 1;
  return FD_BASE + (int)index;
}
static int fake_stat(int fd, struct stat *out) {
  unsigned index = slot(fd);
  if (operation(STAT))
    return -1;
  *out = nodes[index].info;
  if (substitute_slot == (int)index) {
    if (substitute_device)
      ++out->st_dev;
    else
      ++out->st_ino;
  }
  return 0;
}
static int fake_statat(int fd, const char *name, struct stat *out, int value) {
  unsigned index = child(fd, name);
  assert(value == AT_SYMLINK_NOFOLLOW);
  if (operation(STAT_AT))
    return -1;
  if (!nodes[index].exists) {
    fake_errno = ENOENT;
    return -1;
  }
  *out = nodes[index].info;
  return 0;
}
static int fake_mkdirat(int fd, const char *name, mode_t mode) {
  unsigned index = child(fd, name);
  assert(index >= 2 && mode == 0700);
  ++mkdir_calls;
  if (operation(MKDIR)) {
    if (mkdir_side_effect_failure)
      nodes[index].exists = 1;
    return -1;
  }
  assert(!nodes[index].exists);
  nodes[index].exists = 1;
  nodes[index].info.st_mode = S_IFDIR | created_mode;
  nodes[index].info.st_uid = created_owner;
  return 0;
}
static int fake_sync(int fd) {
  (void)slot(fd);
  ++sync_calls;
  return operation(SYNC) ? -1 : 0;
}
static uid_t fake_owner(void) {
  /* geteuid has no error return; failures are injected only in fallible calls.
   */
  assert(!operation(OWNER_CALL));
  return OWNER;
}
static int fake_close(int fd) {
  unsigned index = slot(fd);
  assert(!closed[index] && close_count < 4);
  live[index] = 0;
  if (cancel_on_close)
    close_cancelled = 1;
  closed[index] = 1;
  close_order[close_count++] = (int)index;
  if (close_fail_slot == (int)index || (close_fail_mask & (1u << index))) {
    fake_errno = close_errno;
    return close_raw_return;
  }
  return 0;
}
static int32_t cancelled(void) {
  ++cancel_calls;
  if (cancel_calls == cancel_reentry_at) {
    struct ps5_app_setup_report nested_report;
    assert(ps5_app_setup_namespace(lookup, NULL, &nested_report) == -3);
    assert(!nested_report.operations && !nested_report.opened_mask);
  }
  return close_cancelled || (cancel_at && cancel_calls >= cancel_at);
}
static int64_t lookup(const char *name, void **out) {
  assert(!strcmp(name, "mkdirat"));
  *out = NULL;
  if (lookup_nested) {
    struct ps5_app_setup_report nested_report;
    lookup_nested = 0;
    assert(ps5_app_setup_namespace(lookup, NULL, &nested_report) == -3);
    assert(nested_report.operations == 0);
  }
  if (operation(LOOKUP) || lookup_failure) {
    if (lookup_dirty)
      *out = (void *)(uintptr_t)1;
    return lookup_failure ? lookup_failure : INT64_C(0x100000007);
  }
  if (!lookup_null) {
    __typeof__(&fake_mkdirat) callable = fake_mkdirat;
    _Static_assert(sizeof callable == sizeof *out, "callable ABI");
    memcpy(out, &callable, sizeof callable);
  }
  return 0;
}
static void reset(void) {
  memset(nodes, 0, sizeof nodes);
  memset(live, 0, sizeof live);
  memset(closed, 0, sizeof closed);
  memset(close_order, 0, sizeof close_order);
  memset(trace, 0, sizeof trace);
  for (unsigned index = 0; index < 4; ++index) {
    nodes[index].exists = index < 2;
    nodes[index].info.st_mode = S_IFDIR | (index < 2 ? 0755 : 0700);
    nodes[index].info.st_uid = index < 2 ? 0 : OWNER;
    nodes[index].info.st_dev = 17;
    nodes[index].info.st_ino = 100 + index;
  }
  close_count = 0;
  native_calls = fail_at = mkdir_calls = open_calls = sync_calls = 0;
  fake_errno = 0;
  fail_errno = EIO;
  close_fail_slot = substitute_slot = -1;
  close_fail_mask = 0;
  close_raw_return = -1;
  close_errno = EINTR;
  cancel_on_close = close_cancelled = cancel_reentry_at = 0;
  cancel_at = cancel_calls = 0;
  lookup_null = lookup_dirty = lookup_nested = substitute_device = 0;
  lookup_failure = 0;
  mkdir_side_effect_failure = 0;
  created_mode = 0700;
  created_owner = OWNER;
  setup_active = 0;
  setup_uncertain = 0;
  setup_cleanup_errno = setup_cleanup_return = 0;
  memset(&functions, 0, sizeof functions);
  functions.open_call = fake_open;
  functions.openat_call = fake_openat;
  functions.fstat_call = fake_stat;
  functions.fstatat_call = fake_statat;
  functions.fsync_call = fake_sync;
  functions.close_call = fake_close;
  functions.geteuid_call = fake_owner;
  ready = 1;
  native_error = &fake_errno;
}
static void fully_closed(const struct ps5_app_setup_report *report) {
  assert(report->opened_mask == report->closed_mask);
  unsigned mask = 0;
  for (unsigned index = 0; index < 4; ++index) {
    assert(!live[index]);
    if (closed[index])
      mask |= 1u << index;
  }
  assert(mask == report->closed_mask);
  for (int index = 1; index < close_count; ++index)
    assert(close_order[index - 1] > close_order[index]);
  assert(report->operations <= 32 && close_count <= 4);
  if (report->created_mask & 1)
    assert(nodes[2].exists);
  if (report->created_mask & 2)
    assert(nodes[3].exists);
}
static struct ps5_app_setup_report run(int expected) {
  struct ps5_app_setup_report report;
  assert(ps5_app_setup_namespace(
             lookup,
             (cancel_at || cancel_on_close || cancel_reentry_at) ? cancelled
                                                                 : NULL,
             &report) == expected);
  assert(report.abi_version == PS5_APP_SETUP_ABI && report.status == expected);
  fully_closed(&report);
  return report;
}
static void existing(void) { nodes[2].exists = nodes[3].exists = 1; }
int main(void) {
  struct ps5_app_setup_report report;
  reset();
  report = run(0);
  assert(sizeof report == 64 && report.stage == 6);
  assert(offsetof(struct ps5_app_setup_report, raw_lookup) == 16);
  assert(offsetof(struct ps5_app_setup_report, cleanup_raw_return) == 56);
  assert(offsetof(struct ps5_app_setup_report, prior_cleanup_mask) == 60);
  assert(report.created_mask == 3 && report.verified_mask == 15);
  assert(report.opened_mask == 15 && !report.cleanup_failed_mask);
  assert(mkdir_calls == 2 && sync_calls == 4 && open_calls == 4);
  unsigned complete_operations = report.operations;
  unsigned complete_trace[32];
  memcpy(complete_trace, trace, sizeof complete_trace);
  assert(complete_operations == 21 && complete_operations == native_calls);

  /* Successful repeat validates persistent nodes without recreating/syncing. */
  memset(closed, 0, sizeof closed);
  close_count = 0;
  native_calls = mkdir_calls = sync_calls = open_calls = 0;
  report = run(0);
  assert(!report.created_mask && report.verified_mask == 15);
  assert(!mkdir_calls && !sync_calls && open_calls == 4);
  assert(report.operations == 13);
  reset();
  nodes[2].exists = 1;
  report = run(0);
  assert(report.operations == 17 && report.created_mask == 2);
  assert(mkdir_calls == 1 && sync_calls == 2);

  /* Never create root or /data, even when missing. */
  for (unsigned index = 0; index < 2; ++index) {
    reset();
    nodes[index].exists = 0;
    report = run(-1);
    assert(report.raw_errno == ENOENT && !mkdir_calls && !report.created_mask);
  }
  /* Root/data and existing intermediate directories use FileStore parent
   * policy. */
  for (unsigned index = 0; index < 3; ++index) {
    reset();
    existing();
    nodes[index].info.st_uid = OWNER + 1;
    report = run(-4);
    assert(!report.created_mask);
    reset();
    existing();
    nodes[index].info.st_mode = S_IFDIR | 0777;
    run(-4);
    reset();
    existing();
    nodes[index].info.st_mode = S_IFDIR | 01777;
    run(0);
  }
  reset();
  existing();
  nodes[2].info.st_uid = 0;
  nodes[2].info.st_mode = S_IFDIR | 0755;
  run(0); /* Existing root-owned protected ps5-bt is admitted. */

  /* Symlink/nondirectory paths and mismatching final ownership/mode fail
   * closed. */
  for (unsigned index = 0; index < 4; ++index) {
    for (unsigned kind = 0; kind < 2; ++kind) {
      reset();
      existing();
      nodes[index].info.st_mode = (kind ? S_IFLNK : S_IFREG) | 0700;
      run(-4);
    }
  }
  reset();
  existing();
  nodes[3].info.st_uid = 0;
  run(-4);
  reset();
  existing();
  nodes[3].info.st_mode = S_IFDIR | 0755;
  run(-4);
  reset();
  existing();
  nodes[3].info.st_mode = S_IFDIR | 04700;
  run(-4);
  reset();
  created_mode = 0755;
  report = run(-4);
  assert(report.created_mask == 1 && !nodes[3].exists);
  reset();
  created_owner = 0;
  report = run(-4);
  assert(report.created_mask == 1 && !nodes[3].exists);
  for (unsigned index = 1; index < 4; ++index) {
    reset();
    existing();
    substitute_slot = (int)index;
    run(-4);
    reset();
    existing();
    substitute_slot = (int)index;
    substitute_device = 1;
    run(-4);
  }

  /* Every fallible normal call can fail without retry; EINTR is not special. */
  for (unsigned point = 1; point <= complete_operations; ++point) {
    if (complete_trace[point - 1] == OWNER_CALL)
      continue;
    for (unsigned interrupted = 0; interrupted < 2; ++interrupted) {
      reset();
      fail_at = point;
      fail_errno = interrupted ? EINTR : EIO;
      report = run(-1);
      assert(native_calls == point && report.operations == point);
      if (point != 1)
        assert(report.raw_errno == fail_errno);
      assert(!report.cleanup_failed_mask);
    }
  }
  reset();
  fail_at = 9;
  fail_errno = EEXIST;
  report = run(-1);
  assert(report.raw_errno == EEXIST && mkdir_calls == 1 &&
         !report.created_mask);
  reset();
  fail_at = 8;
  fail_errno = EACCES;
  report = run(-1);
  assert(report.raw_errno == EACCES && !mkdir_calls);
  reset();
  fail_at = 9;
  fail_errno = EINTR;
  mkdir_side_effect_failure = 1;
  report = run(-1);
  assert(!report.created_mask && nodes[2].exists && native_calls == 9);
  /* Cancellation before each operation, before cleanup and after cleanup. */
  for (unsigned point = 1; point <= complete_operations + 2; ++point) {
    reset();
    cancel_at = (int)point;
    report = run(1);
    unsigned completed =
        point <= complete_operations ? point - 1 : complete_operations;
    assert(native_calls == completed && report.operations == completed);
    assert(cancel_calls ==
           (int)(point <= complete_operations + 1 ? point + 1 : point));
    assert(!report.cleanup_failed_mask);
  }
  for (unsigned topology = 0; topology < 2; ++topology) {
    unsigned operations = topology ? 17 : 13;
    for (unsigned point = 1; point <= operations + 2; ++point) {
      reset();
      if (topology)
        nodes[2].exists = 1;
      else
        existing();
      cancel_at = (int)point;
      report = run(1);
      unsigned completed = point <= operations ? point - 1 : operations;
      assert(report.operations == completed && native_calls == completed);
      assert(cancel_calls ==
             (int)(point <= operations + 1 ? point + 1 : point));
    }
  }
  reset();
  cancel_reentry_at = (int)complete_operations + 2;
  run(0);
  assert(cancel_calls == cancel_reentry_at &&
         native_calls == complete_operations);
  reset();
  cancel_on_close = 1;
  report = run(1);
  assert(report.verified_mask == 15 && report.closed_mask == 15);
  reset();
  cancel_on_close = 1;
  close_fail_slot = 0;
  report = run(-1);
  assert(report.cleanup_failed_mask == 1 && report.closed_mask == 15);
  reset();
  cancel_at = 1;
  report = run(1);
  assert(!report.opened_mask && !native_calls);

  reset();
  lookup_null = 1;
  report = run(-1);
  assert(!report.raw_lookup && !report.opened_mask);
  for (unsigned dirty = 0; dirty < 2; ++dirty) {
    reset();
    lookup_failure = -INT64_C(0x100000007);
    lookup_dirty = (int)dirty;
    report = run(-1);
    assert(report.raw_lookup == lookup_failure && !report.opened_mask);
  }
  reset();
  lookup_nested = 1;
  run(0);
  reset();
  memset(&report, 0, sizeof report);
  report.operations = 32;
  assert(!checkpoint(NULL, &report));
  assert(report.status == -2 && report.operations == 32 && !native_calls);
  reset();
  assert(ps5_app_setup_namespace(NULL, NULL, &report) == -2);
  assert(!native_calls);
  assert(ps5_app_setup_namespace(lookup, NULL, NULL) == -2 && !native_calls);

  /* All acquired descriptors are consumed once, even after a prior failure. */
  for (unsigned index = 0; index < 4; ++index) {
    reset();
    close_fail_slot = (int)index;
    report = run(-1);
    assert(report.cleanup_failed_mask == (1u << index));
    assert(report.cleanup_errno == EINTR && report.cleanup_raw_return == -1);
    unsigned calls_before = native_calls;
    int closes_before = close_count;
    assert(ps5_app_setup_namespace(lookup, NULL, &report) == -3);
    assert(report.prior_cleanup_mask == (1u << index));
    assert(!report.operations && !report.opened_mask && !report.closed_mask);
    assert(report.cleanup_errno == EINTR && report.cleanup_raw_return == -1);
    assert(native_calls == calls_before && close_count == closes_before);
  }
  reset();
  close_fail_slot = 0;
  nodes[3].info.st_uid = 0;
  existing();
  report = run(-4);
  assert(report.raw_errno == 0 && report.cleanup_failed_mask == 1);
  reset();
  close_fail_slot = 1;
  cancel_at = 9;
  report = run(1);
  assert(report.cleanup_failed_mask == 2 && report.cleanup_errno == EINTR);
  reset();
  fail_at = 5;
  close_fail_slot = 0;
  report = run(-1);
  assert(report.raw_errno == EIO && report.raw_return == -1);
  assert(report.cleanup_errno == EINTR && report.cleanup_raw_return == -1);
  reset();
  close_fail_mask = 15;
  close_raw_return = -7;
  close_errno = 0;
  report = run(-1);
  assert(report.cleanup_failed_mask == 15 && report.cleanup_raw_return == -7);
  assert(!report.cleanup_errno && close_count == 4);
  unsigned calls_before = native_calls;
  assert(ps5_app_setup_namespace(lookup, cancelled, &report) == -3);
  assert(report.prior_cleanup_mask == 15 && report.cleanup_raw_return == -7);
  assert(!report.cleanup_errno && native_calls == calls_before);
  puts("setup namespace synthetic fault matrix: OK");
  return 0;
}
