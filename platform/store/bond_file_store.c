/* SPDX-License-Identifier: GPL-3.0-or-later
 * Native POSIX candidate. Compile against reviewed target SYSTEM headers; no
 * struct-stat copies, hand-written flag values or guessed PS5 syscall numbers.
 */
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE
#define _DARWIN_C_SOURCE
#include "bond_file_store.h"
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#define MAGIC UINT32_C(0x50354253)
#define ATTEMPTS 64u
#define NAME_BYTES 32u
struct store {
    uint32_t magic;
    int dir_fd;
    int lock_fd;
    uid_t owner;
    uint64_t serial;
    int poisoned;
};

/* Test hooks exist only in the separately compiled synthetic fault harness.
 * A nonzero result injects errno before this stage's native operation. */
#ifdef PS5_BT_STORE_TEST
extern int ps5_bt_store_test_fault(uint32_t stage);
extern void ps5_bt_store_test_after(uint32_t stage, int dir_fd);
extern ssize_t ps5_bt_store_test_read(int, void *, size_t);
extern ssize_t ps5_bt_store_test_write(int, const void *, size_t);
extern int ps5_bt_store_test_close(int);
#define STORE_READ ps5_bt_store_test_read
#define STORE_WRITE ps5_bt_store_test_write
#define STORE_CLOSE ps5_bt_store_test_close
#define FAULT(stage) test_fault(stage)
#define AFTER(stage, fd) ps5_bt_store_test_after(stage, fd)
static int test_fault(uint32_t stage)
{
    int error = ps5_bt_store_test_fault(stage);
    if (error) {
        errno = error;
        return 1;
    }
    return 0;
}
#else
#define FAULT(stage) ((void)(stage), 0)
#define STORE_READ read
#define STORE_WRITE write
#define STORE_CLOSE close
#define AFTER(stage, fd) ((void)0)
#endif

