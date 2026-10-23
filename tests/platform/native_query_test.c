/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "native_query.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static unsigned int query_calls;
static unsigned int lookup_calls;
static int query_status;
static int64_t lookup_status;
static size_t response_length;
static int32_t response_value;
static int missing_symbol;

static int fake_query(const char *name, void *oldp, size_t *oldlenp,
                      const void *newp, size_t newlen)
{
    ++query_calls;
    assert(query_calls == 1); /* No automatic retries, including on failure. */
    assert(strcmp(name, "net.bluetooth.version") == 0);
    assert(oldp && oldlenp);
    assert(*oldlenp == sizeof(int32_t));
    assert(*(int32_t *)oldp == 0); /* Never submit uninitialized storage. */
    assert(newp == NULL && newlen == 0); /* Never submit a value write. */
    *(int32_t *)oldp = response_value;
    *oldlenp = response_length;
    return query_status;
}

static int64_t fake_lookup(const char *name, void **out)
{
    ++lookup_calls;
    assert(lookup_calls == 1);
    assert(strcmp(name, "sysctlbyname") == 0);
    assert(out && *out == NULL);
    /* Host representation test only; not PS5 loader/ABI verification. A dirty
     * non-NULL symbol on resolver failure must still never be invoked. */
    ps5_native_sysctlbyname_fn function = fake_query;
    _Static_assert(sizeof function == sizeof *out,
                   "host pointer test contract");
    if (!missing_symbol)
        memcpy(out, &function, sizeof function);
    return lookup_status;
}

static void reset(void)
{
    query_calls = lookup_calls = 0;
    query_status = 0;
    lookup_status = 0;
    response_length = sizeof(int32_t);
    response_value = 102;
    missing_symbol = 0;
}

static void test_unavailable_and_invalid(void)
{
    struct ps5_native_query_result result;
    reset();
    assert(ps5_native_query_once(fake_query, 0, NULL) ==
           PS5_NATIVE_QUERY_INVALID);
    assert(ps5_native_query_resolve(fake_lookup, NULL) ==
           PS5_NATIVE_QUERY_INVALID);
    assert(query_calls == 0 && lookup_calls == 0);
    memset(&result, 0xa5, sizeof result);
    assert(ps5_native_query_once(NULL, 0, &result) ==
           PS5_NATIVE_QUERY_UNAVAILABLE);
    assert(result.query_called == 0 && result.raw_query_status == 0);
    assert(result.returned_length == 0 && result.value == 0);
    assert(ps5_native_query_resolve(NULL, &result) ==
           PS5_NATIVE_QUERY_UNAVAILABLE);
    assert(query_calls == 0 && lookup_calls == 0);

    const int64_t failures[] = {-1, 13, INT64_MIN, INT64_MAX};
    for (size_t i = 0; i < sizeof failures / sizeof failures[0]; ++i) {
        reset();
        lookup_status = failures[i];
        assert(ps5_native_query_resolve(fake_lookup, &result) ==
               PS5_NATIVE_QUERY_UNAVAILABLE);
        assert(result.raw_resolver_status == failures[i]);
        assert(result.query_called == 0 && result.value == 0);
        assert(lookup_calls == 1 && query_calls == 0);
        assert(ps5_native_query_once(fake_query, failures[i], &result) ==
               PS5_NATIVE_QUERY_UNAVAILABLE);
        assert(query_calls == 0);
    }
    reset();
    missing_symbol =
        1; /* Zero resolver status with a NULL symbol is not success. */
    assert(ps5_native_query_resolve(fake_lookup, &result) ==
           PS5_NATIVE_QUERY_UNAVAILABLE);
    assert(result.raw_resolver_status == 0 && result.query_called == 0);
    assert(query_calls == 0 && lookup_calls == 1);
}

static void test_query_evidence(void)
{
    struct ps5_native_query_result result;
    const int32_t values[] = {0, 102, -1, INT32_MIN, INT32_MAX};
    for (size_t i = 0; i < sizeof values / sizeof values[0]; ++i) {
        reset();
        response_value = values[i];
        assert(ps5_native_query_resolve(fake_lookup, &result) ==
               PS5_NATIVE_QUERY_OK);
        assert(result.raw_resolver_status == 0 && result.raw_query_status == 0);
        assert(result.query_called == 1 &&
               result.returned_length == sizeof(int32_t));
        assert(result.value == values[i]);
        assert(query_calls == 1 && lookup_calls == 1);
    }
    for (size_t length = 0; length <= 16; ++length) {
        reset();
        response_length = length;
        enum ps5_native_query_status expected =
            length == sizeof(int32_t) ? PS5_NATIVE_QUERY_OK
                                      : PS5_NATIVE_QUERY_INVALID_LENGTH;
        assert(ps5_native_query_once(fake_query, 0, &result) == expected);
        assert(result.raw_query_status == 0 &&
               result.returned_length == length);
        assert(result.value == (expected == PS5_NATIVE_QUERY_OK ? 102 : 0));
        assert(query_calls == 1);
    }
    reset();
    response_length = SIZE_MAX;
    assert(ps5_native_query_once(fake_query, 0, &result) ==
           PS5_NATIVE_QUERY_INVALID_LENGTH);
    assert(result.returned_length == SIZE_MAX && result.value == 0);

    const int failures[] = {-1, 13, INT_MIN, INT_MAX};
    for (size_t i = 0; i < sizeof failures / sizeof failures[0]; ++i) {
        reset();
        query_status = failures[i];
        response_length = SIZE_MAX; /* Query error remains primary evidence. */
        assert(ps5_native_query_once(fake_query, 0, &result) ==
               PS5_NATIVE_QUERY_FAILED);
        assert(result.raw_query_status == failures[i]);
        assert(result.returned_length == SIZE_MAX && result.value == 0);
        assert(result.query_called == 1 && query_calls == 1);
    }
}

