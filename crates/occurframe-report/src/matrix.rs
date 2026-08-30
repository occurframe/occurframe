use std::collections::{BTreeMap, BTreeSet};

use occurframe_conformance::{canonical_json, sha256_hex};
use occurframe_runner::{CaseExecution, RunnerRegistry};
use occurframe_wire::{Classification, VerdictStatus};

use crate::{
    Error, Result,
    model::{
        AnswerGroup, CertificationProfile, ConformanceResultRecord, DifferentialMatrix,
        DivergenceKind, DivergenceRecord, MatrixCell, MatrixSummary, MatrixVector,
        ProvenanceRecord, SemanticAnswer, outcome_kind, outcome_name, semantic_answer,
        verdict_name,
    },
};

pub(crate) fn conformance_results(records: &[CaseExecution]) -> Vec<ConformanceResultRecord> {
    let mut results: Vec<_> = records
        .iter()
        .map(|record| ConformanceResultRecord {
            vector_id: record.observation.vector_id.clone(),
            build_id: record.build_id.clone(),
            engine: record.observation.engine.clone(),
            execution_status: record.observation.execution_status,
            outcome: outcome_kind(
                record.observation.execution_status,
                record.observation.engine_outcome.as_ref(),
            ),
            verdict: record.verdict.clone(),
        })
        .collect();
    results.sort_by(|left, right| {
        left.vector_id
            .cmp(&right.vector_id)
            .then_with(|| left.build_id.cmp(&right.build_id))
    });
    results
}

pub(crate) fn derive_matrix(
    profile: &CertificationProfile,
    registry: &RunnerRegistry,
    corpus: &occurframe_conformance::Corpus,
    records: &[CaseExecution],
    tooling_source_sha: &str,
) -> Result<DifferentialMatrix> {
    let mut answer_ids = BTreeMap::<(String, String), String>::new();
    let mut grouped = BTreeMap::<String, BTreeMap<SemanticAnswer, Vec<&CaseExecution>>>::new();
    for record in records {
        if let Some(answer) = semantic_answer(record.observation.engine_outcome.as_ref()) {
            let serialized = canonical_json(&answer)?;
            let answer_id = format!("sha256:{}", sha256_hex(&serialized));
            answer_ids.insert(
                (
                    record.observation.vector_id.clone(),
                    record.build_id.clone(),
                ),
                answer_id,
            );
            grouped
                .entry(record.observation.vector_id.clone())
                .or_default()
                .entry(answer)
                .or_default()
                .push(record);
        }
    }

    let by_vector: BTreeMap<_, _> = corpus
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let mut divergences = Vec::new();
    for (vector_id, answers) in &grouped {
        if answers.len() < 2 {
            continue;
        }
        let vector = by_vector
            .get(vector_id.as_str())
            .ok_or_else(|| Error::Validation(format!("missing vector {vector_id}")))?;
        let mut answer_groups = Vec::new();
        for (answer, answer_records) in answers {
            let answer_id = format!("sha256:{}", sha256_hex(&canonical_json(answer)?));
            let mut builds: Vec<_> = answer_records
                .iter()
                .map(|record| record.build_id.clone())
                .collect();
            builds.sort();
            let mut verdict_counts = BTreeMap::new();
            let mut matched_cases = BTreeSet::new();
            for record in answer_records {
                *verdict_counts
                    .entry(verdict_name(record.verdict.status).to_owned())
                    .or_default() += 1;
                if let Some(label) = &record.verdict.matched_case {
                    matched_cases.insert(label.clone());
                }
            }
            answer_groups.push(AnswerGroup {
                answer_id,
                answer: answer.clone(),
                builds,
                verdict_counts,
                matched_cases: matched_cases.into_iter().collect(),
            });
        }
        answer_groups.sort_by(|left, right| left.answer_id.cmp(&right.answer_id));
        divergences.push(DivergenceRecord {
            vector_id: vector.id.clone(),
            classification: vector.classification,
            semantic_axes: vector.semantic_axes.clone(),
            kind: divergence_kind(vector),
            answer_groups,
        });
    }
    divergences.sort_by(|left, right| left.vector_id.cmp(&right.vector_id));

    let mut provenance_by_build = BTreeMap::new();
    for record in records {
        provenance_by_build
            .entry(record.build_id.clone())
            .or_insert_with(|| ProvenanceRecord {
                build_id: record.build_id.clone(),
                runner: record.observation.runner.clone(),
                engine: record.observation.engine.clone(),
                runtime: record.observation.runtime.clone(),
                dialect_ids: record
                    .observation
                    .dialect_ids
                    .iter()
                    .map(|id| id.0.clone())
                    .collect(),
                semantic_profile_claims: record.observation.semantic_profile_claims.clone(),
                tzdb_provenance: record.observation.tzdb_provenance.clone(),
            });
    }

    let cells = records
        .iter()
        .map(|record| MatrixCell {
            vector_id: record.observation.vector_id.clone(),
            build_id: record.build_id.clone(),
            execution_status: record.observation.execution_status,
            outcome_kind: outcome_kind(
                record.observation.execution_status,
                record.observation.engine_outcome.as_ref(),
            ),
            engine_outcome: record.observation.engine_outcome.clone(),
            verdict: record.verdict.clone(),
            semantic_answer_id: answer_ids
                .get(&(
                    record.observation.vector_id.clone(),
                    record.build_id.clone(),
                ))
                .cloned(),
            warnings: record.observation.warnings.clone(),
            provenance_ref: record.build_id.clone(),
        })
        .collect();

    let vectors = corpus
        .vectors
        .iter()
        .map(|vector| MatrixVector {
            vector_id: vector.id.clone(),
            family: vector.family.clone(),
            title: vector.title.clone(),
            operation: vector.operation.clone(),
            classification: vector.classification,
            semantic_axes: vector.semantic_axes.clone(),
        })
        .collect();

    let summary = matrix_summary(registry, corpus, records, &divergences);
    Ok(DifferentialMatrix {
        schema_version: "1.0.0".into(),
        certification_id: profile.certification_id.clone(),
        certification_profile_version: profile.certification_profile_version.clone(),
        tooling_source_sha: tooling_source_sha.into(),
        corpus_sha: profile.corpus.sha.clone(),
        corpus_version: profile.corpus.corpus_version.clone(),
        runner_protocol_version: profile.runner_protocol_version.clone(),
        canonical_platform: profile
            .canonical_platform
            .pointer("/identity")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unrecorded")
            .into(),
        summary,
        provenance: provenance_by_build.into_values().collect(),
        vectors,
        cells,
        semantic_divergences: divergences,
    })
}

