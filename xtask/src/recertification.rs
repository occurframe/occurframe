use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::Serialize;
use serde_json::Value;

const CATEGORIES: [&str; 7] = [
    "expected_corpus_correction",
    "protocol_adapter_transport_change",
    "hermetic_environment_effect",
    "tooling_scoring_effect",
    "fresh_engine_difference",
    "historical_ambiguity",
    "unresolved",
];

#[derive(Debug, Serialize)]
pub(crate) struct CertificationReconciliation {
    schema_version: &'static str,
    previous_certification: CertificationIdentity,
    current_certification: CertificationIdentity,
    expected_corpus_corrections: Vec<CorrectionSummary>,
    metadata_only_corrections: Vec<CorrectionSummary>,
    pub(crate) category_counts: BTreeMap<String, usize>,
    pub(crate) previous_cells: usize,
    pub(crate) current_cells: usize,
    pub(crate) comparable_cells: usize,
    pub(crate) changed_cells: usize,
    pub(crate) unresolved: usize,
    cells: Vec<ReconciledCell>,
}

#[derive(Debug, Serialize)]
struct CertificationIdentity {
    corpus_version: String,
    corpus_source_revision: String,
    corpus_canonical_digest: String,
    tooling_source_revision: String,
    runner_protocol_version: String,
    semantic_bundle_digest: String,
}

#[derive(Debug, Serialize)]
struct CorrectionSummary {
    vector_id: String,
    changed_cells: usize,
}

