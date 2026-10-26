/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef PS5_BT_BOND_FILE_STORE_H
#define PS5_BT_BOND_FILE_STORE_H
#include <stdint.h>

#define PS5_BT_STORE_ABI_VERSION 1u
#define PS5_BT_STORE_RECORD_BYTES 32u
#define PS5_BT_STORE_PATH_MAX 512u
#define PS5_BT_STORE_COMMIT_ATTEMPTED 1u
#define PS5_BT_STORE_COMMITTED 2u
#define PS5_BT_STORE_CLEANUP_UNKNOWN 4u

enum ps5_bt_store_status {
    PS5_BT_STORE_OK = 0,
    PS5_BT_STORE_ABSENT = 1,
    PS5_BT_STORE_INVALID = -1,
    PS5_BT_STORE_IO = -2,
    PS5_BT_STORE_DURABILITY_UNKNOWN = -3,
    PS5_BT_STORE_SECURITY = -4,
    PS5_BT_STORE_BUSY = -5,
    PS5_BT_STORE_LIMIT = -6,
    PS5_BT_STORE_POISONED = -7
};
enum ps5_bt_store_stage {
    PS5_BT_STORE_VALIDATE = 1,
    PS5_BT_STORE_DIRECTORY = 2,
    PS5_BT_STORE_LOCK = 3,
    PS5_BT_STORE_METADATA = 4,
    PS5_BT_STORE_READ = 5,
    PS5_BT_STORE_TEMP = 6,
    PS5_BT_STORE_WRITE = 7,
    PS5_BT_STORE_FILE_SYNC = 8,
    PS5_BT_STORE_REVALIDATE = 9,
    PS5_BT_STORE_RENAME = 10,
    PS5_BT_STORE_DIRECTORY_SYNC = 11,
    PS5_BT_STORE_UNLINK = 12,
    PS5_BT_STORE_CLOSE = 13
};
struct ps5_bt_store_diagnostic {
    uint32_t abi_version;
    uint32_t stage;
    int32_t status;
    int32_t raw_errno;
    uint32_t flags;
    int32_t cleanup_errno;
};

/* Caller provides zero-initialized, queried-size/alignment storage. A successful
 * open owns it until exactly one close, including when a success report is dirty.
 * A nonzero open return owns no live context. Close consumes the context on every
 * return and must never be retried. No function retains path/peer/data/report
 * pointers. Reports contain no path, identity or record bytes. Host system stat,
 * flags and uid/off_t layouts stay private to the C implementation. */
uint32_t ps5_bt_store_abi_version(void);
uint32_t ps5_bt_store_storage_size(void);
uint32_t ps5_bt_store_storage_alignment(void);
int32_t ps5_bt_store_open(void *storage, uint32_t capacity, const uint8_t *path,
                          uint32_t path_len, struct ps5_bt_store_diagnostic *report);
int32_t ps5_bt_store_load(void *storage, const uint8_t peer[6], uint8_t output[32],
                          uint32_t *out_len, struct ps5_bt_store_diagnostic *report);
int32_t ps5_bt_store_save(void *storage, const uint8_t peer[6], const uint8_t record[32],
                          struct ps5_bt_store_diagnostic *report);
int32_t ps5_bt_store_delete(void *storage, const uint8_t peer[6],
                            struct ps5_bt_store_diagnostic *report);
int32_t ps5_bt_store_close(void *storage, struct ps5_bt_store_diagnostic *report);
#endif
