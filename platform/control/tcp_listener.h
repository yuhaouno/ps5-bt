/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_TCP_LISTENER_H
#define PS5_BT_TCP_LISTENER_H
#include <stdint.h>
#define PS5_BT_LISTENER_ABI_VERSION 1u
#define PS5_BT_LISTENER_IO_MAX 65536u
#define PS5_BT_LISTENER_CLEANUP_UNKNOWN 1u
enum ps5_bt_listener_status { PS5_BT_LISTENER_OK=0, PS5_BT_LISTENER_WOULD_BLOCK=1,
    PS5_BT_LISTENER_INVALID=-1, PS5_BT_LISTENER_IO=-2, PS5_BT_LISTENER_BUSY=-3,
    PS5_BT_LISTENER_LIMIT=-4, PS5_BT_LISTENER_UNSUPPORTED=-5, PS5_BT_LISTENER_POISONED=-6 };
enum ps5_bt_listener_stage { PS5_BT_LISTENER_VALIDATE=1, PS5_BT_LISTENER_SOCKET=2,
    PS5_BT_LISTENER_NONBLOCK=3, PS5_BT_LISTENER_CLOEXEC=4, PS5_BT_LISTENER_SIGPIPE=5,
    PS5_BT_LISTENER_BIND=6, PS5_BT_LISTENER_LISTEN=7, PS5_BT_LISTENER_NAME=8,
    PS5_BT_LISTENER_ACCEPT=9, PS5_BT_LISTENER_READ=10, PS5_BT_LISTENER_WRITE=11,
    PS5_BT_LISTENER_CLIENT_CLOSE=12, PS5_BT_LISTENER_CLOSE=13 };
struct ps5_bt_listener_diagnostic { uint32_t abi_version,stage; int32_t status,raw_errno,cleanup_errno; uint32_t flags; };
/* Zero-initialized queried-size/alignment caller storage remains borrowed until
 * close. No buffer/report/address pointers are retained. One client token only;
 * no native descriptor or sockaddr layout crosses this ABI. Close consumes each
 * descriptor even on error, never retries EINTR, reports cleanup uncertainty. */
uint32_t ps5_bt_listener_abi_version(void);
uint32_t ps5_bt_listener_storage_size(void);
uint32_t ps5_bt_listener_storage_alignment(void);
int32_t ps5_bt_listener_bind(void *,uint32_t,const uint8_t address[4],uint16_t port,uint16_t *bound_port,struct ps5_bt_listener_diagnostic *);
int32_t ps5_bt_listener_accept(void *,uint64_t *client,struct ps5_bt_listener_diagnostic *);
int32_t ps5_bt_listener_read(void *,uint64_t client,uint8_t *,uint32_t capacity,uint32_t *length,struct ps5_bt_listener_diagnostic *);
int32_t ps5_bt_listener_write(void *,uint64_t client,const uint8_t *,uint32_t length,uint32_t *written,struct ps5_bt_listener_diagnostic *);
int32_t ps5_bt_listener_client_close(void *,uint64_t client,struct ps5_bt_listener_diagnostic *);
int32_t ps5_bt_listener_close(void *,struct ps5_bt_listener_diagnostic *);
#endif