static void wipe(void *data, size_t size)
{
    volatile unsigned char *bytes = data;
    while (size--)
        *bytes++ = 0;
}
static void init_report(struct ps5_bt_store_diagnostic *r)
{
    memset(r, 0, sizeof *r);
    r->abi_version = PS5_BT_STORE_ABI_VERSION;
}
static int32_t result(struct ps5_bt_store_diagnostic *r, int status, unsigned stage,
                      int raw_errno)
{
    r->status = status;
    r->stage = stage;
    r->raw_errno = raw_errno;
    return status;
}
static int32_t io(struct ps5_bt_store_diagnostic *r, unsigned stage)
{
    int error = errno;
    return result(r, PS5_BT_STORE_IO, stage, error);
}
static int close_once(int *fd, struct ps5_bt_store_diagnostic *r)
{
    if (*fd < 0)
        return 0;
    int owned = *fd;
    *fd = -1;
    /* close(EINTR) must never be retried: descriptor ownership is uncertain. */
    if (STORE_CLOSE(owned)) {
        int error = errno;
        if (!r->cleanup_errno)
            r->cleanup_errno = error;
        r->flags |= PS5_BT_STORE_CLEANUP_UNKNOWN;
        return -1;
    }
    return 0;
}
static int sync_fd(int fd, unsigned stage)
{
    for (unsigned n = 0; n < ATTEMPTS; n++) {
        if (!FAULT(stage) && fsync(fd) == 0)
            return 0;
        if (errno != EINTR)
            return -1;
    }
    errno = EINTR;
    return -1;
}
static int file_stat(int fd, struct stat *st)
{
    for (unsigned n = 0; n < ATTEMPTS; n++) {
        if (fstat(fd, st) == 0)
            return 0;
        if (errno != EINTR)
            return -1;
    }
    errno = EINTR;
    return -1;
}
static int name_stat(int fd, const char *name, struct stat *st)
{
    for (unsigned n = 0; n < ATTEMPTS; n++) {
        if (fstatat(fd, name, st, AT_SYMLINK_NOFOLLOW) == 0)
            return 0;
        if (errno != EINTR)
            return -1;
    }
    errno = EINTR;
    return -1;
}
static int safe_file(const struct stat *st, uid_t uid, off_t length)
{
    return S_ISREG(st->st_mode) && (st->st_mode & 07777) == 0600 &&
           st->st_uid == uid && st->st_nlink == 1 && st->st_size == length;
}
static int same_file(const struct stat *a, const struct stat *b)
{
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino;
}
static void peer_name(const uint8_t peer[6], char out[NAME_BYTES])
{
    static const char hex[] = "0123456789abcdef";
    memcpy(out, "p5bd-", 5);
    for (unsigned i = 0; i < 6; i++) {
        out[5 + i * 2] = hex[peer[i] >> 4];
        out[6 + i * 2] = hex[peer[i] & 15];
    }
    memcpy(out + 17, ".bond", 6);
}
static int record_valid(const uint8_t peer[6], const uint8_t record[32])
{
    return memcmp(record, "P5BD", 4) == 0 && record[4] == 1 && record[5] == 3 &&
           (record[6] == 4 || record[6] == 5 || record[6] == 7 || record[6] == 8) &&
           record[7] >= 7 && record[7] <= 16 && memcmp(record + 8, peer, 6) == 0 &&
           record[14] == 0 && record[15] == 0;
}
static int context(struct store *s, struct ps5_bt_store_diagnostic *r)
{
    if (!s || s->magic != MAGIC || s->dir_fd < 0 || s->lock_fd < 0)
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    if (s->poisoned)
        return result(r, PS5_BT_STORE_POISONED, PS5_BT_STORE_VALIDATE, 0);
    struct stat directory, lock, named;
    if (file_stat(s->dir_fd, &directory) || file_stat(s->lock_fd, &lock) ||
        name_stat(s->dir_fd, ".p5bt.lock", &named))
        return io(r, PS5_BT_STORE_REVALIDATE);
    if (geteuid() != s->owner || !S_ISDIR(directory.st_mode) ||
        directory.st_uid != s->owner || (directory.st_mode & 07777) != 0700 ||
        directory.st_nlink == 0 || !safe_file(&lock, s->owner, 0) ||
        !safe_file(&named, s->owner, 0) || !same_file(&lock, &named))
        return result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_REVALIDATE, 0);
    return 0;
}

uint32_t ps5_bt_store_abi_version(void) { return PS5_BT_STORE_ABI_VERSION; }
uint32_t ps5_bt_store_storage_size(void) { return (uint32_t)sizeof(struct store); }
uint32_t ps5_bt_store_storage_alignment(void) { return (uint32_t)_Alignof(struct store); }

