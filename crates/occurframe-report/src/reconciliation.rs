use std::collections::{BTreeMap, BTreeSet};

use occurframe_runner::{CaseExecution, ReproducibilityStatus, RunnerRegistry};
use occurframe_wire::{EngineOutcome, ExecutionStatus, TzdbRelease};
use serde_json::Value;

use crate::{
    Error, Result,
    model::{
        CertificationProfile, LegacyBuildMap, LegacyBuildMapping, LegacyTzdb, OutcomeKind,
        ReconciliationCategory, ReconciliationCell, ReconciliationReport, outcome_kind,
    },
};

pub(crate) fn reconcile(
    profile: &CertificationProfile,
    registry: &RunnerRegistry,
    corpus: &occurframe_conformance::Corpus,
    records: &[CaseExecution],
    legacy_matrix: &Value,
    build_map: &LegacyBuildMap,
    environment: &Value,
) -> Result<ReconciliationReport> {
    validate_map(registry, build_map)?;
    let cells = legacy_matrix
        .get("cells")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            Error::Legacy("legacy matrix does not contain an object named cells".into())
        })?;
    let current_by_cell: BTreeMap<_, _> = records
        .iter()
        .map(|record| {
            (
                (
                    record.build_id.as_str(),
                    record.observation.vector_id.as_str(),
                ),
                record,
            )
        })
        .collect();
    let vectors: BTreeMap<_, _> = corpus
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let corrections: BTreeMap<_, _> = build_map
        .adapter_corrections
        .iter()
        .flat_map(|correction| {
            correction.build_ids.iter().map(|build_id| {
                (
                    (build_id.as_str(), correction.operation.as_str()),
                    correction.evidence.as_str(),
                )
            })
        })
        .collect();
    let canonical_platform_matches = environment
        .pointer("/observed/os/id")
        .and_then(Value::as_str)
        == Some("ubuntu")
        && environment
            .pointer("/observed/os/version_id")
            .and_then(Value::as_str)
            == Some("24.04")
        && environment
            .pointer("/observed/architecture")
            .and_then(Value::as_str)
            .is_some_and(|arch| arch == "x86_64" || arch == "amd64");

    let mut reconciliation_cells = Vec::new();
    let mut comparable_cells = 0_usize;
    let mut observed_differences = 0_usize;
    let mut ambiguous_inventory = 0_usize;

    for mapping in &build_map.mappings {
        for vector_id in legacy_vector_ids(cells, &mapping.legacy_engine_key) {
            let legacy_key = format!("{vector_id}||{}", mapping.legacy_engine_key);
            let legacy = cells
                .get(&legacy_key)
                .ok_or_else(|| Error::Legacy(format!("missing legacy cell {legacy_key}")))?;
            let legacy_status = legacy
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Legacy(format!("{legacy_key} has no status")))?;
            let current = current_by_cell.get(&(mapping.build_id.as_str(), vector_id.as_str()));
            let vector = vectors.get(vector_id.as_str()).ok_or_else(|| {
                Error::Legacy(format!("legacy cell references unknown vector {vector_id}"))
            })?;
            if legacy_status == "error" {
                ambiguous_inventory += 1;
                reconciliation_cells.push(ReconciliationCell {
                    vector_id: vector_id.clone(),
                    build_id: mapping.build_id.clone(),
                    legacy_engine_key: mapping.legacy_engine_key.clone(),
                    category: ReconciliationCategory::LegacyAmbiguousError,
                    legacy_status: legacy_status.into(),
                    new_outcome: current.map(|record| {
                        outcome_kind(
                            record.observation.execution_status,
                            record.observation.engine_outcome.as_ref(),
                        )
                    }),
                    evidence: "RC1 retained only its overloaded error status; no safe v2 rejection/error attribution is possible".into(),
                });
                if current.is_some() {
                    comparable_cells += 1;
                }
                continue;
            }
            let Some(current) = current else {
                continue;
            };
            comparable_cells += 1;
            let (category, evidence, differs) = classify_comparison(
                mapping,
                vector,
                current,
                legacy,
                canonical_platform_matches,
                corrections
                    .get(&(mapping.build_id.as_str(), vector.operation.as_str()))
                    .copied(),
            );
            if differs {
                observed_differences += 1;
            }
            reconciliation_cells.push(ReconciliationCell {
                vector_id: vector_id.clone(),
                build_id: mapping.build_id.clone(),
                legacy_engine_key: mapping.legacy_engine_key.clone(),
                category,
                legacy_status: legacy_status.into(),
                new_outcome: Some(outcome_kind(
                    current.observation.execution_status,
                    current.observation.engine_outcome.as_ref(),
                )),
                evidence,
            });
        }
    }
    reconciliation_cells.sort_by(|left, right| {
        left.vector_id
            .cmp(&right.vector_id)
            .then_with(|| left.build_id.cmp(&right.build_id))
    });

    if ambiguous_inventory != profile.reconciliation.known_ambiguous_legacy_cells {
        return Err(Error::Legacy(format!(
            "expected {} legacy ambiguous errors, found {ambiguous_inventory}",
            profile.reconciliation.known_ambiguous_legacy_cells
        )));
    }

    let mut category_counts = BTreeMap::new();
    for cell in &reconciliation_cells {
        *category_counts
            .entry(reconciliation_name(cell.category).to_owned())
            .or_default() += 1;
    }
    for category in [
        ReconciliationCategory::ExactMatch,
        ReconciliationCategory::ProtocolRefinement,
        ReconciliationCategory::TzdbProvenanceChange,
        ReconciliationCategory::RuntimeEnvironmentChange,
        ReconciliationCategory::AdapterCorrection,
        ReconciliationCategory::FreshEngineBehaviorDifference,
        ReconciliationCategory::LegacyAmbiguousError,
        ReconciliationCategory::UnresolvedDifference,
    ] {
        category_counts
            .entry(reconciliation_name(category).to_owned())
            .or_insert(0);
    }
    let unresolved_differences = reconciliation_cells
        .iter()
        .filter(|cell| cell.category == ReconciliationCategory::UnresolvedDifference)
        .cloned()
        .collect();
    let mut not_comparable_builds: Vec<_> = registry
        .builds
        .iter()
        .filter(|build| build.reproducibility.status == ReproducibilityStatus::Unreproducible)
        .map(|build| build.build_id.clone())
        .collect();
    not_comparable_builds.sort();

    Ok(ReconciliationReport {
        schema_version: "1.0.0".into(),
        legacy_status: "historical_evidence_only".into(),
        comparable_cells,
        classified_observed_differences: observed_differences,
        category_counts,
        legacy_ambiguous_error_inventory: ambiguous_inventory,
        not_comparable_builds,
        cells: reconciliation_cells,
        unresolved_differences,
    })
}

