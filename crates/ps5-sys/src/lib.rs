// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw FFI only. Safety contract: platform/ps5/include/ps5_bt.h and docs/ABI.md.
#![no_std]

pub const ABI_VERSION: u32 = 0x0001_0000;
pub const OK: i32 = 0;
pub const UNSUPPORTED: i32 = -1;
pub const INVALID: i32 = -2;
pub const BUSY: i32 = -3;
pub const BUFFER_TOO_SMALL: i32 = -4;
pub const IO: i32 = -5;
pub const DISCONNECTED: i32 = -6;
pub const INTEGRITY_LOST: i32 = -7;
pub const TIMEOUT: i32 = -8;
pub const EVENT: u32 = 1;
pub const ACL: u32 = 2;

unsafe extern "C" {
    pub fn ps5_bt_abi_version() -> u32;
    pub fn ps5_bt_init(out_session: *mut u32) -> i32;
    pub fn ps5_bt_shutdown(session: u32) -> i32;
    pub fn ps5_bt_send_hci_command(session: u32, data: *const u8, len: u32) -> i32;
    pub fn ps5_bt_send_acl(session: u32, data: *const u8, len: u32) -> i32;
    pub fn ps5_bt_receive(
        session: u32,
        data: *mut u8,
        capacity: u32,
        out_len: *mut u32,
        out_kind: *mut u32,
        timeout_ms: u32,
    ) -> i32;
    pub fn ps5_platform_log(data: *const u8, len: u32);
    pub fn ps5_platform_abort() -> !;
}
