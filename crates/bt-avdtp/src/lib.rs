// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded AVDTP initiator signaling for the conservative SBC source path.
//!
//! This crate owns no transport, clock, allocator or retry policy. It emits and
//! consumes exactly one complete AVDTP signaling message at a time in
//! caller-owned buffers. See `docs/AVDTP.md` for the transport contract.
#![no_std]
#![forbid(unsafe_code)]

mod capability;
mod session;
mod signaling;

pub use capability::{
    CapabilityError, SbcCapabilities, SbcConfiguration, ServiceCapabilities, ServiceCapability,
    ServiceCapabilityIter,
};
pub use session::{
    Accepted, Command, Error, Initiator, Outcome, PendingCommand, RejectDetail, RemoteReject,
    StreamState,
};
pub use signaling::{
    EndpointType, Header, MediaType, Message, MessageType, PacketType, ParseError, SignalId,
    StreamEndpoint, StreamEndpointId, StreamEndpointIter, StreamEndpoints,
};
