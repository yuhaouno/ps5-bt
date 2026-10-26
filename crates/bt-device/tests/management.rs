// SPDX-License-Identifier: GPL-3.0-or-later
use bt_device::{
    ApprovalToken, BOND_RECORD_BYTES, BondPolicy, BondRecord, BondStore, DeviceManager,
    DeviceNotice, Error, KeyType, Persistence, RecordError, StorageFailure, StoreError,
    StoreOperation,
};
use bt_link::{Config, Peer, SecretKey, State};
use std::{cell::RefCell, rc::Rc};

const PEER: [u8; 6] = [1, 2, 3, 4, 5, 6];
fn peer() -> Peer {
    Peer::from_hci_bytes(PEER)
}
fn policy() -> BondPolicy {
    BondPolicy {
        minimum_key_size: 16,
        require_mitm: false,
        require_secure_connections: false,
    }
}
fn config() -> Config {
    Config {
        packet_types: 0x3306,
        page_scan_repetition: 1,
        allow_role_switch: false,
        connection_timeout_us: 1000,
        security_timeout_us: 1000,
        cleanup_timeout_us: 100,
        minimum_key_size: 16,
    }
}
#[derive(Default)]
struct Memory {
    record: Option<[u8; BOND_RECORD_BYTES]>,
    loads: usize,
    saves: usize,
    deletes: usize,
    load_error: Option<StoreError>,
    save_error: Option<StoreError>,
    delete_error: Option<StoreError>,
    claimed_length: Option<usize>,
}
#[derive(Clone, Default)]
struct Store(Rc<RefCell<Memory>>);
impl BondStore for Store {
    fn load(
        &mut self,
        _peer: Peer,
        output: &mut [u8; BOND_RECORD_BYTES],
    ) -> Result<Option<usize>, StoreError> {
        let mut m = self.0.borrow_mut();
        m.loads += 1;
        if let Some(error) = m.load_error {
            output.fill(0x44);
            return Err(error);
        }
        match m.record {
            Some(bytes) => {
                output.copy_from_slice(&bytes);
                Ok(Some(m.claimed_length.unwrap_or(BOND_RECORD_BYTES)))
            }
            None => {
                output.fill(0x55);
                Ok(None)
            }
        }
    }
    fn save_atomic(
        &mut self,
        _peer: Peer,
        bytes: &[u8; BOND_RECORD_BYTES],
    ) -> Result<(), StoreError> {
        let mut m = self.0.borrow_mut();
        m.saves += 1;
        if let Some(error) = m.save_error {
            return Err(error);
        }
        m.record = Some(*bytes);
        Ok(())
    }
    fn delete_atomic(&mut self, _peer: Peer) -> Result<(), StoreError> {
        let mut m = self.0.borrow_mut();
        m.deletes += 1;
        if let Some(error) = m.delete_error {
            return Err(error);
        }
        m.record = None;
        Ok(())
    }
}
fn store_with_bond() -> Store {
    let store = Store::default();
    let mut bytes = [0; BOND_RECORD_BYTES];
    BondRecord::new(
        peer(),
        SecretKey::new([0x44; 16]),
        KeyType::UnauthenticatedP192,
        16,
    )
    .unwrap()
    .encode(&mut bytes)
    .unwrap();
    store.0.borrow_mut().record = Some(bytes);
    store
}
fn manager(store: Store) -> DeviceManager<Store> {
    let mut m = DeviceManager::new(store, policy()).unwrap();
    m.select(peer()).unwrap();
    m.start(1, 0, config()).unwrap();
    m
}
fn event(code: u8, p: &[u8]) -> Vec<u8> {
    let mut bytes = vec![code, p.len() as u8];
    bytes.extend_from_slice(p);
    bytes
}
fn status(opcode: u16) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    event(0x0f, &[0, 1, lo, hi])
}
fn complete(opcode: u16) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    let mut p = vec![1, lo, hi, 0];
    p.extend_from_slice(&PEER);
    event(0x0e, &p)
}
fn accept(m: &mut DeviceManager<Store>, now: u64, opcode: u16) -> Vec<u8> {
    let mut out = [0; 25];
    let ticket = m.prepare_command(now, &mut out).unwrap().unwrap();
    assert_eq!(ticket.opcode(), opcode);
    let len = m.command_len().unwrap();
    m.confirm_accepted(ticket).unwrap();
    out[..len].to_vec()
}
fn begin(m: &mut DeviceManager<Store>, found: bool) {
    accept(m, 0, 0x0405);
    m.receive(1, 1, &status(0x0405)).unwrap();
    let mut p = vec![0, 0x42, 0];
    p.extend_from_slice(&PEER);
    p.extend_from_slice(&[1, 0]);
    m.receive(1, 2, &event(3, &p)).unwrap();
    accept(m, 3, 0x0411);
    m.receive(1, 4, &status(0x0411)).unwrap();
    assert_eq!(
        m.receive(1, 5, &event(0x17, &PEER)),
        Ok(DeviceNotice::KeyLookup { found })
    );
    let opcode = if found { 0x040b } else { 0x040c };
    accept(m, 6, opcode);
    m.receive(1, 7, &complete(opcode)).unwrap();
}
fn confirmation(m: &mut DeviceManager<Store>) -> ApprovalToken {
    m.receive(1, 8, &event(0x31, &PEER)).unwrap();
    accept(m, 9, 0x042b);
    m.receive(1, 10, &complete(0x042b)).unwrap();
    let mut p = PEER.to_vec();
    p.extend_from_slice(&123_456u32.to_le_bytes());
    match m.receive(1, 11, &event(0x33, &p)).unwrap() {
        DeviceNotice::NeedConfirmation {
            token,
            numeric_value: 123_456,
        } => token,
        _ => panic!(),
    }
}
fn stage(m: &mut DeviceManager<Store>, key_type: u8) -> Result<DeviceNotice, Error> {
    let token = confirmation(m);
    m.approve(token, 12, true).unwrap();
    accept(m, 12, 0x042c);
    m.receive(1, 13, &complete(0x042c)).unwrap();
    let mut p = vec![0];
    p.extend_from_slice(&PEER);
    m.receive(1, 14, &event(0x36, &p)).unwrap();
    let mut p = PEER.to_vec();
    p.extend_from_slice(&[0x77; 16]);
    p.push(key_type);
    m.receive(1, 15, &event(0x18, &p))
}
fn ready(m: &mut DeviceManager<Store>) -> Result<DeviceNotice, Error> {
    m.receive(1, 16, &event(6, &[0, 0x42, 0])).unwrap();
    accept(m, 17, 0x0413);
    m.receive(1, 18, &status(0x0413)).unwrap();
    m.receive(1, 19, &event(8, &[0, 0x42, 0, 1])).unwrap();
    accept(m, 20, 0x1408);
    m.receive(1, 21, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
}
fn close(m: &mut DeviceManager<Store>, now: u64) {
    accept(m, now, 0x0406);
    m.receive(1, now + 1, &status(0x0406)).unwrap();
    m.receive(1, now + 2, &event(5, &[0, 0x42, 0, 0x16]))
        .unwrap();
    assert_eq!(m.link().unwrap().state(), State::Closed);
}

#[test]
fn stored_reconnect_validates_and_supplies_selected_record_without_resaving() {
    let store = store_with_bond();
    let mut m = manager(store.clone());
    begin(&mut m, true);
    assert_eq!(
        ready(&mut m),
        Ok(DeviceNotice::Ready {
            saved_new_bond: false
        })
    );
    assert_eq!(m.ready_connection().unwrap().1, 1);
    assert_eq!(m.status().persistence, Persistence::Loaded);
    assert_eq!(store.0.borrow().loads, 1);
    assert_eq!(store.0.borrow().saves, 0);
}
#[test]
fn fresh_bond_is_saved_only_after_owned_authenticated_ready() {
    let store = Store::default();
    let mut m = manager(store.clone());
    begin(&mut m, false);
    assert_eq!(stage(&mut m, 4), Ok(DeviceNotice::BondStaged));
    assert!(m.status().provisional_bond);
    assert!(m.ready_connection().is_none());
    assert_eq!(store.0.borrow().saves, 0);
    assert_eq!(
        ready(&mut m),
        Ok(DeviceNotice::Ready {
            saved_new_bond: true
        })
    );
    assert_eq!(store.0.borrow().saves, 1);
    assert!(!m.status().provisional_bond);
    let bytes = store.0.borrow().record.unwrap();
    let record = BondRecord::decode(&bytes, peer(), policy()).unwrap();
    assert_eq!(record.key_type(), KeyType::UnauthenticatedP192);
    assert_eq!(record.verified_minimum(), 16);
    assert_eq!(&bytes[16..], [0x77; 16]);
}
#[test]
fn failed_save_is_observable_and_requires_explicit_retry() {
    let store = Store::default();
    store.0.borrow_mut().save_error = Some(StoreError::Backend(-12));
    let mut m = manager(store.clone());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    let failure = StorageFailure {
        operation: StoreOperation::Save,
        source: StoreError::Backend(-12),
    };
    assert_eq!(ready(&mut m), Err(Error::Storage(failure)));
    assert!(m.ready_connection().is_some());
    assert!(m.status().provisional_bond);
    // A later encryption-size revalidation does not silently retry failed persistence.
    m.receive(1, 22, &event(8, &[0, 0x42, 0, 1])).unwrap();
    accept(&mut m, 23, 0x1408);
    assert_eq!(
        m.receive(1, 24, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16])),
        Ok(DeviceNotice::Ready {
            saved_new_bond: false
        })
    );
    assert_eq!(store.0.borrow().saves, 1);
    assert_eq!(m.status().persistence, Persistence::Failed(failure));
    store.0.borrow_mut().save_error = None;
    m.retry_save(25).unwrap();
    assert_eq!(store.0.borrow().saves, 2);
    assert!(!m.status().provisional_bond);
}
#[test]
fn forget_during_provisional_pairing_prevents_late_ready_save() {
    let store = Store::default();
    let mut m = manager(store.clone());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    let outcome = m.forget(16).unwrap();
    assert_eq!(outcome.cleanup, Ok(()));
    assert_eq!(outcome.persistence, Ok(()));
    assert!(m.status().revoked);
    assert!(!m.status().provisional_bond);
    assert!(m.ready_connection().is_none());
    assert!(m.receive(1, 17, &event(6, &[0, 0x42, 0])).is_err());
    assert_eq!(store.0.borrow().saves, 0);
    close(&mut m, 18);
    assert!(store.0.borrow().record.is_none());
}
#[test]
fn forget_cannot_resurrect_bond_from_inflight_accepted_key_size_result() {
    let store = Store::default();
    let mut m = manager(store.clone());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    m.receive(1, 16, &event(6, &[0, 0x42, 0])).unwrap();
    accept(&mut m, 17, 0x0413);
    m.receive(1, 18, &status(0x0413)).unwrap();
    m.receive(1, 19, &event(8, &[0, 0x42, 0, 1])).unwrap();
    accept(&mut m, 20, 0x1408);
    m.forget(21).unwrap();
    m.receive(1, 22, &event(0x0e, &[1, 8, 0x14, 0, 0x42, 0, 16]))
        .unwrap();
    assert!(m.ready_connection().is_none());
    assert_eq!(store.0.borrow().saves, 0);
    close(&mut m, 23);
}
#[test]
fn failed_delete_blocks_lookup_reselection_and_save_until_explicit_retry() {
    let store = store_with_bond();
    store.0.borrow_mut().delete_error = Some(StoreError::DurabilityUnknown);
    let mut m = manager(store.clone());
    begin(&mut m, true);
    ready(&mut m).unwrap();
    let outcome = m.forget(22).unwrap();
    assert_eq!(outcome.persistence, Err(StoreError::DurabilityUnknown));
    assert!(m.status().forget_pending);
    assert_eq!(m.select(peer()), Err(Error::ForgetPending));
    assert_eq!(m.retry_save(22), Err(Error::Revoked));
    close(&mut m, 23);
    assert_eq!(m.start(2, 26, config()), Err(Error::Revoked));
    store.0.borrow_mut().delete_error = None;
    m.retry_forget().unwrap();
    assert!(!m.status().forget_pending);
    m.select(peer()).unwrap();
    m.start(2, 27, config()).unwrap();
    assert_eq!(store.0.borrow().loads, 1);
    assert!(store.0.borrow().record.is_none());
}
#[test]
fn stale_epoch_and_ui_token_never_authorize_later_pairing() {
    let store = Store::default();
    let mut m = manager(store);
    begin(&mut m, false);
    let token = confirmation(&mut m);
    assert!(m.receive(0, 12, &event(0x17, &PEER)).is_err());
    m.forget(12).unwrap();
    assert_eq!(m.approve(token, 13, true), Err(Error::Revoked));
    close(&mut m, 14);
    m.select(peer()).unwrap();
    m.start(2, 17, config()).unwrap();
    assert_eq!(m.approve(token, 18, true), Err(Error::StaleApproval));
    assert!(m.ready_connection().is_none());
}
#[test]
fn load_error_is_not_absence_and_record_corruption_aborts() {
    let store = Store::default();
    store.0.borrow_mut().load_error = Some(StoreError::Backend(-9));
    let mut m = manager(store.clone());
    accept(&mut m, 0, 0x0405);
    m.receive(1, 1, &status(0x0405)).unwrap();
    let mut p = vec![0, 0x42, 0];
    p.extend_from_slice(&PEER);
    p.extend_from_slice(&[1, 0]);
    m.receive(1, 2, &event(3, &p)).unwrap();
    accept(&mut m, 3, 0x0411);
    m.receive(1, 4, &status(0x0411)).unwrap();
    assert_eq!(
        m.receive(1, 5, &event(0x17, &PEER)),
        Err(Error::Storage(StorageFailure {
            operation: StoreOperation::Load,
            source: StoreError::Backend(-9)
        }))
    );
    assert_eq!(m.link().unwrap().state(), State::Disconnecting);
    assert_eq!(store.0.borrow().saves, 0);
    accept(&mut m, 6, 0x0406);

    let store = store_with_bond();
    store.0.borrow_mut().record.as_mut().unwrap()[4] = 2;
    let mut m = manager(store);
    accept(&mut m, 0, 0x0405);
    m.receive(1, 1, &status(0x0405)).unwrap();
    m.receive(1, 2, &event(3, &p)).unwrap();
    accept(&mut m, 3, 0x0411);
    m.receive(1, 4, &status(0x0411)).unwrap();
    assert_eq!(
        m.receive(1, 5, &event(0x17, &PEER)),
        Err(Error::Record(RecordError::Version(2)))
    );
    assert_eq!(m.link().unwrap().state(), State::Disconnecting);
}
#[test]
fn stronger_policy_rejects_new_unauthenticated_key_without_persisting() {
    let store = Store::default();
    let mut p = policy();
    p.require_mitm = true;
    let mut m = DeviceManager::new(store.clone(), p).unwrap();
    m.select(peer()).unwrap();
    m.start(1, 0, config()).unwrap();
    begin(&mut m, false);
    assert_eq!(
        stage(&mut m, 4),
        Err(Error::Record(RecordError::MitmRequired))
    );
    assert_eq!(store.0.borrow().saves, 0);
    assert!(!m.status().provisional_bond);
}
#[test]
fn timeout_or_uncertain_transport_discards_provisional_and_approval() {
    let store = Store::default();
    let mut m = manager(store.clone());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    assert!(m.tick(1002).is_err());
    assert!(!m.status().provisional_bond);
    assert_eq!(store.0.borrow().saves, 0);
    let mut m = manager(Store::default());
    begin(&mut m, false);
    let token = confirmation(&mut m);
    assert!(m.fail_transport(bt_core::Error::Backend(-88)).is_err());
    assert!(!m.status().approval_pending);
    assert_eq!(m.approve(token, 12, true), Err(Error::StaleApproval));
}
#[test]
fn rejecting_approval_never_stores_a_key_and_stop_works_without_fresh_clock() {
    let store = Store::default();
    let mut m = manager(store.clone());
    begin(&mut m, false);
    let token = confirmation(&mut m);
    m.approve(token, 12, false).unwrap();
    accept(&mut m, 12, 0x042d);
    m.receive(1, 13, &complete(0x042d)).unwrap();
    assert_eq!(m.link().unwrap().state(), State::Disconnecting);
    assert_eq!(store.0.borrow().saves, 0);
    let mut m = manager(Store::default());
    m.cancel_at_last_observation().unwrap();
    assert_eq!(m.link().unwrap().state(), State::Closed);
}
#[test]
fn active_selection_and_epoch_reuse_are_rejected() {
    let mut m = manager(Store::default());
    assert_eq!(
        m.select(Peer::from_hci_bytes([9; 6])),
        Err(Error::ActiveLink)
    );
    m.cancel(1).unwrap();
    assert_eq!(m.start(1, 2, config()), Err(Error::EpochNotAdvanced));
    m.start(2, 2, config()).unwrap();
    assert!(m.ready_connection().is_none());
}

