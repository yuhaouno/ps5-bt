// SPDX-License-Identifier: GPL-3.0-or-later
use bt_app::{Application, Phase};
use bt_control::management::{
    Action, Actions, Approval, Device, DeviceId, Manager, Name, OwnerPhase, Snapshot, Ticket,
};
use bt_control::{
    Capability, CommandOutcome, Control, LinkState, PendingCommand, Status, StreamState,
};
use bt_device::{ApprovalToken, BondStore};
use bt_discovery::{Inquiry, ResultSlot};

/// Supplied by actual platform evidence. Host implementation does not make a
/// missing PS5 backend Ready. Defaults deliberately expose no active operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub transport: Capability,
    pub discovery: Capability,
    pub capture: Capability,
    pub codec: Capability,
    pub playback: Capability,
}
impl Default for Capabilities {
    fn default() -> Self {
        Self {
            transport: Capability::Unavailable,
            discovery: Capability::Unavailable,
            capture: Capability::Unavailable,
            codec: Capability::Unavailable,
            playback: Capability::Unavailable,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub session: u64,
    pub first_scan_epoch: u64,
    pub first_link_epoch: u64,
    pub result_valid_us: u64,
    pub inquiry: bt_discovery::Config,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Discovery,
    Application,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    ClockRegressed,
    Exhausted,
    StaleDevice,
    ExpiredDevice,
    InvalidState,
    Unavailable,
    Management(bt_control::management::Error),
    Legacy(bt_control::FinishError),
    Discovery(bt_discovery::Error),
    Application(bt_app::Error),
    Transport(bt_core::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LookupFailure {
    pub device: DeviceId,
    pub error: bt_app::Error,
}

#[derive(Clone, Copy)]
struct Mapping {
    id: DeviceId,
    scan: bt_discovery::DeviceId,
    record: bt_discovery::DeviceRecord,
    bond: Option<bool>,
}
#[derive(Clone, Copy)]
struct Challenge {
    id: u64,
    token: ApprovalToken,
    number: u32,
}

/// Stable HTTP management ownership survives replacement of a clean connection
/// attempt. Application is borrowed for each step; external platform teardown
/// and reconstruction must preserve its Recovery and establish a newer epoch.
pub struct Runner<'a, const N: usize> {
    config: Config,
    capabilities: Capabilities,
    inquiry: Inquiry<'a>,
    scanning: bool,
    manager: Manager<N>,
    legacy: Control,
    mapping: [Option<Mapping>; N],
    len: usize,
    selected: Option<DeviceId>,
    next_device: u64,
    next_challenge: u64,
    challenge: Option<Challenge>,
    scan_epoch: u64,
    link_epoch: u64,
    active_link_epoch: Option<u64>,
    last_us: u64,
    regular: Option<Ticket>,
    urgent: Option<Ticket>,
    legacy_ticket: Option<PendingCommand>,
    last_error: Option<i32>,
    lookup_failure: Option<LookupFailure>,
    cancelled_scan: bool,
    clock_failed: bool,
    shutting_down: bool,
}
impl<'a, const N: usize> Runner<'a, N> {
    pub fn new(
        config: Config,
        capabilities: Capabilities,
        slots: &'a mut [ResultSlot],
        now: u64,
    ) -> Result<Self, Error> {
        if N == 0
            || N > 256
            || slots.len() != N
            || config.session == 0
            || config.first_scan_epoch == 0
            || config.first_link_epoch == 0
            || config.result_valid_us == 0
        {
            return Err(Error::InvalidConfig);
        }
        let mut inquiry = Inquiry::new(config.first_scan_epoch, now, config.inquiry, slots)
            .map_err(Error::Discovery)?;
        inquiry.cancel(now).map_err(Error::Discovery)?; // no radio submission exists yet
        let status = Status {
            transport: capabilities.transport,
            capture: capabilities.capture,
            codec: capabilities.codec,
            playback: capabilities.playback,
            link: LinkState::Disconnected,
            stream: StreamState::Stopped,
            last_error: None,
            revision: 0,
        };
        let snapshot = Snapshot {
            status,
            owner_phase: OwnerPhase::Idle,
            cleanup_required: false,
            scanning: false,
            selected: None,
            approval: None,
            available: Actions::none(),
        };
        let manager = Manager::new(config.session, snapshot).map_err(Error::Management)?;
        Ok(Self {
            config,
            capabilities,
            inquiry,
            scanning: false,
            manager,
            legacy: Control::new(status),
            mapping: [None; N],
            len: 0,
            selected: None,
            next_device: 0,
            next_challenge: 0,
            challenge: None,
            scan_epoch: config.first_scan_epoch,
            link_epoch: config.first_link_epoch,
            active_link_epoch: None,
            last_us: now,
            regular: None,
            urgent: None,
            legacy_ticket: None,
            last_error: None,
            lookup_failure: None,
            cancelled_scan: false,
            clock_failed: false,
            shutting_down: false,
        })
    }
    pub fn lookup_failure(&self) -> Option<LookupFailure> {
        self.lookup_failure
    }
    pub fn manager(&self) -> &Manager<N> {
        &self.manager
    }
    /// Only authenticated service submission may mutate these queues; their
    /// observations and completion tickets remain owned by this runner.
    pub fn controls(&mut self) -> (&mut Control, &mut Manager<N>) {
        (&mut self.legacy, &mut self.manager)
    }
    pub fn inquiry_snapshot(&self) -> bt_discovery::Snapshot {
        self.inquiry.snapshot()
    }
    pub const fn active_link_epoch(&self) -> Option<u64> {
        self.active_link_epoch
    }
    pub const fn scan_epoch(&self) -> u64 {
        self.scan_epoch
    }
    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    /// Latch global shutdown before another management command can execute.
    /// This cancels discovery and requests ordered application disconnect; it
    /// does not claim native cleanup or throw away a healthy partial media PDU.
    pub fn request_shutdown<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<(), Error> {
        self.time(app, now)?;
        if self.shutting_down {
            return Ok(());
        }
        self.shutting_down = true;
        self.challenge = None;
        if self.scanning && !self.inquiry.is_quiescent() {
            self.inquiry.cancel(now).map_err(Error::Discovery)?;
            self.cancelled_scan = true;
        }
        if !matches!(
            app.status().phase,
            Phase::Idle | Phase::Closed | Phase::Failed
        ) {
            app.disconnect(now).map_err(Error::Application)?;
        }
        self.publish(app, now)
    }
    /// Advance only bounded protocol teardown. The owner must continue inbound
    /// routing/control output and healthy media-prefix drain while this returns
    /// false. true is protocol quiescence, never native transfer/credit proof.
    pub fn poll_shutdown<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<bool, Error> {
        self.time(app, now)?;
        if !self.shutting_down {
            return Err(Error::InvalidState);
        }
        if self.scanning && self.inquiry.tick(now).is_err() {
            self.last_error = Some(-101);
        }
        if let Err(error) = app.tick(now) {
            self.last_error = Some(-102);
            if app.status().phase != Phase::Failed {
                return Err(Error::Application(error));
            }
        }
        let quiescent = self.inquiry.is_quiescent()
            && matches!(app.status().phase, Phase::Idle | Phase::Closed)
            && !app.status().cleanup_required;
        if quiescent {
            self.finish_shutdown_tickets(app, now, CommandOutcome::Cancelled)?;
            self.scanning = false;
        }
        self.publish(app, now)?;
        Ok(quiescent)
    }
    /// Clock-independent terminal cancellation. Native cleanup/reconciliation
    /// remains required; the caller owns accepted-prefix evidence and resources.
    pub fn abort_shutdown<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
    ) -> Result<(), Error> {
        self.shutting_down = true;
        self.challenge = None;
        if !matches!(app.status().phase, Phase::Idle | Phase::Closed) {
            app.cancel();
        }
        if !self.inquiry.is_quiescent() {
            let _ = self.inquiry.fail_transport(bt_core::Error::IntegrityLost);
        }
        self.last_error = Some(-107);
        self.finish_shutdown_tickets(app, self.last_us, CommandOutcome::Failed(-107))?;
        self.publish(app, self.last_us)
    }
    fn finish_shutdown_tickets<S: BondStore>(
        &mut self,
        app: &Application<'_, S>,
        now: u64,
        outcome: CommandOutcome,
    ) -> Result<(), Error> {
        for _ in 0..2 {
            if let Some(ticket) = self.manager.take_pending() {
                if self.manager.urgent().is_some_and(|p| p.ticket == ticket) {
                    self.urgent = Some(ticket);
                } else {
                    self.regular = Some(ticket);
                }
            }
        }
        for ticket in [self.urgent, self.regular].into_iter().flatten() {
            self.finish(ticket, outcome, app, now)?;
        }
        if let Some(ticket) = self.legacy.pending() {
            self.legacy.finish(ticket, outcome).map_err(Error::Legacy)?;
        }
        self.legacy_ticket = None;
        Ok(())
    }
    fn time<S: BondStore>(&mut self, app: &mut Application<'_, S>, now: u64) -> Result<(), Error> {
        if self.clock_failed {
            return Err(Error::ClockRegressed);
        }
        if now < self.last_us {
            self.clock_failed = true;
            app.cancel();
            let _ = self.inquiry.fail_transport(bt_core::Error::IntegrityLost);
            self.challenge = None;
            self.last_error = Some(-100);
            // Publish using the last accepted monotonic instant. No store lookup,
            // native cleanup or retry is permitted on an invalid clock sample.
            self.publish(app, self.last_us)?;
            // Claim at most the two bounded queue slots, including work that has
            // not started, so no command remains executable after this failure.
            for _ in 0..2 {
                if let Some(ticket) = self.manager.take_pending() {
                    if self.manager.urgent().is_some_and(|p| p.ticket == ticket) {
                        self.urgent = Some(ticket);
                    } else {
                        self.regular = Some(ticket);
                    }
                }
            }
            for ticket in [self.urgent, self.regular].into_iter().flatten() {
                self.finish(ticket, CommandOutcome::Failed(-100), app, self.last_us)?;
            }
            if let Some(ticket) = self.legacy.pending() {
                self.legacy
                    .finish(ticket, CommandOutcome::Failed(-100))
                    .map_err(Error::Legacy)?;
            }
            self.legacy_ticket = None;
            self.publish(app, self.last_us)?;
            return Err(Error::ClockRegressed);
        }
        self.last_us = now;
        Ok(())
    }
    /// One observation/command quantum. No waits, native calls or hidden retry.
    pub fn step<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<(), Error> {
        self.time(app, now)?;
        if self.shutting_down {
            return self.poll_shutdown(app, now).map(|_| ());
        }
        if self.scanning && self.inquiry.tick(now).is_err() {
            self.last_error = Some(-101);
        }
        if let Err(error) = app.tick(now) {
            self.last_error = Some(-102);
            if app.status().phase != Phase::Failed {
                return Err(Error::Application(error));
            }
        }
        self.observe(app, now)?;
        self.finish_ready(app, now)?;
        if let Some(ticket) = self.manager.take_pending() {
            if self.manager.urgent().is_some_and(|p| p.ticket == ticket) {
                self.urgent = Some(ticket);
            } else {
                self.regular = Some(ticket);
            }
            if let Err(error) = self.execute(app, ticket.action, now) {
                self.last_error = Some(-103);
                self.finish(ticket, CommandOutcome::Failed(-103), app, now)?;
                return Err(error);
            }
        } else if self.legacy_ticket.is_none()
            && self.regular.is_none()
            && self.urgent.is_none()
            && let Some(ticket) = self.legacy.pending()
        {
            self.legacy_ticket = Some(ticket);
            let action = match ticket.command {
                bt_control::Command::Connect => Action::Connect,
                bt_control::Command::Start => Action::Start,
                bt_control::Command::Stop => Action::Stop,
            };
            if let Err(error) = self.execute(app, action, now) {
                self.legacy
                    .finish(ticket, CommandOutcome::Failed(-103))
                    .map_err(Error::Legacy)?;
                self.legacy_ticket = None;
                return Err(error);
            }
        }
        self.observe(app, now)?;
        self.finish_ready(app, now)
    }
    pub fn receive_hci<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        route: Route,
        epoch: u64,
        now: u64,
        packet: &[u8],
    ) -> Result<(), Error> {
        self.time(app, now)?;
        let result = match route {
            Route::Discovery => self
                .inquiry
                .receive(epoch, now, packet)
                .map(|_| ())
                .map_err(Error::Discovery),
            Route::Application => {
                if self.active_link_epoch != Some(epoch) {
                    return Err(Error::InvalidState);
                }
                app.receive_hci(epoch, now, packet)
                    .map(|_| ())
                    .map_err(Error::Application)
            }
        };
        if result.is_err() {
            self.last_error = Some(-105);
        }
        self.observe(app, now)?;
        self.finish_ready(app, now)?;
        result
    }
    /// Caller must establish command authority/allowance; Busy rejects the
    /// whole command. Scratch is scrubbed even on failure and must not be logged.
    pub fn pump_discovery<S: BondStore, T: crate::CommandSink + ?Sized>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
        transport: &mut T,
        scratch: &mut [u8],
    ) -> Result<bool, Error> {
        self.time(app, now)?;
        if !self.scanning {
            return Ok(false);
        }
        let Some(ticket) = self
            .inquiry
            .prepare_command(now, scratch)
            .map_err(Error::Discovery)?
        else {
            return Ok(false);
        };
        let result = transport.send_owned_command(self.scan_epoch, &scratch[..ticket.len()]);
        scratch.fill(0);
        match result {
            Ok(()) => {
                self.inquiry
                    .confirm_accepted(ticket)
                    .map_err(Error::Discovery)?;
                Ok(true)
            }
            Err(bt_core::Error::Busy) => Ok(false),
            Err(error) => {
                let _ = self.inquiry.fail_transport(error);
                self.last_error = Some(-104);
                Err(Error::Transport(error))
            }
        }
    }

