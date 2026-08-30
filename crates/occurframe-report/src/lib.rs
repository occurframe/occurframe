//! Deterministic presentation of Occurframe differential evidence.
//!
//! This crate derives matrices, answer groups, summaries, provenance tables,
//! and Phase II reconciliation from already-normalized observations and
//! conformance-owned verdicts. It performs no recurrence evaluation and never
//! changes observations, expectations, or scoring.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

mod matrix;
mod model;
mod reconciliation;
mod render;

pub use model::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use occurframe_conformance::{canonical_json_line, canonical_pretty_json, sha256_hex};
use occurframe_runner::{
    CaseExecution, ReproducibilityStatus, RunnerRegistry, semantic_observation_ndjson,
};
use occurframe_wire::{ExecutionStatus, OFFICIAL_BUDGET_MS, RUNNER_PROTOCOL_VERSION};
use serde_json::Value;

const OBSERVATIONS: &str = "observations.ndjson";
const CONFORMANCE: &str = "conformance-results.ndjson";
const MATRIX_JSON: &str = "matrix.json";
const MATRIX_CSV: &str = "matrix.csv";
const DIFFERENTIAL_REPORT: &str = "differential-report.md";
const RECONCILIATION_JSON: &str = "reconciliation.json";
const RECONCILIATION_MD: &str = "reconciliation.md";
const ENVIRONMENT: &str = "environment.json";
const MANIFEST: &str = "certification-manifest.json";
const CHECKSUMS: &str = "SHA256SUMS";

