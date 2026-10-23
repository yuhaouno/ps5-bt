// SPDX-License-Identifier: GPL-3.0-or-later
use bt_sdp::{
    ContinuationState, DataElement, Limits, MAX_DEPTH, ParseError, Pdu,
    ServiceSearchAttributeResponse, Uuid,
};

const LIMITS: Limits = Limits {
    max_depth: MAX_DEPTH,
    max_elements: 1024,
    max_bytes: 131_072,
};

fn parse(bytes: &[u8]) -> Result<DataElement<'_>, ParseError> {
    DataElement::parse(bytes, LIMITS)
}

fn pdu(id: u8, parameters: &[u8]) -> Vec<u8> {
    let mut packet = vec![id, 0x12, 0x34];
    packet.extend_from_slice(&(parameters.len() as u16).to_be_bytes());
    packet.extend_from_slice(parameters);
    packet
}

#[test]
fn scalar_values_preserve_width_sign_and_byte_order() {
    assert_eq!(parse(&[0]), Ok(DataElement::Nil));
    assert_eq!(
        parse(&[0x0a, 0x12, 0x34, 0x56, 0x78]),
        Ok(DataElement::Unsigned(0x12345678))
    );
    for size in 0..=4 {
        let length = 1 << size;
        let mut bytes = vec![0x08 | size];
        bytes.extend(vec![0xff; length]);
        let expected = u128::MAX >> (128 - length * 8);
        assert_eq!(parse(&bytes), Ok(DataElement::Unsigned(expected)));
        bytes[0] = 0x10 | size;
        assert_eq!(parse(&bytes), Ok(DataElement::Signed(-1)));
        bytes[1] = 0x80;
        bytes[2..].fill(0);
        let minimum = if length == 16 {
            i128::MIN
        } else {
            -(1_i128 << (length * 8 - 1))
        };
        assert_eq!(parse(&bytes), Ok(DataElement::Signed(minimum)));
        bytes[1] = 0x7f;
        bytes[2..].fill(0xff);
        let maximum = i128::MAX >> (128 - length * 8);
        assert_eq!(parse(&bytes), Ok(DataElement::Signed(maximum)));
    }
    assert_eq!(
        parse(&[0x19, 0x11, 0x0b]),
        Ok(DataElement::Uuid(Uuid::U16(0x110b)))
    );
    assert_eq!(
        parse(&[0x1a, 1, 2, 3, 4]),
        Ok(DataElement::Uuid(Uuid::U32(0x01020304)))
    );
    let mut uuid = vec![0x1c];
    uuid.extend_from_slice(&[0xff; 16]);
    assert_eq!(parse(&uuid), Ok(DataElement::Uuid(Uuid::U128(u128::MAX))));
    for value in 0..=255 {
        assert_eq!(parse(&[0x28, value]), Ok(DataElement::Boolean(value != 0)));
    }
    assert_eq!(
        parse(&[0x25, 2, 0xff, 0]),
        Ok(DataElement::Text(&[0xff, 0]))
    );
    assert_eq!(parse(&[0x45, 2, b'a', b':']), Ok(DataElement::Url(b"a:")));
}

#[test]
fn every_descriptor_matches_the_specification_type_size_table() {
    for descriptor in 0..=255_u8 {
        let kind = descriptor >> 3;
        let size = descriptor & 7;
        let valid = match kind {
            0 | 5 => size == 0,
            1 | 2 => size <= 4,
            3 => matches!(size, 1 | 2 | 4),
            4 | 6 | 7 | 8 => size >= 5,
            _ => false,
        };
        let mut bytes = vec![descriptor];
        let extra = if size < 5 {
            if kind == 0 { 0 } else { 1 << size }
        } else {
            1 << (size - 5)
        };
        bytes.extend(vec![0; extra]);
        let result = parse(&bytes);
        assert_eq!(result.is_ok(), valid, "descriptor {descriptor:#x}");
        if !valid {
            assert_eq!(result, Err(ParseError::InvalidTypeSize));
        }
    }
}

#[test]
fn every_truncation_and_extra_byte_of_all_legal_type_sizes_fails() {
    let mut cases = vec![vec![0]];
    for kind in [1, 2, 3, 5] {
        for size in 0..=4 {
            if (kind == 3 && !matches!(size, 1 | 2 | 4)) || (kind == 5 && size != 0) {
                continue;
            }
            let mut bytes = vec![(kind << 3) | size];
            bytes.extend(vec![0; 1 << size]);
            cases.push(bytes);
        }
    }
    for kind in [4, 6, 7, 8] {
        for size in 5..=7 {
            let mut bytes = vec![(kind << 3) | size];
            bytes.extend(vec![0; (1 << (size - 5)) - 1]);
            bytes.push(3);
            bytes.extend_from_slice(&[0, 0, 0]);
            cases.push(bytes);
        }
    }
    for mut bytes in cases {
        assert!(parse(&bytes).is_ok());
        for end in 0..bytes.len() {
            assert!(parse(&bytes[..end]).is_err(), "{bytes:x?} at {end}");
        }
        bytes.push(0);
        assert_eq!(parse(&bytes), Err(ParseError::Length));
    }
}

