// SPDX-License-Identifier: GPL-3.0-or-later
use bt_link::{Peer, SecretKey};

pub const BOND_RECORD_VERSION: u8 = 1;
pub const BOND_RECORD_BYTES: usize = 32;
const MAGIC: &[u8; 4] = b"P5BD";
const AUTHENTICATED_ENCRYPTED: u8 = 3;

/// Supported SSP key types, not interchangeable with link authentication status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyType {
    UnauthenticatedP192 = 4,
    AuthenticatedP192 = 5,
    UnauthenticatedP256 = 7,
    AuthenticatedP256 = 8,
}
impl KeyType {
    pub fn from_raw(raw: u8) -> Result<Self, RecordError> {
        match raw {
            4 => Ok(Self::UnauthenticatedP192),
            5 => Ok(Self::AuthenticatedP192),
            7 => Ok(Self::UnauthenticatedP256),
            8 => Ok(Self::AuthenticatedP256),
            _ => Err(RecordError::UnsupportedKeyType(raw)),
        }
    }
    pub const fn raw(self) -> u8 {
        self as u8
    }
    pub const fn authenticated(self) -> bool {
        matches!(self, Self::AuthenticatedP192 | Self::AuthenticatedP256)
    }
    pub const fn secure_connections(self) -> bool {
        matches!(self, Self::UnauthenticatedP256 | Self::AuthenticatedP256)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    Length,
    Magic,
    Version(u8),
    Flags,
    Reserved,
    PeerMismatch,
    UnsupportedKeyType(u8),
    KeySize(u8),
    MitmRequired,
    SecureConnectionsRequired,
    InvalidPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BondPolicy {
    pub minimum_key_size: u8,
    pub require_mitm: bool,
    pub require_secure_connections: bool,
}
impl BondPolicy {
    pub fn validate(self) -> Result<(), RecordError> {
        if !(7..=16).contains(&self.minimum_key_size) {
            return Err(RecordError::InvalidPolicy);
        }
        Ok(())
    }
    fn check(self, key_type: KeyType, verified_minimum: u8) -> Result<(), RecordError> {
        self.validate()?;
        if !(7..=16).contains(&verified_minimum) || verified_minimum < self.minimum_key_size {
            return Err(RecordError::KeySize(verified_minimum));
        }
        if self.require_mitm && !key_type.authenticated() {
            return Err(RecordError::MitmRequired);
        }
        if self.require_secure_connections && !key_type.secure_connections() {
            return Err(RecordError::SecureConnectionsRequired);
        }
        Ok(())
    }
}

/// No Debug/Clone: owns secret key material. Metadata is a protected store's
/// declaration of previously observed security; parsing is not authentication.
///
/// ```compile_fail
/// fn log_record(record: &bt_device::BondRecord) {
///     println!("{record:?}"); // Secret-bearing records must not acquire Debug.
/// }
/// ```
pub struct BondRecord {
    peer: Peer,
    key: SecretKey,
    key_type: KeyType,
    verified_minimum: u8,
}
impl BondRecord {
    /// For reviewed stores/importers. The manager constructs fresh records only
    /// from a real link notification and commits them after that link's Ready.
    pub fn new(
        peer: Peer,
        key: SecretKey,
        key_type: KeyType,
        verified_minimum: u8,
    ) -> Result<Self, RecordError> {
        BondPolicy {
            minimum_key_size: 7,
            require_mitm: false,
            require_secure_connections: false,
        }
        .check(key_type, verified_minimum)?;
        Ok(Self {
            peer,
            key,
            key_type,
            verified_minimum,
        })
    }
    pub const fn peer(&self) -> Peer {
        self.peer
    }
    pub const fn key_type(&self) -> KeyType {
        self.key_type
    }
    /// Lower bound actually checked, not necessarily the exact negotiated size.
    pub const fn verified_minimum(&self) -> u8 {
        self.verified_minimum
    }
    pub fn check_policy(&self, policy: BondPolicy) -> Result<(), RecordError> {
        policy.check(self.key_type, self.verified_minimum)
    }
    pub(crate) fn into_key(self) -> SecretKey {
        self.key
    }

    /// Protected local storage bytes. Not a network format and not encrypted.
    pub fn encode(&self, output: &mut [u8]) -> Result<usize, RecordError> {
        if output.len() < BOND_RECORD_BYTES {
            return Err(RecordError::Length);
        }
        output[..4].copy_from_slice(MAGIC);
        output[4] = BOND_RECORD_VERSION;
        output[5] = AUTHENTICATED_ENCRYPTED;
        output[6] = self.key_type.raw();
        output[7] = self.verified_minimum;
        output[8..14].copy_from_slice(&self.peer.hci_bytes());
        output[14..16].fill(0);
        output[16..32].copy_from_slice(self.key.expose());
        Ok(BOND_RECORD_BYTES)
    }
    pub fn decode(
        bytes: &[u8],
        expected_peer: Peer,
        policy: BondPolicy,
    ) -> Result<Self, RecordError> {
        if bytes.len() != BOND_RECORD_BYTES {
            return Err(RecordError::Length);
        }
        if &bytes[..4] != MAGIC {
            return Err(RecordError::Magic);
        }
        if bytes[4] != BOND_RECORD_VERSION {
            return Err(RecordError::Version(bytes[4]));
        }
        if bytes[5] != AUTHENTICATED_ENCRYPTED {
            return Err(RecordError::Flags);
        }
        if bytes[14..16] != [0, 0] {
            return Err(RecordError::Reserved);
        }
        if bytes[8..14] != expected_peer.hci_bytes() {
            return Err(RecordError::PeerMismatch);
        }
        let key_type = KeyType::from_raw(bytes[6])?;
        policy.check(key_type, bytes[7])?;
        let mut key = [0; 16];
        key.copy_from_slice(&bytes[16..32]);
        Ok(Self {
            peer: expected_peer,
            key: SecretKey::new(key),
            key_type,
            verified_minimum: bytes[7],
        })
    }
}
