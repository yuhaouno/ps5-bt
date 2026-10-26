// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded codec ownership. Host and target archive linking are separately selected.
#![no_std]

#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
mod sbc;

#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
pub use sbc::{EncodedFrame, Encoder, EncoderStorage, StorageRequirements};
