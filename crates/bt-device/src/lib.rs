// SPDX-License-Identifier: GPL-3.0-or-later
//! Protected selected-device and local bond management; no platform filesystem.
//! See docs/DEVICE-MANAGEMENT.md for store obligations and PS5 validation limits.
#![no_std]
#![forbid(unsafe_code)]

mod manager;
mod record;

pub use manager::{
    ApprovalToken, BondStore, DeviceManager, DeviceNotice, DeviceStatus, Error, ForgetOutcome,
    Persistence, StorageFailure, StoreError, StoreOperation,
};
pub use record::{
    BOND_RECORD_BYTES, BOND_RECORD_VERSION, BondPolicy, BondRecord, KeyType, RecordError,
};
