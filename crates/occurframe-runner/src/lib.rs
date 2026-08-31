//! Process supervision for language-neutral Occurframe protocol-v3 runners.
//!
//! The crate contains transport and containment logic only. Recurrence parsing,
//! recurrence evaluation, timezone resolution, and scoring policy remain outside
//! this crate. Completed and failed observations are normalized and scored by
//! `occurframe-conformance`.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

mod batch;
mod config;
mod diagnostics;
mod environment;
mod process;

pub use batch::{
    CaseExecution, RunnerDiagnostic, run_batch, semantic_observation_digest,
    semantic_observation_ndjson,
};
pub use config::{
    LaunchConfig, ProtocolSchema, Reproducibility, ReproducibilityStatus, RunnerBuild,
    RunnerRegistry, TzdbReleaseKind,
};
pub use diagnostics::DEFAULT_STDERR_TAIL_BYTES;
pub use process::{DEFAULT_INFRASTRUCTURE_WATCHDOG, RunnerSupervisor};

use std::io;

/// Errors in configuration, serialization, or top-level batch setup.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("protocol schema error: {0}")]
    ProtocolSchema(String),
    #[error("conformance error: {0}")]
    Conformance(#[from] occurframe_conformance::Error),
}

/// Result type for runner orchestration.
pub type Result<T> = std::result::Result<T, Error>;