#[test]
fn malformed_store_length_cannot_escape_fixed_buffer() {
    for length in [0, BOND_RECORD_BYTES - 1, BOND_RECORD_BYTES + 1, usize::MAX] {
        let store = store_with_bond();
        store.0.borrow_mut().claimed_length = Some(length);
        let mut m = manager(store);
        accept(&mut m, 0, 0x0405);
        m.receive(1, 1, &status(0x0405)).unwrap();
        let mut p = vec![0, 0x42, 0];
        p.extend_from_slice(&PEER);
        p.extend_from_slice(&[1, 0]);
        m.receive(1, 2, &event(3, &p)).unwrap();
        accept(&mut m, 3, 0x0411);
        m.receive(1, 4, &status(0x0411)).unwrap();
        assert_eq!(
            m.receive(1, 5, &event(0x17, &PEER)),
            Err(Error::Record(RecordError::Length))
        );
        assert_eq!(m.link().unwrap().state(), State::Disconnecting);
    }
}

#[test]
fn stopping_discards_provisional_and_marks_it_observably() {
    let mut m = manager(Store::default());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    m.cancel(16).unwrap();
    assert!(!m.status().provisional_bond);
    assert_eq!(m.status().persistence, Persistence::Discarded);
    assert!(m.pending_confirmation().is_none());
}

