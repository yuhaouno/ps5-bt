/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Original selected-export bridge; source/layout review in
 * PS5-POSIX-IMPORTS.md. No libc/import library, syscall number or native layout
 * is invented here. */
#include "posix_imports.h"
#include <stdarg.h>
_Static_assert(sizeof(struct ps5_app_posix_report) == 48, "report ABI");
struct functions {
  int *(*error_call)(void);
  __typeof__(&open) open_call;
  __typeof__(&openat) openat_call;
  __typeof__(&fcntl) fcntl_call;
  __typeof__(&socket) socket_call;
  __typeof__(&bind) bind_call;
  __typeof__(&listen) listen_call;
  __typeof__(&accept) accept_call;
  __typeof__(&getsockname) getsockname_call;
  __typeof__(&setsockopt) setsockopt_call;
  __typeof__(&recv) recv_call;
  __typeof__(&send) send_call;
  __typeof__(&close) close_call;
  __typeof__(&read) read_call;
  __typeof__(&write) write_call;
  __typeof__(&fstat) fstat_call;
  __typeof__(&fstatat) fstatat_call;
  __typeof__(&fchmod) fchmod_call;
  __typeof__(&flock) flock_call;
  __typeof__(&fsync) fsync_call;
  __typeof__(&renameat) renameat_call;
  __typeof__(&unlinkat) unlinkat_call;
  __typeof__(&geteuid) geteuid_call;
};
static struct functions functions;
static int ready, opening, ever_ready, fallback_error;
static int *native_error;
void ps5_app_scrub(void *data, size_t bytes) {
  volatile unsigned char *p = data;
  while (bytes--)
    *p++ = 0;
}
static void report_init(struct ps5_app_posix_report *r, uint32_t stage) {
  memset(r, 0, sizeof *r);
  r->abi_version = PS5_APP_POSIX_ABI;
  r->stage = stage;
}
static int32_t result(struct ps5_app_posix_report *r, int32_t status,
                      int error) {
  r->status = status;
  r->raw_errno = error;
  return status;
}
int *ps5_app_posix_error(void) {
  return ready ? native_error : &fallback_error;
}
static int unavailable(void) {
  fallback_error = EIO;
  return -1;
}
int32_t ps5_app_posix_init_cancel(ps5_app_lookup_fn lookup,
                                  ps5_app_cancel_fn cancelled,
                                  struct ps5_app_posix_report *r) {
  if (!r)
    return -2;
  report_init(r, 1);
  if (!lookup)
    return result(r, -2, 0);
  if (ready || opening || ever_ready)
    return result(r, -3, 0);
  opening = 1;
  r->stage = 2;
  struct functions candidate = {0};
  void *symbol = 0;
#define RESOLVE(field, name)                                                   \
  do {                                                                         \
    if (cancelled && cancelled())                                              \
      goto cancelled_init;                                                     \
    symbol = 0;                                                                \
    r->raw_lookup = lookup(name, &symbol);                                     \
    if (cancelled && cancelled())                                              \
      goto cancelled_init;                                                     \
    if (r->raw_lookup || !symbol)                                              \
      goto failed;                                                             \
    _Static_assert(sizeof(candidate.field) == sizeof(symbol),                  \
                   "callable representation");                                 \
    memcpy(&candidate.field, &symbol, sizeof symbol);                          \
    ++r->resolved;                                                             \
  } while (0)
  RESOLVE(error_call, "__error");
  RESOLVE(open_call, "open");
  RESOLVE(openat_call, "openat");
  RESOLVE(fcntl_call, "fcntl");
  RESOLVE(socket_call, "socket");
  RESOLVE(bind_call, "bind");
  RESOLVE(listen_call, "listen");
  RESOLVE(accept_call, "accept");
  RESOLVE(getsockname_call, "getsockname");
  RESOLVE(setsockopt_call, "setsockopt");
  RESOLVE(recv_call, "recv");
  RESOLVE(send_call, "send");
  RESOLVE(close_call, "close");
  RESOLVE(read_call, "read");
  RESOLVE(write_call, "write");
  RESOLVE(fstat_call, "fstat");
  RESOLVE(fstatat_call, "fstatat");
  RESOLVE(fchmod_call, "fchmod");
  RESOLVE(flock_call, "flock");
  RESOLVE(fsync_call, "fsync");
  RESOLVE(renameat_call, "renameat");
  RESOLVE(unlinkat_call, "unlinkat");
  RESOLVE(geteuid_call, "geteuid");
#undef RESOLVE
  r->stage = 3;
  if (cancelled && cancelled())
    goto cancelled_init;
  native_error = candidate.error_call();
  if (!native_error)
    goto failed;
  functions = candidate;
  ready = ever_ready = 1;
  opening = 0;
  ps5_app_scrub(&candidate, sizeof candidate);
  return 0;
cancelled_init:
  native_error = 0;
  opening = 0;
  ps5_app_scrub(&candidate, sizeof candidate);
  return result(r, 1, 0);
failed:
  native_error = 0;
  opening = 0;
  ps5_app_scrub(&candidate, sizeof candidate);
  return result(r, -1, 0);
}
int32_t ps5_app_posix_init(ps5_app_lookup_fn lookup,
                           struct ps5_app_posix_report *r) {
  return ps5_app_posix_init_cancel(lookup, NULL, r);
}
int32_t ps5_app_posix_shutdown(struct ps5_app_posix_report *r) {
  if (!r)
    return -2;
  report_init(r, 8);
  if (!ready)
    return result(r, -2, 0);
  /* Caller consumed every descriptor first, including uncertain close. No
   * pointer is dereferenced after the matching factory lifetime has ended. */
  ready = 0;
  native_error = 0;
  ps5_app_scrub(&functions, sizeof functions);
  return 0;
}
int ps5_app_socket(int domain, int type, int protocol) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.socket_call(domain, type, protocol);
}
int ps5_app_bind(int fd, const struct sockaddr *address, socklen_t length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.bind_call(fd, address, length);
}
int ps5_app_listen(int fd, int backlog) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.listen_call(fd, backlog);
}
int ps5_app_accept(int fd, struct sockaddr *address, socklen_t *length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.accept_call(fd, address, length);
}
int ps5_app_getsockname(int fd, struct sockaddr *address, socklen_t *length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.getsockname_call(fd, address, length);
}
int ps5_app_setsockopt(int fd, int level, int option, const void *value,
                       socklen_t length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.setsockopt_call(fd, level, option, value, length);
}
ssize_t ps5_app_recv(int fd, void *buffer, size_t length, int flags) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.recv_call(fd, buffer, length, flags);
}
ssize_t ps5_app_send(int fd, const void *buffer, size_t length, int flags) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.send_call(fd, buffer, length, flags);
}
int ps5_app_close(int fd) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.close_call(fd);
}
ssize_t ps5_app_read(int fd, void *buffer, size_t length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.read_call(fd, buffer, length);
}
ssize_t ps5_app_write(int fd, const void *buffer, size_t length) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.write_call(fd, buffer, length);
}
int ps5_app_fstat(int fd, struct stat *info) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.fstat_call(fd, info);
}
int ps5_app_fstatat(int fd, const char *name, struct stat *info, int flags) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.fstatat_call(fd, name, info, flags);
}
int ps5_app_fchmod(int fd, mode_t mode) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.fchmod_call(fd, mode);
}
int ps5_app_flock(int fd, int operation) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.flock_call(fd, operation);
}
int ps5_app_fsync(int fd) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.fsync_call(fd);
}
int ps5_app_renameat(int old_fd, const char *old_name, int new_fd,
                     const char *new_name) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.renameat_call(old_fd, old_name, new_fd, new_name);
}
int ps5_app_unlinkat(int fd, const char *name, int flags) {
  if (!ready) {
    unavailable();
    return -1;
  }
  return functions.unlinkat_call(fd, name, flags);
}
uid_t ps5_app_geteuid(void) {
  if (!ready) {
    unavailable();
    return (uid_t)-1;
  }
  return functions.geteuid_call();
}
int ps5_app_open(const char *name, int flags, ...) {
  if (!ready)
    return unavailable();
  mode_t mode = 0;
  if (flags & O_CREAT) {
    va_list ap;
    va_start(ap, flags);
    mode = (mode_t)va_arg(ap, int);
    va_end(ap);
  }
  return functions.open_call(name, flags, mode);
}
int ps5_app_openat(int fd, const char *name, int flags, ...) {
  if (!ready)
    return unavailable();
  mode_t mode = 0;
  if (flags & O_CREAT) {
    va_list ap;
    va_start(ap, flags);
    mode = (mode_t)va_arg(ap, int);
    va_end(ap);
  }
  return functions.openat_call(fd, name, flags, mode);
}
int ps5_app_fcntl(int fd, int command, ...) {
  if (!ready)
    return unavailable();
  if (command != F_GETFL && command != F_SETFL && command != F_GETFD &&
      command != F_SETFD) {
    *native_error = EINVAL;
    return -1;
  }
  int value = 0;
  if (command == F_SETFL || command == F_SETFD) {
    va_list ap;
    va_start(ap, command);
    value = va_arg(ap, int);
    va_end(ap);
  }
  return functions.fcntl_call(fd, command, value);
}
void *ps5_app_memchr(const void *data, int value, size_t bytes) {
  const unsigned char *p = data;
  for (size_t i = 0; i < bytes; ++i)
    if (p[i] == (unsigned char)value)
      return (void *)(p + i);
  return 0;
}
int32_t ps5_app_random_token_cancel(uint8_t output[32],
                                    ps5_app_cancel_fn cancelled,
                                    struct ps5_app_posix_report *r) {
  if (!r || !output)
    return -2;
  report_init(r, 4);
  memset(output, 0, 32);
  if (!ready)
    return result(r, -1, 0);
  if (cancelled && cancelled())
    return result(r, 1, 0);
  uint8_t random[16] = {0};
  int status = 0;
  int fd = ps5_app_open("/dev/random",
                        O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
  if (fd < 0) {
    r->raw_return = fd;
    return result(r, -5, *native_error);
  }
  struct stat info;
  r->stage = 5;
  int raw = 0;
  if (cancelled && cancelled())
    status = result(r, 1, 0);
  else
    raw = ps5_app_fstat(fd, &info);
  if (!status && (raw || !S_ISCHR(info.st_mode))) {
    r->raw_return = raw;
    status = result(r, -5, raw ? *native_error : 0);
  }
  size_t used = 0;
  r->stage = status ? r->stage : 6;
  for (unsigned call = 0; !status && used < sizeof random && call < 16;
       ++call) {
    if (cancelled && cancelled()) {
      status = result(r, 1, 0);
      break;
    }
    ssize_t got = ps5_app_read(fd, random + used, sizeof random - used);
    r->raw_return = got;
    if (got <= 0 || (size_t)got > sizeof random - used)
      status = result(r, -5, got < 0 ? *native_error : 0);
    else
      used += (size_t)got;
  }
  if (!status && used != sizeof random)
    status = result(r, -5, 0);
  raw = ps5_app_close(fd); /* consume exactly once, never retry EINTR */
  if (raw) {
    r->cleanup_flags = 1;
    r->cleanup_errno = *native_error;
    if (!status) {
      r->stage = 7;
      r->raw_return = raw;
      status = result(r, -6, *native_error);
    }
  }
  if (!status) {
    static const char hex[] = "0123456789abcdef";
    for (size_t i = 0; i < 16; ++i) {
      output[2 * i] = (uint8_t)hex[random[i] >> 4];
      output[2 * i + 1] = (uint8_t)hex[random[i] & 15];
    }
  }
  ps5_app_scrub(random, sizeof random);
  return status;
}

int32_t ps5_app_random_token(uint8_t output[32],
                             struct ps5_app_posix_report *r) {
  return ps5_app_random_token_cancel(output, NULL, r);
}