int32_t ps5_bt_store_open(void *storage, uint32_t capacity, const uint8_t *path,
                          uint32_t length, struct ps5_bt_store_diagnostic *r)
{
    if (!r)
        return PS5_BT_STORE_INVALID;
    init_report(r);
    if (!storage || capacity < sizeof(struct store) ||
        (uintptr_t)storage % _Alignof(struct store) || !path || length < 2 ||
        length > PS5_BT_STORE_PATH_MAX || path[0] != '/' || path[length - 1] == '/' ||
        memchr(path, 0, length))
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    struct store *s = storage;
    if (s->magic == MAGIC)
        return result(r, PS5_BT_STORE_BUSY, PS5_BT_STORE_VALIDATE, 0);
    memset(s, 0, sizeof *s);
    s->dir_fd = s->lock_fd = -1;
    s->owner = geteuid();
    int parent = open("/", O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (parent < 0)
        return io(r, PS5_BT_STORE_DIRECTORY);
    uint32_t cursor = 1;
    int status = PS5_BT_STORE_OK;
    while (cursor < length) {
        uint32_t start = cursor;
        while (cursor < length && path[cursor] != '/')
            cursor++;
        uint32_t count = cursor - start;
        char component[256];
        if (!count || count > 255 || (count == 1 && path[start] == '.') ||
            (count == 2 && path[start] == '.' && path[start + 1] == '.')) {
            status = result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_DIRECTORY, 0);
            break;
        }
        memcpy(component, path + start, count);
        component[count] = 0;
        int child = openat(parent, component, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
        if (child < 0) {
            status = io(r, PS5_BT_STORE_DIRECTORY);
            break;
        }
        if (close_once(&parent, r)) {
            close_once(&child, r);
            status = result(r, PS5_BT_STORE_IO, PS5_BT_STORE_CLOSE, r->cleanup_errno);
            break;
        }
        parent = child;
        struct stat st;
        if (file_stat(parent, &st)) {
            status = io(r, PS5_BT_STORE_DIRECTORY);
            break;
        }
        int final = cursor == length;
        if (!S_ISDIR(st.st_mode) ||
            (final ? st.st_uid != s->owner || (st.st_mode & 07777) != 0700 :
             (st.st_uid != 0 && st.st_uid != s->owner) ||
             ((st.st_mode & 0022) && !(st.st_mode & S_ISVTX)))) {
            status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_DIRECTORY, 0);
            break;
        }
        if (!final)
            cursor++;
    }
    if (status) {
        close_once(&parent, r);
        return status;
    }
    s->dir_fd = parent;
    s->lock_fd = openat(parent, ".p5bt.lock", O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
    int new_lock = s->lock_fd >= 0;
    if (s->lock_fd < 0 && errno == EEXIST)
        s->lock_fd = openat(parent, ".p5bt.lock", O_RDWR | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
    if (s->lock_fd < 0) {
        status = io(r, PS5_BT_STORE_LOCK);
        goto failed;
    }
    struct stat st;
    if ((new_lock && fchmod(s->lock_fd, 0600)) || file_stat(s->lock_fd, &st)) {
        status = io(r, PS5_BT_STORE_LOCK);
        goto failed;
    }
    if (!safe_file(&st, s->owner, 0)) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_LOCK, 0);
        goto failed;
    }
    if (flock(s->lock_fd, LOCK_EX | LOCK_NB)) {
        int error = errno;
        status = result(r, PS5_BT_STORE_BUSY, PS5_BT_STORE_LOCK, error);
        goto failed;
    }
    if (new_lock && (sync_fd(s->lock_fd, PS5_BT_STORE_FILE_SYNC) || sync_fd(parent, PS5_BT_STORE_DIRECTORY_SYNC))) {
        status = io(r, PS5_BT_STORE_LOCK);
        goto failed;
    }
    s->magic = MAGIC;
    status = context(s, r);
    if (status) {
        s->magic = 0;
        goto failed;
    }
    return result(r, PS5_BT_STORE_OK, PS5_BT_STORE_DIRECTORY, 0);
failed:
    close_once(&s->lock_fd, r);
    close_once(&s->dir_fd, r);
    return status;
}

