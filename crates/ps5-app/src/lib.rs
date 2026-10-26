// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed-storage candidate assembly. No transport authority or radio operations.
#![no_std]
mod config;
mod factory;
pub mod generated;
#[cfg(feature = "native-resources")]
mod native;
pub use config::*;
pub use factory::*;
#[cfg(feature = "native-resources")]
pub use native::*;
