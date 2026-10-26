// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Config, ConfigError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ResourceKind {
    Clock,
    Store,
    Listener,
    Capture,
    Encoder,
}
const ORDER: [ResourceKind; 5] = [
    ResourceKind::Clock,
    ResourceKind::Store,
    ResourceKind::Listener,
    ResourceKind::Capture,
    ResourceKind::Encoder,
];
/// Bounded redacted native error. Never contains a path, pointer, key or sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceError {
    pub code: i32,
    pub stage: u32,
    pub raw_status: i32,
    pub raw_lookup: i64,
    pub flags: u32,
    pub cleanup: [i32; 3],
    pub raw_time: [i64; 2],
}
impl ResourceError {
    pub const fn local(code: i32) -> Self {
        Self {
            code,
            stage: 0,
            raw_status: 0,
            raw_lookup: 0,
            flags: 0,
            cleanup: [0; 3],
            raw_time: [0; 2],
        }
    }
}
/// NativePorts below provides concrete existing constructors. This injection
/// boundary also permits failure tests without pretending simulation is PS5.
/// Each failed acquisition must consume its own partial native state. Cleanup
/// consumes once even on uncertainty; no method retries, creates radio authority
/// or resets a recovery budget. Explicit store handoff must be restored first.
pub trait ResourcePorts {
    fn acquire(&mut self, resource: ResourceKind, now_us: u64) -> Result<(), ResourceError>;
    fn cleanup(&mut self, resource: ResourceKind) -> Result<(), ResourceError>;
    /// True while ownership is explicitly handed to an external owner.
    fn cleanup_deferred(&self, _resource: ResourceKind) -> bool {
        false
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactoryPhase {
    Starting,
    Ready,
    Failed,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactoryReport {
    pub phase: FactoryPhase,
    pub acquired: [bool; 5],
    pub required: [bool; 5],
    pub cleanup: [Option<Result<(), ResourceError>>; 5],
    pub primary: Option<(ResourceKind, ResourceError)>,
}
impl FactoryReport {
    pub fn cleanup_uncertain(self) -> bool {
        ((self.phase == FactoryPhase::Failed || self.phase == FactoryPhase::Closed)
            && self.acquired.iter().any(|active| *active))
            || self
                .cleanup
                .iter()
                .any(|result| matches!(result, Some(Err(_))))
            || self.primary.is_some_and(|(_, error)| error.flags != 0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactoryFailure {
    pub resource: ResourceKind,
    pub error: ResourceError,
    pub report: FactoryReport,
}
pub struct ResourceFactory<'config, P: ResourcePorts> {
    pub(crate) ports: P,
    pub(crate) config: Config<'config>,
    report: FactoryReport,
}
impl<'config, P: ResourcePorts> ResourceFactory<'config, P> {
    pub fn new(config: Config<'config>, ports: P) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            ports,
            config,
            report: FactoryReport {
                phase: FactoryPhase::Starting,
                acquired: [false; 5],
                required: ORDER.map(|resource| config.required(resource)),
                cleanup: [None; 5],
                primary: None,
            },
        })
    }
    pub const fn config(&self) -> Config<'config> {
        self.config
    }
    pub const fn report(&self) -> FactoryReport {
        self.report
    }
    // Fault reports retain primary plus every cleanup result without allocation.
    #[allow(clippy::result_large_err)]
    pub fn acquire(&mut self, resource: ResourceKind, now_us: u64) -> Result<(), FactoryFailure> {
        let index = resource as usize;
        if !matches!(
            self.report.phase,
            FactoryPhase::Starting | FactoryPhase::Ready
        ) || self.report.cleanup[index].is_some()
        {
            return Err(self.failure(resource, ResourceError::local(-1001)));
        }
        if !self.report.required[index] {
            return Ok(());
        }
        if self.report.acquired[index] {
            return Ok(());
        }
        if ORDER[..index].iter().any(|earlier| {
            self.report.required[*earlier as usize] && !self.report.acquired[*earlier as usize]
        }) {
            return Err(self.failure(resource, ResourceError::local(-1002)));
        }
        if let Err(error) = self.ports.acquire(resource, now_us) {
            self.report.primary = Some((resource, error));
            self.report.phase = FactoryPhase::Failed;
            self.unwind();
            return Err(self.failure(resource, error));
        }
        self.report.acquired[index] = true;
        if ORDER
            .iter()
            .all(|r| !self.report.required[*r as usize] || self.report.acquired[*r as usize])
        {
            self.report.phase = FactoryPhase::Ready;
        }
        Ok(())
    }
    pub fn cleanup(&mut self, resource: ResourceKind) -> Result<(), ResourceError> {
        let index = resource as usize;
        if let Some(result) = self.report.cleanup[index] {
            return result;
        }
        if !self.report.acquired[index] {
            return Ok(());
        }
        if ORDER[index + 1..]
            .iter()
            .any(|later| self.report.acquired[*later as usize])
        {
            return Err(ResourceError::local(-1003));
        }
        if self.ports.cleanup_deferred(resource) {
            return Err(ResourceError {
                flags: 1,
                ..ResourceError::local(-1010)
            });
        }
        // Consume before calling the backend: cleanup failure cannot authorize retry.
        self.report.acquired[index] = false;
        let result = self.ports.cleanup(resource);
        self.report.cleanup[index] = Some(result);
        result
    }
    pub fn shutdown(&mut self) -> FactoryReport {
        self.unwind();
        if self.report.acquired.iter().any(|active| *active) {
            self.report.phase = FactoryPhase::Failed;
        } else if self.report.phase != FactoryPhase::Failed {
            self.report.phase = FactoryPhase::Closed;
        }
        self.report
    }
    fn unwind(&mut self) {
        for resource in ORDER.iter().rev() {
            let _ = self.cleanup(*resource);
        }
    }
    fn failure(&self, resource: ResourceKind, error: ResourceError) -> FactoryFailure {
        FactoryFailure {
            resource,
            error,
            report: self.report,
        }
    }
}
impl<P: ResourcePorts> Drop for ResourceFactory<'_, P> {
    fn drop(&mut self) {
        self.unwind();
    }
}