#[test]
fn clock_regression_revokes_ready_and_all_pending_security_authority() {
    let mut m = manager(store_with_bond());
    begin(&mut m, true);
    ready(&mut m).unwrap();
    assert_eq!(
        m.tick(20),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );
    assert!(m.status().clock_failed);
    assert!(m.ready_connection().is_none());
    assert_eq!(m.link().unwrap().state(), State::Disconnecting);
    close(&mut m, 22);
    assert_eq!(
        m.start(2, 25, config()),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );
    assert_eq!(
        m.select(peer()),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );

    let mut m = manager(Store::default());
    begin(&mut m, false);
    let token = confirmation(&mut m);
    assert_eq!(
        m.receive(1, 10, &event(0x17, &PEER)),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );
    assert!(m.pending_confirmation().is_none());
    assert_eq!(m.approve(token, 12, true), Err(Error::StaleApproval));

    let mut m = manager(Store::default());
    begin(&mut m, false);
    stage(&mut m, 4).unwrap();
    assert_eq!(
        m.tick(14),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );
    assert!(!m.status().provisional_bond);
    assert_eq!(m.status().persistence, Persistence::Discarded);
}

#[test]
fn clock_regression_does_not_hide_transport_cleanup_uncertainty() {
    let mut m = manager(Store::default());
    begin(&mut m, false);
    confirmation(&mut m);
    let raw = bt_core::Error::Backend(-88);
    m.fail_transport(raw).unwrap_err();
    assert_eq!(
        m.tick(10),
        Err(Error::Link(bt_link::Error::ClockRegression))
    );
    assert!(m.status().clock_failed);
    assert_eq!(
        m.status().cleanup_error,
        Some(bt_link::Error::Failed(bt_link::Failure::Transport(raw)))
    );
    assert_eq!(
        m.link().unwrap().failure(),
        Some(bt_link::Failure::Transport(raw))
    );
    assert!(m.ready_connection().is_none());
}
