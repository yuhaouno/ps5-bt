// SPDX-License-Identifier: GPL-3.0-or-later
use bt_sdp::{
    A2dpSinkDiscovery, DiscoveryError, DiscoveryLimits, DiscoveryProgress, DiscoveryState,
    ParseError, SinkService,
};

const LIMITS: DiscoveryLimits = DiscoveryLimits {
    max_chunk_bytes: 64,
    max_total_bytes: 256,
    max_nodes: 64,
    max_depth: 8,
    max_chunks: 8,
};

fn valid_attributes() -> Vec<u8> {
    vec![
        0x35, 0x32, // all service records
        0x35, 0x30, // one service record
        0x09, 0x00, 0x01, 0x35, 0x03, 0x19, 0x11, 0x0b, // ServiceClassIDList
        0x09, 0x00, 0x04, 0x35, 0x10, // ProtocolDescriptorList
        0x35, 0x06, 0x19, 0x01, 0x00, 0x09, 0x00, 0x19, // L2CAP / AVDTP PSM
        0x35, 0x06, 0x19, 0x00, 0x19, 0x09, 0x01, 0x03, // AVDTP 1.3
        0x09, 0x00, 0x09, 0x35, 0x08, // BluetoothProfileDescriptorList
        0x35, 0x06, 0x19, 0x11, 0x0d, 0x09, 0x01, 0x03, // A2DP 1.3
        0x09, 0x03, 0x11, 0x09, 0x00, 0x01, // SupportedFeatures
    ]
}

fn response(transaction: u16, attributes: &[u8], continuation: &[u8]) -> Vec<u8> {
    let parameter_len = 2 + attributes.len() + 1 + continuation.len();
    let mut bytes = vec![0x07];
    bytes.extend_from_slice(&transaction.to_be_bytes());
    bytes.extend_from_slice(&(parameter_len as u16).to_be_bytes());
    bytes.extend_from_slice(&(attributes.len() as u16).to_be_bytes());
    bytes.extend_from_slice(attributes);
    bytes.push(continuation.len() as u8);
    bytes.extend_from_slice(continuation);
    bytes
}

fn client(storage: &mut [u8]) -> A2dpSinkDiscovery<'_> {
    A2dpSinkDiscovery::new(storage, LIMITS).unwrap()
}

#[test]
fn exact_request_selects_audio_sink_and_only_required_attributes() {
    let mut storage = [0xa5; 256];
    let mut discovery = client(&mut storage);
    let mut output = [0xcc; 64];
    let length = discovery.start(7, 0x1234, 100, &mut output).unwrap();
    assert_eq!(
        &output[..length],
        &[
            0x06, 0x12, 0x34, 0x00, 0x16, // PDU and parameter length
            0x35, 0x03, 0x19, 0x11, 0x0b, // Audio Sink search pattern
            0x00, 0x40, // MaximumAttributeByteCount
            0x35, 0x0c, // four individual, bounded attribute IDs
            0x09, 0x00, 0x01, 0x09, 0x00, 0x04, 0x09, 0x00, 0x09, 0x09, 0x03, 0x11,
            0x00, // no continuation
        ]
    );
    assert!(output[length..].iter().all(|byte| *byte == 0xcc));
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);

    let mut too_short = [0x5a; 26];
    discovery.cancel(7).unwrap();
    assert_eq!(
        discovery.start(7, 9, 100, &mut too_short),
        Err(DiscoveryError::OutputTooSmall)
    );
    assert!(too_short.iter().all(|byte| *byte == 0x5a));
    assert_eq!(discovery.state(), DiscoveryState::Cancelled);
}

#[test]
fn scripted_multi_response_discovers_sink_across_arbitrary_splits() {
    let attributes = valid_attributes();
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    discovery.start(4, 10, 500, &mut request).unwrap();

    let first = response(10, &attributes[..1], &[0xaa]);
    assert_eq!(
        discovery.accept_response(4, 1, &first, &mut []),
        Ok(DiscoveryProgress::ContinuationRequired {
            accumulated_bytes: 1,
            accepted_chunks: 1,
        })
    );
    let continued = discovery.continue_request(4, 11, &mut request).unwrap();
    assert_eq!(&request[1..3], &[0, 11]);
    assert_eq!(&request[continued - 2..continued], &[1, 0xaa]);

    let second = response(11, &attributes[1..19], &[0xbb, 0xcc]);
    assert!(matches!(
        discovery.accept_response(4, 2, &second, &mut []),
        Ok(DiscoveryProgress::ContinuationRequired { .. })
    ));
    discovery.continue_request(4, 12, &mut request).unwrap();
    let final_response = response(12, &attributes[19..], &[]);
    let mut scratch = [0; 256];
    assert_eq!(
        discovery.accept_response(4, 3, &final_response, &mut scratch),
        Ok(DiscoveryProgress::Complete(SinkService {
            avdtp_version: 0x0103,
            profile_version: 0x0103,
            supported_features: Some(1),
        }))
    );
    assert_eq!(discovery.state(), DiscoveryState::Complete);
    assert_eq!(discovery.accumulated(), attributes);
}