#[test]
fn variable_lengths_at_wire_boundaries_do_not_wrap_or_overrun() {
    for (size, length) in [(5, 255), (6, 65_535)] {
        let mut bytes = vec![0x20 | size];
        bytes.extend(vec![0xff; 1 << (size - 5)]);
        bytes.extend(vec![0; length]);
        assert!(parse(&bytes).is_ok());
        for end in 0..bytes.len() {
            assert!(parse(&bytes[..end]).is_err());
        }
        bytes.push(0);
        assert_eq!(parse(&bytes), Err(ParseError::Length));
    }
    for kind in [4, 6, 7, 8] {
        assert_eq!(
            parse(&[(kind << 3) | 7, 0xff, 0xff, 0xff, 0xff]),
            Err(ParseError::Length)
        );
        assert_eq!(
            parse(&[(kind << 3) | 7, 0x80, 0, 0, 0]),
            Err(ParseError::Length)
        );
    }
    assert_eq!(parse(&[0x26, 0, 1, 0xff]), Ok(DataElement::Text(&[0xff])));
    assert_eq!(
        parse(&[0x27, 0, 0, 0, 1, 0xff]),
        Ok(DataElement::Text(&[0xff]))
    );
}

fn nested(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0];
    for _ in 1..depth {
        let mut outer = vec![0x35, bytes.len() as u8];
        outer.extend(bytes);
        bytes = outer;
    }
    bytes
}

#[test]
fn complete_tree_validation_enforces_exact_depth_work_and_byte_bounds() {
    for depth in 1..=MAX_DEPTH {
        let bytes = nested(depth);
        let exact = Limits {
            max_depth: depth,
            max_elements: depth,
            max_bytes: bytes.len(),
        };
        assert!(DataElement::parse(&bytes, exact).is_ok());
        assert_eq!(
            DataElement::parse(
                &bytes,
                Limits {
                    max_elements: depth - 1,
                    ..exact
                }
            ),
            Err(ParseError::ElementLimit)
        );
        assert_eq!(
            DataElement::parse(
                &bytes,
                Limits {
                    max_bytes: bytes.len() - 1,
                    ..exact
                }
            ),
            Err(ParseError::ByteLimit)
        );
        if depth > 1 {
            assert_eq!(
                DataElement::parse(
                    &bytes,
                    Limits {
                        max_depth: depth - 1,
                        ..exact
                    }
                ),
                Err(ParseError::DepthLimit)
            );
        }
        for end in 0..bytes.len() {
            assert!(parse(&bytes[..end]).is_err());
        }
    }
    assert_eq!(parse(&nested(MAX_DEPTH + 1)), Err(ParseError::DepthLimit));
    for max_depth in [0, MAX_DEPTH + 1, usize::MAX] {
        assert_eq!(
            DataElement::parse(
                &[0],
                Limits {
                    max_depth,
                    ..LIMITS
                }
            ),
            Err(ParseError::InvalidLimits)
        );
    }
    // Width counts independently of nesting; an empty container itself counts.
    let siblings = [0x35, 5, 0, 0x35, 0, 0, 0];
    assert!(
        DataElement::parse(
            &siblings,
            Limits {
                max_elements: 5,
                ..LIMITS
            }
        )
        .is_ok()
    );
    assert_eq!(
        DataElement::parse(
            &siblings,
            Limits {
                max_elements: 4,
                ..LIMITS
            }
        ),
        Err(ParseError::ElementLimit)
    );
    // The outer length is valid but a child runs beyond its own container.
    assert_eq!(parse(&[0x35, 3, 0x35, 1, 0x09]), Err(ParseError::Length));
    assert_eq!(parse(&[0x35, 2, 0, 0xff]), Err(ParseError::InvalidTypeSize));
}

#[test]
fn child_iteration_is_typed_and_fused() {
    let DataElement::Sequence(mut children) =
        parse(&[0x35, 8, 0x19, 0x11, 0x0b, 0x3d, 3, 0x09, 0, 1]).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        children.next(),
        Some(Ok(DataElement::Uuid(Uuid::U16(0x110b))))
    );
    let DataElement::Alternative(mut alternatives) = children.next().unwrap().unwrap() else {
        panic!()
    };
    assert_eq!(alternatives.next(), Some(Ok(DataElement::Unsigned(1))));
    assert_eq!(alternatives.next(), None);
    assert_eq!(alternatives.next(), None);
    assert_eq!(children.next(), None);
    assert_eq!(children.next(), None);
}

