/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Independent fixed-path read-only cases; no fstatat, create or permission
 * repair. Native declarations/layouts/flags come from the reviewed SDK bridge.
 */
#include "open_compare.h"
#include "posix_imports.h"
_Static_assert(sizeof(struct ps5_app_open_compare_report) == 120,
               "open comparison report ABI");
static int open_compare_active;
static uint32_t open_compare_uncertain;
static int open_compare_cleanup_errno, open_compare_cleanup_return;

static int32_t compare_finish(struct ps5_app_open_compare_report *r,
                              int32_t status, int error) {
  if (!r->status) {
    r->status = status;
    r->raw_errno = error;
  }
  return r->status;
}
static int compare_checkpoint(ps5_app_cancel_fn cancelled,
                              struct ps5_app_open_compare_report *r,
                              uint32_t stage) {
  r->stage = stage;
  if (cancelled && cancelled()) {
    compare_finish(r, 1, 0);
    return 0;
  }
  if (r->operations >= 7 || stage < 1 || stage > 7 ||
      (r->attempted_mask & (1u << (stage - 1)))) {
    compare_finish(r, -2, 0);
    return 0;
  }
  ++r->operations;
  r->attempted_mask |= 1u << (stage - 1);
  return 1;
}
static void compare_native_error(struct ps5_app_open_compare_report *r,
                                 uint32_t stage, int raw, int error) {
  if (!r->status) {
    r->failed_stage = stage;
    r->raw_return = raw;
    compare_finish(r, -1, error);
  }
}
static uint32_t compare_rejections(const struct stat *st, uid_t owner) {
  uint32_t rejected = 0;
  if (!S_ISDIR(st->st_mode))
    rejected |= PS5_APP_OPEN_COMPARE_REJECT_TYPE;
  if (st->st_uid != 0 && st->st_uid != owner)
    rejected |= PS5_APP_OPEN_COMPARE_REJECT_PARENT_OWNER;
  if ((st->st_mode & 0022) && !(st->st_mode & S_ISVTX))
    rejected |= PS5_APP_OPEN_COMPARE_REJECT_PARENT_WRITE;
  return rejected;
}
static uint32_t compare_matches(const struct stat *a, const struct stat *b) {
  uint32_t matches = 0;
  if (a->st_dev == b->st_dev)
    matches |= PS5_APP_OPEN_COMPARE_MATCH_DEVICE;
  if (a->st_ino == b->st_ino)
    matches |= PS5_APP_OPEN_COMPARE_MATCH_INODE;
  if (a->st_mode == b->st_mode)
    matches |= PS5_APP_OPEN_COMPARE_MATCH_MODE;
  if (a->st_uid == b->st_uid)
    matches |= PS5_APP_OPEN_COMPARE_MATCH_UID;
  if (a->st_gid == b->st_gid)
    matches |= PS5_APP_OPEN_COMPARE_MATCH_GID;
  return matches;
}

