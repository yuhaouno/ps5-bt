// SPDX-License-Identifier: GPL-3.0-or-later
//! Portable contracts. No platform APIs, allocation, or hardware assumptions.
#![no_std]
#![forbid(unsafe_code)]

pub mod media;
pub mod sbc;
pub use sbc::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};

/// Stable categories; retain unexpected backend statuses for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    Invalid,
    Busy,
    BufferTooSmall,
    Io,
    Disconnected,
    IntegrityLost,
    Timeout,
    AbiMismatch,
    Backend(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketKind {
    Event,
    Acl,
}

/// Exactly one complete HCI frame, including its header, without an H4 byte.
#[derive(Debug, PartialEq, Eq)]
pub struct Received<'a> {
    pub kind: PacketKind,
    pub bytes: &'a [u8],
}

/// A single-owner transport. Success means accepted, not transmitted/credited.
/// Implementations must preserve native traffic and report framing/queue loss.
/// `Error::Busy` rejects the entire send without accepting bytes. Other send
/// errors have no generic non-acceptance guarantee and require reconciliation.
pub trait BluetoothTransport {
    fn send_command(&mut self, packet: &[u8]) -> Result<(), Error>;
    fn send_acl(&mut self, packet: &[u8]) -> Result<(), Error>;
    /// Zero timeout polls; positive waits are bounded. No packet is `Ok(None)`.
    fn receive<'a>(
        &mut self,
        buffer: &'a mut [u8],
        timeout_ms: u32,
    ) -> Result<Option<Received<'a>>, Error>;
}

pub trait Clock {
    /// Monotonic microseconds; origin is platform-defined, never wall time.
    fn now_us(&self) -> u64;
}

/// Initial capture request; actual support must be measured by each platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmFormat {
    pub sample_rate_hz: u32,
    pub channels: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmRead {
    /// Whole frames, not individual interleaved samples or bytes.
    pub frames: usize,
    /// Timestamp of first frame in the platform Clock domain, if established.
    pub first_frame_us: Option<u64>,
    pub discontinuity: bool,
}

/// Proposed interleaved f32 capture contract, currently without a PS5 backend.
pub trait PcmSource {
    fn format(&self) -> PcmFormat;
    /// Write whole frames only. Never retain the caller's storage.
    fn read(&mut self, samples: &mut [f32]) -> Result<PcmRead, Error>;
}