#[test]
fn continuation_requires_a_new_transaction_id_without_mutating_output() {
    let attributes = valid_attributes();
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0xa5; 64];
    discovery.start(4, 10, 500, &mut request).unwrap();
    discovery
        .accept_response(4, 1, &response(10, &attributes[..8], &[0xaa]), &mut [])
        .unwrap();
    request.fill(0x5a);
    assert_eq!(
        discovery.continue_request(4, 10, &mut request),
        Err(DiscoveryError::TransactionReuse)
    );
    assert_eq!(request, [0x5a; 64]);
    assert_eq!(discovery.state(), DiscoveryState::ContinuationReady);
}

#[test]
fn framing_truncation_and_stale_transaction_are_atomic() {
    let attributes = valid_attributes();
    let packet = response(21, &attributes, &[]);
    for end in 0..packet.len() {
        let mut storage = [0xa5; 256];
        let mut discovery = client(&mut storage);
        let mut request = [0; 64];
        let mut scratch = [0; 256];
        discovery.start(1, 21, 100, &mut request).unwrap();
        assert!(
            discovery
                .accept_response(1, 1, &packet[..end], &mut scratch)
                .is_err()
        );
        assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
        assert!(discovery.accumulated().is_empty());
    }

    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    let mut scratch = [0; 256];
    discovery.start(9, 21, 100, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(9, 1, &response(20, &attributes, &[]), &mut scratch),
        Err(DiscoveryError::Parse(ParseError::TransactionMismatch))
    );
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
    assert!(discovery.accumulated().is_empty());
}

#[test]
fn continuation_cycles_empty_progress_and_chunk_limits_fail_closed() {
    let attributes = valid_attributes();
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    discovery.start(2, 1, 100, &mut request).unwrap();
    discovery
        .accept_response(2, 1, &response(1, &attributes[..8], &[1]), &mut [])
        .unwrap();
    let accepted = discovery.accumulated().to_vec();
    discovery.continue_request(2, 2, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(2, 2, &response(2, &attributes[8..16], &[1]), &mut []),
        Err(DiscoveryError::ContinuationLoop)
    );
    assert_eq!(discovery.accumulated(), accepted);
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);

    assert_eq!(
        discovery.accept_response(2, 2, &response(2, &[], &[2]), &mut []),
        Err(DiscoveryError::EmptyContinuationChunk)
    );
    assert_eq!(discovery.accumulated(), accepted);

    let tight = DiscoveryLimits {
        max_chunks: 1,
        ..LIMITS
    };
    let mut tight_storage = [0; 256];
    let mut tight_client = A2dpSinkDiscovery::new(&mut tight_storage, tight).unwrap();
    tight_client.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        tight_client.accept_response(1, 1, &response(1, &[0x35], &[9]), &mut []),
        Err(DiscoveryError::ChunkLimit)
    );
    assert!(tight_client.accumulated().is_empty());
}

#[test]
fn all_seen_continuations_reject_longer_cycles() {
    let attributes = valid_attributes();
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    discovery.start(3, 1, 100, &mut request).unwrap();
    discovery
        .accept_response(3, 1, &response(1, &attributes[..2], &[0xa0]), &mut [])
        .unwrap();
    discovery.continue_request(3, 2, &mut request).unwrap();
    discovery
        .accept_response(3, 2, &response(2, &attributes[2..4], &[0xb0]), &mut [])
        .unwrap();
    discovery.continue_request(3, 3, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(3, 3, &response(3, &attributes[4..6], &[0xa0]), &mut []),
        Err(DiscoveryError::ContinuationLoop)
    );
    assert_eq!(discovery.accumulated(), &attributes[..4]);
}