int32_t ps5_bt_store_load(void *storage, const uint8_t peer[6], uint8_t output[32],
                          uint32_t *out_len, struct ps5_bt_store_diagnostic *r)
{
    if (!r)
        return PS5_BT_STORE_INVALID;
    init_report(r);
    if (out_len)
        *out_len = 0;
    if (output)
        memset(output, 0, 32);
    if (!peer || !output || !out_len)
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    struct store *s = storage;
    int status = context(s, r);
    if (status)
        return status;
    char name[NAME_BYTES];
    peer_name(peer, name);
    int fd = openat(s->dir_fd, name, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
    if (fd < 0)
        return errno == ENOENT ? result(r, PS5_BT_STORE_ABSENT, PS5_BT_STORE_READ, 0) : io(r, PS5_BT_STORE_READ);
    struct stat before, after, named;
    uint8_t bytes[33] = {0};
    if (file_stat(fd, &before)) {
        status = io(r, PS5_BT_STORE_METADATA);
        goto done;
    }
    if (!safe_file(&before, s->owner, 32)) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_METADATA, 0);
        goto done;
    }
    size_t count = 0;
    unsigned step = 0;
    for (; step < ATTEMPTS && count < sizeof bytes; step++) {
        ssize_t got = FAULT(PS5_BT_STORE_READ) ? -1 : STORE_READ(fd, bytes + count, sizeof bytes - count);
        if (got > 0)
            count += (size_t)got;
        else if (got == 0)
            break;
        else if (errno != EINTR) {
            status = io(r, PS5_BT_STORE_READ);
            goto done;
        }
    }
    if (step == ATTEMPTS && count != 32) {
        status = result(r, PS5_BT_STORE_LIMIT, PS5_BT_STORE_READ, EINTR);
        goto done;
    }
    if (count != 32 || !record_valid(peer, bytes)) {
        status = result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_READ, 0);
        goto done;
    }
    AFTER(PS5_BT_STORE_READ, s->dir_fd);
    status = context(s, r);
    if (status)
        goto done;
    if (file_stat(fd, &after) || name_stat(s->dir_fd, name, &named)) {
        status = io(r, PS5_BT_STORE_REVALIDATE);
        goto done;
    }
    if (!safe_file(&after, s->owner, 32) || !safe_file(&named, s->owner, 32) ||
        !same_file(&before, &after) || !same_file(&before, &named)) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_REVALIDATE, 0);
        goto done;
    }
done:
    if (close_once(&fd, r)) {
        s->poisoned = 1;
        if (!status)
            status = result(r, PS5_BT_STORE_IO, PS5_BT_STORE_CLOSE, r->cleanup_errno);
    }
    if (!status) {
        memcpy(output, bytes, 32);
        *out_len = 32;
    }
    wipe(bytes, sizeof bytes);
    return status ? status : result(r, PS5_BT_STORE_OK, PS5_BT_STORE_READ, 0);
}

static int inspect_existing(struct store *s, const char *name, struct stat *st,
                            int *exists, struct ps5_bt_store_diagnostic *r)
{
    if (name_stat(s->dir_fd, name, st)) {
        if (errno == ENOENT) {
            *exists = 0;
            return 0;
        }
        return io(r, PS5_BT_STORE_METADATA);
    }
    *exists = 1;
    if (!safe_file(st, s->owner, st->st_size))
        return result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_METADATA, 0);
    return 0;
}
static void cleanup_temp(struct store *s, const char *temp, int *fd,
                         const struct stat *expected, int expected_valid,
                         struct ps5_bt_store_diagnostic *r)
{
    struct stat opened, named;
    if (!expected_valid && *fd >= 0 && !file_stat(*fd, &opened)) {
        expected = &opened;
        expected_valid = 1;
    }
    if (close_once(fd, r))
        s->poisoned = 1;
    int named_result = name_stat(s->dir_fd, temp, &named);
    if (named_result && errno == ENOENT)
        return;
    if (named_result || !expected_valid || !same_file(expected, &named)) {
        if (named_result && !r->cleanup_errno)
            r->cleanup_errno = errno;
        r->flags |= PS5_BT_STORE_CLEANUP_UNKNOWN;
        s->poisoned = 1;
        return; /* Never unlink a substituted foreign inode. */
    }
    if (unlinkat(s->dir_fd, temp, 0)) {
        if (!r->cleanup_errno)
            r->cleanup_errno = errno;
        r->flags |= PS5_BT_STORE_CLEANUP_UNKNOWN;
        s->poisoned = 1;
    } else if (sync_fd(s->dir_fd, PS5_BT_STORE_DIRECTORY_SYNC)) {
        if (!r->cleanup_errno)
            r->cleanup_errno = errno;
        r->flags |= PS5_BT_STORE_CLEANUP_UNKNOWN;
        s->poisoned = 1;
    }
}