static void assert_bounded_report(const struct ps5_native_query_result *result)
{
    char reference[PS5_NATIVE_QUERY_REPORT_CAPACITY + 1];
    char guarded[PS5_NATIVE_QUERY_REPORT_CAPACITY + 2];
    size_t length = 123;
    memset(reference, 'X', sizeof reference);
    assert(ps5_native_query_encode_report(result, reference,
                                          sizeof reference - 1,
                                          &length) == PS5_NATIVE_QUERY_OK);
    assert(length > 0 && length <= PS5_NATIVE_QUERY_REPORT_CAPACITY);
    assert(reference[length] == 'X'); /* Length delimited, no hidden NUL. */
    assert(reference[length - 1] == '\n');
    for (size_t i = 0; i + 1 < length; ++i)
        assert(reference[i] >= ' ' && reference[i] <= '~');

    for (size_t capacity = 0; capacity <= length + 1; ++capacity) {
        memset(guarded, 'X', sizeof guarded);
        size_t actual = 999;
        enum ps5_native_query_status status = ps5_native_query_encode_report(
            result, guarded + 1, capacity, &actual);
        assert(actual == length);
        assert(guarded[0] == 'X');
        if (capacity < length) {
            assert(status == PS5_NATIVE_QUERY_CAPACITY);
            for (size_t i = 0; i < sizeof guarded; ++i)
                assert(guarded[i] == 'X');
        } else {
            assert(status == PS5_NATIVE_QUERY_OK);
            assert(memcmp(guarded + 1, reference, length) == 0);
            for (size_t i = length + 1; i < sizeof guarded; ++i)
                assert(guarded[i] == 'X');
        }
    }
    size_t required = 0;
    assert(ps5_native_query_encode_report(result, NULL, 0, &required) ==
           PS5_NATIVE_QUERY_CAPACITY);
    assert(required == length);
}

static void test_reports(void)
{
    struct ps5_native_query_result result;
    char data[PS5_NATIVE_QUERY_REPORT_CAPACITY];
    size_t length;
    reset();
    assert(ps5_native_query_once(fake_query, 0, &result) ==
           PS5_NATIVE_QUERY_OK);
    assert(ps5_native_query_encode_report(&result, data, sizeof data,
                                          &length) == PS5_NATIVE_QUERY_OK);
    static const char expected[] =
        "native_query_v1 name=net.bluetooth.version status=present "
        "resolver_status=0"
        " query_called=1 query_status=0 returned_length=4 stack_version=102"
        " controller=unknown routing=unsupported errno=unknown\n";
    assert(length == sizeof expected - 1 &&
           memcmp(data, expected, length) == 0);
    assert_bounded_report(&result);
    reset();
    response_value = INT32_MIN;
    assert(ps5_native_query_once(fake_query, 0, &result) ==
           PS5_NATIVE_QUERY_OK);
    assert_bounded_report(&result);
    reset();
    assert(ps5_native_query_once(NULL, INT64_MIN, &result) ==
           PS5_NATIVE_QUERY_UNAVAILABLE);
    assert_bounded_report(&result);
    reset();
    query_status = INT_MIN;
    response_length = SIZE_MAX;
    assert(ps5_native_query_once(fake_query, 0, &result) ==
           PS5_NATIVE_QUERY_FAILED);
    assert_bounded_report(&result);
    reset();
    response_length = SIZE_MAX;
    assert(ps5_native_query_once(fake_query, 0, &result) ==
           PS5_NATIVE_QUERY_INVALID_LENGTH);
    assert_bounded_report(&result);

    memset(data, 'X', sizeof data);
    length = 42;
    assert(ps5_native_query_encode_report(NULL, data, sizeof data, &length) ==
           PS5_NATIVE_QUERY_INVALID);
    assert(length == 0);
    assert(ps5_native_query_encode_report(&result, NULL, 1, &length) ==
           PS5_NATIVE_QUERY_INVALID);
    assert(ps5_native_query_encode_report(&result, data, sizeof data, NULL) ==
           PS5_NATIVE_QUERY_INVALID);
    result.status = (enum ps5_native_query_status)12345;
    assert(ps5_native_query_encode_report(&result, data, sizeof data,
                                          &length) == PS5_NATIVE_QUERY_INVALID);
    result.status = PS5_NATIVE_QUERY_OK;
    result.query_called = INT_MIN;
    assert(ps5_native_query_encode_report(&result, data, sizeof data,
                                          &length) == PS5_NATIVE_QUERY_INVALID);
    for (size_t i = 0; i < sizeof data; ++i)
        assert(data[i] == 'X');
    assert(query_calls == 1); /* Rendering never repeats the query. */
}

int main(void)
{
    test_unavailable_and_invalid();
    test_query_evidence();
    test_reports();
    puts("PASS synthetic single native query, resolver failures and bounded "
         "reports");
    return 0;
}
