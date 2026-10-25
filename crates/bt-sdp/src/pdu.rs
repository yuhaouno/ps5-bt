// SPDX-License-Identifier: GPL-3.0-or-later
use crate::ParseError;

/// Exact five-byte-header-bearing SDP frame. Unknown IDs are retained here;
/// their parameter semantics are not validated by this generic framing layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pdu<'a> {
    pub id: u8,
    pub transaction_id: u16,
    pub parameters: &'a [u8],
}

impl<'a> Pdu<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 5 {
            return Err(ParseError::Length);
        }
        let parameter_length = usize::from(u16::from_be_bytes([bytes[3], bytes[4]]));
        if bytes.len() - 5 != parameter_length {
            return Err(ParseError::Length);
        }
        Ok(Self {
            id: bytes[0],
            transaction_id: u16::from_be_bytes([bytes[1], bytes[2]]),
            parameters: &bytes[5..],
        })
    }

    pub fn expect_transaction(self, expected: u16) -> Result<Self, ParseError> {
        if self.transaction_id != expected {
            return Err(ParseError::TransactionMismatch);
        }
        Ok(self)
    }
}

/// Opaque server state; no interpretation, address, offset or retry semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContinuationState<'a>(&'a [u8]);

impl<'a> ContinuationState<'a> {
    /// Parse the entire length-prefixed field, accepting zero through 16 bytes.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let Some((&length, state)) = bytes.split_first() else {
            return Err(ParseError::InvalidContinuation);
        };
        if length > 16 || state.len() != usize::from(length) {
            return Err(ParseError::InvalidContinuation);
        }
        Ok(Self(state))
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.0
    }

    pub const fn is_complete(self) -> bool {
        self.0.is_empty()
    }
}

/// One response chunk, not necessarily a whole data element. The caller must
/// concatenate chunks in bounded storage and validate the completed tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceSearchAttributeResponse<'a> {
    pub transaction_id: u16,
    pub attribute_lists: &'a [u8],
    pub continuation: ContinuationState<'a>,
}

impl<'a> ServiceSearchAttributeResponse<'a> {
    /// Match the current request ID and enforce its MaximumAttributeByteCount.
    /// This validates framing only, without establishing session ownership.
    pub fn parse(
        bytes: &'a [u8],
        expected_transaction: u16,
        maximum_attribute_bytes: u16,
    ) -> Result<Self, ParseError> {
        let pdu = Pdu::parse(bytes)?.expect_transaction(expected_transaction)?;
        if pdu.id != 0x07 {
            return Err(ParseError::UnexpectedPdu);
        }
        let p = pdu.parameters;
        if p.len() < 2 {
            return Err(ParseError::Length);
        }
        let count = u16::from_be_bytes([p[0], p[1]]);
        if maximum_attribute_bytes < 7 || count > maximum_attribute_bytes {
            return Err(ParseError::AttributeByteCount);
        }
        let rest = &p[2..];
        let count = usize::from(count);
        if rest.len() < count {
            return Err(ParseError::Length);
        }
        Ok(Self {
            transaction_id: pdu.transaction_id,
            attribute_lists: &rest[..count],
            continuation: ContinuationState::parse(&rest[count..])?,
        })
    }
}
