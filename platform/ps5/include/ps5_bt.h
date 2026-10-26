/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_H
#define PS5_BT_H

#include <stdint.h>

/* Our experimental ABI, not SDK functions. Normative semantics: docs/ABI.md. */
#define PS5_BT_ABI_VERSION UINT32_C(0x00010000)
#define PS5_BT_OK INT32_C(0)
#define PS5_BT_UNSUPPORTED INT32_C(-1)
#define PS5_BT_INVALID INT32_C(-2)
#define PS5_BT_BUSY INT32_C(-3)
#define PS5_BT_BUFFER_TOO_SMALL INT32_C(-4)
#define PS5_BT_IO INT32_C(-5)
#define PS5_BT_DISCONNECTED INT32_C(-6)
#define PS5_BT_INTEGRITY_LOST INT32_C(-7)
#define PS5_BT_TIMEOUT INT32_C(-8)
#define PS5_BT_EVENT UINT32_C(1)
#define PS5_BT_ACL UINT32_C(2)

uint32_t ps5_bt_abi_version(void);
int32_t ps5_bt_init(uint32_t *out_session);
int32_t ps5_bt_shutdown(uint32_t session);
int32_t ps5_bt_send_hci_command(uint32_t session, const uint8_t *data, uint32_t len);
int32_t ps5_bt_send_acl(uint32_t session, const uint8_t *data, uint32_t len);
int32_t ps5_bt_receive(uint32_t session, uint8_t *data, uint32_t capacity,
                      uint32_t *out_len, uint32_t *out_kind, uint32_t timeout_ms);
void ps5_platform_log(const uint8_t *data, uint32_t len);
_Noreturn void ps5_platform_abort(void);

/* Rust entry consumed by our C bootstrap. */
int32_t ps5_rust_probe(void);
/* Independently selected bounded control-only entry. secret is 32 lowercase hex
 * bytes from a protected native source, valid only for the synchronous call. */
int32_t ps5_rust_application_probe(int64_t (*lookup)(const char *, void **), const uint8_t *secret, uint32_t secret_len);
void ps5_rust_application_request_stop(void);
int32_t ps5_rust_application_stop_requested(void);

#endif
