/* SPDX-License-Identifier: GPL-3.0-or-later */
/* HOST TEST ONLY. Synthetic hardware. Never include in a PS5 link recipe. */
#include "ps5_bt.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int active;
static int sent;
static int mode(const char *name)
{
    const char *value = getenv("PS5_BT_TEST_MODE");
    return value && strcmp(value, name) == 0;
}

uint32_t ps5_bt_abi_version(void)
{
    return mode("abi_mismatch") ? 0 : PS5_BT_ABI_VERSION;
}

int32_t ps5_bt_init(uint32_t *out)
{
    assert(out && !active);
    *out = 0;
    fputs("test=synthetic_init\n", stderr);
    if (mode("backend_status")) return 77;
    if (mode("zero_token")) return PS5_BT_OK;
    *out = 0x1234;
    active = 1;
    return PS5_BT_OK;
}

int32_t ps5_bt_shutdown(uint32_t session)
{
    assert(active && session == 0x1234);
    active = 0;
    fputs("test=synthetic_shutdown\n", stderr);
    return mode("shutdown_failure") ? PS5_BT_IO : PS5_BT_OK;
}

int32_t ps5_bt_send_hci_command(uint32_t session, const uint8_t *data, uint32_t len)
{
    static const uint8_t expected[] = {1, 0x10, 0};
    assert(active && session == 0x1234 && data);
    assert(len == sizeof expected && memcmp(data, expected, len) == 0);
    assert(!sent);
    sent = 1;
    return mode("send_failure") ? PS5_BT_BUSY : PS5_BT_OK;
}

int32_t ps5_bt_send_acl(uint32_t session, const uint8_t *data, uint32_t len)
{
    (void)session; (void)data; (void)len;
    assert(0 && "diagnostic must never send ACL");
    return PS5_BT_UNSUPPORTED;
}

int32_t ps5_bt_receive(uint32_t session, uint8_t *data, uint32_t capacity,
                      uint32_t *out_len, uint32_t *out_kind, uint32_t timeout_ms)
{
    /* Same hand-constructed wire event as tests/fixtures/hci/local-version.bin. */
    static const uint8_t response[] = {0x0e, 12, 1, 1, 0x10, 0, 9,
                                       0x34, 0x12, 9, 0xcd, 0xab, 0x78, 0x56};
    assert(active && sent && session == 0x1234 && data && out_len && out_kind);
    assert(timeout_ms == 1000 && capacity >= sizeof response);
    *out_len = 0;
    *out_kind = 0;
    if (mode("timeout")) return PS5_BT_TIMEOUT;
    if (mode("dirty_timeout")) { *out_kind = PS5_BT_EVENT; return PS5_BT_TIMEOUT; }
    if (mode("receive_failure")) return PS5_BT_INTEGRITY_LOST;
    memcpy(data, response, sizeof response);
    *out_len = (uint32_t)sizeof response;
    *out_kind = PS5_BT_EVENT;
    if (mode("oversized_length")) *out_len = capacity + 1; /* No invalid write. */
    if (mode("unknown_kind")) *out_kind = 99;
    if (mode("truncated")) --*out_len;
    if (mode("foreign_opcode")) data[3] = 2;
    if (mode("controller_failure")) data[5] = 0x0c;
    return PS5_BT_OK;
}
