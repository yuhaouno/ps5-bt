// SPDX-License-Identifier: GPL-3.0-or-later
//! Fallible cached native clock and one finite wait request per caller step.
//! This deliberately does not implement the infallible `bt_core::Clock` trait.
use core::{
    ffi::{c_char, c_void},
    marker::PhantomData,
    mem::align_of,
    ptr::NonNull,
};
pub const ABI_VERSION: u32 = 0x0001_0000;
pub const MAX_WAIT_SLICE_US: u32 = 1000;
pub type Lookup = unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> i64;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockDiagnostic {
    pub abi_version: u32,
    pub stage: u32,
    pub status: i32,
    pub raw_status: i32,
    pub raw_lookup: i64,
    pub requested_us: u32,
    pub reserved: u32,
    pub raw_seconds: i64,
    pub raw_nanoseconds: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockWaitError {
    InvalidStorage,
    Closed,
    DeadlineOverflow,
    Abi(ClockDiagnostic),
    Native(ClockDiagnostic),
    InvalidTime(ClockDiagnostic),
    TimeOverflow(ClockDiagnostic),
    ClockRegressed(ClockDiagnostic),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    Cancelled,
    DeadlineReached {
        observed_us: u64,
    },
    /// The native call returned success. No elapsed duration is inferred.
    WaitReturned {
        observed_before_us: u64,
        requested_us: u32,
    },
}
pub fn deadline_after(now_us: u64, delay_us: u64) -> Result<u64, ClockWaitError> {
    now_us
        .checked_add(delay_us)
        .ok_or(ClockWaitError::DeadlineOverflow)
}
#[repr(C, align(16))]
pub struct ClockWaitStorage<const N: usize = 128> {
    bytes: [u8; N],
}
impl<const N: usize> ClockWaitStorage<N> {
    pub const fn new() -> Self {
        Self { bytes: [0; N] }
    }
}
impl<const N: usize> Default for ClockWaitStorage<N> {
    fn default() -> Self {
        Self::new()
    }
}
unsafe extern "C" {
    fn ps5_clock_wait_abi_version() -> u32;
    fn ps5_clock_wait_storage_size() -> u32;
    fn ps5_clock_wait_storage_alignment() -> u32;
    fn ps5_clock_wait_init(
        storage: *mut c_void,
        capacity: u32,
        lookup: Lookup,
        report: *mut ClockDiagnostic,
    ) -> i32;
    fn ps5_clock_wait_now(storage: *mut c_void, now: *mut u64, report: *mut ClockDiagnostic)
    -> i32;
    fn ps5_clock_wait_slice(
        storage: *mut c_void,
        requested: u32,
        report: *mut ClockDiagnostic,
    ) -> i32;
    fn ps5_clock_wait_close(storage: *mut c_void, report: *mut ClockDiagnostic) -> i32;
}
/// Exclusive cached clock functions, no native resource allocation.
/// ```compile_fail
/// fn send<T:Send>(){}
/// send::<ps5_platform::clock_wait::ClockWait<'static>>();
/// ```
pub struct ClockWait<'storage, const N: usize = 128> {
    pointer: NonNull<c_void>,
    active: bool,
    failure: Option<ClockWaitError>,
    last_now: Option<u64>,
    diagnostic: ClockDiagnostic,
    _storage: PhantomData<&'storage mut ClockWaitStorage<N>>,
    _single_thread: PhantomData<*mut ()>,
}
fn checked(status: i32, diagnostic: ClockDiagnostic, stage: u32) -> Result<(), ClockWaitError> {
    if diagnostic.abi_version != ABI_VERSION
        || diagnostic.stage != stage
        || diagnostic.status != status
        || diagnostic.reserved != 0
        || (status == 0 && (diagnostic.raw_status != 0 || diagnostic.raw_lookup != 0))
    {
        return Err(ClockWaitError::Abi(diagnostic));
    }
    match status {
        0 => Ok(()),
        -6 => Err(ClockWaitError::InvalidTime(diagnostic)),
        -7 => Err(ClockWaitError::TimeOverflow(diagnostic)),
        -8 => Err(ClockWaitError::ClockRegressed(diagnostic)),
        _ => Err(ClockWaitError::Native(diagnostic)),
    }
}
impl<'storage, const N: usize> ClockWait<'storage, N> {
    /// # Safety
    /// Lookup must obey PLATFORM-CLOCK.md's synchronous resolver contract and
    /// return callable `clock_gettime`/`usleep` with the reviewed target signatures.
    /// Exports must remain valid for this entire borrow. The existing loader
    /// resolver stays initialized; callbacks must not reenter this owner. This
    /// constructor does not authorize a hardware run or establish native timing.
    pub unsafe fn new(
        storage: &'storage mut ClockWaitStorage<N>,
        lookup: Lookup,
    ) -> Result<Self, ClockWaitError> {
        // SAFETY: pure integer-only ABI queries retain no pointers.
        let (version, size, alignment) = unsafe {
            (
                ps5_clock_wait_abi_version(),
                ps5_clock_wait_storage_size() as usize,
                ps5_clock_wait_storage_alignment() as usize,
            )
        };
        if version != ABI_VERSION
            || size == 0
            || size > N
            || N > u32::MAX as usize
            || !alignment.is_power_of_two()
            || alignment > align_of::<ClockWaitStorage<N>>()
        {
            return Err(ClockWaitError::InvalidStorage);
        }
        let pointer = NonNull::from(&mut storage.bytes).cast::<c_void>();
        let mut diagnostic = ClockDiagnostic::default();
        // SAFETY: fresh zeroed or previously closed exclusive stable context,
        // valid report and resolver meeting the constructor's lifetime contract.
        let status =
            unsafe { ps5_clock_wait_init(pointer.as_ptr(), N as u32, lookup, &mut diagnostic) };
        let stage = if status != 0 && diagnostic.stage == 1 {
            1
        } else {
            2
        };
        let validated = checked(status, diagnostic, stage).and({
            if diagnostic.requested_us == 0
                && diagnostic.raw_seconds == 0
                && diagnostic.raw_nanoseconds == 0
            {
                Ok(())
            } else {
                Err(ClockWaitError::Abi(diagnostic))
            }
        });
        if let Err(error) = validated {
            if status == 0 {
                let mut cleanup = ClockDiagnostic::default();
                // SAFETY: native success established a cached context even if
                // its diagnostic was incoherent; consume it before releasing storage.
                let _ = unsafe { ps5_clock_wait_close(pointer.as_ptr(), &mut cleanup) };
            }
            return Err(error);
        }
        Ok(Self {
            pointer,
            active: true,
            failure: None,
            last_now: None,
            diagnostic,
            _storage: PhantomData,
            _single_thread: PhantomData,
        })
    }
    pub const fn last_diagnostic(&self) -> ClockDiagnostic {
        self.diagnostic
    }
    fn ready(&self) -> Result<(), ClockWaitError> {
        if !self.active {
            return Err(ClockWaitError::Closed);
        }
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(())
    }
    fn fail<T>(&mut self, error: ClockWaitError) -> Result<T, ClockWaitError> {
        self.failure = Some(error);
        Err(error)
    }
    pub fn now_us(&mut self) -> Result<u64, ClockWaitError> {
        self.ready()?;
        let (mut now, mut diagnostic) = (0, ClockDiagnostic::default());
        // SAFETY: exclusive initialized context and disjoint fixed-size outputs;
        // synchronous clock call does not retain any pointer.
        let status =
            unsafe { ps5_clock_wait_now(self.pointer.as_ptr(), &mut now, &mut diagnostic) };
        self.diagnostic = diagnostic;
        if let Err(error) = checked(status, diagnostic, 3) {
            if now != 0 {
                return self.fail(ClockWaitError::Abi(diagnostic));
            }
            return self.fail(error);
        }
        let expected = if diagnostic.raw_seconds >= 0
            && (0..1_000_000_000).contains(&diagnostic.raw_nanoseconds)
        {
            (diagnostic.raw_seconds as u64)
                .checked_mul(1_000_000)
                .and_then(|seconds| seconds.checked_add(diagnostic.raw_nanoseconds as u64 / 1000))
        } else {
            None
        };
        if expected != Some(now) || diagnostic.requested_us != 0 {
            return self.fail(ClockWaitError::Abi(diagnostic));
        }
        if self.last_now.is_some_and(|last| now < last) {
            return self.fail(ClockWaitError::ClockRegressed(diagnostic));
        }
        self.last_now = Some(now);
        Ok(now)
    }
    /// Check Stop/Cancel in caller state between invocations. Cancellation skips
    /// clock and wait. Otherwise read now once, then request at most one <=1ms
    /// wait. The next outer-loop turn rechecks clock/deadlines and services Stop.
    pub fn wait_slice(
        &mut self,
        deadline_us: u64,
        cancelled: bool,
    ) -> Result<WaitOutcome, ClockWaitError> {
        self.ready()?;
        if cancelled {
            return Ok(WaitOutcome::Cancelled);
        }
        let now = self.now_us()?;
        if now >= deadline_us {
            return Ok(WaitOutcome::DeadlineReached { observed_us: now });
        }
        let requested = (deadline_us - now).min(MAX_WAIT_SLICE_US as u64) as u32;
        let mut diagnostic = ClockDiagnostic::default();
        // SAFETY: live exclusive context; request is 1..1000 us and report lives
        // for this one synchronous usleep call. There is no retry on interruption.
        let status =
            unsafe { ps5_clock_wait_slice(self.pointer.as_ptr(), requested, &mut diagnostic) };
        self.diagnostic = diagnostic;
        if let Err(error) = checked(status, diagnostic, 4) {
            return self.fail(error);
        }
        if diagnostic.requested_us != requested
            || diagnostic.raw_seconds != 0
            || diagnostic.raw_nanoseconds != 0
        {
            return self.fail(ClockWaitError::Abi(diagnostic));
        }
        Ok(WaitOutcome::WaitReturned {
            observed_before_us: now,
            requested_us: requested,
        })
    }
    pub fn close(&mut self) -> Result<(), ClockWaitError> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let mut diagnostic = ClockDiagnostic::default();
        // SAFETY: consume our cached context once. C releases no external
        // resource, clears state and retains no pointer to borrowed storage.
        let status = unsafe { ps5_clock_wait_close(self.pointer.as_ptr(), &mut diagnostic) };
        self.diagnostic = diagnostic;
        checked(status, diagnostic, 5)
    }
}
impl<const N: usize> Drop for ClockWait<'_, N> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
