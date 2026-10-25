// SPDX-License-Identifier: GPL-3.0-or-later
use crate::ParseError;

/// Hard implementation ceiling, including the root element at depth one.
pub const MAX_DEPTH: usize = 16;

/// All bounds are caller policy. No default large/unbounded parse is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_depth: usize,
    pub max_elements: usize,
    /// Encoded root bytes, including headers and all descendants.
    pub max_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uuid {
    U16(u16),
    U32(u32),
    U128(u128),
}

/// Integers are big-endian, with signed values sign-extended. Strings and URLs
/// remain bytes; no UTF-8, URL, profile or attribute semantics are assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataElement<'a> {
    Nil,
    Unsigned(u128),
    Signed(i128),
    Uuid(Uuid),
    Text(&'a [u8]),
    Boolean(bool),
    Sequence(Elements<'a>),
    Alternative(Elements<'a>),
    Url(&'a [u8]),
}

impl<'a> DataElement<'a> {
    /// Validate exactly one full tree before returning any element. Validation
    /// is iterative, allocation-free and bounded by all three caller limits.
    pub fn parse(bytes: &'a [u8], limits: Limits) -> Result<Self, ParseError> {
        if limits.max_depth == 0 || limits.max_depth > MAX_DEPTH {
            return Err(ParseError::InvalidLimits);
        }
        if bytes.len() > limits.max_bytes {
            return Err(ParseError::ByteLimit);
        }
        if limits.max_elements == 0 {
            return Err(ParseError::ElementLimit);
        }
        let (root, tail) = decode(bytes)?;
        if !tail.is_empty() {
            return Err(ParseError::Length);
        }
        let mut pending: [&[u8]; MAX_DEPTH] = [&[]; MAX_DEPTH];
        pending[0] = bytes;
        let mut depth = 1;
        let mut remaining = limits.max_elements;
        while depth > 0 {
            let current = pending[depth - 1];
            if current.is_empty() {
                depth -= 1;
                continue;
            }
            if remaining == 0 {
                return Err(ParseError::ElementLimit);
            }
            remaining -= 1;
            let (element, tail) = decode(current)?;
            pending[depth - 1] = tail;
            if let Self::Sequence(children) | Self::Alternative(children) = element
                && !children.remaining.is_empty()
            {
                if depth == limits.max_depth {
                    return Err(ParseError::DepthLimit);
                }
                pending[depth] = children.remaining;
                depth += 1;
            }
        }
        Ok(root)
    }
}

/// Immediate children of a validated sequence/alternative. Iteration does not
/// recursively revalidate descendants. An error ends this iterator permanently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Elements<'a> {
    remaining: &'a [u8],
}

impl<'a> Iterator for Elements<'a> {
    type Item = Result<DataElement<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_encoded()
            .map(|result| result.map(|(element, _)| element))
    }
}

impl<'a> Elements<'a> {
    /// Return the decoded child together with its complete encoded bytes.
    /// Profile-specific validators use this to enforce integer wire widths
    /// without weakening the generic value-oriented public representation.
    pub(crate) fn next_encoded(
        &mut self,
    ) -> Option<Result<(DataElement<'a>, &'a [u8]), ParseError>> {
        if self.remaining.is_empty() {
            return None;
        }
        let encoded = self.remaining;
        match decode(self.remaining) {
            Ok((element, tail)) => {
                let consumed = encoded.len() - tail.len();
                self.remaining = tail;
                Some(Ok((element, &encoded[..consumed])))
            }
            Err(error) => {
                self.remaining = &[];
                Some(Err(error))
            }
        }
    }
}

impl core::iter::FusedIterator for Elements<'_> {}

fn decode(bytes: &[u8]) -> Result<(DataElement<'_>, &[u8]), ParseError> {
    let Some((&descriptor, after_descriptor)) = bytes.split_first() else {
        return Err(ParseError::Length);
    };
    let kind = descriptor >> 3;
    let size = descriptor & 7;
    let valid = match kind {
        0 | 5 => size == 0,
        1 | 2 => size <= 4,
        3 => matches!(size, 1 | 2 | 4),
        4 | 6 | 7 | 8 => size >= 5,
        _ => false,
    };
    if !valid {
        return Err(ParseError::InvalidTypeSize);
    }
    let (length, payload) = match size {
        0..=4 => (
            if kind == 0 { 0 } else { 1_usize << size },
            after_descriptor,
        ),
        5..=7 => {
            let width = 1_usize << (size - 5);
            if after_descriptor.len() < width {
                return Err(ParseError::Length);
            }
            let mut length = 0_usize;
            for byte in &after_descriptor[..width] {
                length = length
                    .checked_mul(256)
                    .and_then(|n| n.checked_add(usize::from(*byte)))
                    .ok_or(ParseError::LengthOverflow)?;
            }
            (length, &after_descriptor[width..])
        }
        _ => return Err(ParseError::InvalidTypeSize),
    };
    // Subtraction/slicing avoids header + u32 wire-length overflow entirely.
    if payload.len() < length {
        return Err(ParseError::Length);
    }
    let (value, tail) = payload.split_at(length);
    let mut unsigned = 0_u128;
    if matches!(kind, 1..=3) {
        for byte in value {
            unsigned = (unsigned << 8) | u128::from(*byte);
        }
    }
    let element = match kind {
        0 => DataElement::Nil,
        1 => DataElement::Unsigned(unsigned),
        2 => {
            let shift = 128 - length * 8;
            DataElement::Signed(((unsigned << shift) as i128) >> shift)
        }
        3 => DataElement::Uuid(match length {
            2 => Uuid::U16(unsigned as u16),
            4 => Uuid::U32(unsigned as u32),
            _ => Uuid::U128(unsigned),
        }),
        4 => DataElement::Text(value),
        5 => DataElement::Boolean(value[0] != 0),
        6 => DataElement::Sequence(Elements { remaining: value }),
        7 => DataElement::Alternative(Elements { remaining: value }),
        8 => DataElement::Url(value),
        _ => return Err(ParseError::InvalidTypeSize),
    };
    Ok((element, tail))
}
