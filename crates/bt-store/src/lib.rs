// SPDX-License-Identifier: GPL-3.0-or-later
//! Protected local file-store boundary. No default native calls or allocation.
#![no_std]

pub const ABI_VERSION: u32 = 1;
pub const MAX_PATH_BYTES: usize = 512;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    pub abi_version: u32,
    pub stage: u32,
    pub status: i32,
    pub raw_errno: i32,
    pub flags: u32,
    pub cleanup_errno: i32,
}

#[cfg(feature = "external-posix")]
mod file;
#[cfg(feature = "external-posix")]
pub use file::{CloseFailure, FileStore, StoreStorage};
