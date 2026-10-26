// SPDX-License-Identifier: GPL-3.0-or-later
use bt_discovery::{Event, NAME_CAPACITY, Observation, WireError};
fn event(code: u8, p: &[u8]) -> Vec<u8> {
    let mut out = vec![code, p.len() as u8];
    out.extend_from_slice(p);
    out
}
fn standard(seed: u8) -> [u8; 14] {
    [seed, 2, 3, 4, 5, 6, 2, 0, 0, 0x04, 0x24, 0, 0x34, 0x12]
}
fn rssi(seed: u8, value: i8) -> [u8; 14] {
    [
        seed,
        2,
        3,
        4,
        5,
        6,
        1,
        0,
        0x04,
        0x24,
        0,
        0x34,
        0x12,
        value as u8,
    ]
}
fn packet(code: u8, records: &[[u8; 14]]) -> Vec<u8> {
    let mut p = vec![records.len() as u8];
    for r in records {
        p.extend_from_slice(r);
    }
    event(code, &p)
}
fn extended(eir: &[u8; 240]) -> Vec<u8> {
    let mut p = vec![1];
    p.extend_from_slice(&rssi(1, -43));
    p.extend_from_slice(eir);
    event(0x2f, &p)
}
fn observations(bytes: &[u8]) -> Vec<Observation> {
    match Event::parse(bytes).unwrap() {
        Event::Results(r) => r.iter().collect(),
        _ => panic!(),
    }
}
#[test]
fn interleaved_standard_and_rssi_entries_have_exact_distinct_layouts() {
    for code in [2, 0x22] {
        let data = if code == 2 {
            packet(code, &[standard(1), standard(9)])
        } else {
            packet(code, &[rssi(1, -43), rssi(9, 20)])
        };
        let found = observations(&data);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].peer.hci_bytes(), [1, 2, 3, 4, 5, 6]);
        assert_eq!(found[1].peer.hci_bytes(), [9, 2, 3, 4, 5, 6]);
        assert_eq!(found[0].class_of_device, [4, 0x24, 0]);
        assert_eq!(found[0].clock_offset, 0x1234);
        assert_eq!(found[1].clock_offset, 0x1234);
        assert_eq!(found[0].rssi_dbm, if code == 2 { None } else { Some(-43) });
        assert_eq!(found[1].rssi_dbm, if code == 2 { None } else { Some(20) });
    }
    let mut old = standard(1);
    old[7] = 0xff;
    old[8] = 0xff;
    old[13] |= 0x80;
    let found = observations(&packet(2, &[old]));
    assert_eq!(found[0].clock_offset, 0x1234);
}
#[test]
fn extended_result_decodes_only_validated_name_not_unknown_oob_data() {
    let mut eir = [0; 240];
    eir[..9].copy_from_slice(&[6, 9, b'H', b'e', b'l', b'l', b'o', 1, 0x0e]);
    let found = observations(&extended(&eir));
    let name = found[0].name.unwrap();
    assert_eq!(name.as_str(), "Hello");
    assert!(name.source_complete());
    assert!(!name.truncated());
    assert_eq!(found[0].rssi_dbm, Some(-43));
    let found = observations(&extended(&[0; 240]));
    assert!(found[0].name.is_none());
    let mut empty = [0; 240];
    empty[..2].copy_from_slice(&[1, 9]);
    assert_eq!(
        observations(&extended(&empty))[0].name.unwrap().as_str(),
        ""
    );
}
#[test]
fn name_storage_truncates_only_on_utf8_boundary_with_explicit_metadata() {
    let text = "é".repeat(40);
    let mut eir = [0; 240];
    eir[0] = (text.len() + 1) as u8;
    eir[1] = 8;
    eir[2..2 + text.len()].copy_from_slice(text.as_bytes());
    let name = observations(&extended(&eir))[0].name.unwrap();
    assert_eq!(name.as_str().len(), NAME_CAPACITY);
    assert!(name.truncated());
    assert!(!name.source_complete());
    assert_eq!(name.as_str(), "é".repeat(NAME_CAPACITY / 2));
    // Crossing a three-byte character at byte64 retains a shorter valid prefix.
    let text = "界".repeat(30);
    let mut eir = [0; 240];
    eir[0] = (text.len() + 1) as u8;
    eir[1] = 9;
    eir[2..2 + text.len()].copy_from_slice(text.as_bytes());
    let name = observations(&extended(&eir))[0].name.unwrap();
    assert_eq!(name.as_str().len(), 63);
    assert!(name.truncated());
    assert!(name.source_complete());
}
#[test]
fn malformed_eir_utf8_controls_duplicate_name_and_nonzero_padding_are_rejected() {
    for data in [
        vec![2, 9, 0xff],
        vec![2, 9, b'\n'],
        vec![2, 8, b'A', 2, 9, b'B'],
        vec![0, 1],
    ] {
        let mut eir = [0; 240];
        eir[..data.len()].copy_from_slice(&data);
        assert!(Event::parse(&extended(&eir)).is_err());
    }
    let mut eir = [0; 240];
    eir[0] = 240;
    assert!(matches!(
        Event::parse(&extended(&eir)),
        Err(WireError::InvalidEir)
    ));
    let mut eir = [0; 240];
    eir[0] = 239;
    eir[1] = 9;
    eir[2..].fill(b'A');
    let name = observations(&extended(&eir))[0].name.unwrap();
    assert_eq!(name.as_str().len(), 64);
    assert!(name.truncated());
}
#[test]
fn every_result_packet_truncation_trailing_byte_and_invalid_suffix_is_atomic() {
    for bytes in [
        packet(2, &[standard(1), standard(9)]),
        packet(0x22, &[rssi(1, -1), rssi(9, -127)]),
        extended(&[0; 240]),
    ] {
        assert!(Event::parse(&bytes).is_ok());
        for len in 0..bytes.len() {
            assert!(Event::parse(&bytes[..len]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Event::parse(&trailing).is_err());
        let mut short = bytes.clone();
        short.pop();
        short[1] -= 1;
        assert!(Event::parse(&short).is_err());
    }
    for code in [2, 0x22, 0x2f] {
        assert!(matches!(
            Event::parse(&event(code, &[0])),
            Err(WireError::InvalidCount)
        ));
    }
    let mut bytes = extended(&[0; 240]);
    bytes[2] = 2;
    assert!(matches!(Event::parse(&bytes), Err(WireError::InvalidCount)));
    let mut second = standard(9);
    second[6] = 3;
    assert!(matches!(
        Event::parse(&packet(2, &[standard(1), second])),
        Err(WireError::InvalidPageScanRepetition)
    ));
    for value in [-128i8, 21, 127] {
        assert!(matches!(
            Event::parse(&packet(0x22, &[rssi(1, value)])),
            Err(WireError::InvalidRssi)
        ));
    }
}
#[test]
fn maximum_hci_event_uses_eighteen_interleaved_standard_results() {
    let records: Vec<_> = (0..18).map(standard).collect();
    let bytes = packet(2, &records);
    assert_eq!(bytes.len(), 255);
    let values = observations(&bytes);
    assert_eq!(values.len(), 18);
    for (i, r) in values.iter().enumerate() {
        assert_eq!(r.peer.hci_bytes()[0], i as u8);
    }
    // A historical15-byte RSSI entry is not guessed to be the Core6 14-byte layout.
    let mut p = vec![1];
    p.extend_from_slice(&[0; 15]);
    assert!(Event::parse(&event(0x22, &p)).is_err());
}
#[test]
fn foreign_frames_and_opcode_completions_are_returned_for_dispatch() {
    assert!(matches!(
        Event::parse(&event(0xff, &[])),
        Err(WireError::ForeignEvent(0xff))
    ));
    assert!(matches!(
        Event::parse(&event(0x0e, &[1, 1, 0x10, 0, 1, 2, 3, 4, 5, 6])),
        Err(WireError::ForeignOpcode(0x1001))
    ));
    assert!(matches!(
        Event::parse(&event(0x0e, &[1, 0, 0])),
        Err(WireError::ForeignOpcode(0))
    ));
    assert!(Event::parse(&event(0x0e, &[1, 2, 4])).is_err());
    assert!(Event::parse(&event(0x0f, &[0, 1, 1, 4, 0])).is_err());
}
