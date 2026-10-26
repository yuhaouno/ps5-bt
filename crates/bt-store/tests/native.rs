// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "native-posix")]
use bt_device::{BOND_RECORD_BYTES, BondPolicy, BondRecord, BondStore, KeyType};
use bt_link::{Peer, SecretKey};
use bt_store::{FileStore, StoreStorage};
use std::{
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{PermissionsExt, symlink},
    },
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "ps5-bt-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }
    fn path(&self) -> &[u8] {
        self.0.as_os_str().as_bytes()
    }
    fn bond(&self) -> PathBuf {
        self.0.join("p5bd-010203040506.bond")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn peer() -> Peer {
    Peer::from_hci_bytes([1, 2, 3, 4, 5, 6])
}
fn record(value: u8) -> [u8; BOND_RECORD_BYTES] {
    let mut bytes = [0; BOND_RECORD_BYTES];
    BondRecord::new(
        peer(),
        SecretKey::new([value; 16]),
        KeyType::UnauthenticatedP192,
        16,
    )
    .unwrap()
    .encode(&mut bytes)
    .unwrap();
    bytes
}

#[test]
fn atomic_store_roundtrip_reopen_replace_and_idempotent_durable_delete() {
    let fixture = Fixture::new();
    let mut storage = StoreStorage::<1024>::new();
    let mut store = FileStore::open(&mut storage, fixture.path()).unwrap();
    let mut out = [0xa5; 32];
    assert_eq!(store.load(peer(), &mut out), Ok(None));
    assert_eq!(out, [0; 32]);
    store.save_atomic(peer(), &record(0x44)).unwrap();
    assert_eq!(
        fs::metadata(fixture.bond()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(store.load(peer(), &mut out), Ok(Some(32)));
    assert_eq!(out, record(0x44));
    store.save_atomic(peer(), &record(0x77)).unwrap();
    store.close().unwrap();
    let mut store = FileStore::open(&mut storage, fixture.path()).unwrap();
    assert_eq!(store.load(peer(), &mut out), Ok(Some(32)));
    assert_eq!(out, record(0x77));
    assert!(
        BondRecord::decode(
            &out,
            peer(),
            BondPolicy {
                minimum_key_size: 16,
                require_mitm: false,
                require_secure_connections: false
            }
        )
        .is_ok()
    );
    store.delete_atomic(peer()).unwrap();
    store.delete_atomic(peer()).unwrap();
    assert_eq!(store.load(peer(), &mut out), Ok(None));
    assert!(!fixture.bond().exists());
    store.close().unwrap();
    let names: Vec<_> = fs::read_dir(&fixture.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [".p5bt.lock"]);
}

#[test]
fn exclusive_store_owner_rejects_second_context_until_exactly_once_drop() {
    let fixture = Fixture::new();
    let mut first = StoreStorage::<1024>::new();
    let mut second = StoreStorage::<1024>::new();
    let store = FileStore::open(&mut first, fixture.path()).unwrap();
    let failure = match FileStore::open(&mut second, fixture.path()) {
        Err(error) => error,
        Ok(_) => panic!("second owner"),
    };
    assert_eq!(failure.stage, 3);
    assert_eq!(failure.status, -5);
    drop(store);
    FileStore::open(&mut second, fixture.path())
        .unwrap()
        .close()
        .unwrap();
}

#[test]
fn wrong_modes_symlinks_hardlinks_and_invalid_records_are_not_loaded_or_deleted() {
    let fixture = Fixture::new();
    let mut storage = StoreStorage::<1024>::new();
    let mut store = FileStore::open(&mut storage, fixture.path()).unwrap();
    store.save_atomic(peer(), &record(0x44)).unwrap();
    fs::set_permissions(fixture.bond(), fs::Permissions::from_mode(0o644)).unwrap();
    let mut out = [0xa5; 32];
    assert!(store.load(peer(), &mut out).is_err());
    assert_eq!(out, [0; 32]);
    assert!(store.delete_atomic(peer()).is_err());
    fs::set_permissions(fixture.bond(), fs::Permissions::from_mode(0o600)).unwrap();
    let extra = fixture.0.join("extra");
    fs::hard_link(fixture.bond(), &extra).unwrap();
    assert!(store.load(peer(), &mut out).is_err());
    assert!(store.save_atomic(peer(), &record(0x77)).is_err());
    assert!(store.delete_atomic(peer()).is_err());
    fs::remove_file(&extra).unwrap();
    fs::rename(fixture.bond(), &extra).unwrap();
    symlink(&extra, fixture.bond()).unwrap();
    assert!(store.load(peer(), &mut out).is_err());
    assert!(store.delete_atomic(peer()).is_err());
    assert_eq!(fs::read(&extra).unwrap(), record(0x44));
    fs::remove_file(fixture.bond()).unwrap();
    fs::rename(extra, fixture.bond()).unwrap();
    let mut invalid = record(0x44);
    invalid[4] = 2;
    assert!(store.save_atomic(peer(), &invalid).is_err());
    fs::write(fixture.bond(), [0; 33]).unwrap();
    assert!(store.load(peer(), &mut out).is_err());
    assert_eq!(out, [0; 32]);
    // Explicit forget must remove a protected but corrupt record; load/save
    // rejection must not make that device impossible to forget.
    store.delete_atomic(peer()).unwrap();
    assert!(!fixture.bond().exists());
}

#[test]
fn unsafe_directory_paths_modes_and_changed_protection_fail_closed() {
    let fixture = Fixture::new();
    let mut storage = StoreStorage::<1024>::new();
    for path in [
        b"relative".as_slice(),
        b"/tmp/../tmp",
        b"/tmp//x",
        b"/tmp/\0x",
        b"/",
    ] {
        assert!(FileStore::open(&mut storage, path).is_err());
    }
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(FileStore::open(&mut storage, fixture.path()).is_err());
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700)).unwrap();
    let alias = fixture.0.with_extension("symlink");
    symlink(&fixture.0, &alias).unwrap();
    assert!(FileStore::open(&mut storage, alias.as_os_str().as_bytes()).is_err());
    fs::remove_file(alias).unwrap();
    let mut store = FileStore::open(&mut storage, fixture.path()).unwrap();
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(store.save_atomic(peer(), &record(0x44)).is_err());
    assert!(!fixture.bond().exists());
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn insufficient_opaque_storage_cannot_open_files() {
    let fixture = Fixture::new();
    let mut small = StoreStorage::<1>::new();
    assert!(FileStore::open(&mut small, fixture.path()).is_err());
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
}
