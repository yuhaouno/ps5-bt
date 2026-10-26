/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Independent fixed-path read-only cases; no fstatat, create or permission
 * repair. Native declarations/layouts/flags come from the reviewed SDK bridge.
 */
#include "raw_openat.h"
#include "posix_imports.h"
#include "syscall.h"
_Static_assert(sizeof(struct ps5_app_raw_openat_report) == 136,
               "open comparison report ABI");
static int raw_openat_active;
static uint32_t raw_openat_uncertain;
static uint32_t raw_openat_ownership_unknown;
static int raw_openat_cleanup_errno, raw_openat_cleanup_return;

static int32_t route_finish(struct ps5_app_raw_openat_report *r, int32_t status,
                            int error) {
  if (!r->status) {
    r->status = status;
    r->raw_errno = error;
  }
  return r->status;
}
static int route_checkpoint(ps5_app_cancel_fn cancelled,
                            struct ps5_app_raw_openat_report *r,
                            uint32_t stage) {
  r->stage = stage;
  if (cancelled && cancelled()) {
    route_finish(r, 1, 0);
    return 0;
  }
  if (r->operations >= 7 || stage < 1 || stage > 7 ||
      (r->attempted_mask & (1u << (stage - 1)))) {
    route_finish(r, -2, 0);
    return 0;
  }
  ++r->operations;
  r->attempted_mask |= 1u << (stage - 1);
  return 1;
}
static void route_native_error(struct ps5_app_raw_openat_report *r,
                               uint32_t stage, int64_t raw, int error) {
  if (!r->status) {
    r->failed_stage = stage;
    r->raw_return_lo = (uint32_t)(uint64_t)raw;
    r->raw_return_hi = (uint32_t)((uint64_t)raw >> 32);
    route_finish(r, -1, error);
  }
}
static void route_unknown(struct ps5_app_raw_openat_report *r, uint32_t stage,
                          int64_t raw) {
  r->raw_ownership_unknown = raw_openat_ownership_unknown = 1;
  if (!r->status) {
    r->failed_stage = stage;
    r->raw_return_lo = (uint32_t)(uint64_t)raw;
    r->raw_return_hi = (uint32_t)((uint64_t)raw >> 32);
    route_finish(r, -4, 0);
  }
}
static uint32_t route_rejections(const struct stat *st, uid_t owner) {
  uint32_t rejected = 0;
  if (!S_ISDIR(st->st_mode))
    rejected |= PS5_APP_RAW_OPENAT_REJECT_TYPE;
  if (st->st_uid != 0 && st->st_uid != owner)
    rejected |= PS5_APP_RAW_OPENAT_REJECT_PARENT_OWNER;
  if ((st->st_mode & 0022) && !(st->st_mode & S_ISVTX))
    rejected |= PS5_APP_RAW_OPENAT_REJECT_PARENT_WRITE;
  return rejected;
}
static uint32_t route_matches(const struct stat *a, const struct stat *b) {
  uint32_t matches = 0;
  if (a->st_dev == b->st_dev)
    matches |= PS5_APP_RAW_OPENAT_MATCH_DEVICE;
  if (a->st_ino == b->st_ino)
    matches |= PS5_APP_RAW_OPENAT_MATCH_INODE;
  if (a->st_mode == b->st_mode)
    matches |= PS5_APP_RAW_OPENAT_MATCH_MODE;
  if (a->st_uid == b->st_uid)
    matches |= PS5_APP_RAW_OPENAT_MATCH_UID;
  if (a->st_gid == b->st_gid)
    matches |= PS5_APP_RAW_OPENAT_MATCH_GID;
  return matches;
}

