use std::{path::Path, time::Duration};

use occurframe_conformance::{canonical_json_line, score, sha256_hex};
use occurframe_wire::{ConformanceVerdict, Diagnostic, NormalizedObservation, Vector};
use serde::{Deserialize, Serialize};

use crate::{DEFAULT_STDERR_TAIL_BYTES, ProtocolSchema, Result, RunnerBuild, RunnerSupervisor};

/// Non-semantic transport diagnostic retained separately from normalized output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerDiagnostic {
    pub diagnostic: Diagnostic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
}

/// One case execution, including the canonical observation and conformance-owned
/// verdict. `build_id` and transport diagnostics are not serialized into the
/// semantic observation bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseExecution {
    pub build_id: String,
    pub observation: NormalizedObservation,
    pub verdict: ConformanceVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_diagnostic: Option<RunnerDiagnostic>,
}

/// Execute a deterministic vector subset against every selected runner build.
#[must_use]
pub fn run_batch(
    builds: &[RunnerBuild],
    vectors: &[Vector],
    repository_root: &Path,
    schema: &ProtocolSchema,
    infrastructure_watchdog: Duration,
) -> Vec<CaseExecution> {
    let mut ordered_builds = builds.to_vec();
    ordered_builds.sort_by(|left, right| left.build_id.cmp(&right.build_id));
    let mut ordered_vectors = vectors.to_vec();
    ordered_vectors.sort_by(|left, right| left.id.cmp(&right.id));

    let mut records = Vec::new();
    for build in ordered_builds {
        let mut supervisor = RunnerSupervisor::new(
            build,
            repository_root.to_path_buf(),
            schema.clone(),
            infrastructure_watchdog,
            DEFAULT_STDERR_TAIL_BYTES,
        );
        for (index, vector) in ordered_vectors.iter().enumerate() {
            records.push(supervisor.execute(vector, index));
        }
    }
    records.sort_by(|left, right| {
        left.observation
            .vector_id
            .cmp(&right.observation.vector_id)
            .then_with(|| left.build_id.cmp(&right.build_id))
    });
    records
}

/// Canonical, LF-terminated NDJSON containing only normalized semantic records.
pub fn semantic_observation_ndjson(records: &[CaseExecution]) -> Result<Vec<u8>> {
    let mut ordered: Vec<_> = records.iter().collect();
    ordered.sort_by(|left, right| {
        left.observation
            .vector_id
            .cmp(&right.observation.vector_id)
            .then_with(|| left.build_id.cmp(&right.build_id))
    });
    let mut bytes = Vec::new();
    for record in ordered {
        bytes.extend(canonical_json_line(&record.observation)?);
    }
    Ok(bytes)
}

/// SHA-256 digest of the canonical semantic observation NDJSON.
pub fn semantic_observation_digest(records: &[CaseExecution]) -> Result<String> {
    Ok(sha256_hex(&semantic_observation_ndjson(records)?))
}

pub(crate) fn execution(
    build_id: String,
    vector: &Vector,
    observation: NormalizedObservation,
    runner_diagnostic: Option<RunnerDiagnostic>,
) -> CaseExecution {
    let verdict = score(&vector.expectation, &observation);
    CaseExecution {
        build_id,
        observation,
        verdict,
        runner_diagnostic,
    }
}
