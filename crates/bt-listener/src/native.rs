// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ABI_VERSION, BindConfig, Diagnostic};
use bt_control::pump::{IoError, SocketIo};
use core::{ffi::c_void, marker::PhantomData, mem::align_of, ptr::NonNull};
const ABI_ERROR: i32 = -1000;
const INVALID: i32 = -1001;
const MAX_IO: usize = 65536;
const CLEANUP_UNKNOWN: u32 = 1;
unsafe extern "C" {
    fn ps5_bt_listener_abi_version() -> u32;
    fn ps5_bt_listener_storage_size() -> u32;
    fn ps5_bt_listener_storage_alignment() -> u32;
    fn ps5_bt_listener_bind(
        storage: *mut c_void,
        capacity: u32,
        address: *const u8,
        port: u16,
        bound_port: *mut u16,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_listener_accept(
        storage: *mut c_void,
        client: *mut u64,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_listener_read(
        storage: *mut c_void,
        client: u64,
        output: *mut u8,
        capacity: u32,
        length: *mut u32,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_listener_write(
        storage: *mut c_void,
        client: u64,
        data: *const u8,
        length: u32,
        written: *mut u32,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_listener_client_close(
        storage: *mut c_void,
        client: u64,
        report: *mut Diagnostic,
    ) -> i32;
    fn ps5_bt_listener_close(storage: *mut c_void, report: *mut Diagnostic) -> i32;
}
#[repr(C, align(64))]
pub struct ListenerStorage<const N: usize> {
    bytes: [u8; N],
}
impl<const N: usize> ListenerStorage<N> {
    pub const fn new() -> Self {
        Self { bytes: [0; N] }
    }
}
impl<const N: usize> Default for ListenerStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}
/// Exclusive owner; C retains only state inside the borrowed stable storage.
/// No SDK/native struct or file descriptor is represented in Rust.
/// ```compile_fail
/// fn send<T: Send>() {}
/// send::<bt_listener::Listener<'static, 128>>();
/// ```
pub struct Listener<'storage, const N: usize> {
    storage: NonNull<c_void>,
    _borrow: PhantomData<&'storage mut ListenerStorage<N>>,
    _single_owner: PhantomData<*mut ()>,
    active: bool,
    poisoned: bool,
    port: u16,
    diagnostic: Diagnostic,
    managed: Option<ManagedClient>,
}
fn invalid() -> Diagnostic {
    Diagnostic {
        abi_version: ABI_VERSION,
        stage: 1,
        status: INVALID,
        ..Diagnostic::default()
    }
}
fn coherent(status: i32, r: Diagnostic) -> bool {
    r.abi_version == ABI_VERSION
        && r.status == status
        && (1..=13).contains(&r.stage)
        && r.flags & !CLEANUP_UNKNOWN == 0
        && r.raw_errno >= 0
        && r.cleanup_errno >= 0
        && (status != 0 || (r.raw_errno == 0 && r.cleanup_errno == 0 && r.flags == 0))
        && (status != 1 || (r.raw_errno > 0 && r.cleanup_errno == 0 && r.flags == 0))
}
fn abi_failure(mut r: Diagnostic) -> Diagnostic {
    r.abi_version = ABI_VERSION;
    r.status = ABI_ERROR;
    r
}
fn code(r: Diagnostic) -> i32 {
    if r.raw_errno > 0 {
        r.raw_errno
    } else {
        r.status
    }
}
impl<'storage, const N: usize> Listener<'storage, N> {
    pub fn bind(
        storage: &'storage mut ListenerStorage<N>,
        config: BindConfig,
    ) -> Result<Self, Diagnostic> {
        if config.address[0] == 0
            || config.address[0] >= 224
            || (config.port == 0 && config.address != [127, 0, 0, 1])
        {
            return Err(invalid());
        }
        // SAFETY: integer-only ABI/layout queries neither dereference nor retain pointers.
        let (version, size, alignment) = unsafe {
            (
                ps5_bt_listener_abi_version(),
                ps5_bt_listener_storage_size() as usize,
                ps5_bt_listener_storage_alignment() as usize,
            )
        };
        if version != ABI_VERSION
            || size == 0
            || size > N
            || alignment == 0
            || !alignment.is_power_of_two()
            || alignment > align_of::<ListenerStorage<N>>()
        {
            return Err(invalid());
        }
        let pointer = NonNull::from(&mut storage.bytes).cast::<c_void>();
        if pointer.as_ptr() as usize & (alignment - 1) != 0 {
            return Err(invalid());
        }
        let mut r = Diagnostic::default();
        let mut port = 0;
        // SAFETY: queried-size/alignment, exclusively borrowed initialized storage;
        // fixed four-byte address and output/report are valid for this synchronous call.
        let status = unsafe {
            ps5_bt_listener_bind(
                pointer.as_ptr(),
                size as u32,
                config.address.as_ptr(),
                config.port,
                &mut port,
                &mut r,
            )
        };
        if status != 0 {
            return Err(if coherent(status, r) && port == 0 {
                r
            } else {
                abi_failure(r)
            });
        }
        if !coherent(status, r)
            || r.stage != 6
            || port == 0
            || (config.port != 0 && port != config.port)
        {
            let mut cleanup = Diagnostic::default();
            // SAFETY: success establishes an owned C context even if its report is dirty.
            // Consume it once before releasing the caller's exclusive storage borrow.
            let closed = unsafe { ps5_bt_listener_close(pointer.as_ptr(), &mut cleanup) };
            r = abi_failure(r);
            if !coherent(closed, cleanup) || closed != 0 {
                r.flags |= CLEANUP_UNKNOWN;
                r.cleanup_errno = if cleanup.raw_errno > 0 {
                    cleanup.raw_errno
                } else {
                    cleanup.cleanup_errno
                };
            }
            return Err(r);
        }
        Ok(Self {
            storage: pointer,
            _borrow: PhantomData,
            _single_owner: PhantomData,
            active: true,
            poisoned: false,
            port,
            diagnostic: r,
            managed: None,
        })
    }
    pub const fn local_port(&self) -> u16 {
        self.port
    }
    pub const fn last_diagnostic(&self) -> Diagnostic {
        self.diagnostic
    }
    pub fn accept<'listener>(
        &'listener mut self,
    ) -> Result<Option<Client<'listener, 'storage, N>>, Diagnostic> {
        if !self.active || self.managed.is_some() {
            return Err(invalid());
        }
        if self.poisoned {
            return Err(self.diagnostic);
        }
        let mut token = 0;
        let mut r = Diagnostic::default();
        // SAFETY: unique live context; token/report outputs live only for this call.
        let status = unsafe { ps5_bt_listener_accept(self.storage.as_ptr(), &mut token, &mut r) };
        self.diagnostic = r;
        if !coherent(status, r)
            || (status == 0 && (token == 0 || r.stage != 9))
            || (status != 0 && token != 0)
        {
            let failure = abi_failure(r);
            let cleanup = self.close_inner();
            self.diagnostic = failure;
            if let Err(error) = cleanup {
                self.diagnostic.flags |= CLEANUP_UNKNOWN;
                self.diagnostic.cleanup_errno = if error.raw_errno > 0 {
                    error.raw_errno
                } else {
                    error.cleanup_errno
                };
            }
            return Err(self.diagnostic);
        }
        if r.flags & CLEANUP_UNKNOWN != 0 {
            self.poisoned = true;
        }
        match status {
            0 => Ok(Some(Client {
                listener: self,
                token,
                active: true,
                terminal: false,
                close_result: None,
                diagnostic: r,
            })),
            1 => Ok(None),
            _ => Err(r),
        }
    }
    pub(crate) fn managed_for(&self, owner: [usize; 2]) -> Result<bool, Diagnostic> {
        if !self.active {
            return Err(invalid());
        }
        match self.managed {
            Some(client) if client.owner != owner => Err(invalid()),
            client => Ok(client.is_some()),
        }
    }
    pub(crate) fn accept_managed(&mut self, owner: [usize; 2]) -> Result<bool, Diagnostic> {
        let Some(mut client) = self.accept()? else {
            return Ok(false);
        };
        let state = ManagedClient {
            owner,
            token: client.token,
            terminal: client.terminal,
            close_result: client.close_result,
            diagnostic: client.diagnostic,
        };
        // The native resource moves into Listener's managed slot. No borrow or
        // pointer to this movable Rust owner survives the synchronous call.
        client.active = false;
        client.listener.managed = Some(state);
        Ok(true)
    }
    pub(crate) fn with_managed<T>(
        &mut self,
        owner: [usize; 2],
        work: impl FnOnce(&mut Client<'_, 'storage, N>) -> T,
    ) -> Result<T, Diagnostic> {
        self.managed_for(owner)?;
        let state = self.managed.take().ok_or_else(invalid)?;
        let mut client = Client {
            listener: self,
            token: state.token,
            active: true,
            terminal: state.terminal,
            close_result: state.close_result,
            diagnostic: state.diagnostic,
        };
        let result = work(&mut client);
        if client.active {
            client.listener.managed = Some(ManagedClient {
                owner,
                token: client.token,
                terminal: client.terminal,
                close_result: client.close_result,
                diagnostic: client.diagnostic,
            });
        }
        // If work panics, Client's ordinary Drop still consumes the socket.
        // On a normal return the slot above or close() owns the resource.
        client.active = false;
        Ok(result)
    }
    pub fn close(mut self) -> Result<Diagnostic, Diagnostic> {
        self.close_inner()
    }
    fn close_inner(&mut self) -> Result<Diagnostic, Diagnostic> {
        if !self.active {
            return if self.diagnostic.status == 0 {
                Ok(self.diagnostic)
            } else {
                Err(self.diagnostic)
            };
        }
        self.active = false;
        self.managed = None;
        let mut r = Diagnostic::default();
        // SAFETY: consumes the unique context exactly once; no borrowed client can
        // coexist with this Rust call. C closes both owned descriptors if necessary.
        let status = unsafe { ps5_bt_listener_close(self.storage.as_ptr(), &mut r) };
        if !coherent(status, r) || (status == 0 && r.stage != 13) {
            r = abi_failure(r);
            r.flags |= CLEANUP_UNKNOWN;
        }
        self.diagnostic = r;
        if status == 0 && r.status == 0 {
            Ok(r)
        } else {
            Err(r)
        }
    }
}
impl<const N: usize> Drop for Listener<'_, N> {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}
#[derive(Clone, Copy)]
struct ManagedClient {
    // Stable, exclusively borrowed HTTP buffers identify the one service. A
    // different live service cannot borrow the same buffers through safe Rust.
    owner: [usize; 2],
    token: u64,
    terminal: bool,
    close_result: Option<Result<(), i32>>,
    diagnostic: Diagnostic,
}
/// One accepted socket. Its mutable borrow prevents a second accepted client,
/// listener teardown or concurrent native use while a ConnectionPump owns it.
/// ```compile_fail,E0499
/// fn two_clients(listener: &mut bt_listener::Listener<'_, 128>) {
///     let first = listener.accept().unwrap();
///     let second = listener.accept().unwrap();
///     drop(first);
///     drop(second);
/// }
/// ```
pub struct Client<'listener, 'storage, const N: usize> {
    listener: &'listener mut Listener<'storage, N>,
    token: u64,
    active: bool,
    terminal: bool,
    close_result: Option<Result<(), i32>>,
    diagnostic: Diagnostic,
}
impl<const N: usize> Client<'_, '_, N> {
    pub const fn last_diagnostic(&self) -> Diagnostic {
        self.diagnostic
    }
    fn checked(
        &mut self,
        status: i32,
        r: Diagnostic,
        count: u32,
        capacity: usize,
        stage: u32,
    ) -> Result<usize, IoError> {
        self.diagnostic = r;
        self.listener.diagnostic = r;
        if !coherent(status, r)
            || (status == 0 && r.stage != stage)
            || count as usize > capacity
            || (status != 0 && count != 0)
        {
            self.terminal = true;
            self.diagnostic = abi_failure(r);
            self.listener.diagnostic = self.diagnostic;
            return Err(IoError::Failed(ABI_ERROR));
        }
        match status {
            0 => Ok(count as usize),
            1 => Err(IoError::WouldBlock),
            _ => {
                self.terminal = true;
                Err(IoError::Failed(code(r)))
            }
        }
    }
}
impl<const N: usize> SocketIo for Client<'_, '_, N> {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, IoError> {
        if !self.active || self.terminal || output.is_empty() || output.len() > MAX_IO {
            return Err(IoError::Failed(INVALID));
        }
        let mut count = 0;
        let mut r = Diagnostic::default();
        // SAFETY: exclusive live client token, bounded writable output/count/report;
        // native recv is synchronous and retains none of these pointers.
        let status = unsafe {
            ps5_bt_listener_read(
                self.listener.storage.as_ptr(),
                self.token,
                output.as_mut_ptr(),
                output.len() as u32,
                &mut count,
                &mut r,
            )
        };
        let result = self.checked(status, r, count, output.len(), 10);
        if result.is_err() {
            output.fill(0);
        }
        result
    }
    fn write(&mut self, data: &[u8]) -> Result<usize, IoError> {
        if !self.active || self.terminal || data.is_empty() || data.len() > MAX_IO {
            return Err(IoError::Failed(INVALID));
        }
        let mut count = 0;
        let mut r = Diagnostic::default();
        // SAFETY: exclusive live client token and bounded readable bytes; synchronous
        // send accepts only the reported prefix and retains no caller pointer.
        let status = unsafe {
            ps5_bt_listener_write(
                self.listener.storage.as_ptr(),
                self.token,
                data.as_ptr(),
                data.len() as u32,
                &mut count,
                &mut r,
            )
        };
        self.checked(status, r, count, data.len(), 11)
    }
    fn close(&mut self) -> Result<(), i32> {
        if let Some(result) = self.close_result {
            return result;
        }
        self.active = false;
        let mut r = Diagnostic::default();
        // SAFETY: consumes this unique client once, even on close failure. Listener
        // storage is still borrowed and no pointer into an I/O buffer is retained.
        let status = unsafe {
            ps5_bt_listener_client_close(self.listener.storage.as_ptr(), self.token, &mut r)
        };
        if !coherent(status, r) || (status == 0 && r.stage != 12) {
            r = abi_failure(r);
            r.flags |= CLEANUP_UNKNOWN;
        }
        self.diagnostic = r;
        self.listener.diagnostic = r;
        let result = if status == 0 && r.status == 0 {
            Ok(())
        } else {
            self.listener.poisoned = true;
            Err(code(r))
        };
        self.close_result = Some(result);
        result
    }
}
impl<const N: usize> Drop for Client<'_, '_, N> {
    fn drop(&mut self) {
        if self.active {
            let _ = SocketIo::close(self);
        }
    }
}