#[test]
fn malicious_nesting_and_final_rejection_do_not_publish_or_append() {
    let mut nested = vec![0];
    for _ in 1..=8 {
        let mut outer = vec![0x35, nested.len() as u8];
        outer.extend(nested);
        nested = outer;
    }
    let mut storage = [0x5a; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    let mut scratch = [0; 256];
    discovery.start(1, 1, 100, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(1, 1, &response(1, &nested, &[]), &mut scratch),
        Err(DiscoveryError::Parse(ParseError::DepthLimit))
    );
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
    assert!(discovery.accumulated().is_empty());

    let attributes = valid_attributes();
    let first = response(1, &attributes[..10], &[1]);
    discovery.accept_response(1, 2, &first, &mut []).unwrap();
    discovery.continue_request(1, 2, &mut request).unwrap();
    let before = discovery.accumulated().to_vec();
    assert_eq!(
        discovery.accept_response(1, 3, &response(2, &attributes[10..], &[]), &mut [0; 8]),
        Err(DiscoveryError::ScratchTooSmall)
    );
    assert_eq!(discovery.accumulated(), before);
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
}

#[test]
fn total_byte_and_node_limits_reject_without_committing_final_bytes() {
    let attributes = valid_attributes();
    let mut request = [0; 64];
    let mut scratch = [0; 256];

    let mut byte_storage = [0xa5; 256];
    let mut byte_client = A2dpSinkDiscovery::new(
        &mut byte_storage,
        DiscoveryLimits {
            max_total_bytes: attributes.len() - 1,
            ..LIMITS
        },
    )
    .unwrap();
    byte_client.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        byte_client.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
        Err(DiscoveryError::TotalByteLimit)
    );
    assert!(byte_client.accumulated().is_empty());

    let mut node_storage = [0xa5; 256];
    let mut node_client = A2dpSinkDiscovery::new(
        &mut node_storage,
        DiscoveryLimits {
            max_nodes: 1,
            ..LIMITS
        },
    )
    .unwrap();
    node_client.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        node_client.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
        Err(DiscoveryError::Parse(ParseError::ElementLimit))
    );
    assert!(node_client.accumulated().is_empty());
}

fn assert_semantic_error(mut attributes: Vec<u8>, offset: usize, value: u8, error: DiscoveryError) {
    attributes[offset] = value;
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    let mut scratch = [0; 256];
    discovery.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
        Err(error)
    );
    assert!(discovery.accumulated().is_empty());
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
}

fn replace_u16_width(
    attributes: &mut Vec<u8>,
    offset: usize,
    value: u16,
    width: usize,
    length_offsets: &[usize],
) {
    let replacement = match width {
        1 => vec![0x08, value as u8],
        4 => vec![0x0a, 0, 0, (value >> 8) as u8, value as u8],
        _ => unreachable!(),
    };
    let delta = replacement.len() as i8 - 3;
    attributes.splice(offset..offset + 3, replacement);
    for length_offset in length_offsets {
        attributes[*length_offset] = attributes[*length_offset]
            .checked_add_signed(delta)
            .unwrap();
    }
}

#[test]
fn profile_uint16_fields_reject_equal_uint8_and_uint32_values() {
    let cases = [
        // Attribute ID, plus PSM, AVDTP version, profile version and features.
        (4, 0x0001, &[1, 3][..], DiscoveryError::InvalidAttributeList),
        (
            22,
            0x0019,
            &[1, 3, 16, 18][..],
            DiscoveryError::WrongProtocolDescriptorList,
        ),
        (
            30,
            0x0103,
            &[1, 3, 16, 26][..],
            DiscoveryError::WrongProtocolDescriptorList,
        ),
        (
            43,
            0x0103,
            &[1, 3, 37, 39][..],
            DiscoveryError::WrongProfileDescriptorList,
        ),
        (
            49,
            0x0001,
            &[1, 3][..],
            DiscoveryError::InvalidSupportedFeatures,
        ),
    ];
    for (offset, value, length_offsets, expected) in cases {
        for width in [1, 4] {
            let mut attributes = valid_attributes();
            replace_u16_width(&mut attributes, offset, value, width, length_offsets);
            let mut storage = [0xa5; 256];
            let mut discovery = client(&mut storage);
            let mut request = [0; 64];
            let mut scratch = [0; 256];
            discovery.start(1, 1, 10, &mut request).unwrap();
            assert_eq!(
                discovery.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
                Err(expected),
                "offset={offset} width={width}"
            );
            assert!(discovery.accumulated().is_empty());
            assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
        }
    }
}

