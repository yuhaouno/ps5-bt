/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Fixed read-only consistency experiment. Native types/flags come only from
 * the selected SDK headers and matching initialized POSIX bridge.
 */
#include "namespace_probe.h"
#include "posix_imports.h"
_Static_assert(sizeof(struct ps5_app_namespace_report) == 80,
               "namespace report ABI");
static int namespace_active;
static uint32_t namespace_uncertain;
static int namespace_cleanup_errno, namespace_cleanup_return;

static int32_t namespace_finish(struct ps5_app_namespace_report *r,
                                int32_t status, int error) {
  if (!r->status) {
    r->status = status;
    r->raw_errno = error;
  }
  return r->status;
}
static int namespace_checkpoint(ps5_app_cancel_fn cancelled,
                                struct ps5_app_namespace_report *r,
                                uint32_t stage) {
  r->stage = stage;
  if (cancelled && cancelled()) {
    namespace_finish(r, 1, 0);
    return 0;
  }
  if (r->operations >= 7) {
    namespace_finish(r, -2, 0);
    return 0;
  }
  ++r->operations;
  return 1;
}
static uint32_t namespace_rejections(const struct stat *st, uid_t owner) {
  uint32_t rejected = 0;
  if (!S_ISDIR(st->st_mode))
    rejected |= PS5_APP_NAMESPACE_REJECT_TYPE;
  if (st->st_uid != 0 && st->st_uid != owner)
    rejected |= PS5_APP_NAMESPACE_REJECT_PARENT_OWNER;
  if ((st->st_mode & 0022) && !(st->st_mode & S_ISVTX))
    rejected |= PS5_APP_NAMESPACE_REJECT_PARENT_WRITE;
  return rejected;
}
static uint32_t namespace_matches(const struct stat *a, const struct stat *b) {
  uint32_t matches = 0;
  if (a->st_dev == b->st_dev)
    matches |= PS5_APP_NAMESPACE_MATCH_DEVICE;
  if (a->st_ino == b->st_ino)
    matches |= PS5_APP_NAMESPACE_MATCH_INODE;
  if (a->st_mode == b->st_mode)
    matches |= PS5_APP_NAMESPACE_MATCH_MODE;
  if (a->st_uid == b->st_uid)
    matches |= PS5_APP_NAMESPACE_MATCH_UID;
  if (a->st_gid == b->st_gid)
    matches |= PS5_APP_NAMESPACE_MATCH_GID;
  return matches;
}

int32_t ps5_app_namespace_probe(ps5_app_cancel_fn cancelled,
                                struct ps5_app_namespace_report *r) {
  if (!r)
    return -2;
  memset(r, 0, sizeof *r);
  r->abi_version = PS5_APP_NAMESPACE_ABI;
  r->stage = 1;
  if (namespace_active || namespace_uncertain) {
    r->prior_cleanup_mask = namespace_uncertain;
    r->cleanup_errno = namespace_cleanup_errno;
    r->cleanup_raw_return = namespace_cleanup_return;
    return namespace_finish(r, -3, 0);
  }
  namespace_active = 1;
  int owned[2] = {-1, -1};
  struct stat root_fd = {0}, root_path = {0}, data_path = {0}, data_fd = {0};
  if (!namespace_checkpoint(cancelled, r, 1))
    goto cleanup;
  /* geteuid has no error return. Keep the identity local, never in raw_return.
   */
  uid_t owner = ps5_app_geteuid();
  if (!namespace_checkpoint(cancelled, r, 2))
    goto cleanup;
  const int flags = O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
  r->raw_return = ps5_app_open("/", flags);
  if (r->raw_return < 0)
    goto native_error;
  owned[0] = r->raw_return;
  r->opened_mask |= 1;
  if (!namespace_checkpoint(cancelled, r, 3))
    goto cleanup;
  r->raw_return = ps5_app_fstat(owned[0], &root_fd);
  if (r->raw_return)
    goto native_error;
  r->observed_mask |= 1;
  r->root_fd_reject = namespace_rejections(&root_fd, owner);
  if (!namespace_checkpoint(cancelled, r, 4))
    goto cleanup;
  r->raw_return =
      ps5_app_fstatat(owned[0], ".", &root_path, AT_SYMLINK_NOFOLLOW);
  if (r->raw_return)
    goto native_error;
  r->observed_mask |= 2;
  r->root_path_reject = namespace_rejections(&root_path, owner);
  r->root_match_mask = namespace_matches(&root_fd, &root_path);
  if (!namespace_checkpoint(cancelled, r, 5))
    goto cleanup;
  r->raw_return =
      ps5_app_fstatat(owned[0], "data", &data_path, AT_SYMLINK_NOFOLLOW);
  if (r->raw_return)
    goto native_error;
  r->observed_mask |= 4;
  r->data_path_reject = namespace_rejections(&data_path, owner);
  if (!namespace_checkpoint(cancelled, r, 6))
    goto cleanup;
  r->raw_return = ps5_app_openat(owned[0], "data", flags);
  if (r->raw_return < 0)
    goto native_error;
  owned[1] = r->raw_return;
  r->opened_mask |= 2;
  if (!namespace_checkpoint(cancelled, r, 7))
    goto cleanup;
  r->raw_return = ps5_app_fstat(owned[1], &data_fd);
  if (r->raw_return)
    goto native_error;
  r->observed_mask |= 8;
  r->data_fd_reject = namespace_rejections(&data_fd, owner);
  r->data_match_mask = namespace_matches(&data_path, &data_fd);
  r->same_device = root_fd.st_dev == data_fd.st_dev ? 2 : 1;
  r->stage = 8;
  if (cancelled && cancelled())
    namespace_finish(r, 1, 0);
  goto cleanup;
native_error:
  namespace_finish(r, -1, *ps5_app_posix_error());
cleanup:
  for (unsigned index = 2; index; --index) {
    unsigned slot = index - 1;
    if (owned[slot] >= 0) {
      int fd = owned[slot];
      owned[slot] = -1;
      /* Uncancellable consume-once cleanup; even close(EINTR) is not retried.
       */
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
    namespace_finish(r, -1, r->cleanup_errno);
  namespace_uncertain |= r->cleanup_failed_mask;
  if (r->cleanup_failed_mask) {
    namespace_cleanup_errno = r->cleanup_errno;
    namespace_cleanup_return = r->cleanup_raw_return;
  }
  if (cancelled && cancelled())
    namespace_finish(r, 1, 0);
  namespace_active = 0;
  return r->status;
}
