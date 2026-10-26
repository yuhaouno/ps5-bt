// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded selected-peer Classic connection/security candidate. No transport access.
//! See docs/CLASSIC-LINK.md for prerequisites, security limits and hardware status.
#![no_std]
#![forbid(unsafe_code)]

mod session;
mod wire;

pub use session::{
    CommandTicket, Config, Error, Failure, LinkSession, Notice, Phase, RequestId, State,
};
pub use wire::{CompletionResult, Event, Peer, SecretKey, WireError};