pub const REQUIRED_SEMANTIC_ARTIFACTS: [&str; 7] = [
    OBSERVATIONS,
    CONFORMANCE,
    MATRIX_JSON,
    MATRIX_CSV,
    DIFFERENTIAL_REPORT,
    RECONCILIATION_JSON,
    RECONCILIATION_MD,
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("formatting error: {0}")]
    Format(#[from] std::fmt::Error),
    #[error("conformance error: {0}")]
    Conformance(#[from] occurframe_conformance::Error),
    #[error("runner error: {0}")]
    Runner(#[from] occurframe_runner::Error),
    #[error("certification validation error: {0}")]
    Validation(String),
    #[error("legacy reconciliation error: {0}")]
    Legacy(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct BundleInput<'a> {
    pub profile: &'a CertificationProfile,
    pub registry: &'a RunnerRegistry,
    pub corpus: &'a occurframe_conformance::Corpus,
    pub executions: &'a [CaseExecution],
    pub environment: &'a Value,
    pub tooling_source_sha: &'a str,
    pub legacy_matrix: &'a Value,
    pub legacy_build_map: &'a LegacyBuildMap,
    pub observation_schema: &'a Value,
    pub conformance_schema: &'a Value,
}

pub fn load_profile(path: &Path) -> Result<CertificationProfile> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn load_legacy_build_map(path: &Path) -> Result<LegacyBuildMap> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn load_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn generate_bundle(input: &BundleInput<'_>, output: &Path) -> Result<CertificationSummary> {
    validate_profile(input)?;
    validate_completeness(input)?;
    validate_environment(input.profile, input.environment)?;
    validate_observations(input.executions, input.observation_schema)?;

    let results = matrix::conformance_results(input.executions);
    validate_verdicts(&results, input.conformance_schema)?;
    let matrix = matrix::derive_matrix(
        input.profile,
        input.registry,
        input.corpus,
        input.executions,
        input.tooling_source_sha,
    )?;
    let reconciliation = reconciliation::reconcile(
        input.profile,
        input.registry,
        input.corpus,
        input.executions,
        input.legacy_matrix,
        input.legacy_build_map,
        input.environment,
    )?;

    let mut files = BTreeMap::<String, Vec<u8>>::new();
    files.insert(
        OBSERVATIONS.into(),
        semantic_observation_ndjson(input.executions)?,
    );
    files.insert(CONFORMANCE.into(), ndjson(&results)?);
    files.insert(MATRIX_JSON.into(), canonical_pretty_json(&matrix)?);
    files.insert(MATRIX_CSV.into(), render::matrix_csv(&matrix)?);
    files.insert(
        DIFFERENTIAL_REPORT.into(),
        render::differential_markdown(&matrix)?,
    );
    files.insert(
        RECONCILIATION_JSON.into(),
        canonical_pretty_json(&reconciliation)?,
    );
    files.insert(
        RECONCILIATION_MD.into(),
        render::reconciliation_markdown(&reconciliation)?,
    );
    files.insert(
        ENVIRONMENT.into(),
        canonical_pretty_json(input.environment)?,
    );

    let semantic_artifacts = artifact_digests(&files, &REQUIRED_SEMANTIC_ARTIFACTS)?;
    let semantic_bundle_digest = combined_digest(&semantic_artifacts);
    let environment_artifact = artifact_digest(
        ENVIRONMENT,
        files
            .get(ENVIRONMENT)
            .ok_or_else(|| Error::Validation("missing environment artifact".into()))?,
    );
    let manifest = CertificationManifest {
        schema_version: "1.0.0".into(),
        artifact_kind: "candidate_evidence_base_not_normative_authority".into(),
        certification_id: input.profile.certification_id.clone(),
        certification_profile_version: input.profile.certification_profile_version.clone(),
        tooling_source_sha: input.tooling_source_sha.into(),
        corpus_repository: input.profile.corpus.repository.clone(),
        corpus_sha: input.profile.corpus.sha.clone(),
        corpus_version: input.profile.corpus.corpus_version.clone(),
        runner_protocol_version: input.profile.runner_protocol_version.clone(),
        configured_builds: input.registry.builds.len(),
        reproducible_builds: input
            .profile
            .engine_configuration_set
            .reproducible_builds
            .len(),
        unreproducible_builds: input
            .profile
            .engine_configuration_set
            .unreproducible_builds
            .len(),
        vectors: input.corpus.vectors.len(),
        observations: input.executions.len(),
        semantic_bundle_digest: semantic_bundle_digest.clone(),
        semantic_artifacts,
        environment_artifact,
    };
    files.insert(MANIFEST.into(), canonical_pretty_json(&manifest)?);
    let checksums = checksum_text(&files);
    files.insert(CHECKSUMS.into(), checksums.into_bytes());
    write_files(output, &files)?;
    verify_bundle_checksums(output)?;

    Ok(CertificationSummary {
        output_directory: output.to_string_lossy().into_owned(),
        configured_builds: matrix.summary.configured_builds,
        reproducible_builds: matrix.summary.reproducible_builds,
        unreproducible_builds: matrix.summary.unreproducible_builds,
        vectors: matrix.summary.vectors,
        observations: matrix.summary.actual_observations,
        semantic_bundle_digest,
        outcome_counts: matrix.summary.outcome_counts.clone(),
        verdict_counts: matrix.summary.verdict_counts.clone(),
        semantic_divergence_vectors: matrix.summary.semantic_divergence_vectors,
        normative_violation_vectors: matrix.summary.normative_violation_vectors,
        reconciliation_counts: reconciliation.category_counts,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeterminismVerification {
    pub first_digest: String,
    pub second_digest: String,
    pub byte_equal: bool,
    pub artifacts: Vec<String>,
}

pub fn verify_deterministic_bundles(
    profile: &CertificationProfile,
    first: &Path,
    second: &Path,
) -> Result<DeterminismVerification> {
    let expected: BTreeSet<_> = REQUIRED_SEMANTIC_ARTIFACTS.into_iter().collect();
    let configured: BTreeSet<_> = profile
        .determinism
        .semantic_artifacts
        .iter()
        .map(String::as_str)
        .collect();
    if expected != configured {
        return Err(Error::Validation(
            "profile semantic artifact set differs from the certification contract".into(),
        ));
    }
    let mut first_digests = Vec::new();
    let mut second_digests = Vec::new();
    for name in REQUIRED_SEMANTIC_ARTIFACTS {
        let first_bytes = fs::read(first.join(name))?;
        let second_bytes = fs::read(second.join(name))?;
        if first_bytes != second_bytes {
            return Err(Error::Validation(format!(
                "semantic artifact {name} differs between certification runs"
            )));
        }
        first_digests.push(artifact_digest(name, &first_bytes));
        second_digests.push(artifact_digest(name, &second_bytes));
    }
    let first_digest = combined_digest(&first_digests);
    let second_digest = combined_digest(&second_digests);
    if first_digest != second_digest {
        return Err(Error::Validation(
            "semantic certification digests differ".into(),
        ));
    }
    verify_bundle_checksums(first)?;
    verify_bundle_checksums(second)?;
    Ok(DeterminismVerification {
        first_digest,
        second_digest,
        byte_equal: true,
        artifacts: REQUIRED_SEMANTIC_ARTIFACTS
            .iter()
            .map(ToString::to_string)
            .collect(),
    })
}

pub fn verify_bundle_checksums(directory: &Path) -> Result<()> {
    let contents = fs::read_to_string(directory.join(CHECKSUMS))?;
    for line in contents.lines() {
        let (expected, name) = line
            .split_once("  ")
            .ok_or_else(|| Error::Validation(format!("invalid checksum line {line:?}")))?;
        let bytes = fs::read(directory.join(name))?;
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(Error::Validation(format!(
                "digest mismatch for {name}: expected {expected}, found {actual}"
            )));
        }
    }
    Ok(())
}

fn validate_profile(input: &BundleInput<'_>) -> Result<()> {
    let profile = input.profile;
    if profile.schema_version != "1.0.0"
        || profile.runner_protocol_version != RUNNER_PROTOCOL_VERSION
        || profile.corpus.corpus_version != "1.0.0-rc2"
        || profile.corpus.vector_count != input.corpus.vectors.len()
        || profile.execution.official_engine_timeout_ms != OFFICIAL_BUDGET_MS
        || profile.execution.infrastructure_watchdog_ms != 30_000
        || !profile.execution.one_terminal_observation_per_build_vector
        || profile.execution.vector_prefiltering != "forbidden"
        || profile.execution.missing_capability_representation != "explicit unsupported observation"
        || profile.determinism.runs_required != 2
    {
        return Err(Error::Validation(
            "certification profile violates a frozen RC2 execution invariant".into(),
        ));
    }
    if input.tooling_source_sha.len() != 40
        || !input
            .tooling_source_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::Validation(
            "tooling source SHA must be a full 40-character Git object ID".into(),
        ));
    }
    if input.registry.builds.len() != profile.engine_configuration_set.configured_builds {
        return Err(Error::Validation(
            "profile configured build count differs from runner registry".into(),
        ));
    }
    let registry_reproducible: BTreeSet<_> = input
        .registry
        .builds
        .iter()
        .filter(|build| build.reproducibility.status == ReproducibilityStatus::Reproducible)
        .map(|build| build.build_id.as_str())
        .collect();
    let profile_reproducible: BTreeSet<_> = profile
        .engine_configuration_set
        .reproducible_builds
        .iter()
        .map(String::as_str)
        .collect();
    if registry_reproducible != profile_reproducible {
        return Err(Error::Validation(
            "profile reproducible build set differs from runner registry".into(),
        ));
    }
    let registry_unreproducible: BTreeSet<_> = input
        .registry
        .builds
        .iter()
        .filter(|build| build.reproducibility.status == ReproducibilityStatus::Unreproducible)
        .map(|build| build.build_id.as_str())
        .collect();
    let profile_unreproducible: BTreeSet<_> = profile
        .engine_configuration_set
        .unreproducible_builds
        .iter()
        .map(|build| build.build_id.as_str())
        .collect();
    if profile
        .engine_configuration_set
        .unreproducible_builds
        .iter()
        .any(|build| build.status != "unreproducible_provenance")
    {
        return Err(Error::Validation(
            "profile must classify every omitted build as unreproducible_provenance".into(),
        ));
    }
    if registry_unreproducible != profile_unreproducible {
        return Err(Error::Validation(
            "profile unreproducible build set differs from runner registry".into(),
        ));
    }
    for build in &input.registry.builds {
        let configured = profile.runner_versions.get(&build.runner.name);
        if configured != Some(&build.runner.version) {
            return Err(Error::Validation(format!(
                "runner version for {} differs from profile",
                build.build_id
            )));
        }
    }
    Ok(())
}

fn validate_completeness(input: &BundleInput<'_>) -> Result<()> {
    let expected_builds: BTreeSet<_> = input
        .profile
        .engine_configuration_set
        .reproducible_builds
        .iter()
        .map(String::as_str)
        .collect();
    let expected_vectors: BTreeSet<_> = input
        .corpus
        .vectors
        .iter()
        .map(|vector| vector.id.as_str())
        .collect();
    let expected_count = expected_builds.len() * expected_vectors.len();
    if input.executions.len() != expected_count {
        return Err(Error::Validation(format!(
            "expected {expected_count} observations, found {}",
            input.executions.len()
        )));
    }
    let mut observed = BTreeSet::new();
    let build_by_id: BTreeMap<_, _> = input
        .registry
        .builds
        .iter()
        .map(|build| (build.build_id.as_str(), build))
        .collect();
    for execution in input.executions {
        if !expected_builds.contains(execution.build_id.as_str())
            || !expected_vectors.contains(execution.observation.vector_id.as_str())
        {
            return Err(Error::Validation(format!(
                "unexpected certification cell {} / {}",
                execution.build_id, execution.observation.vector_id
            )));
        }
        if !observed.insert((
            execution.build_id.as_str(),
            execution.observation.vector_id.as_str(),
        )) {
            return Err(Error::Validation(format!(
                "duplicate certification cell {} / {}",
                execution.build_id, execution.observation.vector_id
            )));
        }
        if execution.observation.execution_status == ExecutionStatus::RunnerFailure {
            return Err(Error::Validation(format!(
                "unexpected runner_failure at {} / {}: {:?}",
                execution.build_id, execution.observation.vector_id, execution.runner_diagnostic
            )));
        }
        let build = build_by_id[execution.build_id.as_str()];
        if execution.observation.runner.provenance.is_none()
            || execution.observation.engine.provenance.is_none()
            || execution.observation.runtime.version != build.runtime_requirement
            || execution.observation.corpus_version != input.profile.corpus.corpus_version
            || execution.observation.protocol_version != input.profile.runner_protocol_version
        {
            return Err(Error::Validation(format!(
                "missing or mismatched provenance at {} / {}",
                execution.build_id, execution.observation.vector_id
            )));
        }
    }
    for build in expected_builds {
        for vector in &expected_vectors {
            if !observed.contains(&(build, *vector)) {
                return Err(Error::Validation(format!(
                    "missing certification cell {build} / {vector}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_environment(profile: &CertificationProfile, environment: &Value) -> Result<()> {
    for pointer in [
        "/canonical_platform",
        "/observed/os/id",
        "/observed/os/version_id",
        "/observed/architecture",
        "/container/image_reference",
        "/container/image_digest",
        "/runtimes",
        "/tzdb_provenance_sources",
        "/engine_dependencies",
    ] {
        if environment.pointer(pointer).is_none() {
            return Err(Error::Validation(format!(
                "environment evidence omits {pointer}"
            )));
        }
    }
    if environment
        .pointer("/container/image_digest")
        .and_then(Value::as_str)
        == Some("unrecorded")
        || environment
            .pointer("/container/base_image_digest")
            .and_then(Value::as_str)
            == Some("unrecorded")
    {
        return Err(Error::Validation(
            "canonical certification requires recorded container and base-image digests".into(),
        ));
    }
    let expected_platform = profile
        .canonical_platform
        .pointer("/identity")
        .and_then(Value::as_str);
    let expected_tzdb = profile
        .canonical_platform
        .pointer("/pinned_tzdb/system_zoneinfo_release")
        .and_then(Value::as_str);
    let expected_dataform = profile
        .canonical_platform
        .pointer("/pinned_tzdb/dataform")
        .and_then(Value::as_str);
    if environment
        .pointer("/canonical_platform")
        .and_then(Value::as_str)
        != expected_platform
        || environment
            .pointer("/observed/os/id")
            .and_then(Value::as_str)
            != Some("ubuntu")
        || environment
            .pointer("/observed/os/version_id")
            .and_then(Value::as_str)
            != Some("24.04")
        || environment
            .pointer("/observed/architecture")
            .and_then(Value::as_str)
            != Some("x86_64")
        || environment
            .pointer("/tzdb_provenance_sources/system_zoneinfo/release")
            .and_then(Value::as_str)
            != expected_tzdb
        || environment
            .pointer("/tzdb_provenance_sources/system_zoneinfo/dataform")
            .and_then(Value::as_str)
            != expected_dataform
    {
        return Err(Error::Validation(
            "observed platform or system tzdb differs from the canonical profile".into(),
        ));
    }
    let runtimes = environment
        .get("runtimes")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Validation("environment runtimes must be an array".into()))?;
    if runtimes
        .iter()
        .any(|runtime| runtime.get("matches_configured").and_then(Value::as_bool) != Some(true))
    {
        return Err(Error::Validation(
            "an observed runtime differs from its configured requirement".into(),
        ));
    }
    Ok(())
}

fn validate_observations(records: &[CaseExecution], schema: &Value) -> Result<()> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| Error::Validation(format!("observation schema: {error}")))?;
    for record in records {
        let value = serde_json::to_value(&record.observation)?;
        let errors: Vec<_> = validator
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "observation {} / {} is schema-invalid: {}",
                record.build_id,
                record.observation.vector_id,
                errors.join("; ")
            )));
        }
    }
    Ok(())
}

fn validate_verdicts(records: &[ConformanceResultRecord], schema: &Value) -> Result<()> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| Error::Validation(format!("conformance schema: {error}")))?;
    for record in records {
        let value = serde_json::to_value(&record.verdict)?;
        let errors: Vec<_> = validator
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "verdict {} / {} is schema-invalid: {}",
                record.build_id,
                record.vector_id,
                errors.join("; ")
            )));
        }
    }
    Ok(())
}

