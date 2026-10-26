// SPDX-License-Identifier: GPL-3.0-or-later
use bt_link::{CompletionResult, Config, Event, LinkSession, Peer, WireError};

fn packet(code: u8, p: &[u8]) -> Vec<u8> {
    let mut bytes = vec![code, p.len() as u8];
    bytes.extend_from_slice(p);
    bytes
}
#[test]
fn all_supported_event_shapes_reject_every_truncation_and_trailing_octet() {
    let shapes: &[(u8, &[u8])] = &[
        (0x0f, &[0, 1, 5, 4]),
        (0x0e, &[1, 8, 4, 0, 1, 2, 3, 4, 5, 6]),
        (0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]),
        (3, &[0, 0x42, 0, 1, 2, 3, 4, 5, 6, 1, 0]),
        (5, &[0, 0x42, 0, 0x16]),
        (6, &[0, 0x42, 0]),
        (8, &[0, 0x42, 0, 1]),
        (0x59, &[0, 0x42, 0, 2, 16]),
        (0x16, &[1, 2, 3, 4, 5, 6]),
        (0x17, &[1, 2, 3, 4, 5, 6]),
        (
            0x18,
            &[
                1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4,
            ],
        ),
        (0x31, &[1, 2, 3, 4, 5, 6]),
        (0x32, &[1, 2, 3, 4, 5, 6, 3, 0, 4]),
        (0x33, &[1, 2, 3, 4, 5, 6, 0x3f, 0x42, 0x0f, 0]),
        (0x34, &[1, 2, 3, 4, 5, 6]),
        (0x35, &[1, 2, 3, 4, 5, 6]),
        (0x36, &[0, 1, 2, 3, 4, 5, 6]),
    ];
    for (code, p) in shapes {
        let bytes = packet(*code, p);
        assert!(Event::parse(&bytes).is_ok());
        for len in 0..bytes.len() {
            assert!(Event::parse(&bytes[..len]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Event::parse(&trailing).is_err());
        // Correct outer framing is insufficient: command-specific payload is exact.
        let mut short = bytes.clone();
        short.pop();
        short[1] -= 1;
        assert!(Event::parse(&short).is_err());
        trailing[1] += 1;
        assert!(Event::parse(&trailing).is_err());
    }
}
#[test]
fn reserved_fields_numeric_limits_and_handle_range_are_checked() {
    for value in [1_000_000u32, u32::MAX] {
        let mut p = vec![1, 2, 3, 4, 5, 6];
        p.extend_from_slice(&value.to_le_bytes());
        assert!(matches!(
            Event::parse(&packet(0x33, &p)),
            Err(WireError::InvalidField)
        ));
    }
    assert!(Event::parse(&packet(3, &[0, 0x42, 0, 1, 2, 3, 4, 5, 6, 0, 0])).is_err());
    assert!(Event::parse(&packet(6, &[0, 0, 0x0f])).is_err());
    assert!(Event::parse(&packet(8, &[0, 0x42, 0, 3])).is_err());
    assert!(Event::parse(&packet(0x59, &[0, 0x42, 0, 1, 0])).is_err());
    assert!(Event::parse(&packet(0x59, &[0, 0x42, 0, 1, 17])).is_err());
    for (field, value) in [(6, 4), (7, 2), (8, 6)] {
        let mut p = [1, 2, 3, 4, 5, 6, 3, 0, 4];
        p[field] = value;
        assert!(Event::parse(&packet(0x32, &p)).is_err());
    }
}
#[test]
fn failed_connection_and_key_size_do_not_decode_undefined_fields() {
    assert!(matches!(
        Event::parse(&packet(3, &[4, 0xff, 0xff, 1, 2, 3, 4, 5, 6, 0xff, 0xff])).unwrap(),
        Event::ConnectionComplete {
            status: 4,
            handle: None,
            encrypted: false,
            ..
        }
    ));
    assert!(matches!(
        Event::parse(&packet(0x0e, &[1, 8, 0x14, 0x2f, 0x42, 0, 0xff])).unwrap(),
        Event::CommandComplete {
            status: 0x2f,
            result: CompletionResult::EncryptionKeySize { size: None, .. },
            ..
        }
    ));
}
#[test]
fn unknown_events_do_not_become_successful_drops() {
    assert!(matches!(
        Event::parse(&packet(0xff, &[])),
        Err(WireError::UnsupportedEvent(0xff))
    ));
    assert!(matches!(
        Event::parse(&packet(0x0e, &[1, 1, 0x10, 0])),
        Err(WireError::UnsupportedEvent(0x0e))
    ));
}
#[test]
fn peer_debug_is_redacted_and_config_rejects_reserved_or_unbounded_values() {
    let peer = Peer::from_hci_bytes([1, 2, 3, 4, 5, 6]);
    assert_eq!(format!("{peer:?}"), "Peer(REDACTED)");
    let good = Config {
        packet_types: 0x3306,
        page_scan_repetition: 1,
        allow_role_switch: false,
        connection_timeout_us: 1,
        security_timeout_us: 1,
        cleanup_timeout_us: 1,
        minimum_key_size: 16,
    };
    for c in [
        Config {
            packet_types: 1,
            ..good
        },
        Config {
            page_scan_repetition: 3,
            ..good
        },
        Config {
            connection_timeout_us: 0,
            ..good
        },
        Config {
            minimum_key_size: 6,
            ..good
        },
        Config {
            minimum_key_size: 17,
            ..good
        },
    ] {
        assert!(LinkSession::new(peer, 1, 0, c).is_err());
    }
    assert!(LinkSession::new(peer, 0, 0, good).is_err());
    assert!(LinkSession::new(peer, 1, u64::MAX, good).is_err());
}

#[test]
fn failed_key_size_retains_mandatory_handle_and_optional_trailer_is_not_interpreted() {
    for p in [
        &[1, 8, 0x14, 0x0c, 0x42, 0][..],
        &[1, 8, 0x14, 0x0c, 0x42, 0, 0xff][..],
    ] {
        assert!(
            matches!(Event::parse(&packet(0x0e, p)).unwrap(), Event::CommandComplete {
            status: 0x0c, result: CompletionResult::EncryptionKeySize { handle, size: None }, ..
        } if handle.raw() == 0x42)
        );
    }
    for p in [
        &[1, 8, 0x14, 0x0c][..],
        &[1, 8, 0x14, 0x0c, 0x42][..],
        &[1, 8, 0x14, 0x0c, 0, 0x0f][..],
        &[1, 8, 0x14, 0, 0x42, 0][..],
    ] {
        assert!(Event::parse(&packet(0x0e, p)).is_err());
    }
    assert!(matches!(
        Event::parse(&packet(0x0e, &[1, 0x0b, 4, 5, 1, 2, 3, 4, 5, 6])).unwrap(),
        Event::CommandComplete {
            status: 5,
            result: CompletionResult::Peer(_),
            ..
        }
    ));
    // A known normally-Complete command retains its routing field even for error 1.
    assert!(Event::parse(&packet(0x0e, &[1, 0x0b, 4, 1])).is_err());
    assert!(Event::parse(&packet(0x0e, &[1, 8, 0x14, 1])).is_err());
}
