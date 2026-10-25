// SPDX-License-Identifier: GPL-3.0-or-later
//! Borrowed SDP framing, bounded data elements and A2DP Sink discovery.
//! See docs/SDP.md for the exact specification baseline and limitations.
#![no_std]
#![forbid(unsafe_code)]

mod client;
mod element;
mod pdu;

pub use client::{
    A2dpSinkDiscovery, DiscoveryError, DiscoveryLimits, DiscoveryProgress, DiscoveryState,
    MAX_DISCOVERY_CHUNKS, SinkService,
};
pub use element::{DataElement, Elements, Limits, MAX_DEPTH, Uuid};
pub use pdu::{ContinuationState, Pdu, ServiceSearchAttributeResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Length,
    LengthOverflow,
    InvalidTypeSize,
    InvalidLimits,
    ByteLimit,
    DepthLimit,
    ElementLimit,
    UnexpectedPdu,
    TransactionMismatch,
    InvalidContinuation,
    AttributeByteCount,
}