int32_t ps5_app_raw_openat(ps5_app_cancel_fn cancelled,
                           struct ps5_app_raw_openat_report *r) {
  if (!r)
    return -2;
  memset(r, 0, sizeof *r);
  r->abi_version = PS5_APP_RAW_OPENAT_ABI;
  r->stage = 1;
  if (raw_openat_active || raw_openat_uncertain ||
      raw_openat_ownership_unknown) {
    r->prior_cleanup_mask = raw_openat_uncertain;
    r->cleanup_errno = raw_openat_cleanup_errno;
    r->cleanup_raw_return = raw_openat_cleanup_return;
    r->prior_raw_ownership_unknown = raw_openat_ownership_unknown;
    return route_finish(r, -3, 0);
  }
  raw_openat_active = 1;
  int owned[3] = {-1, -1, -1};
  struct stat root = {0}, imported = {0}, checked_stat = {0};
  if (!route_checkpoint(cancelled, r, 1))
    goto cleanup;
  uid_t owner = ps5_app_geteuid(); /* Never emit the native identity. */
  if (!route_checkpoint(cancelled, r, 2))
    goto cleanup;
  const int flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
  int raw = ps5_app_open("/", flags);
  if (raw < 0) {
    route_native_error(r, 2, raw, *ps5_app_posix_error());
    goto cleanup;
  }
  owned[0] = raw;
  r->opened_mask |= 1;
  if (!route_checkpoint(cancelled, r, 3))
    goto cleanup;
  raw = ps5_app_fstat(owned[0], &root);
  if (raw) {
    route_native_error(r, 3, raw, *ps5_app_posix_error());
    goto cleanup;
  }
  r->observed_mask |= 1;
  r->root_reject = route_rejections(&root, owner);
  if (!route_checkpoint(cancelled, r, 4))
    goto cleanup;
  r->imported_open_raw = ps5_app_openat(owned[0], "data", flags);
  if (r->imported_open_raw < 0) {
    r->imported_open_errno = *ps5_app_posix_error();
    route_native_error(r, 4, r->imported_open_raw, r->imported_open_errno);
  } else {
    if (r->imported_open_raw == owned[0]) {
      route_unknown(r, 4, r->imported_open_raw);
      goto cleanup;
    }
    owned[1] = r->imported_open_raw;
    r->opened_mask |= 2;
    if (!route_checkpoint(cancelled, r, 5))
      goto cleanup;
    r->imported_stat_raw = ps5_app_fstat(owned[1], &imported);
    if (r->imported_stat_raw) {
      r->imported_stat_errno = *ps5_app_posix_error();
      route_native_error(r, 5, r->imported_stat_raw, r->imported_stat_errno);
    } else {
      r->observed_mask |= 2;
      r->imported_reject = route_rejections(&imported, owner);
      r->imported_same_device = root.st_dev == imported.st_dev ? 2 : 1;
    }
  }
  if (!route_checkpoint(cancelled, r, 6))
    goto cleanup;
  _Static_assert(sizeof(long) == 8 && sizeof(int) == 4, "selected raw ABI");
  long checked = __crt_syscall(PS5_APP_SOURCE_SYS_OPENAT, (long)owned[0],
                               (long)(uintptr_t) "data", (long)flags, (long)0,
                               (long)0, (long)0);
  r->checked_open_lo = (uint32_t)(uint64_t)checked;
  r->checked_open_hi = (uint32_t)((uint64_t)checked >> 32);
  if (checked < 0) {
    r->checked_open_kind = 2;
    /* Checked raw errors are -errno; never consult the unrelated errno slot. */
    route_native_error(r, 6, checked, 0);
  } else if ((uint64_t)checked > INT32_MAX) {
    r->checked_open_kind = 3;
    route_unknown(r, 6, checked);
    goto cleanup;
  } else if (checked == owned[0] || (owned[1] >= 0 && checked == owned[1])) {
    r->checked_open_kind = 4;
    route_unknown(r, 6, checked);
    goto cleanup;
  } else {
    r->checked_open_kind = 1;
    owned[2] = (int)checked;
    r->opened_mask |= 4;
    if (!route_checkpoint(cancelled, r, 7))
      goto cleanup;
    r->checked_stat_raw = ps5_app_fstat(owned[2], &checked_stat);
    if (r->checked_stat_raw) {
      r->checked_stat_errno = *ps5_app_posix_error();
      route_native_error(r, 7, r->checked_stat_raw, r->checked_stat_errno);
    } else {
      r->observed_mask |= 4;
      r->checked_reject = route_rejections(&checked_stat, owner);
      r->checked_same_device = root.st_dev == checked_stat.st_dev ? 2 : 1;
    }
  }
  if ((r->observed_mask & 6) == 6)
    r->data_match_mask = route_matches(&imported, &checked_stat);
  r->stage = 8;
  if (cancelled && cancelled())
    route_finish(r, 1, 0);
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
    route_finish(r, -1, r->cleanup_errno);
  raw_openat_uncertain |= r->cleanup_failed_mask;
  if (r->cleanup_failed_mask) {
    raw_openat_cleanup_errno = r->cleanup_errno;
    raw_openat_cleanup_return = r->cleanup_raw_return;
  }
  if (cancelled && cancelled())
    route_finish(r, 1, 0);
  raw_openat_active = 0;
  return r->status;
}
