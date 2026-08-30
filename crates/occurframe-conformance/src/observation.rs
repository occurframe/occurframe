use occurframe_wire::{
    ExecutionStatus, HelloMessage, NormalizedObservation, RUNNER_PROTOCOL_VERSION, ResultMessage,
};

use crate::{Result, canonical_json, sha256_hex};

/// Normalize a completed protocol-v2 result without changing occurrence order.
#[must_use]
pub fn normalize_completed(
    corpus_version: &str,
    vector_id: &str,
    hello: &HelloMessage,
    result: ResultMessage,
) -> NormalizedObservation {
    NormalizedObservation {
        protocol_version: RUNNER_PROTOCOL_VERSION.into(),
        corpus_version: corpus_version.into(),
        vector_id: vector_id.into(),
        runner: hello.runner.clone(),
        engine: hello.engine.clone(),
        runtime: hello.runtime.clone(),
        dialect_ids: hello.dialect_ids.clone(),
        semantic_profile_claims: hello.semantic_profile_claims.clone(),
        tzdb_provenance: hello.tzdb_provenance.clone(),
        execution_status: ExecutionStatus::Completed,
        engine_outcome: Some(result.outcome),
        warnings: result.warnings,
    }
}

/// Normalize timeout or runner failure. Neither is represented as an engine outcome.
#[must_use]
pub fn normalize_execution_failure(
    corpus_version: &str,
    vector_id: &str,
    hello: &HelloMessage,
    status: ExecutionStatus,
) -> NormalizedObservation {
    assert_ne!(status, ExecutionStatus::Completed);
    NormalizedObservation {
        protocol_version: RUNNER_PROTOCOL_VERSION.into(),
        corpus_version: corpus_version.into(),
        vector_id: vector_id.into(),
        runner: hello.runner.clone(),
        engine: hello.engine.clone(),
        runtime: hello.runtime.clone(),
        dialect_ids: hello.dialect_ids.clone(),
        semantic_profile_claims: hello.semantic_profile_claims.clone(),
        tzdb_provenance: hello.tzdb_provenance.clone(),
        execution_status: status,
        engine_outcome: None,
        warnings: Vec::new(),
    }
}

/// Digest the stable semantic observation. The wire model intentionally has no timing field.
pub fn semantic_observation_digest(observation: &NormalizedObservation) -> Result<String> {
    Ok(sha256_hex(&canonical_json(observation)?))
}