    /// Command-only bring-up before a connection/ACL reservation exists. A
    /// non-command offer is retained and reports false; establish the real ACL
    /// owner and use OutputPump next. Never drive two command pumps concurrently.
    pub fn pump_application_commands<S: BondStore, T: crate::CommandSink + ?Sized>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
        transport: &mut T,
        scratch: &mut [u8],
    ) -> Result<bool, Error> {
        self.time(app, now)?;
        if self.scanning {
            return Err(Error::InvalidState);
        }
        let Some(epoch) = self.active_link_epoch else {
            return Ok(false);
        };
        let Some(submission) = app.prepare(now, scratch).map_err(Error::Application)? else {
            return Ok(false);
        };
        let bt_app::Submission::Command { len, .. } = submission else {
            scratch.fill(0);
            return Ok(false);
        };
        let result = transport.send_owned_command(epoch, &scratch[..len]);
        scratch.fill(0);
        match result {
            Ok(()) => {
                app.confirm_accepted(now, submission)
                    .map_err(Error::Application)?;
                Ok(true)
            }
            Err(bt_core::Error::Busy) => Ok(false),
            Err(error) => {
                let _ = app.submission_failed(submission, error);
                self.last_error = Some(-104);
                Err(Error::Transport(error))
            }
        }
    }
    fn execute<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        action: Action,
        now: u64,
    ) -> Result<(), Error> {
        if !self.snapshot(app, now).available.contains(action) {
            return Err(Error::Unavailable);
        }
        match action {
            Action::Scan => {
                if !matches!(app.status().phase, Phase::Idle | Phase::Closed)
                    || !self.inquiry.is_quiescent()
                {
                    return Err(Error::InvalidState);
                }
                self.scan_epoch = self.scan_epoch.checked_add(1).ok_or(Error::Exhausted)?;
                self.inquiry
                    .reset(self.scan_epoch, now, self.config.inquiry)
                    .map_err(Error::Discovery)?;
                self.lookup_failure = None;
                self.mapping.fill(None);
                self.len = 0;
                self.selected = None;
                self.challenge = None;
                self.scanning = true;
                self.cancelled_scan = false;
            }
            Action::CancelScan => {
                self.inquiry.cancel(now).map_err(Error::Discovery)?;
                self.cancelled_scan = true;
            }
            Action::Select(id) => {
                let candidate = self.candidate(id, now)?;
                self.inquiry
                    .connection_candidate(candidate.scan)
                    .map_err(Error::Discovery)?;
                self.selected = Some(id);
            }
            Action::Connect | Action::Pair => {
                let candidate = self.candidate(self.selected.ok_or(Error::StaleDevice)?, now)?;
                self.inquiry
                    .connection_candidate(candidate.scan)
                    .map_err(Error::Discovery)?;
                if action == Action::Pair && candidate.bond != Some(false) {
                    return Err(Error::Unavailable);
                }
                self.link_epoch = self.link_epoch.checked_add(1).ok_or(Error::Exhausted)?;
                app.connect_discovered(
                    candidate.record.peer,
                    candidate.record.page_scan_repetition,
                    self.link_epoch,
                    now,
                )
                .map_err(Error::Application)?;
                self.active_link_epoch = Some(self.link_epoch);
            }
            Action::Approve { token } | Action::Reject { token } => {
                let challenge = self
                    .challenge
                    .filter(|c| c.id == token)
                    .ok_or(Error::InvalidState)?;
                app.approve(
                    challenge.token,
                    now,
                    matches!(action, Action::Approve { .. }),
                )
                .map_err(Error::Application)?;
            }
            Action::Start => app.start(now).map_err(Error::Application)?,
            Action::Stop => app.stop(now).map_err(Error::Application)?,
            Action::Disconnect => app.disconnect(now).map_err(Error::Application)?,
            Action::Forget => {
                if app.status().device.forget_pending {
                    app.retry_forget().map_err(Error::Application)?;
                } else {
                    let selected = self.mapping[..self.len]
                        .iter()
                        .flatten()
                        .find(|map| Some(map.id) == self.selected)
                        .copied()
                        .ok_or(Error::StaleDevice)?;
                    let outcome = app
                        .forget_device(selected.record.peer, now)
                        .map_err(Error::Application)?;
                    if outcome.persistence.is_err() {
                        return Err(Error::Unavailable);
                    }
                }
                if let Some(id) = self.selected
                    && let Some(m) = self.mapping[..self.len]
                        .iter_mut()
                        .flatten()
                        .find(|m| m.id == id)
                {
                    m.bond = Some(false);
                }
            }
        }
        Ok(())
    }
    fn candidate(&self, id: DeviceId, now: u64) -> Result<Mapping, Error> {
        let mapping = self.mapping[..self.len]
            .iter()
            .flatten()
            .find(|m| m.id == id)
            .copied()
            .ok_or(Error::StaleDevice)?;
        if now
            .checked_sub(mapping.record.last_seen_us)
            .is_none_or(|age| age > self.config.result_valid_us)
        {
            return Err(Error::ExpiredDevice);
        }
        Ok(mapping)
    }
    fn observe<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<(), Error> {
        // One new device/store lookup per observation. Known identities update only
        // bounded metadata; no unbounded filesystem work is hidden in polling.
        for record in self.inquiry.results() {
            if let Some(map) = self.mapping[..self.len]
                .iter_mut()
                .flatten()
                .find(|map| map.scan == record.id)
            {
                map.record = *record;
                continue;
            }
            if self.len == N {
                break;
            }
            self.next_device = self.next_device.checked_add(1).ok_or(Error::Exhausted)?;
            let bond = match app.has_bond(record.peer) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.last_error = Some(-106);
                    self.lookup_failure = Some(LookupFailure {
                        device: DeviceId(self.next_device),
                        error,
                    });
                    None
                }
            };
            self.mapping[self.len] = Some(Mapping {
                id: DeviceId(self.next_device),
                scan: record.id,
                record: *record,
                bond,
            });
            self.len += 1;
            break;
        }
        match app.pending_confirmation() {
            Some((token, number)) if self.challenge.is_none_or(|old| old.token != token) => {
                self.next_challenge = self.next_challenge.checked_add(1).ok_or(Error::Exhausted)?;
                self.challenge = Some(Challenge {
                    id: self.next_challenge,
                    token,
                    number,
                });
            }
            None => self.challenge = None,
            _ => {}
        }
        self.publish(app, now)
    }
    fn publish<S: BondStore>(&mut self, app: &Application<'_, S>, now: u64) -> Result<(), Error> {
        let snapshot = self.snapshot(app, now);
        let placeholder = Device {
            id: DeviceId(1),
            name: Name::new("").map_err(Error::Management)?,
            bonded: None,
            audio_sink: None,
        };
        let mut devices = [placeholder; N];
        for (device, map) in devices
            .iter_mut()
            .zip(self.mapping[..self.len].iter().flatten())
        {
            *device = Device {
                id: map.id,
                name: Name::new(
                    map.record
                        .name
                        .as_ref()
                        .map_or("Bluetooth device", |name| name.as_str()),
                )
                .map_err(Error::Management)?,
                bonded: map.bond,
                audio_sink: None,
            };
        }
        self.manager
            .observe(snapshot, &devices[..self.len])
            .map_err(Error::Management)?;
        if self.legacy.pending().is_none() {
            let _ = self.legacy.observe(snapshot.status);
        }
        Ok(())
    }
    fn snapshot<S: BondStore>(&self, app: &Application<'_, S>, now: u64) -> Snapshot {
        let state = app.status();
        let scan = self.inquiry.snapshot();
        let scanning = !self.clock_failed
            && self.scanning
            && !self.inquiry.is_quiescent()
            && scan.failure.is_none();
        let cleanup = state.phase == Phase::Failed && state.cleanup_required
            || scan.cleanup_required && scan.failure.is_some();
        let owner_phase = if cleanup {
            OwnerPhase::RequiresCleanup
        } else if self.clock_failed {
            OwnerPhase::Failed
        } else if scanning {
            OwnerPhase::Scanning
        } else {
            match state.phase {
                Phase::Idle | Phase::Closed => OwnerPhase::Idle,
                Phase::Connecting => {
                    if state.device.approval_pending {
                        OwnerPhase::Pairing
                    } else {
                        OwnerPhase::Connecting
                    }
                }
                Phase::Discovering => OwnerPhase::Connecting,
                Phase::Connected | Phase::Stopped => OwnerPhase::Connected,
                Phase::Negotiating => OwnerPhase::Starting,
                Phase::Streaming => OwnerPhase::Streaming,
                Phase::Stopping => OwnerPhase::Stopping,
                Phase::Disconnecting => OwnerPhase::Disconnecting,
                Phase::Failed => OwnerPhase::Failed,
            }
        };
        let status = Status {
            transport: if self.clock_failed {
                Capability::Unavailable
            } else {
                self.capabilities.transport
            },
            capture: self.capabilities.capture,
            codec: self.capabilities.codec,
            playback: self.capabilities.playback,
            link: if state.device.ready {
                LinkState::Connected
            } else if matches!(state.phase, Phase::Idle | Phase::Closed) {
                LinkState::Disconnected
            } else {
                LinkState::Connecting
            },
            stream: match state.phase {
                Phase::Negotiating => StreamState::Starting,
                Phase::Streaming => StreamState::Streaming,
                Phase::Stopping => StreamState::Stopping,
                _ => StreamState::Stopped,
            },
            last_error: self.last_error,
            revision: 0,
        };
        let mut available = Actions::none();
        if scanning {
            available = available.with(Action::CancelScan);
        }
        if !scanning && !cleanup && matches!(state.phase, Phase::Idle | Phase::Closed) {
            if self.capabilities.discovery == Capability::Ready
                && self.capabilities.transport == Capability::Ready
            {
                available = available.with(Action::Scan);
            }
            if self.inquiry.is_quiescent() && self.len > 0 {
                available = available.with(Action::Select(DeviceId(0)));
            }
        }
        if state.phase == Phase::Idle
            && !scanning
            && !cleanup
            && self
                .selected
                .is_some_and(|id| self.candidate(id, now).is_ok())
            && self.capabilities.transport == Capability::Ready
        {
            available = available.with(Action::Connect).with(Action::Pair);
        }
        if matches!(state.phase, Phase::Connected | Phase::Stopped)
            && self.capabilities.playback == Capability::Ready
            && self.capabilities.capture == Capability::Ready
            && self.capabilities.codec == Capability::Ready
            && self.capabilities.transport == Capability::Ready
        {
            available = available.with(Action::Start);
        }
        if matches!(
            state.phase,
            Phase::Negotiating | Phase::Streaming | Phase::Stopping
        ) {
            available = available.with(Action::Stop);
        }
        if !matches!(state.phase, Phase::Idle | Phase::Closed | Phase::Failed) {
            available = available.with(Action::Disconnect);
        }
        if let Some(challenge) = self.challenge {
            available = available
                .with(Action::Approve {
                    token: challenge.id,
                })
                .with(Action::Reject {
                    token: challenge.id,
                });
        }
        if self.selected.is_some()
            && (matches!(state.phase, Phase::Idle | Phase::Closed)
                || state.device.selected
                || state.device.forget_pending)
        {
            available = available.with(Action::Forget);
        }
        if self.clock_failed || self.shutting_down {
            available = Actions::none();
        }
        Snapshot {
            status,
            owner_phase,
            cleanup_required: cleanup,
            scanning,
            selected: self.selected,
            approval: self.challenge.and_then(|c| {
                Some(Approval {
                    token: c.id,
                    device: self.selected?,
                    number: Some(c.number),
                })
            }),
            available,
        }
    }
    fn finish_ready<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<(), Error> {
        for ticket in [self.urgent, self.regular].into_iter().flatten() {
            if let Some(outcome) = self.outcome(ticket.action, app) {
                self.finish(ticket, outcome, app, now)?;
            }
        }
        if let Some(ticket) = self.legacy_ticket {
            let action = match ticket.command {
                bt_control::Command::Connect => Action::Connect,
                bt_control::Command::Start => Action::Start,
                bt_control::Command::Stop => Action::Stop,
            };
            if let Some(outcome) = self.outcome(action, app) {
                self.legacy.finish(ticket, outcome).map_err(Error::Legacy)?;
                self.legacy_ticket = None;
            }
        }
        Ok(())
    }
    fn outcome<S: BondStore>(
        &self,
        action: Action,
        app: &Application<'_, S>,
    ) -> Option<CommandOutcome> {
        let state = app.status();
        match action {
            Action::Scan | Action::CancelScan if self.inquiry.snapshot().failure.is_some() => {
                Some(CommandOutcome::Failed(-101))
            }
            Action::Scan | Action::CancelScan if self.inquiry.is_quiescent() => {
                Some(if action == Action::Scan && self.cancelled_scan {
                    CommandOutcome::Cancelled
                } else {
                    CommandOutcome::Success
                })
            }
            Action::Scan | Action::CancelScan => None,
            Action::Select(_) | Action::Approve { .. } | Action::Reject { .. } | Action::Forget => {
                Some(CommandOutcome::Success)
            }
            _ if state.phase == Phase::Failed => Some(CommandOutcome::Failed(-102)),
            Action::Connect | Action::Pair if state.phase == Phase::Connected => {
                Some(CommandOutcome::Success)
            }
            Action::Start if state.phase == Phase::Streaming => Some(CommandOutcome::Success),
            Action::Stop if state.phase == Phase::Stopped => Some(CommandOutcome::Success),
            Action::Disconnect if state.phase == Phase::Closed => Some(CommandOutcome::Success),
            Action::Connect | Action::Pair | Action::Start if state.phase == Phase::Closed => {
                Some(CommandOutcome::Cancelled)
            }
            Action::Start if state.phase == Phase::Stopped => Some(CommandOutcome::Cancelled),
            _ => None,
        }
    }
    fn finish<S: BondStore>(
        &mut self,
        ticket: Ticket,
        outcome: CommandOutcome,
        app: &Application<'_, S>,
        now: u64,
    ) -> Result<(), Error> {
        self.manager
            .finish(ticket, outcome, self.snapshot(app, now))
            .map_err(Error::Management)?;
        if self.regular == Some(ticket) {
            self.regular = None;
        }
        if self.urgent == Some(ticket) {
            self.urgent = None;
        }
        if matches!(ticket.action, Action::Scan | Action::CancelScan) && self.inquiry.is_quiescent()
        {
            self.scanning = false;
        }
        Ok(())
    }
}
