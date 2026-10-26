/* SPDX-License-Identifier: GPL-3.0-or-later */
#define PS5_BT_STORE_TEST
#include "../../platform/store/bond_file_store.c"
#include <assert.h>
#include <dirent.h>
#include <stdlib.h>
#include <sys/wait.h>

static unsigned fault_stage, fault_left, short_io, read_eintr, write_eintr;
static unsigned after_stage, after_action, close_interrupt, close_calls;
static unsigned read_calls, write_calls;
static const uint8_t peer[6] = {1, 2, 3, 4, 5, 6};
static const char bond_name[] = "p5bd-010203040506.bond";
static uint8_t old_record[32], new_record[32];
static int fault_errno = EIO;

int ps5_bt_store_test_fault(uint32_t stage)
{
    if (stage == fault_stage && fault_left) {
        fault_left--;
        return fault_errno;
    }
    return 0;
}
ssize_t ps5_bt_store_test_read(int fd, void *out, size_t size)
{
    read_calls++;
    if (read_eintr) { read_eintr--; errno = EINTR; return -1; }
    if (short_io && size > short_io) size = short_io;
    return read(fd, out, size);
}
ssize_t ps5_bt_store_test_write(int fd, const void *out, size_t size)
{
    write_calls++;
    if (write_eintr) { write_eintr--; errno = EINTR; return -1; }
    if (short_io && size > short_io) size = short_io;
    return write(fd, out, size);
}
int ps5_bt_store_test_close(int fd)
{
    close_calls++;
    int rc = close(fd);
    if (close_interrupt) { close_interrupt--; errno = EINTR; return -1; }
    return rc;
}
static void replace_name(int dir, const char *name)
{
    assert(renameat(dir, name, dir, ".old") == 0);
    int fd = openat(dir, name, O_WRONLY | O_CREAT | O_EXCL, 0600);
    assert(fd >= 0);
    assert(write(fd, new_record, 32) == 32);
    assert(close(fd) == 0);
}
void ps5_bt_store_test_after(uint32_t stage, int dir)
{
    if (stage != after_stage) return;
    after_stage = 0;
    if (after_action == 1) _exit(97);
    if (after_action == 2) replace_name(dir, bond_name);
    if (after_action == 3) assert(fchmod(dir, 0755) == 0);
    if (after_action == 4) {
        int copy = dup(dir);
        assert(copy >= 0);
        DIR *stream = fdopendir(copy);
        assert(stream);
        struct dirent *entry;
        int changed = 0;
        while ((entry = readdir(stream))) {
            if (!strncmp(entry->d_name, ".p5bt-", 6)) {
                assert(unlinkat(dir, entry->d_name, 0) == 0);
                assert(symlinkat("witness", dir, entry->d_name) == 0);
                changed = 1;
                break;
            }
        }
        assert(closedir(stream) == 0 && changed);
    }
}
static void reset_hooks(void)
{
    fault_stage = fault_left = short_io = read_eintr = write_eintr = 0;
    after_stage = after_action = close_interrupt = close_calls = 0;
    read_calls = write_calls = 0;
    fault_errno = EIO;
}
union storage { max_align_t align; uint8_t bytes[1024]; };
struct fixture { char path[PS5_BT_STORE_PATH_MAX + 1]; union storage storage; struct ps5_bt_store_diagnostic r; };
static void make_fixture(struct fixture *f)
{
    memset(f, 0, sizeof *f);
    char root[PS5_BT_STORE_PATH_MAX + 1];
    assert(realpath("/tmp", root));
    int n = snprintf(f->path, sizeof f->path, "%s/ps5-bt-store-c-XXXXXX", root);
    assert(n > 0 && (size_t)n < sizeof f->path);
    assert(mkdtemp(f->path));
    assert(chmod(f->path, 0700) == 0);
    reset_hooks();
    assert(ps5_bt_store_open(f->storage.bytes, sizeof f->storage.bytes, (uint8_t *)f->path, (uint32_t)strlen(f->path), &f->r) == 0);
}
static void reopen(struct fixture *f)
{
    reset_hooks();
    memset(&f->storage, 0, sizeof f->storage);
    assert(ps5_bt_store_open(f->storage.bytes, sizeof f->storage.bytes, (uint8_t *)f->path, (uint32_t)strlen(f->path), &f->r) == 0);
}
static void remove_fixture(struct fixture *f)
{
    reset_hooks();
    if (((struct store *)f->storage.bytes)->magic == MAGIC)
        assert(ps5_bt_store_close(f->storage.bytes, &f->r) == 0);
    assert(chmod(f->path, 0700) == 0);
    DIR *directory = opendir(f->path);
    assert(directory);
    struct dirent *entry;
    int fd = dirfd(directory);
    while ((entry = readdir(directory))) {
        if (strcmp(entry->d_name, ".") && strcmp(entry->d_name, ".."))
            assert(unlinkat(fd, entry->d_name, 0) == 0);
    }
    assert(closedir(directory) == 0);
    assert(rmdir(f->path) == 0);
}
static void expect_loaded(struct fixture *f, const uint8_t expected[32])
{
    uint8_t out[32]; uint32_t length = 9;
    assert(ps5_bt_store_load(f->storage.bytes, peer, out, &length, &f->r) == 0);
    assert(length == 32 && !memcmp(out, expected, 32));
}
static void save(struct fixture *f, const uint8_t record[32])
{
    assert(ps5_bt_store_save(f->storage.bytes, peer, record, &f->r) == 0);
}
static void initialize_records(void)
{
    memcpy(old_record, "P5BD\x01\x03\x04\x10", 8);
    memcpy(old_record + 8, peer, 6);
    memset(old_record + 16, 0x44, 16);
    memcpy(new_record, old_record, 32);
    memset(new_record + 16, 0x77, 16);
}
static void test_short_io_and_interrupts(void)
{
    struct fixture f; make_fixture(&f);
    short_io = 1; write_eintr = 3;
    save(&f, old_record);
    assert(write_calls == 35);
    read_eintr = 2;
    expect_loaded(&f, old_record);
    assert(read_calls == 35); /* 32 one-byte reads, EOF, two interruptions. */
    write_eintr = 1000; write_calls = 0;
    assert(ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r) == PS5_BT_STORE_LIMIT);
    assert(write_calls == ATTEMPTS && f.r.raw_errno == EINTR);
    reset_hooks(); expect_loaded(&f, old_record);
    read_eintr = 1000; read_calls = 0;
    uint8_t out[32]; uint32_t length;
    assert(ps5_bt_store_load(f.storage.bytes, peer, out, &length, &f.r) == PS5_BT_STORE_LIMIT);
    assert(read_calls == ATTEMPTS && f.r.raw_errno == EINTR && length == 0);
    remove_fixture(&f);
}
static void test_precommit_errors_preserve_old_record(void)
{
    for (unsigned stage = PS5_BT_STORE_TEMP; stage <= PS5_BT_STORE_FILE_SYNC; stage++) {
        struct fixture f; make_fixture(&f); save(&f, old_record);
        fault_stage = stage; fault_left = 1;
        int rc = ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r);
        assert(rc == PS5_BT_STORE_IO && f.r.raw_errno == EIO && f.r.stage == stage);
        assert(!(f.r.flags & PS5_BT_STORE_COMMIT_ATTEMPTED));
        reset_hooks(); expect_loaded(&f, old_record);
        remove_fixture(&f);
    }
}
static void test_commit_boundary_errors_are_unknown(void)
{
    struct fixture f; make_fixture(&f); save(&f, old_record);
    fault_stage = PS5_BT_STORE_RENAME; fault_left = 1;
    assert(ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r) == PS5_BT_STORE_DURABILITY_UNKNOWN);
    assert(f.r.raw_errno == EIO && (f.r.flags & PS5_BT_STORE_COMMIT_ATTEMPTED));
    reset_hooks(); expect_loaded(&f, old_record);
    fault_stage = PS5_BT_STORE_DIRECTORY_SYNC; fault_left = 1;
    assert(ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r) == PS5_BT_STORE_DURABILITY_UNKNOWN);
    assert(f.r.raw_errno == EIO && (f.r.flags & PS5_BT_STORE_COMMITTED));
    reset_hooks(); expect_loaded(&f, new_record);
    fault_stage = PS5_BT_STORE_DIRECTORY_SYNC; fault_left = 1;
    assert(ps5_bt_store_delete(f.storage.bytes, peer, &f.r) == PS5_BT_STORE_DURABILITY_UNKNOWN);
    assert(f.r.raw_errno == EIO && (f.r.flags & PS5_BT_STORE_COMMITTED));
    reset_hooks();
    assert(ps5_bt_store_delete(f.storage.bytes, peer, &f.r) == 0);
    remove_fixture(&f);
}
static void test_replacement_races_and_protection_change(void)
{
    for (unsigned stage = 0; stage < 3; stage++) {
        struct fixture f; make_fixture(&f); save(&f, old_record);
        after_stage = stage == 0 ? PS5_BT_STORE_READ : stage == 1 ? PS5_BT_STORE_FILE_SYNC : PS5_BT_STORE_METADATA;
        after_action = 2;
        int rc;
        if (stage == 0) {
            uint8_t out[32]; memset(out, 0xa5, 32); uint32_t size = 9;
            rc = ps5_bt_store_load(f.storage.bytes, peer, out, &size, &f.r);
            uint8_t zero[32] = {0};
            assert(!memcmp(out, zero, 32) && size == 0);
        } else if (stage == 1) rc = ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r);
        else rc = ps5_bt_store_delete(f.storage.bytes, peer, &f.r);
        assert(rc == PS5_BT_STORE_SECURITY);
        reset_hooks(); expect_loaded(&f, new_record);
        remove_fixture(&f);
    }
    struct fixture f; make_fixture(&f); save(&f, old_record);
    after_stage = PS5_BT_STORE_FILE_SYNC; after_action = 3;
    assert(ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r) == PS5_BT_STORE_SECURITY);
    assert(fchmod(((struct store *)f.storage.bytes)->dir_fd, 0700) == 0);
    reset_hooks(); expect_loaded(&f, old_record);
    remove_fixture(&f);
}
static void test_substituted_temp_is_never_deleted_as_owned(void)
{
    struct fixture f; make_fixture(&f); save(&f, old_record);
    int dir = ((struct store *)f.storage.bytes)->dir_fd;
    int witness = openat(dir, "witness", O_CREAT | O_EXCL | O_WRONLY, 0600);
    assert(witness >= 0 && write(witness, "safe", 4) == 4 && close(witness) == 0);
    after_stage = PS5_BT_STORE_FILE_SYNC; after_action = 4;
    assert(ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r) == PS5_BT_STORE_SECURITY);
    assert(f.r.flags & PS5_BT_STORE_CLEANUP_UNKNOWN);
    assert(((struct store *)f.storage.bytes)->poisoned);
    struct stat st;
    assert(fstatat(dir, "witness", &st, AT_SYMLINK_NOFOLLOW) == 0 && st.st_size == 4);
    remove_fixture(&f);
}
static void test_interrupted_close_is_once_and_poison_is_explicit(void)
{
    struct fixture f; make_fixture(&f); save(&f, old_record);
    reset_hooks(); close_interrupt = 1;
    assert(ps5_bt_store_close(f.storage.bytes, &f.r) == PS5_BT_STORE_IO);
    assert(close_calls == 2 && f.r.raw_errno == EINTR && (f.r.flags & PS5_BT_STORE_CLEANUP_UNKNOWN));
    assert(ps5_bt_store_close(f.storage.bytes, &f.r) == PS5_BT_STORE_INVALID);
    assert(close_calls == 2);
    remove_fixture(&f);
}
static void test_process_crash_checkpoints_keep_whole_records(void)
{
    for (unsigned stage = PS5_BT_STORE_FILE_SYNC; stage <= PS5_BT_STORE_UNLINK; stage++) {
        if (stage != PS5_BT_STORE_FILE_SYNC && stage != PS5_BT_STORE_RENAME && stage != PS5_BT_STORE_UNLINK) continue;
        struct fixture f; make_fixture(&f); save(&f, old_record);
        assert(ps5_bt_store_close(f.storage.bytes, &f.r) == 0);
        pid_t child = fork(); assert(child >= 0);
        if (!child) {
            reopen(&f);
            after_stage = stage; after_action = 1;
            if (stage == PS5_BT_STORE_UNLINK) ps5_bt_store_delete(f.storage.bytes, peer, &f.r);
            else ps5_bt_store_save(f.storage.bytes, peer, new_record, &f.r);
            _exit(98);
        }
        int status; assert(waitpid(child, &status, 0) == child);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 97);
        reopen(&f);
        if (stage == PS5_BT_STORE_UNLINK) {
            uint8_t out[32]; uint32_t length;
            assert(ps5_bt_store_load(f.storage.bytes, peer, out, &length, &f.r) == PS5_BT_STORE_ABSENT);
        } else expect_loaded(&f, stage == PS5_BT_STORE_FILE_SYNC ? old_record : new_record);
        remove_fixture(&f);
    }
}
static void test_metadata_validation_rejects_wrong_owner_links_and_types(void)
{
    struct stat st; memset(&st, 0, sizeof st);
    st.st_mode = S_IFREG | 0600; st.st_uid = geteuid(); st.st_nlink = 1; st.st_size = 32;
    assert(safe_file(&st, geteuid(), 32));
    st.st_uid = geteuid() == 0 ? 1 : 0; assert(!safe_file(&st, geteuid(), 32));
    st.st_uid = geteuid(); st.st_nlink = 2; assert(!safe_file(&st, geteuid(), 32));
    st.st_nlink = 1; st.st_mode = S_IFIFO | 0600; assert(!safe_file(&st, geteuid(), 32));
}
int main(void)
{
    initialize_records();
    test_short_io_and_interrupts();
    test_precommit_errors_preserve_old_record();
    test_commit_boundary_errors_are_unknown();
    test_replacement_races_and_protection_change();
    test_substituted_temp_is_never_deleted_as_owned();
    test_interrupted_close_is_once_and_poison_is_explicit();
    test_process_crash_checkpoints_keep_whole_records();
    test_metadata_validation_rejects_wrong_owner_links_and_types();
    puts("PASS protected file store: bounded I/O, commit/cleanup faults, substitution and process-crash checkpoints");
    return 0;
}