fn ndjson<T: serde::Serialize>(records: &[T]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(canonical_json_line(record)?);
    }
    Ok(bytes)
}

fn artifact_digests(
    files: &BTreeMap<String, Vec<u8>>,
    names: &[&str],
) -> Result<Vec<ArtifactDigest>> {
    names
        .iter()
        .map(|name| {
            files
                .get(*name)
                .map(|bytes| artifact_digest(name, bytes))
                .ok_or_else(|| Error::Validation(format!("missing semantic artifact {name}")))
        })
        .collect()
}

fn artifact_digest(path: &str, bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest {
        path: path.into(),
        sha256: sha256_hex(bytes),
        bytes: bytes.len(),
    }
}

fn combined_digest(artifacts: &[ArtifactDigest]) -> String {
    let mut material = Vec::new();
    for artifact in artifacts {
        material.extend_from_slice(artifact.path.as_bytes());
        material.push(0);
        material.extend_from_slice(artifact.sha256.as_bytes());
        material.push(b'\n');
    }
    sha256_hex(&material)
}

fn checksum_text(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut output = String::new();
    for (name, bytes) in files {
        output.push_str(&sha256_hex(bytes));
        output.push_str("  ");
        output.push_str(name);
        output.push('\n');
    }
    output
}

fn write_files(output: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    fs::create_dir_all(output)?;
    for (name, bytes) in files {
        fs::write(output.join(name), bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_contract_names_are_stable_and_complete() {
        assert_eq!(
            REQUIRED_SEMANTIC_ARTIFACTS,
            [
                "observations.ndjson",
                "conformance-results.ndjson",
                "matrix.json",
                "matrix.csv",
                "differential-report.md",
                "reconciliation.json",
                "reconciliation.md",
            ]
        );
    }

    #[test]
    fn combined_digest_includes_names_and_order() {
        let one = vec![ArtifactDigest {
            path: "a".into(),
            sha256: "1".into(),
            bytes: 1,
        }];
        let two = vec![ArtifactDigest {
            path: "b".into(),
            sha256: "1".into(),
            bytes: 1,
        }];
        assert_ne!(combined_digest(&one), combined_digest(&two));
    }
}
