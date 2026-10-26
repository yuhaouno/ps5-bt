/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Original fixed-namespace candidate. mkdirat's type and every stat/flag comes
 * from the pinned target SDK headers, never a guessed syscall/layout. */
#include "setup_namespace.h"
#include "posix_imports.h"
_Static_assert(sizeof(struct ps5_app_setup_report) == 64, "setup report ABI");
static int setup_active;
static uint32_t setup_uncertain;
static int setup_cleanup_errno, setup_cleanup_return;
static int32_t finish(struct ps5_app_setup_report *r, int32_t status,
                      int error) {
  if (!r->status) {
    r->status = status;
    r->raw_errno = error;
  }
  return r->status;
}
static int checkpoint(ps5_app_cancel_fn cancelled,
                      struct ps5_app_setup_report *r) {
  if (cancelled && cancelled()) {
    finish(r, 1, 0);
    return 0;
  }
  /* Fixed finite sequence; this cap also catches future accidental expansion.
   */
  if (r->operations >= 32) {
    finish(r, -2, 0);
    return 0;
  }
  ++r->operations;
  return 1;
}
static int protected_parent(const struct stat *st, uid_t owner) {
  return S_ISDIR(st->st_mode) && (st->st_uid == 0 || st->st_uid == owner) &&
         (!(st->st_mode & 0022) || (st->st_mode & S_ISVTX));
}
static int same_node(const struct stat *a, const struct stat *b) {
  return a->st_dev == b->st_dev && a->st_ino == b->st_ino;
}
int32_t ps5_app_setup_namespace(ps5_app_lookup_fn lookup,
                                ps5_app_cancel_fn cancelled,
                                struct ps5_app_setup_report *r) {
  if (!r)
    return -2;
  memset(r, 0, sizeof *r);
  r->abi_version = PS5_APP_SETUP_ABI;
  r->stage = 1;
  if (!lookup)
    return finish(r, -2, 0);
  if (setup_active || setup_uncertain) {
    r->prior_cleanup_mask = setup_uncertain;
    r->cleanup_errno = setup_cleanup_errno;
    r->cleanup_raw_return = setup_cleanup_return;
    return finish(r, -3, 0);
  }
  setup_active = 1;
  int owned[4] = {-1, -1, -1, -1};
  __typeof__(&mkdirat) make_directory = 0;
  void *symbol = 0;
  if (!checkpoint(cancelled, r))
    goto cleanup;
  r->raw_lookup = lookup("mkdirat", &symbol);
  if (r->raw_lookup || !symbol) {
    finish(r, -1, 0);
    goto cleanup;
  }
  _Static_assert(sizeof make_directory == sizeof symbol,
                 "callable representation");
  memcpy(&make_directory, &symbol, sizeof symbol);
  if (!checkpoint(cancelled, r))
    goto cleanup;
  uid_t owner = ps5_app_geteuid();
  const char *names[4] = {"/", "data", "ps5-bt", "control"};
  for (unsigned index = 0; index < 4; ++index) {
    r->stage = 2 + index;
    struct stat before;
    int created = 0;
    if (index) {
      if (!checkpoint(cancelled, r))
        goto cleanup;
      r->raw_return = ps5_app_fstatat(owned[index - 1], names[index], &before,
                                      AT_SYMLINK_NOFOLLOW);
      if (r->raw_return) {
        int error = *ps5_app_posix_error();
        if (index < 2 || error != ENOENT) {
          finish(r, -1, error);
          goto cleanup;
        }
        if (!checkpoint(cancelled, r))
          goto cleanup;
        r->raw_return = make_directory(owned[index - 1], names[index], 0700);
        if (r->raw_return) {
          finish(r, -1, *ps5_app_posix_error());
          goto cleanup;
        }
        r->created_mask |= 1u << (index - 2);
        created = 1;
        if (!checkpoint(cancelled, r))
          goto cleanup;
        r->raw_return = ps5_app_fstatat(owned[index - 1], names[index], &before,
                                        AT_SYMLINK_NOFOLLOW);
        if (r->raw_return) {
          finish(r, -1, *ps5_app_posix_error());
          goto cleanup;
        }
      }
      if (!S_ISDIR(before.st_mode)) {
        finish(r, -4, 0);
        goto cleanup;
      }
    }
    if (!checkpoint(cancelled, r))
      goto cleanup;
    int flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
    r->raw_return = index
                        ? ps5_app_openat(owned[index - 1], names[index], flags)
                        : ps5_app_open(names[index], flags);
    if (r->raw_return < 0) {
      finish(r, -1, *ps5_app_posix_error());
      goto cleanup;
    }
    owned[index] = r->raw_return;
    r->opened_mask |= 1u << index;
    if (!checkpoint(cancelled, r))
      goto cleanup;
    struct stat after;
    r->raw_return = ps5_app_fstat(owned[index], &after);
    if (r->raw_return) {
      finish(r, -1, *ps5_app_posix_error());
      goto cleanup;
    }
    if ((index && !same_node(&before, &after)) ||
        !protected_parent(&after, owner) ||
        ((index == 3 || created) &&
         (after.st_uid != owner || (after.st_mode & 07777) != 0700))) {
      finish(r, -4, 0);
      goto cleanup;
    }
    r->verified_mask |= 1u << index;
    if (created) {
      /* No retry or rollback: successful creation is persistent scoped state.
       * A sync failure remains observable and never means proven durability. */
      if (!checkpoint(cancelled, r))
        goto cleanup;
      r->raw_return = ps5_app_fsync(owned[index]);
      if (r->raw_return) {
        finish(r, -1, *ps5_app_posix_error());
        goto cleanup;
      }
      if (!checkpoint(cancelled, r))
        goto cleanup;
      r->raw_return = ps5_app_fsync(owned[index - 1]);
      if (r->raw_return) {
        finish(r, -1, *ps5_app_posix_error());
        goto cleanup;
      }
    }
  }
  r->stage = 6;
  if (cancelled && cancelled())
    finish(r, 1, 0);
cleanup:
  for (unsigned index = 4; index; --index) {
    unsigned slot = index - 1;
    if (owned[slot] >= 0) {
      int fd = owned[slot];
      owned[slot] = -1;
      /* Cleanup is not cancelled and close(EINTR) is never retried. */
      int raw = ps5_app_close(fd);
      r->closed_mask |= 1u << slot;
      if (raw) {
        r->cleanup_failed_mask |= 1u << slot;
        if (!r->cleanup_raw_return) {
          r->cleanup_errno = *ps5_app_posix_error();
          r->cleanup_raw_return = raw;
        }
      }
    }
  }
  if (r->cleanup_failed_mask && !r->status)
    finish(r, -1, r->cleanup_errno);
  setup_uncertain |= r->cleanup_failed_mask;
  if (r->cleanup_failed_mask) {
    setup_cleanup_errno = r->cleanup_errno;
    setup_cleanup_return = r->cleanup_raw_return;
  }
  if (cancelled && cancelled())
    finish(r, 1, 0);
  setup_active = 0;
  return r->status;
}
