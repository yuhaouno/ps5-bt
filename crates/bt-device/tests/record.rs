// SPDX-License-Identifier: GPL-3.0-or-later
use bt_device::{BOND_RECORD_BYTES, BondPolicy, BondRecord, KeyType, RecordError};
use bt_link::{Peer, SecretKey};
fn peer() -> Peer {
    Peer::from_hci_bytes([1, 2, 3, 4, 5, 6])
}
fn policy() -> BondPolicy {
    BondPolicy {
        minimum_key_size: 16,
        require_mitm: false,
        require_secure_connections: false,
    }
}
fn record() -> BondRecord {
    BondRecord::new(
        peer(),
        SecretKey::new([0x44; 16]),
        KeyType::AuthenticatedP256,
        16,
    )
    .unwrap()
}
fn bytes() -> [u8; BOND_RECORD_BYTES] {
    let mut bytes = [0; BOND_RECORD_BYTES];
    record().encode(&mut bytes).unwrap();
    bytes
}
#[test]
fn versioned_record_round_trip_and_short_output_are_exact() {
    let bytes = bytes();
    assert_eq!(
        &bytes[..16],
        b"P5BD\x01\x03\x08\x10\x01\x02\x03\x04\x05\x06\x00\x00"
    );
    let parsed = BondRecord::decode(&bytes, peer(), policy()).unwrap();
    assert_eq!(parsed.peer(), peer());
    assert_eq!(parsed.key_type(), KeyType::AuthenticatedP256);
    let mut out = [0xa5; 31];
    assert_eq!(record().encode(&mut out), Err(RecordError::Length));
    assert_eq!(out, [0xa5; 31]);
    for len in 0..bytes.len() {
        assert!(BondRecord::decode(&bytes[..len], peer(), policy()).is_err());
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(BondRecord::decode(&trailing, peer(), policy()).is_err());
}
#[test]
fn metadata_rejects_wrong_peer_version_flags_reserved_and_unsupported_keys() {
    for (index, value, error) in [
        (0, 0, RecordError::Magic),
        (4, 2, RecordError::Version(2)),
        (5, 1, RecordError::Flags),
        (14, 1, RecordError::Reserved),
        (8, 9, RecordError::PeerMismatch),
        (6, 3, RecordError::UnsupportedKeyType(3)),
        (7, 6, RecordError::KeySize(6)),
        (7, 17, RecordError::KeySize(17)),
    ] {
        let mut data = bytes();
        data[index] = value;
        assert!(matches!(BondRecord::decode(&data,peer(),policy()),Err(e) if e==error));
    }
    for kind in 0..=u8::MAX {
        let mut data = bytes();
        data[6] = kind;
        assert_eq!(
            BondRecord::decode(&data, peer(), policy()).is_ok(),
            matches!(kind, 4 | 5 | 7 | 8)
        );
    }
}
#[test]
fn security_policy_is_explicit_and_rejects_downgrade() {
    let mut out = [0; BOND_RECORD_BYTES];
    BondRecord::new(
        peer(),
        SecretKey::new([0x55; 16]),
        KeyType::UnauthenticatedP192,
        7,
    )
    .unwrap()
    .encode(&mut out)
    .unwrap();
    assert!(matches!(
        BondRecord::decode(&out, peer(), policy()),
        Err(RecordError::KeySize(7))
    ));
    let weak = BondPolicy {
        minimum_key_size: 7,
        ..policy()
    };
    assert!(BondRecord::decode(&out, peer(), weak).is_ok());
    assert!(matches!(
        BondRecord::decode(
            &out,
            peer(),
            BondPolicy {
                require_mitm: true,
                ..weak
            }
        ),
        Err(RecordError::MitmRequired)
    ));
    assert!(matches!(
        BondRecord::decode(
            &out,
            peer(),
            BondPolicy {
                require_secure_connections: true,
                ..weak
            }
        ),
        Err(RecordError::SecureConnectionsRequired)
    ));
    assert!(
        BondPolicy {
            minimum_key_size: 0,
            ..policy()
        }
        .validate()
        .is_err()
    );
}