int32_t ps5_bt_store_save(void *storage, const uint8_t peer[6], const uint8_t record[32],
                          struct ps5_bt_store_diagnostic *r)
{
    if (!r)
        return PS5_BT_STORE_INVALID;
    init_report(r);
    if (!peer || !record || !record_valid(peer, record))
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    struct store *s = storage;
    int status = context(s, r);
    if (status)
        return status;
    char name[NAME_BYTES], temp[NAME_BYTES];
    peer_name(peer, name);
    struct stat original, current, created, named;
    int existed = 0, exists_now = 0;
    status = inspect_existing(s, name, &original, &existed, r);
    if (status)
        return status;
    if (existed && original.st_size != 32)
        return result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_METADATA, 0);
    int fd = -1;
    int created_valid = 0;
    for (unsigned n = 0; n < ATTEMPTS; n++) {
        if (s->serial == UINT64_MAX)
            return result(r, PS5_BT_STORE_LIMIT, PS5_BT_STORE_TEMP, 0);
        uint64_t serial = ++s->serial;
        static const char hex[] = "0123456789abcdef";
        memcpy(temp, ".p5bt-", 6);
        for (unsigned i = 0; i < 16; i++)
            temp[6 + i] = hex[(serial >> ((15 - i) * 4)) & 15];
        memcpy(temp + 22, ".tmp", 5);
        fd = FAULT(PS5_BT_STORE_TEMP) ? -1 : openat(s->dir_fd, temp, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
        if (fd >= 0)
            break;
        if (errno != EEXIST && errno != EINTR)
            return io(r, PS5_BT_STORE_TEMP);
    }
    if (fd < 0)
        return result(r, PS5_BT_STORE_LIMIT, PS5_BT_STORE_TEMP, errno);
    if (file_stat(fd, &created)) {
        status = io(r, PS5_BT_STORE_TEMP);
        goto abort;
    }
    created_valid = 1;
    if (fchmod(fd, 0600) || file_stat(fd, &created)) {
        status = io(r, PS5_BT_STORE_TEMP);
        goto abort;
    }
    if (!safe_file(&created, s->owner, 0)) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_TEMP, 0);
        goto abort;
    }
    size_t count = 0;
    for (unsigned n = 0; n < ATTEMPTS && count < 32; n++) {
        ssize_t put = FAULT(PS5_BT_STORE_WRITE) ? -1 : STORE_WRITE(fd, record + count, 32 - count);
        if (put > 0)
            count += (size_t)put;
        else if (put == 0) {
            status = result(r, PS5_BT_STORE_IO, PS5_BT_STORE_WRITE, 0);
            goto abort;
        } else if (errno != EINTR) {
            status = io(r, PS5_BT_STORE_WRITE);
            goto abort;
        }
    }
    if (count != 32) {
        status = result(r, PS5_BT_STORE_LIMIT, PS5_BT_STORE_WRITE, EINTR);
        goto abort;
    }
    if (sync_fd(fd, PS5_BT_STORE_FILE_SYNC)) {
        status = io(r, PS5_BT_STORE_FILE_SYNC);
        goto abort;
    }
    AFTER(PS5_BT_STORE_FILE_SYNC, s->dir_fd);
    status = context(s, r);
    if (status)
        goto abort;
    if (file_stat(fd, &current) || name_stat(s->dir_fd, temp, &named)) {
        status = io(r, PS5_BT_STORE_REVALIDATE);
        goto abort;
    }
    if (!safe_file(&current, s->owner, 32) || !safe_file(&named, s->owner, 32) ||
        !same_file(&created, &current) || !same_file(&created, &named)) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_REVALIDATE, 0);
        goto abort;
    }
    status = inspect_existing(s, name, &current, &exists_now, r);
    if (status)
        goto abort;
    if (exists_now != existed || (existed && (current.st_size != 32 || !same_file(&current, &original)))) {
        status = result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_REVALIDATE, 0);
        goto abort;
    }
    if (close_once(&fd, r)) {
        s->poisoned = 1;
        status = result(r, PS5_BT_STORE_IO, PS5_BT_STORE_CLOSE, r->cleanup_errno);
        goto abort;
    }
    r->flags |= PS5_BT_STORE_COMMIT_ATTEMPTED;
    if (FAULT(PS5_BT_STORE_RENAME) || renameat(s->dir_fd, temp, s->dir_fd, name)) {
        status = result(r, PS5_BT_STORE_DURABILITY_UNKNOWN, PS5_BT_STORE_RENAME, errno);
        goto abort;
    }
    r->flags |= PS5_BT_STORE_COMMITTED;
    AFTER(PS5_BT_STORE_RENAME, s->dir_fd);
    if (sync_fd(s->dir_fd, PS5_BT_STORE_DIRECTORY_SYNC))
        return result(r, PS5_BT_STORE_DURABILITY_UNKNOWN, PS5_BT_STORE_DIRECTORY_SYNC, errno);
    return result(r, PS5_BT_STORE_OK, PS5_BT_STORE_DIRECTORY_SYNC, 0);
