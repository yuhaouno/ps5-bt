/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "ps5_bt.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define main diagnostic_main
#include "../../platform/ps5/main.c"
#undef main

static unsigned int stage;
static unsigned int rust_calls;
#ifdef PS5_BT_NATIVE_QUERY
static unsigned int query_calls;
static unsigned int lookup_calls;
static unsigned int log_calls;
static int64_t resolver_status;
static int query_status;
static int missing_symbol;
static size_t query_length;
static char report[PS5_NATIVE_QUERY_REPORT_CAPACITY + 1];

static int fixture_query(const char *name, void *oldp, size_t *oldlenp,
                         const void *newp, size_t newlen)
{
    assert(stage == 3);
    stage = 4;
    assert(++query_calls == 1);
    assert(strcmp(name, "net.bluetooth.version") == 0);
    assert(*(int32_t *)oldp == 0 && *oldlenp == sizeof(int32_t));
    assert(newp == NULL && newlen == 0);
    *(int32_t *)oldp = 102;
    *oldlenp = query_length;
    return query_status;
}

int64_t ps5_bootstrap_lookup(const char *name, void **out)
{
    assert(stage == 2);
    stage = 3;
    assert(++lookup_calls == 1);
    assert(strcmp(name, "sysctlbyname") == 0);
    assert(*out == NULL);
    ps5_native_sysctlbyname_fn function = fixture_query;
    _Static_assert(sizeof function == sizeof *out, "host fixture pointer ABI");
    if (!missing_symbol)
        memcpy(out, &function, sizeof function);
    return resolver_status;
}

void ps5_platform_log(const uint8_t *data, uint32_t len)
{
    assert(data && len);
    ++log_calls;
    if (log_calls == 1) {
        static const char begin[] = "event=native_query_begin\n";
        assert(stage == 1 && rust_calls == 1);
        assert(len == sizeof begin - 1 && memcmp(data, begin, len) == 0);
        stage = 2;
    } else {
        assert(log_calls == 2 && (stage == 3 || stage == 4));
        assert(len <= PS5_NATIVE_QUERY_REPORT_CAPACITY);
        memcpy(report, data, len);
        report[len] = '\0'; /* The logger, not the payload, adds a test NUL. */
        stage = 5;
    }
}

static void reset(void)
{
    stage = rust_calls = query_calls = lookup_calls = log_calls = 0;
    resolver_status = query_status = missing_symbol = 0;
    query_length = sizeof(int32_t);
    memset(report, 0, sizeof report);
}

static void run_scenario(const char *expected_status,
                         unsigned int expected_calls)
{
    assert(diagnostic_main() ==
           2); /* Presence never changes transport result. */
    assert(stage == 5 && rust_calls == 1 && lookup_calls == 1 &&
           log_calls == 2);
    assert(query_calls == expected_calls);
    assert(strstr(report, expected_status));
    assert(strstr(report, "controller=unknown routing=unsupported"));
}
#endif

int32_t ps5_rust_probe(void)
{
    assert(stage == 0 && ++rust_calls == 1);
    stage = 1;
    return 2;
}

int main(void)
{
#ifdef PS5_BT_NATIVE_QUERY
    reset();
    run_scenario("status=present", 1);
    assert(strstr(report, "stack_version=102"));
    reset();
    resolver_status = -73;
    run_scenario("status=symbol_unavailable", 0);
    assert(strstr(report, "resolver_status=-73 query_called=0"));
    reset();
    missing_symbol = 1;
    run_scenario("status=symbol_unavailable", 0);
    assert(strstr(report, "resolver_status=0 query_called=0"));
    reset();
    query_status = -1;
    run_scenario("status=query_failed", 1);
    assert(strstr(report, "query_status=-1"));
    assert(strstr(report, "stack_version=unknown"));
    reset();
    query_length = SIZE_MAX;
    run_scenario("status=invalid_length", 1);
    assert(strstr(report, "stack_version=unknown"));
    puts("PASS real optional main Rust ordering, one query and preserved "
         "result");
#else
    /* Link this fixture without native_query.c or any resolver/logger symbol.
     * The baseline must depend only on the existing Rust entry. */
    assert(diagnostic_main() == 2);
    assert(stage == 1 && rust_calls == 1);
    puts(
        "PASS real baseline main without native query/resolver/logger linkage");
#endif
    return 0;
}
