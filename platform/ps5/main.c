/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#ifdef PS5_BT_NATIVE_QUERY
#include "bootstrap/bootstrap.h"
#include "diagnostics/native_query.h"
#endif

int main(void)
{
    int result = (int)ps5_rust_probe();
#ifdef PS5_BT_NATIVE_QUERY
    static const uint8_t begin[] = "event=native_query_begin\n";
    static const uint8_t encode_failed[] = "event=native_query_encode_failed\n";
    struct ps5_native_query_result native;
    char line[PS5_NATIVE_QUERY_REPORT_CAPACITY];
    size_t length = 0;
    ps5_platform_log(begin, (uint32_t)(sizeof begin - 1));
    (void)ps5_native_query_resolve(ps5_bootstrap_lookup, &native);
    if (ps5_native_query_encode_report(&native, line, sizeof line, &length) ==
        PS5_NATIVE_QUERY_OK) {
        ps5_platform_log((const uint8_t *)line, (uint32_t)length);
    } else {
        ps5_platform_log(encode_failed, (uint32_t)(sizeof encode_failed - 1));
    }
#endif
    return result;
}