int32_t ps5_app_open_compare(ps5_app_cancel_fn cancelled,
                             struct ps5_app_open_compare_report *r) {
  if (!r)
    return -2;
  memset(r, 0, sizeof *r);
  r->abi_version = PS5_APP_OPEN_COMPARE_ABI;
  r->stage = 1;
  if (open_compare_active || open_compare_uncertain) {
    r->prior_cleanup_mask = open_compare_uncertain;
    r->cleanup_errno = open_compare_cleanup_errno;
    r->cleanup_raw_return = open_compare_cleanup_return;
    return compare_finish(r, -3, 0);
  }
  open_compare_active = 1;
  int owned[3] = {-1, -1, -1};
  struct stat root = {0}, relative = {0}, absolute = {0};
  if (!compare_checkpoint(cancelled, r, 1))
    goto cleanup;
  uid_t owner = ps5_app_geteuid(); /* Never emit the native identity. */
  if (!compare_checkpoint(cancelled, r, 2))
    goto cleanup;
  const int flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
  int raw = ps5_app_open("/", flags);
  if (raw < 0) {
    compare_native_error(r, 2, raw, *ps5_app_posix_error());
    goto cleanup;
  }
  owned[0] = raw;
  r->opened_mask |= 1;
  if (!compare_checkpoint(cancelled, r, 3))
    goto cleanup;
  raw = ps5_app_fstat(owned[0], &root);
  if (raw) {
    compare_native_error(r, 3, raw, *ps5_app_posix_error());
    goto cleanup;
  }
  r->observed_mask |= 1;
  r->root_reject = compare_rejections(&root, owner);
  if (!compare_checkpoint(cancelled, r, 4))
    goto cleanup;
  r->relative_open_raw = ps5_app_openat(owned[0], "data", flags);
  if (r->relative_open_raw < 0) {
    r->relative_open_errno = *ps5_app_posix_error();
    compare_native_error(r, 4, r->relative_open_raw, r->relative_open_errno);
  } else {
    owned[1] = r->relative_open_raw;
    r->opened_mask |= 2;
    if (!compare_checkpoint(cancelled, r, 5))
      goto cleanup;
    r->relative_stat_raw = ps5_app_fstat(owned[1], &relative);
    if (r->relative_stat_raw) {
      r->relative_stat_errno = *ps5_app_posix_error();
      compare_native_error(r, 5, r->relative_stat_raw, r->relative_stat_errno);
    } else {
      r->observed_mask |= 2;
      r->relative_reject = compare_rejections(&relative, owner);
      r->relative_same_device = root.st_dev == relative.st_dev ? 2 : 1;
    }
  }
  if (!compare_checkpoint(cancelled, r, 6))
    goto cleanup;
  r->absolute_open_raw = ps5_app_open("/data", flags);
  if (r->absolute_open_raw < 0) {
    r->absolute_open_errno = *ps5_app_posix_error();
    compare_native_error(r, 6, r->absolute_open_raw, r->absolute_open_errno);
  } else {
    owned[2] = r->absolute_open_raw;
    r->opened_mask |= 4;
    if (!compare_checkpoint(cancelled, r, 7))
      goto cleanup;
    r->absolute_stat_raw = ps5_app_fstat(owned[2], &absolute);
    if (r->absolute_stat_raw) {
      r->absolute_stat_errno = *ps5_app_posix_error();
      compare_native_error(r, 7, r->absolute_stat_raw, r->absolute_stat_errno);
    } else {
      r->observed_mask |= 4;
      r->absolute_reject = compare_rejections(&absolute, owner);
      r->absolute_same_device = root.st_dev == absolute.st_dev ? 2 : 1;
    }
  }
  if ((r->observed_mask & 6) == 6)
    r->data_match_mask = compare_matches(&relative, &absolute);
  r->stage = 8;
  if (cancelled && cancelled())
    compare_finish(r, 1, 0);
cleanup:
  for (unsigned index = 3; index; --index) {
    unsigned slot = index - 1;
    if (owned[slot] >= 0) {
      int fd = owned[slot];
      owned[slot] = -1;
      /* Consume before the one uncancellable close; never retry EINTR. */
      int closed = ps5_app_close(fd);
      r->closed_mask |= 1u << slot;
      if (closed) {
        r->cleanup_failed_mask |= 1u << slot;
        if (!r->cleanup_raw_return) {
          r->cleanup_errno = *ps5_app_posix_error();
          r->cleanup_raw_return = closed;
        }
      }
    }
  }
  if (r->cleanup_failed_mask && !r->status)
    compare_finish(r, -1, r->cleanup_errno);
  open_compare_uncertain |= r->cleanup_failed_mask;
  if (r->cleanup_failed_mask) {
    open_compare_cleanup_errno = r->cleanup_errno;
    open_compare_cleanup_return = r->cleanup_raw_return;
  }
  if (cancelled && cancelled())
    compare_finish(r, 1, 0);
  open_compare_active = 0;
  return r->status;
}
