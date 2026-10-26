// SPDX-License-Identifier: GPL-3.0-or-later
//! Experimental capture boundary. Native format and time units stay unknown.
#![no_std]

mod bridge;
mod format;
#[cfg(feature = "external-avcap2")]
mod native;
mod owner;
mod timeline;

pub use bridge::*;
pub use format::*;
#[cfg(feature = "external-avcap2")]
pub use native::*;
pub use owner::*;
pub use timeline::*;

pub const ABI_VERSION: u32 = 0x0001_0000;
pub const MAX_READ_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadKind {
    Data,
    Zero,
    Empty,
    Overrun,
    Ended,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOutcome {
    pub kind: ReadKind,
    pub raw_status: i32,
    pub bytes: usize,
    pub discontinuity: bool,
    /// Native metadata is opaque; no field is interpreted as a timestamp.
    pub metadata: [u8; 16],
    pub native_time: Option<NativeTime>,
}

impl ReadOutcome {
    pub const fn empty() -> Self {
        Self {
            kind: ReadKind::Empty,
            raw_status: 0,
            bytes: 0,
            discontinuity: false,
            metadata: [0; 16],
            native_time: None,
        }
    }
}