fn legacy_vector_ids(cells: &serde_json::Map<String, Value>, engine_key: &str) -> Vec<String> {
    let suffix = format!("||{engine_key}");
    let mut ids: Vec<_> = cells
        .keys()
        .filter_map(|key| key.strip_suffix(&suffix).map(ToOwned::to_owned))
        .collect();
    ids.sort();
    ids
}

fn validate_map(registry: &RunnerRegistry, build_map: &LegacyBuildMap) -> Result<()> {
    if build_map.schema_version != "1.0.0" {
        return Err(Error::Legacy(format!(
            "unsupported legacy mapping schema {}",
            build_map.schema_version
        )));
    }
    let registry_ids: BTreeSet<_> = registry
        .builds
        .iter()
        .map(|build| build.build_id.as_str())
        .collect();
    let mapping_ids: BTreeSet<_> = build_map
        .mappings
        .iter()
        .map(|mapping| mapping.build_id.as_str())
        .collect();
    if registry_ids != mapping_ids {
        return Err(Error::Legacy(
            "legacy build mapping must cover the exact configured build set".into(),
        ));
    }
    if build_map
        .mappings
        .iter()
        .any(|mapping| mapping.identity_confidence != "exact")
    {
        return Err(Error::Legacy(
            "certification may only compare exact identity mappings".into(),
        ));
    }
    let builds: BTreeMap<_, _> = registry
        .builds
        .iter()
        .map(|build| (build.build_id.as_str(), build))
        .collect();
    let mut correction_keys = BTreeSet::new();
    for correction in &build_map.adapter_corrections {
        if correction.build_ids.is_empty() || correction.evidence.trim().is_empty() {
            return Err(Error::Legacy(
                "adapter correction requires build identities and concrete evidence".into(),
            ));
        }
        for build_id in &correction.build_ids {
            let build = builds.get(build_id.as_str()).ok_or_else(|| {
                Error::Legacy(format!("adapter correction names unknown build {build_id}"))
            })?;
            if !build.supported_operations.contains(&correction.operation) {
                return Err(Error::Legacy(format!(
                    "adapter correction {build_id} / {} is not a configured operation",
                    correction.operation
                )));
            }
            if !correction_keys.insert((build_id.as_str(), correction.operation.as_str())) {
                return Err(Error::Legacy(format!(
                    "duplicate adapter correction {build_id} / {}",
                    correction.operation
                )));
            }
        }
    }
    Ok(())
}

