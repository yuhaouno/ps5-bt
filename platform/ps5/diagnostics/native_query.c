/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "native_query.h"
#include <limits.h>

_Static_assert(sizeof(int) == sizeof(int32_t), "sysctl integer must be int32");
_Static_assert(INT_MAX == INT32_MAX, "sysctl integer must be signed int32");
_Static_assert(sizeof(ps5_native_sysctlbyname_fn) == sizeof(void *),
               "resolver adapter requires equal code/data pointer sizes");

enum ps5_native_query_status
ps5_native_query_once(ps5_native_sysctlbyname_fn query,
                      int64_t raw_resolver_status,
                      struct ps5_native_query_result *out)
{
    if (!out)
        return PS5_NATIVE_QUERY_INVALID;
    *out = (struct ps5_native_query_result){
        .status = PS5_NATIVE_QUERY_UNAVAILABLE,
        .raw_resolver_status = raw_resolver_status};
    if (!query || raw_resolver_status != 0)
        return out->status;

    int32_t value = 0;
    size_t length = sizeof value;
    out->query_called = 1;
    out->raw_query_status =
        query("net.bluetooth.version", &value, &length, NULL, 0);
    out->returned_length = length;
    if (out->raw_query_status != 0) {
        out->status = PS5_NATIVE_QUERY_FAILED;
    } else if (length != sizeof value) {
        out->status = PS5_NATIVE_QUERY_INVALID_LENGTH;
    } else {
        out->status = PS5_NATIVE_QUERY_OK;
        out->value = value;
    }
    return out->status;
}

enum ps5_native_query_status
ps5_native_query_resolve(ps5_native_lookup_fn lookup,
                         struct ps5_native_query_result *out)
{
    if (!out)
        return PS5_NATIVE_QUERY_INVALID;
    if (!lookup)
        return ps5_native_query_once(NULL, 0, out);
    void *symbol = NULL;
    int64_t raw_status = lookup("sysctlbyname", &symbol);
    ps5_native_sysctlbyname_fn query = NULL;
    if (raw_status == 0 && symbol) {
        /* Selected PS5 x86-64 System V ABI assumption, not ISO C portability.
         * Copy representation without a nonstandard object/function cast.
         * The resolver must supply this exact declared function's entry point.
         */
        const unsigned char *source = (const unsigned char *)&symbol;
        unsigned char *destination = (unsigned char *)&query;
        for (size_t i = 0; i < sizeof query; ++i)
            destination[i] = source[i];
    }
    return ps5_native_query_once(query, raw_status, out);
}

/* Local bounded assembly avoids printf, locale, TLS and C library dependency.
 * If ever extended beyond the fixed maximum, encoding fails without output.
 */
struct report_buffer {
    char data[PS5_NATIVE_QUERY_REPORT_CAPACITY];
    size_t length;
    int overflow;
};

static void append_byte(struct report_buffer *buffer, char value)
{
    if (buffer->length == sizeof buffer->data) {
        buffer->overflow = 1;
        return;
    }
    buffer->data[buffer->length++] = value;
}

static void append_text(struct report_buffer *buffer, const char *text)
{
    while (*text)
        append_byte(buffer, *text++);
}

static void append_unsigned(struct report_buffer *buffer, uint64_t value)
{
    char digits[20];
    size_t count = 0;
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value);
    while (count)
        append_byte(buffer, digits[--count]);
}

static void append_signed(struct report_buffer *buffer, int64_t value)
{
    uint64_t magnitude = (uint64_t)value;
    if (value < 0) {
        append_byte(buffer, '-');
        magnitude = (uint64_t)0 - magnitude;
    }
    append_unsigned(buffer, magnitude);
}

static const char *status_name(enum ps5_native_query_status status)
{
    switch (status) {
    case PS5_NATIVE_QUERY_OK:
        return "present";
    case PS5_NATIVE_QUERY_UNAVAILABLE:
        return "symbol_unavailable";
    case PS5_NATIVE_QUERY_FAILED:
        return "query_failed";
    case PS5_NATIVE_QUERY_INVALID_LENGTH:
        return "invalid_length";
    default:
        return NULL;
    }
}

enum ps5_native_query_status
ps5_native_query_encode_report(const struct ps5_native_query_result *result,
                               char *data, size_t capacity, size_t *out_len)
{
    if (!out_len)
        return PS5_NATIVE_QUERY_INVALID;
    *out_len = 0;
    if (!result || (!data && capacity))
        return PS5_NATIVE_QUERY_INVALID;
    const char *status = status_name(result->status);
    if (!status || (result->query_called != 0 && result->query_called != 1))
        return PS5_NATIVE_QUERY_INVALID;
    struct report_buffer buffer = {.length = 0, .overflow = 0};
    append_text(&buffer, "native_query_v1 name=net.bluetooth.version status=");
    append_text(&buffer, status);
    append_text(&buffer, " resolver_status=");
    append_signed(&buffer, result->raw_resolver_status);
    append_text(&buffer, " query_called=");
    append_unsigned(&buffer, (uint64_t)result->query_called);
    append_text(&buffer, " query_status=");
    if (result->query_called)
        append_signed(&buffer, result->raw_query_status);
    else
        append_text(&buffer, "unknown");
    append_text(&buffer, " returned_length=");
    if (result->query_called)
        append_unsigned(&buffer, result->returned_length);
    else
        append_text(&buffer, "unknown");
    append_text(&buffer, " stack_version=");
    if (result->status == PS5_NATIVE_QUERY_OK)
        append_signed(&buffer, result->value);
    else
        append_text(&buffer, "unknown");
    append_text(&buffer,
                " controller=unknown routing=unsupported errno=unknown\n");
    if (buffer.overflow)
        return PS5_NATIVE_QUERY_INVALID;
    *out_len = buffer.length;
    if (!data || capacity < buffer.length)
        return PS5_NATIVE_QUERY_CAPACITY;
    for (size_t i = 0; i < buffer.length; ++i)
        data[i] = buffer.data[i];
    return PS5_NATIVE_QUERY_OK;
}
