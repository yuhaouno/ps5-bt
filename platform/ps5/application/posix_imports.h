/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_APP_POSIX_IMPORTS_H
#define PS5_BT_APP_POSIX_IMPORTS_H
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/cdefs.h>
#include <sys/file.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>

#include "posix_bridge.h"
int *ps5_app_posix_error(void);
int ps5_app_open(const char *, int, ...);
int ps5_app_openat(int, const char *, int, ...);
int ps5_app_fcntl(int, int, ...);
void *ps5_app_memchr(const void *, int, size_t);
int ps5_app_socket(int domain, int type, int protocol);
int ps5_app_bind(int fd, const struct sockaddr *address, socklen_t length);
int ps5_app_listen(int fd, int backlog);
int ps5_app_accept(int fd, struct sockaddr *address, socklen_t *length);
int ps5_app_getsockname(int fd, struct sockaddr *address, socklen_t *length);
int ps5_app_setsockopt(int fd, int level, int option, const void *value,
                       socklen_t length);
ssize_t ps5_app_recv(int fd, void *buffer, size_t length, int flags);
ssize_t ps5_app_send(int fd, const void *buffer, size_t length, int flags);
int ps5_app_close(int fd);
ssize_t ps5_app_read(int fd, void *buffer, size_t length);
ssize_t ps5_app_write(int fd, const void *buffer, size_t length);
int ps5_app_fstat(int fd, struct stat *info);
int ps5_app_fstatat(int fd, const char *name, struct stat *info, int flags);
int ps5_app_fchmod(int fd, mode_t mode);
int ps5_app_flock(int fd, int operation);
int ps5_app_fsync(int fd);
int ps5_app_renameat(int old_fd, const char *old_name, int new_fd,
                     const char *new_name);
int ps5_app_unlinkat(int fd, const char *name, int flags);
uid_t ps5_app_geteuid(void);
#endif
