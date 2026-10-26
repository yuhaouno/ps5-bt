// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded Classic Inquiry search candidate. No radio access or controller configuration mutation.
//! See docs/DEVICE-DISCOVERY.md for ownership preconditions and tested wire subset.
#![no_std]
#![forbid(unsafe_code)]
mod inquiry;
mod wire;
pub use inquiry::{
    CommandTicket, Config, DeviceId, DeviceRecord, Error, Failure, Inquiry, Notice, ResultSlot,
    Snapshot, State, StopReason,
};
pub use wire::{DeviceName, Event, NAME_CAPACITY, Observation, Results, WireError};

pub use inquiry::MAX_RESULT_SLOTS;
