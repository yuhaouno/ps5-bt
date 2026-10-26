// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{BOND_RECORD_BYTES, BondPolicy, BondRecord, KeyType, RecordError};
use bt_hci::ConnectionHandle;
use bt_link::{CommandTicket, Config, LinkSession, Notice, Peer, RequestId, State};

/// Non-secret platform storage diagnostics; raw status is preserved numerically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    Unavailable,
    Backend(i32),
    DurabilityUnknown,
}

/// One serialized protected local store, with bounded blocking chosen by its
/// platform implementation. Success of save/delete is atomic AND durable;
/// failures may leave the old or new whole record, never a reported-success
/// partial record. No implementation is allowed to log keys or peer bytes.
/// Load must verify permissions/integrity, reject unsafe paths/symlinks and
/// retain no caller buffer. No PS5 implementation is supplied by this crate.
pub trait BondStore {
    /// None means confirmed absent, not permission or I/O failure.
    fn load(
        &mut self,
        peer: Peer,
        output: &mut [u8; BOND_RECORD_BYTES],
    ) -> Result<Option<usize>, StoreError>;
    fn save_atomic(
        &mut self,
        peer: Peer,
        record: &[u8; BOND_RECORD_BYTES],
    ) -> Result<(), StoreError>;
    /// Idempotent: a confirmed absent record is success. Durability includes
    /// the deletion's directory/metadata, not merely a successful unlink call.
    fn delete_atomic(&mut self, peer: Peer) -> Result<(), StoreError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreOperation {
    Load,
    Save,
    Delete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageFailure {
    pub operation: StoreOperation,
    pub source: StoreError,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NoSelection,
    ActiveLink,
    ForgetPending,
    EpochNotAdvanced,
    CounterExhausted,
    StaleApproval,
    Revoked,
    NoPendingSave,
    NotReady,
    Record(RecordError),
    Link(bt_link::Error),
    Storage(StorageFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalToken {
    selection: u64,
    epoch: u64,
    request: RequestId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceNotice {
    None,
    KeyLookup {
        found: bool,
    },
    NeedConfirmation {
        token: ApprovalToken,
        numeric_value: u32,
    },
    BondStaged,
    Ready {
        saved_new_bond: bool,
    },
    Closed {
        reason: u8,
    },
    PairingRejected {
        event: u8,
    },
    CancellationRace {
        status: u8,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Persistence {
    Unchanged,
    Loaded,
    Provisional,
    Saved,
    Discarded,
    Forgotten,
    Failed(StorageFailure),
    Rejected(RecordError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceStatus {
    pub selected: bool,
    pub link_state: Option<State>,
    pub ready: bool,
    pub approval_pending: bool,
    pub provisional_bond: bool,
    pub revoked: bool,
    pub forget_pending: bool,
    pub persistence: Persistence,
    pub cleanup_error: Option<bt_link::Error>,
    pub clock_failed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgetOutcome {
    pub cleanup: Result<(), bt_link::Error>,
    pub persistence: Result<(), StoreError>,
}
struct Provisional {
    selection: u64,
    epoch: u64,
    record: BondRecord,
    save_attempted: bool,
}

/// Owns the LinkSession so a public caller cannot forge Ready or inject notices.
/// All commands still require external transport ownership/allowance and full
/// command acceptance; this manager never submits a byte itself.
pub struct DeviceManager<S: BondStore> {
    store: S,
    policy: BondPolicy,
    selected: Option<Peer>,
    selection: u64,
    link: Option<LinkSession>,
    last_epoch: u64,
    link_minimum: u8,
    last_us: Option<u64>,
    approval: Option<ApprovalToken>,
    approval_value: u32,
    provisional: Option<Provisional>,
    revoked: bool,
    forget_pending: bool,
    persistence: Persistence,
    cleanup_error: Option<bt_link::Error>,
    clock_failed: bool,
}
impl<S: BondStore> DeviceManager<S> {
    pub fn new(store: S, policy: BondPolicy) -> Result<Self, Error> {
        policy.validate().map_err(Error::Record)?;
        Ok(Self {
            store,
            policy,
            selected: None,
            selection: 0,
            link: None,
            last_epoch: 0,
            link_minimum: policy.minimum_key_size,
            last_us: None,
            approval: None,
            approval_value: 0,
            provisional: None,
            revoked: false,
            forget_pending: false,
            persistence: Persistence::Unchanged,
            cleanup_error: None,
            clock_failed: false,
        })
    }
    pub fn selected_peer(&self) -> Option<Peer> {
        self.selected
    }
    /// Protected local UI only; public status must not expose pairing prompts.
    pub fn pending_confirmation(&self) -> Option<(ApprovalToken, u32)> {
        self.approval.map(|token| (token, self.approval_value))
    }
    pub fn link(&self) -> Option<&LinkSession> {
        self.link.as_ref()
    }
    pub fn ready_connection(&self) -> Option<(ConnectionHandle, u64)> {
        if self.revoked || self.clock_failed {
            None
        } else {
            self.link.as_ref().and_then(LinkSession::ready_connection)
        }
    }
    pub fn status(&self) -> DeviceStatus {
        DeviceStatus {
            selected: self.selected.is_some(),
            link_state: self.link.as_ref().map(LinkSession::state),
            ready: self.ready_connection().is_some(),
            approval_pending: self.approval.is_some(),
            provisional_bond: self.provisional.is_some(),
            revoked: self.revoked,
            forget_pending: self.forget_pending,
            persistence: self.persistence,
            cleanup_error: self.cleanup_error,
            clock_failed: self.clock_failed,
        }
    }
    /// Explicit local selection only; discovery/inquiry are absent. A live or
    /// uncertain old link cannot be silently replaced by a new selected peer.
    pub fn select(&mut self, peer: Peer) -> Result<(), Error> {
        if self.clock_failed {
            return Err(Error::Link(bt_link::Error::ClockRegression));
        }
        if self.forget_pending {
            return Err(Error::ForgetPending);
        }
        self.require_finished()?;
        let selection = self
            .selection
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        self.link = None;
        self.selected = Some(peer);
        self.selection = selection;
        self.approval = None;
        self.provisional = None;
        self.revoked = false;
        self.persistence = Persistence::Unchanged;
        self.cleanup_error = None;
        Ok(())
    }
    pub fn start(&mut self, epoch: u64, now_us: u64, config: Config) -> Result<(), Error> {
        if self.clock_failed {
            return Err(Error::Link(bt_link::Error::ClockRegression));
        }
        if self.revoked {
            return Err(Error::Revoked);
        }
        if self.forget_pending {
            return Err(Error::ForgetPending);
        }
        self.require_finished()?;
        let peer = self.selected.ok_or(Error::NoSelection)?;
        if epoch <= self.last_epoch {
            return Err(Error::EpochNotAdvanced);
        }
        if config.minimum_key_size < self.policy.minimum_key_size {
            return Err(Error::Record(RecordError::KeySize(config.minimum_key_size)));
        }
        self.observe(now_us)?;
        let link = LinkSession::new(peer, epoch, now_us, config).map_err(Error::Link)?;
        self.link = Some(link);
        self.last_epoch = epoch;
        self.link_minimum = config.minimum_key_size;
        self.approval = None;
        self.provisional = None;
        self.cleanup_error = None;
        Ok(())
    }
    pub fn prepare_command(
        &mut self,
        now_us: u64,
        output: &mut [u8],
    ) -> Result<Option<CommandTicket>, Error> {
        self.tick(now_us)?;
        self.link
            .as_ref()
            .ok_or(Error::NoSelection)?
            .prepare_command(output)
            .map_err(Error::Link)
    }
    pub fn command_len(&self) -> Option<usize> {
        self.link.as_ref().and_then(LinkSession::command_len)
    }
    pub fn confirm_accepted(&mut self, ticket: CommandTicket) -> Result<(), Error> {
        self.link
            .as_mut()
            .ok_or(Error::NoSelection)?
            .confirm_accepted(ticket)
            .map_err(Error::Link)
    }
    pub fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        let result = self
            .link
            .as_mut()
            .ok_or(Error::NoSelection)?
            .tick(now_us)
            .map_err(Error::Link);
        self.clear_terminal_secrets();
        result
    }
    pub fn receive(
        &mut self,
        epoch: u64,
        now_us: u64,
        bytes: &[u8],
    ) -> Result<DeviceNotice, Error> {
        if epoch != self.last_epoch {
            return Err(Error::Link(bt_link::Error::ForeignEpoch));
        }
        self.observe(now_us)?;
        let result = self
            .link
            .as_mut()
            .ok_or(Error::NoSelection)?
            .receive(epoch, now_us, bytes);
        let notice = match result {
            Ok(notice) => notice,
            Err(error) => {
                self.clear_terminal_secrets();
                return Err(Error::Link(error));
            }
        };
        match notice {
            Notice::None => {
                self.clear_terminal_secrets();
                Ok(DeviceNotice::None)
            }
            Notice::NeedKey { request } => {
                if self.revoked {
                    return Err(Error::Revoked);
                }
                let peer = self.selected.ok_or(Error::NoSelection)?;
                let mut bytes = [0; BOND_RECORD_BYTES];
                let loaded = self.store.load(peer, &mut bytes);
                let record = match loaded {
                    Ok(Some(len)) => {
                        let result = bytes
                            .get(..len)
                            .ok_or(RecordError::Length)
                            .and_then(|slice| BondRecord::decode(slice, peer, self.policy));
                        bytes.fill(0);
                        match result {
                            Ok(record) => Some(record),
                            Err(error) => {
                                self.persistence = Persistence::Rejected(error);
                                self.abort(now_us);
                                return Err(Error::Record(error));
                            }
                        }
                    }
                    Ok(None) => {
                        bytes.fill(0);
                        None
                    }
                    Err(source) => {
                        bytes.fill(0);
                        let failure = StorageFailure {
                            operation: StoreOperation::Load,
                            source,
                        };
                        self.persistence = Persistence::Failed(failure);
                        self.abort(now_us);
                        return Err(Error::Storage(failure));
                    }
                };
                let found = record.is_some();
                self.link
                    .as_mut()
                    .ok_or(Error::NoSelection)?
                    .supply_key(request, now_us, record.map(BondRecord::into_key))
                    .map_err(Error::Link)?;
                if found {
                    self.persistence = Persistence::Loaded;
                }
                Ok(DeviceNotice::KeyLookup { found })
            }
            Notice::NeedConfirmation {
                request,
                numeric_value,
            } => {
                if self.revoked {
                    return Err(Error::Revoked);
                }
                let token = ApprovalToken {
                    selection: self.selection,
                    epoch,
                    request,
                };
                self.approval = Some(token);
                self.approval_value = numeric_value;
                Ok(DeviceNotice::NeedConfirmation {
                    token,
                    numeric_value,
                })
            }
            Notice::ProvisionalBond { key, key_type } => {
                if self.revoked {
                    return Err(Error::Revoked);
                }
                let record = KeyType::from_raw(key_type)
                    .and_then(|kind| {
                        BondRecord::new(
                            self.selected.ok_or(RecordError::PeerMismatch)?,
                            key,
                            kind,
                            self.link_minimum,
                        )
                    })
                    .and_then(|record| {
                        record.check_policy(self.policy)?;
                        Ok(record)
                    });
                match record {
                    Ok(record) => {
                        self.provisional = Some(Provisional {
                            selection: self.selection,
                            epoch,
                            record,
                            save_attempted: false,
                        });
                        self.persistence = Persistence::Provisional;
                        Ok(DeviceNotice::BondStaged)
                    }
                    Err(error) => {
                        self.persistence = Persistence::Rejected(error);
                        self.abort(now_us);
                        Err(Error::Record(error))
                    }
                }
            }
            Notice::Ready => {
                if self.revoked {
                    return Err(Error::Revoked);
                }
                self.approval = None;
                let saved_new_bond = self
                    .provisional
                    .as_ref()
                    .is_some_and(|bond| !bond.save_attempted);
                if saved_new_bond {
                    self.persist_provisional()?;
                }
                Ok(DeviceNotice::Ready { saved_new_bond })
            }
            Notice::Closed { reason } => {
                self.approval = None;
                self.discard_provisional();
                Ok(DeviceNotice::Closed { reason })
            }
            Notice::PairingRejected { event } => {
                self.approval = None;
                self.discard_provisional();
                Ok(DeviceNotice::PairingRejected { event })
            }
            Notice::CancellationRace { status } => Ok(DeviceNotice::CancellationRace { status }),
        }
    }
    pub fn approve(
        &mut self,
        token: ApprovalToken,
        now_us: u64,
        approved: bool,
    ) -> Result<(), Error> {
        if self.revoked {
            return Err(Error::Revoked);
        }
        if self.approval != Some(token)
            || token.selection != self.selection
            || token.epoch != self.last_epoch
        {
            return Err(Error::StaleApproval);
        }
        self.observe(now_us)?;
        let result = self
            .link
            .as_mut()
            .ok_or(Error::NoSelection)?
            .approve(token.request, now_us, approved)
            .map_err(Error::Link);
        if result.is_ok() {
            self.approval = None;
            if !approved {
                self.provisional = None;
            }
        }
        self.clear_terminal_secrets();
        result
    }
    pub fn cancel(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        self.approval = None;
        self.discard_provisional();
        let result = self.link.as_mut().ok_or(Error::NoSelection)?.cancel(now_us);
        self.cleanup_error = result.err();
        result.map_err(Error::Link)
    }
    pub fn fail_transport(&mut self, error: bt_core::Error) -> Result<(), Error> {
        let result = self
            .link
            .as_mut()
            .ok_or(Error::NoSelection)?
            .fail_transport(error)
            .map_err(Error::Link);
        self.clear_terminal_secrets();
        result
    }
    /// Conservative clock-failure stop: uses the last observed monotonic time.
    /// The next real tick may immediately exhaust the remaining cleanup budget.
    pub fn cancel_at_last_observation(&mut self) -> Result<(), Error> {
        self.cancel(self.last_us.unwrap_or(0))
    }
    /// Immediately revokes all lookup/save/UI authority before attempting I/O.
    /// A deletion error keeps this peer blocked until explicit retry succeeds.
    pub fn forget(&mut self, now_us: u64) -> Result<ForgetOutcome, Error> {
        let peer = self.selected.ok_or(Error::NoSelection)?;
        self.observe(now_us)?;
        self.revoked = true;
        self.forget_pending = true;
        self.approval = None;
        self.provisional = None;
        let cleanup = match self.link.as_mut() {
            Some(link) => link.cancel(now_us),
            None => Ok(()),
        };
        self.cleanup_error = cleanup.err();
        let persistence = self.store.delete_atomic(peer);
        self.finish_delete(persistence);
        Ok(ForgetOutcome {
            cleanup,
            persistence,
        })
    }
    pub fn retry_forget(&mut self) -> Result<(), Error> {
        if !self.forget_pending {
            return Err(Error::Revoked);
        }
        let peer = self.selected.ok_or(Error::NoSelection)?;
        let result = self.store.delete_atomic(peer);
        self.finish_delete(result);
        result.map_err(|source| {
            Error::Storage(StorageFailure {
                operation: StoreOperation::Delete,
                source,
            })
        })
    }
    pub fn retry_save(&mut self, now_us: u64) -> Result<(), Error> {
        self.tick(now_us)?;
        self.persist_provisional()
    }
    /// Taking the store does not imply native link cleanup; call only after the
    /// enclosing transport has completed or explicitly reconciled teardown.
    pub fn into_store(self) -> S {
        self.store
    }

    fn observe(&mut self, now_us: u64) -> Result<(), Error> {
        if let Some(last) = self.last_us
            && now_us < last
        {
            self.clock_failed = true;
            self.approval = None;
            self.discard_provisional();
            // Stop using only established time; never infer elapsed time, clear
            // unknown credits or claim an uncertain accepted command was cleaned.
            if let Some(link) = self.link.as_mut() {
                self.cleanup_error = link.cancel(last).err();
            }
            return Err(Error::Link(bt_link::Error::ClockRegression));
        }
        self.last_us = Some(now_us);
        Ok(())
    }
    fn require_finished(&self) -> Result<(), Error> {
        if self
            .link
            .as_ref()
            .is_some_and(|link| link.state() != State::Closed || link.has_pending_command())
        {
            return Err(Error::ActiveLink);
        }
        Ok(())
    }
    fn clear_terminal_secrets(&mut self) {
        if self.link.as_ref().is_some_and(|link| {
            matches!(link.state(), State::Failed | State::Closed) || link.failure().is_some()
        }) {
            self.discard_provisional();
            self.approval = None;
        }
    }
    fn discard_provisional(&mut self) {
        self.provisional = None;
        if self.persistence == Persistence::Provisional {
            self.persistence = Persistence::Discarded;
        }
    }
    fn abort(&mut self, now_us: u64) {
        self.approval = None;
        self.provisional = None;
        if let Some(link) = self.link.as_mut() {
            self.cleanup_error = link.cancel(now_us).err();
        }
    }
    fn persist_provisional(&mut self) -> Result<(), Error> {
        if self.revoked || self.forget_pending {
            return Err(Error::Revoked);
        }
        if self.ready_connection().is_none() {
            return Err(Error::NotReady);
        }
        let provisional = self.provisional.as_mut().ok_or(Error::NoPendingSave)?;
        if provisional.selection != self.selection
            || provisional.epoch != self.last_epoch
            || Some(provisional.record.peer()) != self.selected
        {
            return Err(Error::Revoked);
        }
        let mut bytes = [0; BOND_RECORD_BYTES];
        provisional
            .record
            .encode(&mut bytes)
            .map_err(Error::Record)?;
        provisional.save_attempted = true;
        let result = self.store.save_atomic(provisional.record.peer(), &bytes);
        bytes.fill(0);
        match result {
            Ok(()) => {
                self.provisional = None;
                self.persistence = Persistence::Saved;
                Ok(())
            }
            Err(source) => {
                let failure = StorageFailure {
                    operation: StoreOperation::Save,
                    source,
                };
                self.persistence = Persistence::Failed(failure);
                Err(Error::Storage(failure))
            }
        }
    }
    fn finish_delete(&mut self, result: Result<(), StoreError>) {
        match result {
            Ok(()) => {
                self.forget_pending = false;
                self.persistence = Persistence::Forgotten;
            }
            Err(source) => {
                self.persistence = Persistence::Failed(StorageFailure {
                    operation: StoreOperation::Delete,
                    source,
                });
            }
        }
    }
}