fn divergence_kind(vector: &occurframe_wire::Vector) -> DivergenceKind {
    let timezone_axis = vector
        .semantic_axes
        .iter()
        .any(|axis| axis.starts_with("tz."));
    let timezone_family = vector.family.to_ascii_lowercase().contains("tzdb");
    if timezone_axis || timezone_family {
        return DivergenceKind::TzdbProvenanceDifference;
    }
    match vector.classification {
        Classification::PolicyDependent => DivergenceKind::DocumentedPolicyDifference,
        Classification::DialectDependent => DivergenceKind::DocumentedDialectDifference,
        Classification::AmbiguousStandard => DivergenceKind::StandardAmbiguity,
        Classification::KnownDivergence => DivergenceKind::KnownImplementationDivergence,
        Classification::Normative | Classification::Invalid => DivergenceKind::NormativeViolation,
    }
}

fn matrix_summary(
    registry: &RunnerRegistry,
    corpus: &occurframe_conformance::Corpus,
    records: &[CaseExecution],
    divergences: &[DivergenceRecord],
) -> MatrixSummary {
    let mut outcome_counts = BTreeMap::new();
    let mut verdict_counts = BTreeMap::new();
    for record in records {
        let outcome = outcome_kind(
            record.observation.execution_status,
            record.observation.engine_outcome.as_ref(),
        );
        *outcome_counts
            .entry(outcome_name(outcome).to_owned())
            .or_default() += 1;
        *verdict_counts
            .entry(verdict_name(record.verdict.status).to_owned())
            .or_default() += 1;
    }
    let mut family_counts = BTreeMap::new();
    let mut classification_counts = BTreeMap::new();
    for vector in &corpus.vectors {
        *family_counts.entry(vector.family.clone()).or_default() += 1;
        *classification_counts
            .entry(classification_name(vector.classification).to_owned())
            .or_default() += 1;
    }

    let semantic_nonconformant_vectors: BTreeSet<_> = records
        .iter()
        .filter(|record| {
            record.verdict.status == VerdictStatus::NonConformant
                && semantic_answer(record.observation.engine_outcome.as_ref()).is_some()
        })
        .map(|record| record.observation.vector_id.as_str())
        .collect();
    let normative_violation_vectors = corpus
        .vectors
        .iter()
        .filter(|vector| {
            matches!(
                vector.classification,
                Classification::Normative | Classification::Invalid
            ) && semantic_nonconformant_vectors.contains(vector.id.as_str())
        })
        .count();
    let ambiguous_standard_vectors = corpus
        .vectors
        .iter()
        .filter(|vector| vector.classification == Classification::AmbiguousStandard)
        .count();
    let classifications: BTreeMap<_, _> = corpus
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector.classification))
        .collect();
    let named_policy_conformant_cells = records
        .iter()
        .filter(|record| {
            record.verdict.status == VerdictStatus::Conformant
                && record.verdict.matched_case.is_some()
                && classifications.get(record.observation.vector_id.as_str())
                    == Some(&Classification::PolicyDependent)
        })
        .count();
    let named_dialect_conformant_cells = records
        .iter()
        .filter(|record| {
            record.verdict.status == VerdictStatus::Conformant
                && record.verdict.matched_case.is_some()
                && classifications.get(record.observation.vector_id.as_str())
                    == Some(&Classification::DialectDependent)
        })
        .count();
    let policy_vectors_with_named_conformant_answer: BTreeSet<_> = records
        .iter()
        .filter(|record| {
            record.verdict.status == VerdictStatus::Conformant
                && record.verdict.matched_case.is_some()
                && classifications.get(record.observation.vector_id.as_str())
                    == Some(&Classification::PolicyDependent)
        })
        .map(|record| record.observation.vector_id.as_str())
        .collect();
    let dialect_vectors_with_named_conformant_answer: BTreeSet<_> = records
        .iter()
        .filter(|record| {
            record.verdict.status == VerdictStatus::Conformant
                && record.verdict.matched_case.is_some()
                && classifications.get(record.observation.vector_id.as_str())
                    == Some(&Classification::DialectDependent)
        })
        .map(|record| record.observation.vector_id.as_str())
        .collect();

    MatrixSummary {
        configured_builds: registry.builds.len(),
        reproducible_builds: registry
            .builds
            .iter()
            .filter(|build| {
                build.reproducibility.status
                    == occurframe_runner::ReproducibilityStatus::Reproducible
            })
            .count(),
        unreproducible_builds: registry
            .builds
            .iter()
            .filter(|build| {
                build.reproducibility.status
                    == occurframe_runner::ReproducibilityStatus::Unreproducible
            })
            .count(),
        vectors: corpus.vectors.len(),
        expected_observations: registry
            .builds
            .iter()
            .filter(|build| {
                build.reproducibility.status
                    == occurframe_runner::ReproducibilityStatus::Reproducible
            })
            .count()
            * corpus.vectors.len(),
        actual_observations: records.len(),
        outcome_counts,
        verdict_counts,
        family_counts,
        classification_counts,
        semantic_divergence_vectors: divergences.len(),
        normative_violation_vectors,
        documented_policy_difference_vectors: divergences
            .iter()
            .filter(|item| item.kind == DivergenceKind::DocumentedPolicyDifference)
            .count(),
        documented_dialect_difference_vectors: divergences
            .iter()
            .filter(|item| item.kind == DivergenceKind::DocumentedDialectDifference)
            .count(),
        named_policy_conformant_cells,
        named_dialect_conformant_cells,
        policy_vectors_with_named_conformant_answer: policy_vectors_with_named_conformant_answer
            .len(),
        dialect_vectors_with_named_conformant_answer: dialect_vectors_with_named_conformant_answer
            .len(),
        ambiguous_standard_vectors,
        ambiguous_standard_divergent_vectors: divergences
            .iter()
            .filter(|item| item.kind == DivergenceKind::StandardAmbiguity)
            .count(),
        tzdb_difference_vectors: divergences
            .iter()
            .filter(|item| item.kind == DivergenceKind::TzdbProvenanceDifference)
            .count(),
    }
}

pub(crate) fn classification_name(classification: Classification) -> &'static str {
    match classification {
        Classification::Normative => "NORMATIVE",
        Classification::PolicyDependent => "POLICY_DEPENDENT",
        Classification::DialectDependent => "DIALECT_DEPENDENT",
        Classification::AmbiguousStandard => "AMBIGUOUS_STANDARD",
        Classification::KnownDivergence => "KNOWN_DIVERGENCE",
        Classification::Invalid => "INVALID",
    }
}
