use std::collections::BTreeMap;

use occurframe_wire::{
    Classification, ComponentIdentity, ConformanceVerdict, Diagnostic, EngineOutcome,
    ExecutionStatus, RuntimeIdentity, SemanticValue, TzdbProvenance, VerdictStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertificationProfile {
    pub schema_version: String,
    pub certification_profile_version: String,
    pub certification_id: String,
    pub status: String,
    pub release_posture: Value,
    pub corpus: ProfileCorpus,
    pub tooling: ProfileTooling,
    pub runner_protocol_version: String,
    pub runner_versions: BTreeMap<String, String>,
    pub engine_configuration_set: ProfileBuildSet,
    pub execution: ProfileExecution,
    pub canonical_platform: Value,
    pub required_provenance_fields: Value,
    pub determinism: ProfileDeterminism,
    pub reconciliation: ProfileReconciliation,
    pub artifact_policy: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCorpus {
    pub repository: String,
    pub branch: String,
    pub sha: String,
    pub corpus_version: String,
    pub authority: String,
    pub vector_count: usize,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileTooling {
    pub repository: String,
    pub branch: String,
    pub baseline_sha: String,
    pub intended_source_policy: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileBuildSet {
    pub registry: String,
    pub configured_builds: usize,
    pub reproducible_builds: Vec<String>,
    pub unreproducible_builds: Vec<UnreproducibleBuild>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnreproducibleBuild {
    pub build_id: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileExecution {
    pub official_engine_timeout_ms: u64,
    pub infrastructure_watchdog_ms: u64,
    pub one_terminal_observation_per_build_vector: bool,
    pub vector_prefiltering: String,
    pub missing_capability_representation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDeterminism {
    pub runs_required: usize,
    pub comparison: String,
    pub semantic_artifacts: Vec<String>,
    pub excluded_from_comparison: Vec<String>,
    pub occurrence_ordering: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileReconciliation {
    pub legacy_evidence_root: String,
    pub legacy_status: String,
    pub known_ambiguous_legacy_cells: usize,
    pub phase_two_headline: String,
    pub headline_is_an_acceptance_target: bool,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBuildMap {
    pub schema_version: String,
    pub mappings: Vec<LegacyBuildMapping>,
    #[serde(default)]
    pub adapter_corrections: Vec<AdapterCorrection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBuildMapping {
    pub build_id: String,
    pub legacy_engine_key: String,
    pub identity_confidence: String,
    pub legacy_runtime_version: String,
    pub legacy_tzdb: LegacyTzdb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "release_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LegacyTzdb {
    Exact {
        release: String,
    },
    Bounded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_inclusive: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_inclusive: Option<String>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterCorrection {
    pub build_ids: Vec<String>,
    pub operation: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceResultRecord {
    pub vector_id: String,
    pub build_id: String,
    pub engine: ComponentIdentity,
    pub execution_status: ExecutionStatus,
    pub outcome: OutcomeKind,
    pub verdict: ConformanceVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Occurrences,
    Accepted,
    Rejection,
    Unsupported,
    EngineError,
    Timeout,
    RunnerFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticAnswer {
    Occurrences { occurrences: Vec<String> },
    Accepted,
    Rejection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerGroup {
    pub answer_id: String,
    pub answer: SemanticAnswer,
    pub builds: Vec<String>,
    pub verdict_counts: BTreeMap<String, usize>,
    pub matched_cases: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceKind {
    NormativeViolation,
    DocumentedPolicyDifference,
    DocumentedDialectDifference,
    StandardAmbiguity,
    KnownImplementationDivergence,
    TzdbProvenanceDifference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DivergenceRecord {
    pub vector_id: String,
    pub classification: Classification,
    pub semantic_axes: Vec<String>,
    pub kind: DivergenceKind,
    pub answer_groups: Vec<AnswerGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixVector {
    pub vector_id: String,
    pub family: String,
    pub title: String,
    pub operation: String,
    pub classification: Classification,
    pub semantic_axes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRecord {
    pub build_id: String,
    pub runner: ComponentIdentity,
    pub engine: ComponentIdentity,
    pub runtime: RuntimeIdentity,
    pub dialect_ids: Vec<String>,
    pub semantic_profile_claims: BTreeMap<String, SemanticValue>,
    pub tzdb_provenance: TzdbProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixCell {
    pub vector_id: String,
    pub build_id: String,
    pub execution_status: ExecutionStatus,
    pub outcome_kind: OutcomeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_outcome: Option<EngineOutcome>,
    pub verdict: ConformanceVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_answer_id: Option<String>,
    pub warnings: Vec<Diagnostic>,
    pub provenance_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixSummary {
    pub configured_builds: usize,
    pub reproducible_builds: usize,
    pub unreproducible_builds: usize,
    pub vectors: usize,
    pub expected_observations: usize,
    pub actual_observations: usize,
    pub outcome_counts: BTreeMap<String, usize>,
    pub verdict_counts: BTreeMap<String, usize>,
    pub family_counts: BTreeMap<String, usize>,
    pub classification_counts: BTreeMap<String, usize>,
    pub semantic_divergence_vectors: usize,
    pub normative_violation_vectors: usize,
    pub documented_policy_difference_vectors: usize,
    pub documented_dialect_difference_vectors: usize,
    pub named_policy_conformant_cells: usize,
    pub named_dialect_conformant_cells: usize,
    pub policy_vectors_with_named_conformant_answer: usize,
    pub dialect_vectors_with_named_conformant_answer: usize,
    pub ambiguous_standard_vectors: usize,
    pub ambiguous_standard_divergent_vectors: usize,
    pub tzdb_difference_vectors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentialMatrix {
    pub schema_version: String,
    pub certification_id: String,
    pub certification_profile_version: String,
    pub tooling_source_sha: String,
    pub corpus_sha: String,
    pub corpus_version: String,
    pub runner_protocol_version: String,
    pub canonical_platform: String,
    pub summary: MatrixSummary,
    pub provenance: Vec<ProvenanceRecord>,
    pub vectors: Vec<MatrixVector>,
    pub cells: Vec<MatrixCell>,
    pub semantic_divergences: Vec<DivergenceRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationCategory {
    ExactMatch,
    ProtocolRefinement,
    TzdbProvenanceChange,
    RuntimeEnvironmentChange,
    AdapterCorrection,
    FreshEngineBehaviorDifference,
    LegacyAmbiguousError,
    UnresolvedDifference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationCell {
    pub vector_id: String,
    pub build_id: String,
    pub legacy_engine_key: String,
    pub category: ReconciliationCategory,
    pub legacy_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_outcome: Option<OutcomeKind>,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationReport {
    pub schema_version: String,
    pub legacy_status: String,
    pub comparable_cells: usize,
    pub classified_observed_differences: usize,
    pub category_counts: BTreeMap<String, usize>,
    pub legacy_ambiguous_error_inventory: usize,
    pub not_comparable_builds: Vec<String>,
    pub cells: Vec<ReconciliationCell>,
    pub unresolved_differences: Vec<ReconciliationCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDigest {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertificationManifest {
    pub schema_version: String,
    pub artifact_kind: String,
    pub certification_id: String,
    pub certification_profile_version: String,
    pub tooling_source_sha: String,
    pub corpus_repository: String,
    pub corpus_sha: String,
    pub corpus_version: String,
    pub runner_protocol_version: String,
    pub configured_builds: usize,
    pub reproducible_builds: usize,
    pub unreproducible_builds: usize,
    pub vectors: usize,
    pub observations: usize,
    pub semantic_bundle_digest: String,
    pub semantic_artifacts: Vec<ArtifactDigest>,
    pub environment_artifact: ArtifactDigest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertificationSummary {
    pub output_directory: String,
    pub configured_builds: usize,
    pub reproducible_builds: usize,
    pub unreproducible_builds: usize,
    pub vectors: usize,
    pub observations: usize,
    pub semantic_bundle_digest: String,
    pub outcome_counts: BTreeMap<String, usize>,
    pub verdict_counts: BTreeMap<String, usize>,
    pub semantic_divergence_vectors: usize,
    pub normative_violation_vectors: usize,
    pub reconciliation_counts: BTreeMap<String, usize>,
}

pub(crate) fn outcome_kind(
    status: ExecutionStatus,
    outcome: Option<&EngineOutcome>,
) -> OutcomeKind {
    match (status, outcome) {
        (ExecutionStatus::Timeout, _) => OutcomeKind::Timeout,
        (ExecutionStatus::RunnerFailure, _) | (ExecutionStatus::Completed, None) => {
            OutcomeKind::RunnerFailure
        }
        (_, Some(EngineOutcome::Occurrences { .. })) => OutcomeKind::Occurrences,
        (_, Some(EngineOutcome::Accepted)) => OutcomeKind::Accepted,
        (_, Some(EngineOutcome::Rejection { .. })) => OutcomeKind::Rejection,
        (_, Some(EngineOutcome::Unsupported { .. })) => OutcomeKind::Unsupported,
        (_, Some(EngineOutcome::EngineError { .. })) => OutcomeKind::EngineError,
    }
}

pub(crate) fn semantic_answer(outcome: Option<&EngineOutcome>) -> Option<SemanticAnswer> {
    match outcome {
        Some(EngineOutcome::Occurrences { occurrences }) => Some(SemanticAnswer::Occurrences {
            occurrences: occurrences.clone(),
        }),
        Some(EngineOutcome::Accepted) => Some(SemanticAnswer::Accepted),
        Some(EngineOutcome::Rejection { .. }) => Some(SemanticAnswer::Rejection),
        Some(EngineOutcome::Unsupported { .. } | EngineOutcome::EngineError { .. }) | None => None,
    }
}

pub(crate) fn verdict_name(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Conformant => "conformant",
        VerdictStatus::ConformantAdmissible => "conformant_admissible",
        VerdictStatus::NonConformant => "non_conformant",
        VerdictStatus::Unsupported => "unsupported",
        VerdictStatus::Timeout => "timeout",
        VerdictStatus::InfrastructureFailure => "infrastructure_failure",
        VerdictStatus::RecordedUnscored => "recorded_unscored",
    }
}

pub(crate) fn outcome_name(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::Occurrences => "occurrences",
        OutcomeKind::Accepted => "accepted",
        OutcomeKind::Rejection => "rejection",
        OutcomeKind::Unsupported => "unsupported",
        OutcomeKind::EngineError => "engine_error",
        OutcomeKind::Timeout => "timeout",
        OutcomeKind::RunnerFailure => "runner_failure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic() -> Diagnostic {
        Diagnostic {
            code: "test".into(),
            message: "test".into(),
            details: None,
        }
    }

    #[test]
    fn semantic_answers_exclude_execution_pathologies() {
        assert!(
            semantic_answer(Some(&EngineOutcome::Unsupported {
                diagnostic: diagnostic(),
            }))
            .is_none()
        );
        assert!(
            semantic_answer(Some(&EngineOutcome::EngineError {
                diagnostic: diagnostic(),
            }))
            .is_none()
        );
        assert!(semantic_answer(None).is_none());
    }

    #[test]
    fn semantic_occurrence_answer_retains_order_and_duplicates() {
        let outcome = EngineOutcome::Occurrences {
            occurrences: vec!["b".into(), "a".into(), "a".into()],
        };
        assert_eq!(
            semantic_answer(Some(&outcome)),
            Some(SemanticAnswer::Occurrences {
                occurrences: vec!["b".into(), "a".into(), "a".into()]
            })
        );
    }

    #[test]
    fn engine_error_and_infrastructure_have_distinct_outcome_kinds() {
        assert_eq!(
            outcome_kind(
                ExecutionStatus::Completed,
                Some(&EngineOutcome::EngineError {
                    diagnostic: diagnostic()
                })
            ),
            OutcomeKind::EngineError
        );
        assert_eq!(
            outcome_kind(ExecutionStatus::Timeout, None),
            OutcomeKind::Timeout
        );
        assert_eq!(
            outcome_kind(ExecutionStatus::RunnerFailure, None),
            OutcomeKind::RunnerFailure
        );
    }
}
