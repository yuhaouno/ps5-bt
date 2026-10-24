// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded codec ownership. The real encoder is explicitly host-gated for now.
#![no_std]

#[cfg(feature = "host-libsbc")]
mod sbc;

#[cfg(feature = "host-libsbc")]
pub use sbc::{EncodedFrame, Encoder, EncoderStorage, StorageRequirements};
