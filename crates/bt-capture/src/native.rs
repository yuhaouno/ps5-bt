// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ABI_VERSION, Backend, Diagnostic, MAX_READ_BYTES, ReadKind, ReadOutcome};
use core::{
    ffi::{c_char, c_void},
    marker::PhantomData,
    mem::{align_of, size_of},
    ptr::NonNull,
};
const ABI_ERROR: i32 = -1000;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResolverOps {
    pub abi_version: u32,
    pub struct_bytes: u32,
    pub lookup_current: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> i64,
    pub lookup_module: unsafe extern "C" fn(i32, *const c_char, *mut *mut c_void) -> i64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeRead {
    abi_version: u32,
    kind: u32,
    raw_status: i32,
    bytes: u32,
    flags: u32,
    metadata: [u8; 16],
}
unsafe extern "C" {
    fn ps5_capture_stream_abi_version() -> u32;
    fn ps5_capture_stream_storage_size() -> u32;
    fn ps5_capture_stream_storage_alignment() -> u32;
    fn ps5_capture_stream_open(
        storage: *mut c_void,
        capacity: u32,
        ops: *const ResolverOps,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_capture_stream_start(storage: *mut c_void, report: *mut Diagnostic) -> i32;
    fn ps5_capture_stream_read_once(
        storage: *mut c_void,
        output: *mut u8,
        capacity: u32,
        outcome: *mut NativeRead,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_capture_stream_stop(storage: *mut c_void, report: *mut Diagnostic) -> i32;
    fn ps5_capture_stream_close(storage: *mut c_void, report: *mut Diagnostic) -> i32;
}
#[repr(C, align(16))]
pub struct CaptureStorage<const N: usize = 9216> {
    bytes: [u8; N],
}
impl<const N: usize> CaptureStorage<N> {
    pub const fn new() -> Self {
        Self { bytes: [0; N] }
    }
}
impl<const N: usize> Default for CaptureStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}
/// Exclusive experimental native owner; use through `CaptureOwner` for budgets.
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<bt_capture::Avcap2Backend<'static, 9216>>();
/// ```
pub struct Avcap2Backend<'storage, const N: usize = 9216> {
    pointer: NonNull<c_void>,
    ops: ResolverOps,
    opened: bool,
    poisoned: bool,
    diagnostic: Diagnostic,
    _storage: PhantomData<&'storage mut CaptureStorage<N>>,
    _single_owner: PhantomData<*mut ()>,
}
fn invalid() -> Diagnostic {
    Diagnostic {
        abi_version: ABI_VERSION,
        stage: 1,
        status: -2,
        ..Diagnostic::default()
    }
}
fn checked(status: i32, mut diagnostic: Diagnostic, stage: u32) -> Result<Diagnostic, Diagnostic> {
    if diagnostic.abi_version != ABI_VERSION
        || diagnostic.stage != stage
        || diagnostic.status != status
        || diagnostic.cleanup_flags & !7 != 0
    {
        diagnostic.status = ABI_ERROR;
        return Err(diagnostic);
    }
    if status == 0 {
        Ok(diagnostic)
    } else {
        Err(diagnostic)
    }
}
impl<'storage, const N: usize> Avcap2Backend<'storage, N> {
    /// # Safety
    /// Both callbacks must obey the synchronous reviewed resolver contract in
    /// CAPTURE-STREAM.md and return valid callable exports with the documented
    /// Avcap2 signatures for the entire backend lifetime. The loader's existing
    /// kernel resolver must stay initialized. No concurrent native capture owner
    /// or callback reentry is permitted. This does not authorize execution on PS5.
    pub unsafe fn new(
        storage: &'storage mut CaptureStorage<N>,
        ops: ResolverOps,
    ) -> Result<Self, Diagnostic> {
        // SAFETY: integer-only ABI queries retain no pointers or hardware state.
        let (version, bytes, alignment) = unsafe {
            (
                ps5_capture_stream_abi_version(),
                ps5_capture_stream_storage_size() as usize,
                ps5_capture_stream_storage_alignment() as usize,
            )
        };
        if version != ABI_VERSION
            || bytes == 0
            || bytes > N
            || bytes > u32::MAX as usize
            || !alignment.is_power_of_two()
            || alignment > align_of::<CaptureStorage<N>>()
            || ops.abi_version != ABI_VERSION
            || ops.struct_bytes as usize != size_of::<ResolverOps>()
        {
            return Err(invalid());
        }
        Ok(Self {
            pointer: NonNull::from(&mut storage.bytes).cast(),
            ops,
            opened: false,
            poisoned: false,
            diagnostic: Diagnostic::default(),
            _storage: PhantomData,
            _single_owner: PhantomData,
        })
    }
    pub const fn last_diagnostic(&self) -> Diagnostic {
        self.diagnostic
    }
    fn close_inner(&mut self) -> Result<(), Diagnostic> {
        if !self.opened {
            return Ok(());
        }
        self.opened = false;
        let mut diagnostic = Diagnostic::default();
        // SAFETY: consumes the unique queried-size/alignment context once. No
        // caller buffer is retained; the storage borrow outlives this cleanup.
        let status = unsafe { ps5_capture_stream_close(self.pointer.as_ptr(), &mut diagnostic) };
        self.diagnostic = diagnostic;
        checked(status, diagnostic, 8).map(|_| ())
    }
}
impl<const N: usize> Backend for Avcap2Backend<'_, N> {
    fn open(&mut self) -> Result<(), Diagnostic> {
        if self.opened || self.poisoned {
            return Err(invalid());
        }
        let mut diagnostic = Diagnostic::default();
        // SAFETY: unique stable storage and synchronous resolver table obey the
        // constructor contract. Failed open consumes acquired native resources.
        let status = unsafe {
            ps5_capture_stream_open(
                self.pointer.as_ptr(),
                N.min(u32::MAX as usize) as u32,
                &self.ops,
                &mut diagnostic,
            )
        };
        self.diagnostic = diagnostic;
        let valid = diagnostic.abi_version == ABI_VERSION
            && diagnostic.status == status
            && (1..=4).contains(&diagnostic.stage)
            && diagnostic.cleanup_flags & !7 == 0;
        if status == 0 {
            self.opened = true;
            if !valid || diagnostic.stage != 4 || diagnostic.cleanup_flags != 0 {
                let cleanup = self.close_inner();
                diagnostic.status = ABI_ERROR;
                if let Err(error) = cleanup {
                    diagnostic.cleanup_flags |= error.cleanup_flags | 7;
                }
                self.poisoned = true;
                return Err(diagnostic);
            }
            Ok(())
        } else {
            self.poisoned = true;
            if !valid {
                diagnostic.status = ABI_ERROR;
            }
            Err(diagnostic)
        }
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        if !self.opened || self.poisoned {
            return Err(invalid());
        }
        let mut diagnostic = Diagnostic::default();
        // SAFETY: unique open context and valid synchronous output record.
        let status = unsafe { ps5_capture_stream_start(self.pointer.as_ptr(), &mut diagnostic) };
        self.diagnostic = diagnostic;
        let result = checked(status, diagnostic, 5).map(|_| ());
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn read_once(&mut self, output: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        if !self.opened || self.poisoned || output.is_empty() || output.len() > MAX_READ_BYTES {
            return Err(invalid());
        }
        let mut raw = NativeRead::default();
        let mut diagnostic = Diagnostic::default();
        // SAFETY: live exclusive context, bounded writable output and fixed-size
        // disjoint records; C retains no read pointer and wipes native scratch.
        let status = unsafe {
            ps5_capture_stream_read_once(
                self.pointer.as_ptr(),
                output.as_mut_ptr(),
                output.len() as u32,
                &mut raw,
                &mut diagnostic,
            )
        };
        self.diagnostic = diagnostic;
        if let Err(error) = checked(status, diagnostic, 6) {
            self.poisoned = true;
            output.fill(0);
            return Err(error);
        }
        let kind = match raw.kind {
            1 => ReadKind::Data,
            2 => ReadKind::Zero,
            3 => ReadKind::Empty,
            4 => ReadKind::Overrun,
            5 => ReadKind::Ended,
            6 => ReadKind::Error,
            _ => {
                self.poisoned = true;
                return Err(Diagnostic {
                    status: ABI_ERROR,
                    ..diagnostic
                });
            }
        };
        if raw.abi_version != ABI_VERSION
            || raw.bytes as usize > output.len()
            || raw.flags & !1 != 0
            || (kind != ReadKind::Data && raw.bytes != 0)
            || (kind == ReadKind::Data
                && (raw.bytes == 0 || raw.raw_status <= 0 || raw.bytes != raw.raw_status as u32))
            || (kind == ReadKind::Overrun && raw.flags != 1)
            || diagnostic.raw_status != raw.raw_status
        {
            self.poisoned = true;
            output.fill(0);
            return Err(Diagnostic {
                status: ABI_ERROR,
                ..diagnostic
            });
        }
        Ok(ReadOutcome {
            kind,
            raw_status: raw.raw_status,
            bytes: raw.bytes as usize,
            discontinuity: raw.flags != 0,
            metadata: raw.metadata,
            native_time: None,
        })
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        if !self.opened {
            return Err(invalid());
        }
        let mut diagnostic = Diagnostic::default();
        // SAFETY: consume the started phase once. Native close still consumes
        // open/initialize phases after a failed Stop without retrying Stop.
        let status = unsafe { ps5_capture_stream_stop(self.pointer.as_ptr(), &mut diagnostic) };
        self.diagnostic = diagnostic;
        checked(status, diagnostic, 7).map(|_| ())
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.close_inner()
    }
}
impl<const N: usize> Drop for Avcap2Backend<'_, N> {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}
