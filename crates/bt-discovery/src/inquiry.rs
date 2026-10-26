// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{DeviceName, Event, Observation, WireError};
use bt_link::Peer;
const INQUIRY: u16 = 0x0401;
const CANCEL: u16 = 0x0402;
pub const MAX_RESULT_SLOTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Inquiry_Length in 1.28-second units, 1..=0x30. GIAC is fixed.
    pub inquiry_length: u8,
    /// Explicit controller response bound, 1..=255; unlimited zero is excluded.
    pub max_responses: u8,
    /// Host aggregate deadline, independent of unknown Extended_Inquiry_Length.
    pub scan_timeout_us: u64,
    pub cleanup_timeout_us: u64,
    /// Accepted result events before cancellation; duplicates also consume budget.
    pub max_result_events: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Starting,
    Scanning,
    Stopping,
    Cancelling,
    Complete,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    User,
    ScanDeadline,
    EventBudget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Controller { opcode: u16, status: u8 },
    InquiryComplete(u8),
    CleanupDeadline,
    ClockRegressed,
    Transport(bt_core::Error),
    CounterExhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidState,
    ForeignEpoch,
    StaleId,
    NotQuiescent,
    BufferTooSmall,
    WrongTicket,
    CommandNotAccepted,
    UnexpectedCommand,
    UnexpectedEvent,
    EpochNotAdvanced,
    Wire(WireError),
    Failed(Failure),
}
/// Stable within one scan; not a peer address or authorization token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceId {
    epoch: u64,
    ordinal: u16,
}
impl DeviceId {
    pub const fn parts(self) -> (u64, u16) {
        (self.epoch, self.ordinal)
    }
    pub const fn from_parts(epoch: u64, ordinal: u16) -> Self {
        Self { epoch, ordinal }
    }
}
/// Protected local management metadata. No Debug or unredacted public status.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceRecord {
    pub id: DeviceId,
    pub peer: Peer,
    pub page_scan_repetition: u8,
    pub class_of_device: [u8; 3],
    pub clock_offset: u16,
    pub rssi_dbm: Option<i8>,
    pub rssi_observed_us: Option<u64>,
    pub name: Option<DeviceName>,
    pub last_seen_us: u64,
    pub reports: u32,
}
#[derive(Clone, Copy, Default)]
pub struct ResultSlot {
    record: Option<DeviceRecord>,
}
impl ResultSlot {
    pub const fn empty() -> Self {
        Self { record: None }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandTicket {
    epoch: u64,
    serial: u32,
    opcode: u16,
    len: usize,
}
impl CommandTicket {
    pub const fn opcode(self) -> u16 {
        self.opcode
    }
    pub const fn len(self) -> usize {
        self.len
    }
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub epoch: u64,
    pub state: State,
    pub failure: Option<Failure>,
    pub stored_results: usize,
    pub capacity: usize,
    pub result_events: u16,
    /// Count of reports not retained, not necessarily distinct devices.
    pub unretained_reports: u32,
    pub list_full: bool,
    pub truncated: bool,
    pub stop_reason: Option<StopReason>,
    pub inquiry_complete_status: Option<u8>,
    pub cancel_status: Option<u8>,
    pub pending_command: bool,
    pub cleanup_required: bool,
    pub deadline_us: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    None,
    Results {
        added: u8,
        updated: u8,
        unretained: u8,
        list_full: bool,
    },
    StopRequested(StopReason),
    /// An original Inquiry Complete was observed; pending Cancel may still need ack.
    InquiryComplete {
        status: u8,
    },
    Cancelled,
    CancelRace {
        status: u8,
    },
}
struct Pending {
    ticket: CommandTicket,
    accepted: bool,
}

/// One caller-driven, source-reviewed Inquiry. The enclosing single owner must
/// prove exclusive routing of Inquiry events and real command allowance.
pub struct Inquiry<'a> {
    epoch: u64,
    config: Config,
    slots: &'a mut [ResultSlot],
    len: usize,
    state: State,
    pending: Option<Pending>,
    serial: u32,
    last_us: u64,
    deadline_us: u64,
    failure: Option<Failure>,
    radio_possible: bool,
    status_received: bool,
    inquiry_done: bool,
    uncertain: bool,
    stop_reason: Option<StopReason>,
    result_events: u16,
    unretained_reports: u32,
    list_full: bool,
    truncated: bool,
    complete_status: Option<u8>,
    cancel_status: Option<u8>,
}
impl<'a> Inquiry<'a> {
    pub fn new(
        epoch: u64,
        now_us: u64,
        config: Config,
        slots: &'a mut [ResultSlot],
    ) -> Result<Self, Error> {
        let deadline_us = Self::validate(epoch, now_us, config, slots.len())?;
        slots.fill(ResultSlot::empty());
        Ok(Self {
            epoch,
            config,
            slots,
            len: 0,
            state: State::Starting,
            pending: Some(Pending {
                ticket: CommandTicket {
                    epoch,
                    serial: 1,
                    opcode: INQUIRY,
                    len: 8,
                },
                accepted: false,
            }),
            serial: 1,
            last_us: now_us,
            deadline_us,
            failure: None,
            radio_possible: false,
            status_received: false,
            inquiry_done: false,
            uncertain: false,
            stop_reason: None,
            result_events: 0,
            unretained_reports: 0,
            list_full: false,
            truncated: false,
            complete_status: None,
            cancel_status: None,
        })
    }
    fn validate(epoch: u64, now: u64, c: Config, capacity: usize) -> Result<u64, Error> {
        if epoch == 0
            || !(1..=0x30).contains(&c.inquiry_length)
            || c.max_responses == 0
            || c.scan_timeout_us == 0
            || c.cleanup_timeout_us == 0
            || c.max_result_events == 0
            || capacity == 0
            || capacity > MAX_RESULT_SLOTS
        {
            return Err(Error::InvalidConfig);
        }
        let deadline = now
            .checked_add(c.scan_timeout_us)
            .ok_or(Error::InvalidConfig)?;
        deadline
            .checked_add(c.cleanup_timeout_us)
            .ok_or(Error::InvalidConfig)?;
        Ok(deadline)
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            epoch: self.epoch,
            state: self.state,
            failure: self.failure,
            stored_results: self.len,
            capacity: self.slots.len(),
            result_events: self.result_events,
            unretained_reports: self.unretained_reports,
            list_full: self.list_full,
            truncated: self.truncated,
            stop_reason: self.stop_reason,
            inquiry_complete_status: self.complete_status,
            cancel_status: self.cancel_status,
            pending_command: self.pending.is_some(),
            cleanup_required: !self.is_quiescent(),
            deadline_us: self.deadline_us,
        }
    }
    pub fn results(&self) -> impl ExactSizeIterator<Item = &DeviceRecord> {
        self.slots[..self.len]
            .iter()
            .map(|slot| slot.record.as_ref().expect("populated result prefix"))
    }
    pub fn get(&self, id: DeviceId) -> Result<&DeviceRecord, Error> {
        if id.epoch != self.epoch {
            return Err(Error::StaleId);
        }
        self.slots
            .get(usize::from(id.ordinal))
            .and_then(|slot| slot.record.as_ref())
            .filter(|r| r.id == id)
            .ok_or(Error::StaleId)
    }
    /// Select only after the Inquiry procedure and all command acks are quiescent.
    pub fn connection_candidate(&self, id: DeviceId) -> Result<&DeviceRecord, Error> {
        if !self.is_quiescent() {
            return Err(Error::NotQuiescent);
        }
        if let Some(failure) = self.failure {
            return Err(Error::Failed(failure));
        }
        self.get(id)
    }
    pub fn is_quiescent(&self) -> bool {
        !self.radio_possible && self.pending.is_none() && !self.uncertain
    }
    pub fn prepare_command(
        &mut self,
        now_us: u64,
        output: &mut [u8],
    ) -> Result<Option<CommandTicket>, Error> {
        self.tick(now_us)?;
        if self.state == State::Failed {
            return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
        }
        let Some(p) = &self.pending else {
            return Ok(None);
        };
        if p.accepted {
            return Ok(None);
        }
        if output.len() < p.ticket.len {
            return Err(Error::BufferTooSmall);
        }
        if p.ticket.opcode == INQUIRY {
            output[..8].copy_from_slice(&[
                1,
                4,
                5,
                0x33,
                0x8b,
                0x9e,
                self.config.inquiry_length,
                self.config.max_responses,
            ]);
        } else {
            output[..3].copy_from_slice(&[2, 4, 0]);
        }
        Ok(Some(p.ticket))
    }
    pub fn confirm_accepted(&mut self, ticket: CommandTicket) -> Result<(), Error> {
        let p = self.pending.as_mut().ok_or(Error::WrongTicket)?;
        if p.ticket != ticket || p.accepted {
            return Err(Error::WrongTicket);
        }
        p.accepted = true;
        if ticket.opcode == INQUIRY {
            self.radio_possible = true;
        }
        Ok(())
    }
    pub fn tick(&mut self, now_us: u64) -> Result<Notice, Error> {
        self.clock(now_us)?;
        if matches!(
            self.state,
            State::Complete | State::Cancelled | State::Failed
        ) {
            return Ok(Notice::None);
        }
        if now_us >= self.deadline_us {
            if self.stop_reason.is_some() {
                return self.fail(Failure::CleanupDeadline);
            }
            self.request_stop(now_us, StopReason::ScanDeadline)?;
            return Ok(Notice::StopRequested(StopReason::ScanDeadline));
        }
        Ok(Notice::None)
    }
    pub fn cancel(&mut self, now_us: u64) -> Result<(), Error> {
        self.clock(now_us)?;
        self.request_stop(now_us, StopReason::User)
    }
    pub fn fail_transport(&mut self, error: bt_core::Error) -> Result<(), Error> {
        if error == bt_core::Error::Busy {
            return Err(Error::InvalidState);
        }
        self.uncertain = true;
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.ticket.opcode == INQUIRY)
        {
            self.radio_possible = true;
        }
        self.fail(Failure::Transport(error))
    }
    pub fn reset(&mut self, new_epoch: u64, now_us: u64, config: Config) -> Result<(), Error> {
        if !self.is_quiescent() {
            return Err(Error::NotQuiescent);
        }
        if new_epoch <= self.epoch {
            return Err(Error::EpochNotAdvanced);
        }
        let deadline = Self::validate(new_epoch, now_us, config, self.slots.len())?;
        if now_us < self.last_us {
            return Err(Error::Failed(Failure::ClockRegressed));
        }
        self.slots.fill(ResultSlot::empty());
        self.len = 0;
        self.epoch = new_epoch;
        self.config = config;
        self.state = State::Starting;
        self.serial = 1;
        self.pending = Some(Pending {
            ticket: CommandTicket {
                epoch: new_epoch,
                serial: 1,
                opcode: INQUIRY,
                len: 8,
            },
            accepted: false,
        });
        self.last_us = now_us;
        self.deadline_us = deadline;
        self.failure = None;
        self.radio_possible = false;
        self.status_received = false;
        self.inquiry_done = false;
        self.uncertain = false;
        self.stop_reason = None;
        self.result_events = 0;
        self.unretained_reports = 0;
        self.list_full = false;
        self.truncated = false;
        self.complete_status = None;
        self.cancel_status = None;
        Ok(())
    }
    pub fn receive(&mut self, epoch: u64, now_us: u64, packet: &[u8]) -> Result<Notice, Error> {
        if epoch != self.epoch {
            return Err(Error::ForeignEpoch);
        }
        let event = Event::parse(packet).map_err(Error::Wire)?;
        self.tick(now_us)?;
        match event {
            Event::CommandStatus { opcode, status, .. } => {
                if opcode != INQUIRY && status != 1 {
                    return Err(Error::UnexpectedCommand);
                }
                self.ack(opcode)?;
                if opcode == CANCEL {
                    return self.cancel_completion(status);
                }
                if status != 0 {
                    if opcode == INQUIRY {
                        self.radio_possible = false;
                    }
                    return self.fail(Failure::Controller { opcode, status });
                }
                self.status_received = true;
                if self.stop_reason.is_some() {
                    self.drive_stop()?;
                } else if self.failure.is_none() {
                    self.state = State::Scanning;
                }
                Ok(Notice::None)
            }
            Event::CommandComplete { opcode, status, .. } => {
                if opcode == INQUIRY && status != 1 {
                    return Err(Error::UnexpectedCommand);
                }
                self.ack(opcode)?;
                if opcode == INQUIRY {
                    self.radio_possible = false;
                    return self.fail(Failure::Controller { opcode, status });
                }
                self.cancel_completion(status)
            }
            Event::InquiryComplete { status } => {
                if !self.status_received || self.inquiry_done || !self.radio_possible {
                    return Err(Error::UnexpectedEvent);
                }
                self.inquiry_done = true;
                self.radio_possible = false;
                self.complete_status = Some(status);
                if self.pending.as_ref().is_some_and(|p| !p.accepted) {
                    self.pending = None;
                }
                if status != 0 && self.failure.is_none() {
                    self.failure = Some(Failure::InquiryComplete(status));
                }
                if self.pending.is_some() {
                    self.state = State::Stopping;
                } else {
                    self.finish();
                }
                Ok(Notice::InquiryComplete { status })
            }
            Event::Results(results) => {
                if self.state == State::Failed {
                    return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
                }
                if !self.status_received || self.inquiry_done || !self.radio_possible {
                    return Err(Error::UnexpectedEvent);
                }
                let mut added = 0;
                let mut updated = 0;
                let mut unretained = 0;
                if self.result_events >= self.config.max_result_events {
                    let count = results.len() as u32;
                    self.count_unretained(count)?;
                    self.truncated = true;
                    return Ok(Notice::Results {
                        added: 0,
                        updated: 0,
                        unretained: count as u8,
                        list_full: self.list_full,
                    });
                }
                self.result_events += 1;
                for observation in results.iter() {
                    match self.store(observation, now_us) {
                        0 => added += 1,
                        1 => updated += 1,
                        _ => unretained += 1,
                    }
                }
                self.count_unretained(u32::from(unretained))?;
                if self.result_events == self.config.max_result_events && self.stop_reason.is_none()
                {
                    self.truncated = true;
                    self.request_stop(now_us, StopReason::EventBudget)?;
                }
                Ok(Notice::Results {
                    added,
                    updated,
                    unretained,
                    list_full: self.list_full,
                })
            }
        }
    }
    fn store(&mut self, o: Observation, now: u64) -> u8 {
        if let Some(record) = self.slots[..self.len]
            .iter_mut()
            .find_map(|slot| slot.record.as_mut().filter(|r| r.peer == o.peer))
        {
            record.page_scan_repetition = o.page_scan_repetition;
            record.class_of_device = o.class_of_device;
            record.clock_offset = o.clock_offset;
            record.last_seen_us = now;
            record.reports += 1;
            if o.rssi_dbm.is_some() {
                record.rssi_dbm = o.rssi_dbm;
                record.rssi_observed_us = Some(now);
            }
            if let Some(name) = o.name
                && record
                    .name
                    .is_none_or(|old| name.source_complete() || !old.source_complete())
            {
                record.name = Some(name);
            }
            return 1;
        }
        if self.len == self.slots.len() {
            self.list_full = true;
            self.truncated = true;
            return 2;
        }
        self.slots[self.len].record = Some(DeviceRecord {
            id: DeviceId {
                epoch: self.epoch,
                ordinal: self.len as u16,
            },
            peer: o.peer,
            page_scan_repetition: o.page_scan_repetition,
            class_of_device: o.class_of_device,
            clock_offset: o.clock_offset,
            rssi_dbm: o.rssi_dbm,
            rssi_observed_us: o.rssi_dbm.map(|_| now),
            name: o.name,
            last_seen_us: now,
            reports: 1,
        });
        self.len += 1;
        self.list_full = self.len == self.slots.len();
        0
    }
    fn count_unretained(&mut self, count: u32) -> Result<(), Error> {
        match self.unretained_reports.checked_add(count) {
            Some(total) => {
                self.unretained_reports = total;
                Ok(())
            }
            None => self.fail(Failure::CounterExhausted),
        }
    }
    fn cancel_completion(&mut self, status: u8) -> Result<Notice, Error> {
        self.cancel_status = Some(status);
        if status != 0 && !self.inquiry_done {
            return self.fail(Failure::Controller {
                opcode: CANCEL,
                status,
            });
        }
        self.radio_possible = false;
        self.finish();
        if status != 0 {
            Ok(Notice::CancelRace { status })
        } else {
            Ok(Notice::Cancelled)
        }
    }
    fn request_stop(&mut self, now: u64, reason: StopReason) -> Result<(), Error> {
        if self.uncertain {
            return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
        }
        if self.is_quiescent() {
            return Ok(());
        }
        if self.stop_reason.is_some() {
            return Ok(());
        }
        self.deadline_us = now
            .checked_add(self.config.cleanup_timeout_us)
            .ok_or(Error::InvalidConfig)?;
        self.stop_reason = Some(reason);
        self.truncated = true;
        if self
            .pending
            .as_ref()
            .is_some_and(|p| !p.accepted && p.ticket.opcode == INQUIRY)
        {
            self.pending = None;
            self.finish();
            return Ok(());
        }
        self.state = State::Stopping;
        self.drive_stop()
    }
    fn drive_stop(&mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Ok(());
        }
        if !self.radio_possible {
            self.finish();
            return Ok(());
        }
        if !self.status_received {
            return Ok(());
        }
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(Error::Failed(Failure::CounterExhausted))?;
        self.pending = Some(Pending {
            ticket: CommandTicket {
                epoch: self.epoch,
                serial: self.serial,
                opcode: CANCEL,
                len: 3,
            },
            accepted: false,
        });
        self.state = State::Cancelling;
        Ok(())
    }
    fn ack(&mut self, opcode: u16) -> Result<(), Error> {
        let p = self.pending.as_ref().ok_or(Error::UnexpectedCommand)?;
        if p.ticket.opcode != opcode {
            return Err(Error::UnexpectedCommand);
        }
        if !p.accepted {
            return Err(Error::CommandNotAccepted);
        }
        self.pending = None;
        Ok(())
    }
    fn finish(&mut self) {
        self.state = if self.failure.is_some() {
            State::Failed
        } else if self.inquiry_done {
            State::Complete
        } else {
            State::Cancelled
        };
    }
    fn clock(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_us {
            return self.fail(Failure::ClockRegressed);
        }
        self.last_us = now;
        Ok(())
    }
    fn fail<T>(&mut self, failure: Failure) -> Result<T, Error> {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        self.state = State::Failed;
        Err(Error::Failed(failure))
    }
}
