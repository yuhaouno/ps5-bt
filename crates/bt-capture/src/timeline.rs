// SPDX-License-Identifier: GPL-3.0-or-later
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTime {
    pub value: u64,
    /// Caller-defined domain, never inferred from opaque Avcap2 metadata.
    pub domain: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    None,
    Frames(u64),
    Unknown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeFlags {
    pub native_regressed: bool,
    pub native_jumped: bool,
    pub native_domain_changed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureStamp {
    pub host_observed_us: u64,
    pub native: Option<NativeTime>,
    pub first_sample: u64,
    pub flags: TimeFlags,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    HostRegressed,
    Overflow,
    UnknownGap,
}
/// Host observation, raw native units and sample positions are independent.
/// A raw jump threshold, when supplied, is explicitly in that native domain's units.
#[derive(Clone, Copy, Debug)]
pub struct Timeline {
    next_sample: u64,
    last_host: Option<u64>,
    last_native: Option<NativeTime>,
    max_native_step: Option<u64>,
    unknown_gap: bool,
}
impl Timeline {
    pub const fn new(first_sample: u64, max_native_step: Option<u64>) -> Self {
        Self {
            next_sample: first_sample,
            last_host: None,
            last_native: None,
            max_native_step,
            unknown_gap: false,
        }
    }
    pub const fn next_sample(&self) -> u64 {
        self.next_sample
    }
    pub fn observe(
        &mut self,
        host: u64,
        native: Option<NativeTime>,
        frames: usize,
        gap: Gap,
    ) -> Result<CaptureStamp, TimeError> {
        if self.last_host.is_some_and(|last| host < last) {
            return Err(TimeError::HostRegressed);
        }
        if gap == Gap::Unknown {
            self.unknown_gap = true;
            self.last_host = Some(host);
        }
        if self.unknown_gap {
            return Err(TimeError::UnknownGap);
        }
        let skipped = match gap {
            Gap::Frames(n) => n,
            _ => 0,
        };
        let first_sample = self
            .next_sample
            .checked_add(skipped)
            .ok_or(TimeError::Overflow)?;
        let next = first_sample
            .checked_add(frames as u64)
            .ok_or(TimeError::Overflow)?;
        let mut flags = TimeFlags::default();
        if let (Some(previous), Some(current)) = (self.last_native, native) {
            flags.native_domain_changed = current.domain != previous.domain;
            if !flags.native_domain_changed {
                flags.native_regressed = current.value < previous.value;
                flags.native_jumped = current
                    .value
                    .checked_sub(previous.value)
                    .is_some_and(|delta| self.max_native_step.is_some_and(|bound| delta > bound));
            }
        }
        self.next_sample = next;
        self.last_host = Some(host);
        if native.is_some() {
            self.last_native = native;
        }
        Ok(CaptureStamp {
            host_observed_us: host,
            native,
            first_sample,
            flags,
        })
    }
    /// Explicit new stream/sample epoch after unknown loss. Preserve host monotonicity.
    pub fn restart(&mut self, first_sample: u64) {
        self.next_sample = first_sample;
        self.unknown_gap = false;
        self.last_native = None;
    }
}