#[derive(Debug, Serialize)]
struct ReconciledCell {
    build_id: String,
    vector_id: String,
    classification: &'static str,
    reason: &'static str,
    semantic_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<CellSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<CellSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
struct CellSnapshot {
    execution_status: Value,
    engine_outcome: Value,
    verdict: Value,
}

#[derive(Debug)]
struct Matrix {
    identity: CertificationIdentity,
    cells: BTreeMap<(String, String), CellSnapshot>,
}

pub(crate) fn reconcile_certifications(
    previous_path: &Path,
    current_path: &Path,
    expected_corrections: &[String],
    metadata_only_corrections: &[String],
) -> Result<CertificationReconciliation, Box<dyn std::error::Error>> {
    let previous = load_matrix(previous_path)?;
    let current = load_matrix(current_path)?;
    let expected: BTreeSet<_> = expected_corrections.iter().cloned().collect();
    let metadata_only: BTreeSet<_> = metadata_only_corrections.iter().cloned().collect();
    validate_correction_sets(&expected, &metadata_only)?;

    let keys: BTreeSet<_> = previous
        .cells
        .keys()
        .chain(current.cells.keys())
        .cloned()
        .collect();
    let mut category_counts = CATEGORIES
        .into_iter()
        .map(|category| (category.to_owned(), 0))
        .collect::<BTreeMap<_, _>>();
    let mut cells = Vec::with_capacity(keys.len());
    let mut comparable_cells = 0;
    let mut changed_cells = 0;
    let mut changed_by_vector = BTreeMap::<String, usize>::new();

    for (build_id, vector_id) in keys {
        let previous_cell = previous.cells.get(&(build_id.clone(), vector_id.clone()));
        let current_cell = current.cells.get(&(build_id.clone(), vector_id.clone()));
        let (classification, reason, semantic_change) = match (previous_cell, current_cell) {
            (Some(old), Some(new)) => {
                comparable_cells += 1;
                if snapshots_equal(old, new) {
                    (
                        "protocol_adapter_transport_change",
                        "semantic observation, execution status, and verdict are unchanged; protocol and runner provenance advanced from the previous certification",
                        false,
                    )
                } else if expected.contains(&vector_id) {
                    (
                        "expected_corpus_correction",
                        "cell changed only on a vector declared as a reviewed RC3 corpus correction",
                        true,
                    )
                } else if old.execution_status == new.execution_status
                    && old.engine_outcome == new.engine_outcome
                    && old.verdict != new.verdict
                {
                    (
                        "tooling_scoring_effect",
                        "native observation is unchanged but the authority verdict changed",
                        true,
                    )
                } else {
                    (
                        "unresolved",
                        "native observation or execution status changed outside the declared corpus corrections",
                        true,
                    )
                }
            }
            (Some(_), None) | (None, Some(_)) => (
                "historical_ambiguity",
                "cell exists in only one certification and therefore has no direct comparison",
                true,
            ),
            (None, None) => unreachable!("the union of matrix keys cannot contain an absent key"),
        };
        *category_counts
            .get_mut(classification)
            .expect("all classifications are initialized") += 1;
        if semantic_change {
            changed_cells += 1;
            *changed_by_vector.entry(vector_id.clone()).or_default() += 1;
        }
        cells.push(ReconciledCell {
            build_id,
            vector_id,
            classification,
            reason,
            semantic_change,
            previous: semantic_change.then(|| previous_cell.cloned()).flatten(),
            current: semantic_change.then(|| current_cell.cloned()).flatten(),
        });
    }

    let unresolved = category_counts["unresolved"];
    Ok(CertificationReconciliation {
        schema_version: "1.0.0",
        previous_certification: previous.identity,
        current_certification: current.identity,
        expected_corpus_corrections: correction_summary(&expected, &changed_by_vector),
        metadata_only_corrections: correction_summary(&metadata_only, &changed_by_vector),
        category_counts,
        previous_cells: previous.cells.len(),
        current_cells: current.cells.len(),
        comparable_cells,
        changed_cells,
        unresolved,
        cells,
    })
}

fn correction_summary(
    ids: &BTreeSet<String>,
    changed_by_vector: &BTreeMap<String, usize>,
) -> Vec<CorrectionSummary> {
    ids.iter()
        .map(|vector_id| CorrectionSummary {
            vector_id: vector_id.clone(),
            changed_cells: changed_by_vector.get(vector_id).copied().unwrap_or(0),
        })
        .collect()
}

fn validate_correction_sets(
    expected: &BTreeSet<String>,
    metadata_only: &BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if expected.intersection(metadata_only).next().is_some() {
        return Err("a correction cannot be both semantic and metadata-only".into());
    }
    Ok(())
}

fn snapshots_equal(previous: &CellSnapshot, current: &CellSnapshot) -> bool {
    previous.execution_status == current.execution_status
        && previous.engine_outcome == current.engine_outcome
        && previous.verdict == current.verdict
}

fn load_matrix(path: &Path) -> Result<Matrix, Box<dyn std::error::Error>> {
    let matrix_path = if path.is_dir() {
        path.join("matrix.json")
    } else {
        path.to_owned()
    };
    let root: Value = serde_json::from_slice(&fs::read(&matrix_path)?)?;
    let manifest_path = matrix_path
        .parent()
        .ok_or("matrix path has no parent directory")?
        .join("certification-manifest.json");
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    let cells = required_array(&root, "cells")?
        .iter()
        .map(|cell| {
            let build_id = required_string(cell, "build_id")?;
            let vector_id = required_string(cell, "vector_id")?;
            let snapshot = CellSnapshot {
                execution_status: required_value(cell, "execution_status")?,
                engine_outcome: cell.get("engine_outcome").cloned().unwrap_or(Value::Null),
                verdict: required_value(cell, "verdict")?,
            };
            Ok(((build_id, vector_id), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    Ok(Matrix {
        identity: CertificationIdentity {
            corpus_version: required_string(&root, "corpus_version")?,
            corpus_source_revision: optional_string(&root, "corpus_source_revision")
                .or_else(|| optional_string(&root, "corpus_sha"))
                .ok_or("matrix is missing corpus source revision")?,
            corpus_canonical_digest: optional_string(&root, "corpus_canonical_digest")
                .or_else(|| optional_string(&manifest, "corpus_canonical_digest"))
                .unwrap_or_else(|| "not_recorded_by_previous_schema".to_owned()),
            tooling_source_revision: optional_string(&root, "tooling_source_revision")
                .or_else(|| optional_string(&root, "tooling_source_sha"))
                .ok_or("matrix is missing tooling source revision")?,
            runner_protocol_version: required_string(&root, "runner_protocol_version")?,
            semantic_bundle_digest: required_string(&manifest, "semantic_bundle_digest")?,
        },
        cells,
    })
}

fn required_array<'a>(root: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    root.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("matrix field {key} is missing or not an array"))
}

fn required_string(root: &Value, key: &str) -> Result<String, String> {
    optional_string(root, key).ok_or_else(|| format!("matrix field {key} is missing or not text"))
}

fn optional_string(root: &Value, key: &str) -> Option<String> {
    root.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn required_value(root: &Value, key: &str) -> Result<Value, String> {
    root.get(key)
        .cloned()
        .ok_or_else(|| format!("matrix field {key} is missing"))
}
