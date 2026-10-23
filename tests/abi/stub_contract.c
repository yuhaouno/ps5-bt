/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#include <assert.h>
#include <stddef.h>

int main(void)
{
    uint32_t token = 99, len = 99, kind = 99;
    assert(ps5_bt_abi_version() == PS5_BT_ABI_VERSION);
    assert(ps5_bt_init(NULL) == PS5_BT_INVALID);
    assert(ps5_bt_init(&token) == PS5_BT_UNSUPPORTED && token == 0);
    assert(ps5_bt_receive(0, NULL, 0, &len, &kind, 0) == PS5_BT_UNSUPPORTED);
    assert(len == 0 && kind == 0);
    assert(ps5_bt_receive(0, NULL, 0, NULL, &kind, 0) == PS5_BT_INVALID);
    assert(ps5_bt_shutdown(0) == PS5_BT_UNSUPPORTED);
    assert(ps5_bt_send_hci_command(0, NULL, 0) == PS5_BT_UNSUPPORTED);
    assert(ps5_bt_send_acl(0, NULL, 0) == PS5_BT_UNSUPPORTED);
    return 0;
}
