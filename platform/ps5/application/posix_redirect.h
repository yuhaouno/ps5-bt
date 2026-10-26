/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_APP_POSIX_REDIRECT_H
#define PS5_BT_APP_POSIX_REDIRECT_H
/* Preinclude system headers before POSIX feature macros in the generic C
 * adapters: pinned FreeBSD cdefs default visibility is needed for flock,
 * S_ISVTX and exact native types. No SDK source is modified. */
#include "posix_imports.h"
#define __error ps5_app_posix_error
#define open ps5_app_open
#define openat ps5_app_openat
#define fcntl ps5_app_fcntl
#define memchr ps5_app_memchr
#define socket ps5_app_socket
#define bind ps5_app_bind
#define listen ps5_app_listen
#define accept ps5_app_accept
#define getsockname ps5_app_getsockname
#define setsockopt ps5_app_setsockopt
#define recv ps5_app_recv
#define send ps5_app_send
#define close ps5_app_close
#define read ps5_app_read
#define write ps5_app_write
#define fstat ps5_app_fstat
#define fstatat ps5_app_fstatat
#define fchmod ps5_app_fchmod
#define flock ps5_app_flock
#define fsync ps5_app_fsync
#define renameat ps5_app_renameat
#define unlinkat ps5_app_unlinkat
#define geteuid ps5_app_geteuid
#endif
