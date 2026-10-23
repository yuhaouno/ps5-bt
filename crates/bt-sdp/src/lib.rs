// SPDX-License-Identifier: GPL-3.0-or-later
//! Borrowed SDP framing and bounded data elements, without a client/session.
//! See docs/SDP.md for the exact specification baseline and limitations.
#![no_std]
#![forbid(unsafe_code)]

mod element;
mod pdu;

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
