// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit opt-in listener candidate; default builds perform no native calls.
#![no_std]
pub const ABI_VERSION: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    pub abi_version: u32,
    pub stage: u32,
    pub status: i32,
    pub raw_errno: i32,
    pub cleanup_errno: i32,
    pub flags: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindConfig {
    pub address: [u8; 4],
    pub port: u16,
}
#[cfg(feature = "external-posix")]
mod native;
#[cfg(feature = "external-posix")]
pub use native::{Client, Listener, ListenerStorage};

#[cfg(feature = "external-posix")]
mod service;
#[cfg(feature = "external-posix")]
pub use service::{ServiceConfig, ServiceProgress, ServiceSnapshot, ServiceState, ServiceStop};