fn classify_comparison(
    mapping: &LegacyBuildMapping,
    vector: &occurframe_wire::Vector,
    current: &CaseExecution,
    legacy: &Value,
    platform_matches: bool,
    correction_evidence: Option<&str>,
) -> (ReconciliationCategory, String, bool) {
    let legacy_status = legacy
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let current_kind = outcome_kind(
        current.observation.execution_status,
        current.observation.engine_outcome.as_ref(),
    );
    let current_is_semantic = matches!(
        current_kind,
        OutcomeKind::Occurrences | OutcomeKind::Accepted | OutcomeKind::Rejection
    );

    if legacy_status == "unsupported_op" && current_kind == OutcomeKind::Unsupported {
        return (
            ReconciliationCategory::ProtocolRefinement,
            "RC1 unsupported_op is represented by protocol-v2 unsupported without treating it as semantic disagreement".into(),
            false,
        );
    }
    if legacy_status == "empty"
        && matches!(
            current.observation.engine_outcome,
            Some(EngineOutcome::Occurrences { ref occurrences }) if occurrences.is_empty()
        )
    {
        return (
            ReconciliationCategory::ProtocolRefinement,
            "RC1 empty status is represented by a successful protocol-v2 occurrences outcome containing []".into(),
            false,
        );
    }
    if legacy_status == "timeout"
        && current.observation.execution_status == ExecutionStatus::Timeout
    {
        return (
            ReconciliationCategory::ExactMatch,
            "both runs observed the configured execution budget expire".into(),
            false,
        );
    }
    if legacy_status == "ok" {
        let legacy_occurrences = legacy.get("occurrences").and_then(Value::as_array);
        let same = match (
            legacy_occurrences,
            current.observation.engine_outcome.as_ref(),
        ) {
            (Some(legacy_values), Some(EngineOutcome::Occurrences { occurrences })) => {
                legacy_values.len() == occurrences.len()
                    && legacy_values
                        .iter()
                        .zip(occurrences)
                        .all(|(left, right)| left.as_str() == Some(right.as_str()))
            }
            _ => false,
        };
        if same {
            return (
                ReconciliationCategory::ExactMatch,
                "native occurrence sequence is byte-for-byte equal in original order".into(),
                false,
            );
        }
    }

    if let Some(evidence) = correction_evidence {
        return (
            ReconciliationCategory::AdapterCorrection,
            evidence.into(),
            true,
        );
    }
    let tzdb_sensitive = vector.family == "tzdb.provenance"
        || vector
            .semantic_axes
            .iter()
            .any(|axis| axis.starts_with("tz."));
    if current_is_semantic
        && tzdb_sensitive
        && !tzdb_matches(
            &mapping.legacy_tzdb,
            &current.observation.tzdb_provenance.release,
        )
    {
        return (
            ReconciliationCategory::TzdbProvenanceChange,
            "recorded v2 tzdb provenance differs from the exact/bounded RC1 provenance mapping"
                .into(),
            true,
        );
    }
    if current_is_semantic
        && legacy_status == "ok"
        && (current.observation.runtime.version != mapping.legacy_runtime_version
            || !platform_matches)
    {
        return (
            ReconciliationCategory::RuntimeEnvironmentChange,
            "the pinned runtime or canonical platform differs from the mapped RC1 environment"
                .into(),
            true,
        );
    }
    if current_is_semantic && legacy_status == "ok" {
        return (
            ReconciliationCategory::FreshEngineBehaviorDifference,
            "the same recorded engine, runtime, and tzdb identity produced materially different semantic behavior".into(),
            true,
        );
    }
    (
        ReconciliationCategory::UnresolvedDifference,
        "available evidence is insufficient for a safe attribution".into(),
        true,
    )
}

fn tzdb_matches(legacy: &LegacyTzdb, current: &TzdbRelease) -> bool {
    match (legacy, current) {
        (LegacyTzdb::Exact { release: left }, TzdbRelease::Exact { release: right }) => {
            left == right
        }
        (
            LegacyTzdb::Bounded {
                min_inclusive: left_min,
                max_inclusive: left_max,
            },
            TzdbRelease::Bounded {
                min_inclusive: right_min,
                max_inclusive: right_max,
            },
        ) => left_min == right_min && left_max == right_max,
        (LegacyTzdb::Unknown, TzdbRelease::Unknown) => true,
        _ => false,
    }
}

pub(crate) fn reconciliation_name(category: ReconciliationCategory) -> &'static str {
    match category {
        ReconciliationCategory::ExactMatch => "exact_match",
        ReconciliationCategory::ProtocolRefinement => "protocol_refinement",
        ReconciliationCategory::TzdbProvenanceChange => "tzdb_provenance_change",
        ReconciliationCategory::RuntimeEnvironmentChange => "runtime_environment_change",
        ReconciliationCategory::AdapterCorrection => "adapter_correction",
        ReconciliationCategory::FreshEngineBehaviorDifference => "fresh_engine_behavior_difference",
        ReconciliationCategory::LegacyAmbiguousError => "legacy_ambiguous_error",
        ReconciliationCategory::UnresolvedDifference => "unresolved_difference",
    }
}