#[test]
fn wrong_service_psm_and_profile_are_rejected_atomically() {
    // Last byte of Audio Sink UUID, L2CAP PSM and A2DP profile UUID.
    assert_semantic_error(
        valid_attributes(),
        11,
        0x0c,
        DiscoveryError::WrongServiceClass,
    );
    assert_semantic_error(
        valid_attributes(),
        24,
        0x17,
        DiscoveryError::WrongProtocolDescriptorList,
    );
    assert_semantic_error(
        valid_attributes(),
        42,
        0x0e,
        DiscoveryError::WrongProfileDescriptorList,
    );
}

#[test]
fn duplicate_attributes_are_rejected() {
    let valid = valid_attributes();
    let mut record_payload = valid[4..].to_vec();
    record_payload.extend_from_slice(&[0x09, 0x03, 0x11, 0x09, 0x00, 0x02]);
    let mut attributes = vec![
        0x35,
        (record_payload.len() + 2) as u8,
        0x35,
        record_payload.len() as u8,
    ];
    attributes.extend(record_payload);
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    let mut scratch = [0; 256];
    discovery.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
        Err(DiscoveryError::DuplicateAttribute(0x0311))
    );
    assert!(discovery.accumulated().is_empty());
}

#[test]
fn supported_features_is_optional() {
    let mut attributes = valid_attributes();
    attributes.truncate(attributes.len() - 6);
    attributes[1] = 0x2c;
    attributes[3] = 0x2a;
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    let mut scratch = [0; 256];
    discovery.start(1, 1, 10, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(1, 1, &response(1, &attributes, &[]), &mut scratch),
        Ok(DiscoveryProgress::Complete(SinkService {
            avdtp_version: 0x0103,
            profile_version: 0x0103,
            supported_features: None,
        }))
    );
}

#[test]
fn timeout_cancel_disconnect_and_epoch_reset_are_explicit() {
    let mut storage = [0; 256];
    let mut discovery = client(&mut storage);
    let mut request = [0; 64];
    discovery.start(5, 1, 10, &mut request).unwrap();
    assert!(!discovery.poll_timeout(5, 9).unwrap());
    assert!(discovery.poll_timeout(5, 10).unwrap());
    assert_eq!(discovery.state(), DiscoveryState::TimedOut);
    assert!(!discovery.poll_timeout(5, 11).unwrap());

    discovery.start(5, 2, 20, &mut request).unwrap();
    discovery.cancel(5).unwrap();
    assert_eq!(discovery.state(), DiscoveryState::Cancelled);
    discovery.start(5, 3, 20, &mut request).unwrap();
    discovery.disconnect(5).unwrap();
    assert_eq!(discovery.state(), DiscoveryState::Disconnected);
    assert_eq!(
        discovery.start(5, 4, 20, &mut request),
        Err(DiscoveryError::EpochNotAdvanced)
    );
    assert_eq!(
        discovery.reset_epoch(5),
        Err(DiscoveryError::EpochNotAdvanced)
    );
    discovery.reset_epoch(6).unwrap();
    assert_eq!(discovery.state(), DiscoveryState::Idle);
    assert_eq!(discovery.epoch(), Some(6));
    discovery.start(6, 4, 20, &mut request).unwrap();
    assert_eq!(
        discovery.accept_response(5, 1, &response(4, &valid_attributes(), &[]), &mut [0; 256]),
        Err(DiscoveryError::EpochMismatch)
    );
    assert_eq!(discovery.state(), DiscoveryState::AwaitingResponse);
    assert_eq!(
        discovery.accept_response(6, 20, &response(4, &valid_attributes(), &[]), &mut [0; 256]),
        Err(DiscoveryError::DeadlineExpired)
    );
    assert_eq!(discovery.state(), DiscoveryState::TimedOut);
}

#[test]
fn constructor_rejects_every_unbounded_or_impossible_policy() {
    let mut storage = [0; 256];
    for bad in [
        DiscoveryLimits {
            max_chunk_bytes: 6,
            ..LIMITS
        },
        DiscoveryLimits {
            max_total_bytes: 0,
            ..LIMITS
        },
        DiscoveryLimits {
            max_total_bytes: 257,
            ..LIMITS
        },
        DiscoveryLimits {
            max_nodes: 0,
            ..LIMITS
        },
        DiscoveryLimits {
            max_depth: 0,
            ..LIMITS
        },
        DiscoveryLimits {
            max_depth: 17,
            ..LIMITS
        },
        DiscoveryLimits {
            max_chunks: 0,
            ..LIMITS
        },
        DiscoveryLimits {
            max_chunks: 17,
            ..LIMITS
        },
    ] {
        assert!(matches!(
            A2dpSinkDiscovery::new(&mut storage, bad),
            Err(DiscoveryError::InvalidLimits)
        ));
    }
}