abort:
    cleanup_temp(s, temp, &fd, &created, created_valid, r);
    return status;
}

int32_t ps5_bt_store_delete(void *storage, const uint8_t peer[6],
                            struct ps5_bt_store_diagnostic *r)
{
    if (!r)
        return PS5_BT_STORE_INVALID;
    init_report(r);
    if (!peer)
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    struct store *s = storage;
    int status = context(s, r);
    if (status)
        return status;
    char name[NAME_BYTES];
    peer_name(peer, name);
    struct stat before, after;
    int exists = 0, exists_now = 0;
    status = inspect_existing(s, name, &before, &exists, r);
    if (status)
        return status;
    AFTER(PS5_BT_STORE_METADATA, s->dir_fd);
    status = context(s, r);
    if (status)
        return status;
    status = inspect_existing(s, name, &after, &exists_now, r);
    if (status)
        return status;
    if (exists_now != exists || (exists && !same_file(&before, &after)))
        return result(r, PS5_BT_STORE_SECURITY, PS5_BT_STORE_REVALIDATE, 0);
    if (exists) {
        r->flags |= PS5_BT_STORE_COMMIT_ATTEMPTED;
        if (FAULT(PS5_BT_STORE_UNLINK) || unlinkat(s->dir_fd, name, 0))
            return result(r, PS5_BT_STORE_DURABILITY_UNKNOWN, PS5_BT_STORE_UNLINK, errno);
        r->flags |= PS5_BT_STORE_COMMITTED;
    }
    AFTER(PS5_BT_STORE_UNLINK, s->dir_fd);
    if (sync_fd(s->dir_fd, PS5_BT_STORE_DIRECTORY_SYNC))
        return result(r, PS5_BT_STORE_DURABILITY_UNKNOWN, PS5_BT_STORE_DIRECTORY_SYNC, errno);
    return result(r, PS5_BT_STORE_OK, PS5_BT_STORE_DIRECTORY_SYNC, 0);
}

int32_t ps5_bt_store_close(void *storage, struct ps5_bt_store_diagnostic *r)
{
    if (!r)
        return PS5_BT_STORE_INVALID;
    init_report(r);
    struct store *s = storage;
    if (!s || s->magic != MAGIC)
        return result(r, PS5_BT_STORE_INVALID, PS5_BT_STORE_VALIDATE, 0);
    s->magic = 0;
    int first = close_once(&s->lock_fd, r);
    int second = close_once(&s->dir_fd, r);
    if (first || second)
        return result(r, PS5_BT_STORE_IO, PS5_BT_STORE_CLOSE, r->cleanup_errno);
    wipe(s, sizeof *s);
    return result(r, PS5_BT_STORE_OK, PS5_BT_STORE_CLOSE, 0);
}