#[test]
fn pdu_is_exact_big_endian_and_retains_unknown_ids() {
    let maximum = pdu(0xfe, &vec![0; 65_535]);
    let frame = Pdu::parse(&maximum).unwrap();
    assert_eq!(frame.id, 0xfe);
    assert_eq!(frame.transaction_id, 0x1234);
    assert_eq!(frame.parameters.len(), 65_535);
    for end in 0..maximum.len() {
        assert_eq!(Pdu::parse(&maximum[..end]), Err(ParseError::Length));
    }
    let mut extra = maximum;
    extra.push(0);
    assert_eq!(Pdu::parse(&extra), Err(ParseError::Length));
    assert!(
        Pdu::parse(&[7, 0, 0, 0, 0])
            .unwrap()
            .expect_transaction(0)
            .is_ok()
    );
    assert_eq!(
        Pdu::parse(&[7, 0, 0, 0, 0]).unwrap().expect_transaction(1),
        Err(ParseError::TransactionMismatch)
    );
    let mut fixed = [0; 12];
    for declared in 0..=u16::MAX {
        fixed[3..5].copy_from_slice(&declared.to_be_bytes());
        assert_eq!(Pdu::parse(&fixed).is_ok(), declared == 7);
    }
}

#[test]
fn continuation_validates_every_count_against_available_bytes() {
    let mut field = [0xa5; 19];
    for declared in 0..=255_u8 {
        field[0] = declared;
        for available in 0..=field.len() {
            let result = ContinuationState::parse(&field[..available]);
            assert_eq!(
                result.is_ok(),
                declared <= 16 && available == usize::from(declared) + 1
            );
            if let Ok(state) = result {
                assert_eq!(state.bytes(), &field[1..available]);
                assert_eq!(state.is_complete(), declared == 0);
            }
        }
    }
}

#[test]
fn response_checks_transaction_count_continuation_and_chunk_boundaries() {
    let packet = pdu(7, &[0, 2, 0x35, 0, 0]);
    let response = ServiceSearchAttributeResponse::parse(&packet, 0x1234, 7).unwrap();
    assert_eq!(response.attribute_lists, &[0x35, 0]);
    assert!(response.continuation.is_complete());
    for end in 0..packet.len() {
        assert!(ServiceSearchAttributeResponse::parse(&packet[..end], 0x1234, 7).is_err());
    }
    assert_eq!(
        ServiceSearchAttributeResponse::parse(&packet, 0x3412, 7),
        Err(ParseError::TransactionMismatch)
    );
    assert_eq!(
        ServiceSearchAttributeResponse::parse(&pdu(5, &[0, 2, 0x35, 0, 0]), 0x1234, 7),
        Err(ParseError::UnexpectedPdu)
    );
    assert_eq!(
        ServiceSearchAttributeResponse::parse(&packet, 0x1234, 6),
        Err(ParseError::AttributeByteCount)
    );
    assert_eq!(
        ServiceSearchAttributeResponse::parse(&pdu(7, &[0, 1, 0, 0]), 0x1234, 7),
        Err(ParseError::AttributeByteCount)
    );
    assert_eq!(
        ServiceSearchAttributeResponse::parse(&pdu(7, &[0, 8, 0, 0]), 0x1234, 7),
        Err(ParseError::AttributeByteCount)
    );
    for parameters in [
        &[][..],
        &[0][..],
        &[0, 2, 0][..],
        &[0, 2, 0x35, 0][..],
        &[0, 2, 0x35, 0, 1][..],
        &[0, 2, 0x35, 0, 0, 0][..],
        &[0, 2, 0x35, 0, 17][..],
    ] {
        assert!(ServiceSearchAttributeResponse::parse(&pdu(7, parameters), 0x1234, 7).is_err());
    }
    // Valid wire chunk ends inside a DE header/payload. It must not be parsed
    // independently as a complete DE or interpreted as successful discovery.
    let chunk = pdu(7, &[0, 2, 0x36, 0, 2, 0xaa, 0x55]);
    let partial = ServiceSearchAttributeResponse::parse(&chunk, 0x1234, 7).unwrap();
    assert_eq!(partial.attribute_lists, &[0x36, 0]);
    assert_eq!(partial.continuation.bytes(), &[0xaa, 0x55]);
    assert!(!partial.continuation.is_complete());
    assert_eq!(parse(partial.attribute_lists), Err(ParseError::Length));
}
