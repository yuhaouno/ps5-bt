/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#include <stdio.h>
#include <stdlib.h>

void ps5_platform_log(const uint8_t *data, uint32_t len)
{
    if (data && len) (void)fwrite(data, 1, len, stderr);
    (void)fflush(stderr);
}

_Noreturn void ps5_platform_abort(void)
{
    _Exit(101);
}
