// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded management bridge and owned platform stepping, without platform APIs.
#![no_std]
#![forbid(unsafe_code)]

mod management;
mod pump;
pub use management::{Capabilities, Config, Error, LookupFailure, Route, Runner};
pub use pump::{AudioStep, CommandSink, InputPump, OutputPump, PumpError, PumpProgress};
