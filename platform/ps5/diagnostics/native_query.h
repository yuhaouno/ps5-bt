/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_NATIVE_QUERY_H
#define PS5_NATIVE_QUERY_H

#include <stddef.h>
#include <stdint.h>

#define PS5_NATIVE_QUERY_REPORT_CAPACITY 384u

/* Matches SDK v0.45 include/freebsd/sys/sysctl.h exactly. */
typedef int (*ps5_native_sysctlbyname_fn)(const char *, void *, size_t *,
                                          const void *, size_t);
typedef int64_t (*ps5_native_lookup_fn)(const char *, void **);

enum ps5_native_query_status {
    PS5_NATIVE_QUERY_OK = 0,
    PS5_NATIVE_QUERY_UNAVAILABLE = -1,
    PS5_NATIVE_QUERY_FAILED = -2,
    PS5_NATIVE_QUERY_INVALID_LENGTH = -3,
    PS5_NATIVE_QUERY_INVALID = -4,
    PS5_NATIVE_QUERY_CAPACITY = -5
};

/* Query-local evidence only; never a transport session or controller identity.
 * raw_query_status/returned_length are meaningful only when query_called == 1.
 * value is meaningful only when status == OK; otherwise it is cleared.
 */
struct ps5_native_query_result {
    enum ps5_native_query_status status;
    int64_t raw_resolver_status;
    int query_called;
    int raw_query_status;
    size_t returned_length;
    int32_t value;
};

/* Exactly one fixed name request, with initialized int32 storage and NULL,0
 * write arguments. No retry, errno, I/O, allocation, globals or retained data.
 * Nonzero resolver status or NULL query prevents invocation. A valid callback
 * must obey sysctlbyname's storage bounds and synchronous lifetime contract;
 * post-call length checks cannot repair a malicious callback's buffer write.
 * NULL out returns INVALID without invoking anything.
 */
enum ps5_native_query_status
ps5_native_query_once(ps5_native_sysctlbyname_fn query,
                      int64_t raw_resolver_status,
                      struct ps5_native_query_result *out);

/* One lookup of "sysctlbyname", then the same query. NULL lookup is explicitly
 * unavailable with no calls. Nonzero lookup status ignores even a dirty symbol.
 * This adapter assumes the selected PS5 x86-64 System V code/data pointer
 * representations match; byte-copy conversion is isolated in the
 * implementation. It neither loads modules nor guesses addresses. Use the
 * reviewed bootstrap helper only during main. Host injection tests are not PS5
 * ABI verification.
 */
enum ps5_native_query_status
ps5_native_query_resolve(ps5_native_lookup_fn lookup,
                         struct ps5_native_query_result *out);

/* Deterministic ASCII v1 line without a trailing NUL, <= REPORT_CAPACITY bytes.
 * Requires an unmodified result from the functions above. On invalid input,
 * *out_len is zero. On short capacity, *out_len is the required size and data
 * is unchanged. NULL data with zero capacity may query the required size.
 * Input, data and out_len must be distinct valid objects; no pointer retained.
 */
enum ps5_native_query_status
ps5_native_query_encode_report(const struct ps5_native_query_result *result,
                               char *data, size_t capacity, size_t *out_len);

#endif
