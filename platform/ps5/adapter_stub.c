/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Deliberately no SDK calls, device opens, credential changes, or fake hardware. */
#include "ps5_bt.h"

uint32_t ps5_bt_abi_version(void) { return PS5_BT_ABI_VERSION; }

int32_t ps5_bt_init(uint32_t *out_session)
{
    if (!out_session) return PS5_BT_INVALID;
    *out_session = 0;
    return PS5_BT_UNSUPPORTED;
}

int32_t ps5_bt_shutdown(uint32_t session)
{
    (void)session;
    return PS5_BT_UNSUPPORTED;
}

int32_t ps5_bt_send_hci_command(uint32_t session, const uint8_t *data, uint32_t len)
{
    (void)session; (void)data; (void)len;
    return PS5_BT_UNSUPPORTED;
}

int32_t ps5_bt_send_acl(uint32_t session, const uint8_t *data, uint32_t len)
{
    (void)session; (void)data; (void)len;
    return PS5_BT_UNSUPPORTED;
}

int32_t ps5_bt_receive(uint32_t session, uint8_t *data, uint32_t capacity,
                      uint32_t *out_len, uint32_t *out_kind, uint32_t timeout_ms)
{
    (void)session; (void)data; (void)capacity; (void)timeout_ms;
    if (out_len) *out_len = 0;
    if (out_kind) *out_kind = 0;
    if (!out_len || !out_kind) return PS5_BT_INVALID;
    return PS5_BT_UNSUPPORTED;
}
