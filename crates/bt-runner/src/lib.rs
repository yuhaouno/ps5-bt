// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded management bridge and owned platform stepping, without platform APIs.
#![no_std]
#![forbid(unsafe_code)]

mod application_loop;
mod audio_pipeline;
mod control_candidate;
mod main_loop;
mod management;
mod pump;
mod rebuild;
mod startup;
pub use management::{Capabilities, Config, Error, LookupFailure, Route, Runner};
pub use pump::{AudioStep, CommandSink, InputPump, OutputPump, PumpError, PumpProgress};

pub use main_loop::{
    Continuity, Lane, LoopConfig, LoopPhase, LoopReport, LoopResources, LoopStatistics, MainLoop,
    Quantum, Resource, StopReason, Turn,
};

pub use audio_pipeline::{
    AudioPipeline, PipelineCleanup, PipelineError, PipelineFailure, PipelineSnapshot, SendBuffers,
};

pub use rebuild::{
    CaptureEncoderResources, RebuildError, RebuildFactory, RebuildOwner, RebuildResource,
    RebuildSnapshot, ReopenFailure, ResourceCleanup,
};

pub use application_loop::{
    ApplicationLoop, ApplicationLoopError, ApplicationLoopSnapshot, ApplicationLoopState, HttpWork,
    LoopBuffers, NoHttp, OwnedInput, OwnedPacket, OwnedPacketKind,
};

pub use startup::{
    DriveBudget, DriveFailure, DriveReport, DriveResources, StoreLease, StoreSlot, drive_bounded,
};

pub use control_candidate::{
    ControlStorage, ControlStorageParts, control_application_config, control_recovery_policy,
    control_runner_config,
};
