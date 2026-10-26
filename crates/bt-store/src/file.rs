// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ABI_VERSION, Diagnostic, MAX_PATH_BYTES};
use bt_device::{BOND_RECORD_BYTES, BondStore, StoreError};
use bt_link::Peer;
use core::{ffi::c_void, marker::PhantomData, mem::align_of, ptr::NonNull};

unsafe extern "C" {
    fn ps5_bt_store_abi_version() -> u32;
    fn ps5_bt_store_storage_size() -> u32;
    fn ps5_bt_store_storage_alignment() -> u32;
    fn ps5_bt_store_open(
        storage: *mut c_void,
        capacity: u32,
        path: *const u8,
        length: u32,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_store_load(
        storage: *mut c_void,
        peer: *const u8,
        output: *mut u8,
        out_len: *mut u32,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_store_save(
        storage: *mut c_void,
        peer: *const u8,
        record: *const u8,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_store_delete(storage: *mut c_void, peer: *const u8, report: *mut Diagnostic) -> i32;
    fn ps5_bt_store_close(storage: *mut c_void, report: *mut Diagnostic) -> i32;
}

const CLEANUP_UNKNOWN: u32 = 4;
fn sentinel() -> Diagnostic {
    Diagnostic {
        abi_version: 0,
        stage: 0,
        status: i32::MIN,
        raw_errno: 0,
        flags: 0,
        cleanup_errno: 0,
    }
}
fn valid_report(report: Diagnostic, returned: i32) -> bool {
    report.abi_version == ABI_VERSION
        && report.status == returned
        && (1..=13).contains(&report.stage)
        && report.flags & !7 == 0
        && (!(0..=1).contains(&returned)
            || (report.raw_errno == 0
                && report.cleanup_errno == 0
                && report.flags & CLEANUP_UNKNOWN == 0))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseFailure {
    pub diagnostic: Diagnostic,
    pub returned_status: i32,
}

/// Stable caller-owned storage; C private struct layout is queried, never copied.
#[repr(C, align(64))]
pub struct StoreStorage<const N: usize> {
    bytes: [u8; N],
}
impl<const N: usize> StoreStorage<N> {
    pub const fn new() -> Self {
        Self { bytes: [0; N] }
    }
}
impl<const N: usize> Default for StoreStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Native owner. The feature only declares the C ABI: `external-posix` users
/// must link the separately reviewed matching implementation. `native-posix`
/// compiles the concrete host implementation against its own system headers.
/// No pointers to caller records/paths are retained. The storage borrow pins C
/// state until close, and the marker prevents Send/Sync/reentrant ownership.
///
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<bt_store::FileStore<'static, 1024>>();
/// ```
pub struct FileStore<'storage, const N: usize> {
    storage: NonNull<c_void>,
    _storage: PhantomData<&'storage mut StoreStorage<N>>,
    _single_owner: PhantomData<*mut ()>,
    active: bool,
    diagnostic: Diagnostic,
    returned_status: i32,
    poisoned: bool,
}
impl<'storage, const N: usize> FileStore<'storage, N> {
    pub fn open(
        storage: &'storage mut StoreStorage<N>,
        directory: &[u8],
    ) -> Result<Self, Diagnostic> {
        let mut diagnostic = sentinel();
        // SAFETY: argument-free integer ABI queries retain no caller pointers.
        let (version, required, alignment) = unsafe {
            (
                ps5_bt_store_abi_version(),
                ps5_bt_store_storage_size() as usize,
                ps5_bt_store_storage_alignment() as usize,
            )
        };
        if version != ABI_VERSION
            || required == 0
            || required > N
            || alignment == 0
            || !alignment.is_power_of_two()
            || alignment > align_of::<StoreStorage<N>>()
            || directory.is_empty()
            || directory.len() > MAX_PATH_BYTES
        {
            return Err(diagnostic);
        }
        let pointer = NonNull::from(&mut storage.bytes).cast::<c_void>();
        if pointer.as_ptr() as usize & (alignment - 1) != 0 {
            return Err(diagnostic);
        }
        let capacity = u32::try_from(required).map_err(|_| diagnostic)?;
        // SAFETY: storage is aligned, zero-initialized, large enough and exclusively
        // borrowed until close. Path/report pointers are valid only for this call.
        let status = unsafe {
            ps5_bt_store_open(
                pointer.as_ptr(),
                capacity,
                directory.as_ptr(),
                directory.len() as u32,
                &mut diagnostic,
            )
        };
        if status != 0 || !valid_report(diagnostic, status) {
            // A zero return establishes an acquired C context even if its report
            // is malformed. Consume that ownership before releasing Rust storage.
            if status == 0 {
                let mut cleanup = sentinel();
                // SAFETY: the version-checked open returned success, so this is
                // its unique matching cleanup call; no acquired context is guessed.
                let closed = unsafe { ps5_bt_store_close(pointer.as_ptr(), &mut cleanup) };
                if closed != 0 || !valid_report(cleanup, closed) {
                    diagnostic.flags |= CLEANUP_UNKNOWN | cleanup.flags;
                    if diagnostic.cleanup_errno == 0 {
                        diagnostic.cleanup_errno = if cleanup.raw_errno != 0 {
                            cleanup.raw_errno
                        } else {
                            cleanup.cleanup_errno
                        };
                    }
                }
            }
            return Err(diagnostic);
        }
        Ok(Self {
            storage: pointer,
            _storage: PhantomData,
            _single_owner: PhantomData,
            active: true,
            diagnostic,
            returned_status: status,
            poisoned: false,
        })
    }
    pub const fn last_diagnostic(&self) -> Diagnostic {
        self.diagnostic
    }
    pub const fn last_returned_status(&self) -> i32 {
        self.returned_status
    }
    fn begin_call(&mut self) -> Result<(), StoreError> {
        if self.poisoned {
            return Err(StoreError::Unavailable);
        }
        self.diagnostic = sentinel();
        Ok(())
    }
    fn check(&mut self, status: i32, mutation: bool) -> Result<(), StoreError> {
        self.returned_status = status;
        if !valid_report(self.diagnostic, status) {
            self.poisoned = true;
            self.diagnostic.flags |= CLEANUP_UNKNOWN;
            return Err(if mutation {
                StoreError::DurabilityUnknown
            } else {
                StoreError::Unavailable
            });
        }
        if status == 0 {
            Ok(())
        } else if status == -3 {
            Err(StoreError::DurabilityUnknown)
        } else {
            Err(StoreError::Backend(if self.diagnostic.raw_errno != 0 {
                self.diagnostic.raw_errno
            } else {
                status
            }))
        }
    }
    pub fn close(mut self) -> Result<Diagnostic, CloseFailure> {
        if self.close_inner() {
            Ok(self.diagnostic)
        } else {
            Err(CloseFailure {
                diagnostic: self.diagnostic,
                returned_status: self.returned_status,
            })
        }
    }
    fn close_inner(&mut self) -> bool {
        if !self.active {
            return true;
        }
        self.active = false;
        self.diagnostic = sentinel();
        // SAFETY: consumes the unique live context exactly once, even when its
        // report or return disagrees. Never retry an interrupted/dirty close.
        let returned = unsafe { ps5_bt_store_close(self.storage.as_ptr(), &mut self.diagnostic) };
        self.returned_status = returned;
        if !valid_report(self.diagnostic, returned) {
            self.diagnostic.flags |= CLEANUP_UNKNOWN;
            return false;
        }
        returned == 0
    }
}
impl<const N: usize> BondStore for FileStore<'_, N> {
    fn load(
        &mut self,
        peer: Peer,
        output: &mut [u8; BOND_RECORD_BYTES],
    ) -> Result<Option<usize>, StoreError> {
        output.fill(0);
        self.begin_call()?;
        let mut length = 0;
        let identity = peer.hci_bytes();
        // SAFETY: fixed readable six-byte identity, writable32-byte output and
        // count/report; C retains none and this method serializes the context.
        let status = unsafe {
            ps5_bt_store_load(
                self.storage.as_ptr(),
                identity.as_ptr(),
                output.as_mut_ptr(),
                &mut length,
                &mut self.diagnostic,
            )
        };
        if status == 1 && valid_report(self.diagnostic, status) && length == 0 {
            self.returned_status = status;
            output.fill(0);
            return Ok(None);
        }
        if let Err(error) = self.check(status, false) {
            output.fill(0);
            return Err(error);
        }
        if length as usize != BOND_RECORD_BYTES {
            self.poisoned = true;
            self.diagnostic.flags |= CLEANUP_UNKNOWN;
            output.fill(0);
            return Err(StoreError::Unavailable);
        }
        Ok(Some(BOND_RECORD_BYTES))
    }
    fn save_atomic(
        &mut self,
        peer: Peer,
        record: &[u8; BOND_RECORD_BYTES],
    ) -> Result<(), StoreError> {
        self.begin_call()?;
        let identity = peer.hci_bytes();
        // SAFETY: exact fixed readable identity/record and exclusive live context;
        // C performs synchronous bounded work and retains no data pointer.
        let status = unsafe {
            ps5_bt_store_save(
                self.storage.as_ptr(),
                identity.as_ptr(),
                record.as_ptr(),
                &mut self.diagnostic,
            )
        };
        self.check(status, true)
    }
    fn delete_atomic(&mut self, peer: Peer) -> Result<(), StoreError> {
        self.begin_call()?;
        let identity = peer.hci_bytes();
        // SAFETY: exact fixed readable identity/report and exclusive live context.
        let status = unsafe {
            ps5_bt_store_delete(
                self.storage.as_ptr(),
                identity.as_ptr(),
                &mut self.diagnostic,
            )
        };
        self.check(status, true)
    }
}
impl<const N: usize> Drop for FileStore<'_, N> {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}
