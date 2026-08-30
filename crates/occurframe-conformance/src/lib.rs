//! Authority-layer logic for the Occurframe conformance oracle.
//!
//! The crate loads and validates authored corpus data, verifies the RC1-to-RC2
//! migration, serializes releases deterministically, normalizes observations,
//! selects expectations, and scores observations. It contains no recurrence engine.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

mod canonical;
mod corpus;
mod migration;
mod observation;
mod pack;
mod scoring;

pub use canonical::{canonical_json, canonical_json_line, canonical_pretty_json, sha256_hex};
pub use corpus::{Corpus, ValidationReport, load_and_validate_corpus, validate_schemas};
pub use migration::{MigrationReport, migrate_rc1, verify_migration};
pub use observation::{
    normalize_completed, normalize_execution_failure, semantic_observation_digest,
};
pub use pack::{
    PackReport, pack_release, verify_deterministic_pack, verify_manifest, write_tree_checksums,
};
pub use scoring::score;

use std::io;

/// Errors produced by authority-layer operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("schema error: {0}")]
    Schema(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("migration error: {0}")]
    Migration(String),
}

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;
