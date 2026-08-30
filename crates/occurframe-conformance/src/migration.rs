use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use occurframe_wire::{Lifecycle, NormativeEvidence, SemanticValue, Vector};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    Error, Result, canonical_json_line, canonical_pretty_json,
    corpus::{
        AxisEntry, AxisRegistry, SourceEntry, SourceRegistry, VectorIdEntry, VectorIdRegistry,
    },
    load_and_validate_corpus, sha256_hex,
};

const RC2_SCHEMA_VERSION: &str = "1.0.0";
const RC2_CORPUS_VERSION: &str = "1.0.0-rc2";

/// One RC1 observation whose overloaded `error` status cannot safely map to protocol v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmbiguousLegacyCell {
    pub vector_id: String,
    pub engine_build: String,
    pub legacy_status: String,
}

/// Auditable RC1-to-RC2 migration report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub rc1_vector_count: usize,
    pub rc2_vector_count: usize,
    pub id_equality: bool,
    pub classification_counts: BTreeMap<String, usize>,
    pub family_counts: BTreeMap<String, usize>,
    pub semantic_drift_count: usize,
    pub semantic_drift: Vec<String>,
    pub removed_incumbent_vectors: usize,
    pub removed_incumbent_observations: usize,
    pub registry_coverage: RegistryCoverage,
    pub ambiguous_legacy_observation_count: usize,
    pub legacy_observations_that_cannot_safely_map_to_protocol_v2: Vec<AmbiguousLegacyCell>,
    pub canonical_rc2_digest: String,
}

/// Registry resolution counts captured in the migration report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryCoverage {
    pub sources_referenced: usize,
    pub sources_resolved: usize,
    pub axes_referenced: usize,
    pub axes_resolved: usize,
    pub dialect_ids_referenced: usize,
    pub dialect_ids_resolved: usize,
}

#[derive(Debug, Clone)]
struct MigrationBuild {
    vectors: Vec<Vector>,
    sources: SourceRegistry,
    axes: AxisRegistry,
    vector_ids: VectorIdRegistry,
    incumbent_vectors: usize,
    incumbent_observations: usize,
    ambiguous_cells: Vec<AmbiguousLegacyCell>,
}

/// Migrate RC1 JSONL into one authored RC2 JSON file per vector and authored registries.
pub fn migrate_rc1(legacy_vectors: &Path, corpus_root: &Path) -> Result<MigrationReport> {
    let records = read_rc1(legacy_vectors)?;
    let build = build_expected(&records)?;
    fs::create_dir_all(corpus_root.join("vectors"))?;
    fs::create_dir_all(corpus_root.join("registry"))?;

    for vector in &build.vectors {
        let directory = corpus_root.join("vectors").join(&vector.family);
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(format!("{}.json", vector.id)),
            canonical_pretty_json(vector)?,
        )?;
    }
    fs::write(
        corpus_root.join("registry/sources.json"),
        canonical_pretty_json(&build.sources)?,
    )?;
    fs::write(
        corpus_root.join("registry/semantic-axes.json"),
        canonical_pretty_json(&build.axes)?,
    )?;
    fs::write(
        corpus_root.join("registry/vector-ids.json"),
        canonical_pretty_json(&build.vector_ids)?,
    )?;

    let report = verify_migration(legacy_vectors, corpus_root)?;
    fs::write(
        corpus_root.join("migration-report.json"),
        canonical_pretty_json(&report)?,
    )?;
    Ok(report)
}

/// Independently compare authored RC2 vectors and registries to the preserved RC1 evidence.
pub fn verify_migration(legacy_vectors: &Path, corpus_root: &Path) -> Result<MigrationReport> {
    let records = read_rc1(legacy_vectors)?;
    let expected = build_expected(&records)?;
    let (corpus, validation) = load_and_validate_corpus(corpus_root)?;

    let expected_by_id: BTreeMap<_, _> = expected
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let actual_by_id: BTreeMap<_, _> = corpus
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let expected_ids: BTreeSet<_> = expected_by_id.keys().copied().collect();
    let actual_ids: BTreeSet<_> = actual_by_id.keys().copied().collect();
    let id_equality = expected_ids == actual_ids;

    let mut drift = Vec::new();
    for id in expected_ids.union(&actual_ids) {
        if expected_by_id.get(id) != actual_by_id.get(id) {
            drift.push((*id).to_string());
        }
    }
    if canonicalized_sources(&expected.sources) != canonicalized_sources(&corpus.sources) {
        drift.push("registry/sources.json".into());
    }
    if canonicalized_axes(&expected.axes) != canonicalized_axes(&corpus.axes) {
        drift.push("registry/semantic-axes.json".into());
    }

    let classification_counts = counts(corpus.vectors.iter().map(|vector| {
        serde_json::to_value(vector.classification)
            .expect("classification serializes")
            .as_str()
            .expect("classification is a string")
            .to_owned()
    }));
    let family_counts = counts(corpus.vectors.iter().map(|vector| {
        if vector.family.starts_with("cron.") {
            "cron".to_owned()
        } else if vector.family.starts_with("rrule.") {
            "rrule".to_owned()
        } else {
            "tzdb_provenance".to_owned()
        }
    }));
    let digest = canonical_vector_digest(&corpus.vectors)?;

    Ok(MigrationReport {
        rc1_vector_count: records.len(),
        rc2_vector_count: corpus.vectors.len(),
        id_equality,
        classification_counts,
        family_counts,
        semantic_drift_count: drift.len(),
        semantic_drift: drift,
        removed_incumbent_vectors: expected.incumbent_vectors,
        removed_incumbent_observations: expected.incumbent_observations,
        registry_coverage: RegistryCoverage {
            sources_referenced: validation.sources_referenced,
            sources_resolved: validation.sources_resolved,
            axes_referenced: validation.axes_referenced,
            axes_resolved: validation.axes_resolved,
            dialect_ids_referenced: validation.dialect_ids_referenced,
            dialect_ids_resolved: validation.dialect_ids_resolved,
        },
        ambiguous_legacy_observation_count: expected.ambiguous_cells.len(),
        legacy_observations_that_cannot_safely_map_to_protocol_v2: expected.ambiguous_cells,
        canonical_rc2_digest: digest,
    })
}

fn read_rc1(directory: &Path) -> Result<Vec<Value>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    paths.sort();
    let mut records = Vec::new();
    for path in paths {
        let content = fs::read_to_string(path)?;
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            records.push(serde_json::from_str(line)?);
        }
    }
    records.sort_by(|left, right| string_field(left, "id").cmp(string_field(right, "id")));
    Ok(records)
}

fn build_expected(records: &[Value]) -> Result<MigrationBuild> {
    let mut vectors = Vec::new();
    let mut sources = BTreeMap::<String, SourceEntry>::new();
    let mut axes = BTreeMap::<String, (String, BTreeSet<SemanticValue>)>::new();
    let mut incumbent_vectors = 0;
    let mut incumbent_observations = 0;
    let mut ambiguous_cells = Vec::new();

    for record in records {
        let object = record
            .as_object()
            .ok_or_else(|| Error::Migration("RC1 vector is not an object".into()))?;
        let id = required_string(object, "id")?;
        let policy_axes = axis_names(object.get("policy_axis"));
        let dialect_axes = axis_names(object.get("dialect_axis"));
        for axis in &policy_axes {
            axes.entry(axis.clone())
                .or_insert_with(|| ("policy".into(), BTreeSet::new()));
        }
        for axis in &dialect_axes {
            axes.entry(axis.clone())
                .or_insert_with(|| ("dialect".into(), BTreeSet::new()));
        }

        let expectation_value = object
            .get("expect")
            .cloned()
            .ok_or_else(|| Error::Migration(format!("{id} has no expectation")))?;
        collect_axis_values(&expectation_value, &mut axes)?;
        if let Some(policy) = object
            .get("context")
            .and_then(|context| context.get("policy"))
            .and_then(Value::as_object)
        {
            for (axis, value) in policy {
                let entry = axes
                    .entry(axis.clone())
                    .or_insert_with(|| ("policy".into(), BTreeSet::new()));
                entry.1.insert(semantic_value(value)?);
            }
        }

        let mut evidence = Vec::new();
        let normative = object
            .get("normative")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Migration(format!("{id} has invalid normative evidence")))?;
        for source in normative {
            let source = source
                .as_object()
                .ok_or_else(|| Error::Migration(format!("{id} has non-object source")))?;
            let source_id = required_string(source, "key")?;
            let entry = SourceEntry {
                source_id: source_id.clone(),
                title: required_string(source, "title")?,
                url: required_string(source, "url")?,
            };
            if let Some(existing) = sources.get(&source_id) {
                if existing.title != entry.title || existing.url != entry.url {
                    return Err(Error::Migration(format!(
                        "source {source_id} has inconsistent reusable metadata"
                    )));
                }
            } else {
                sources.insert(source_id.clone(), entry);
            }
            evidence.push(NormativeEvidence {
                source_id,
                quote: optional_string(source, "quote"),
                note: optional_string(source, "note"),
            });
        }

        let incumbents = object.get("incumbents").and_then(Value::as_object);
        if let Some(incumbents) = incumbents {
            incumbent_vectors += 1;
            incumbent_observations += incumbents.len();
            for (engine_build, observation) in incumbents {
                if observation.get("status").and_then(Value::as_str) == Some("error") {
                    ambiguous_cells.push(AmbiguousLegacyCell {
                        vector_id: id.clone(),
                        engine_build: engine_build.clone(),
                        legacy_status: "error".into(),
                    });
                }
            }
        }

        let mut context = object
            .get("context")
            .cloned()
            .ok_or_else(|| Error::Migration(format!("{id} has no context")))?;
        if context.get("dialect").and_then(Value::as_str) == Some("vixie") {
            context["dialect"] = Value::String("cron.vixie@1".into());
        }

        let mut semantic_axes: Vec<_> = policy_axes.into_iter().chain(dialect_axes).collect();
        if let Some(policy) = context.get("policy").and_then(Value::as_object) {
            semantic_axes.extend(policy.keys().cloned());
        }
        if let Some(cases) = expectation_value.get("cases").and_then(Value::as_array) {
            for case in cases {
                if let Some(when) = case.get("when").and_then(Value::as_object) {
                    semantic_axes.extend(when.keys().cloned());
                }
            }
        }
        semantic_axes.sort();
        semantic_axes.dedup();

        let vector = Vector {
            schema_version: RC2_SCHEMA_VERSION.into(),
            corpus_version: RC2_CORPUS_VERSION.into(),
            id,
            family: required_string(object, "family")?,
            title: required_string(object, "title")?,
            kind: required_string(object, "kind")?,
            operation: required_string(object, "op")?,
            input: object
                .get("input")
                .cloned()
                .ok_or_else(|| Error::Migration("missing input".into()))?,
            context,
            classification: serde_json::from_value(
                object
                    .get("classification")
                    .cloned()
                    .ok_or_else(|| Error::Migration("missing classification".into()))?,
            )?,
            semantic_axes,
            normative_evidence: evidence,
            expectation: serde_json::from_value(expectation_value)?,
            rationale: required_string(object, "rationale")?,
            tags: serde_json::from_value(
                object
                    .get("tags")
                    .cloned()
                    .ok_or_else(|| Error::Migration("missing tags".into()))?,
            )?,
            lifecycle: Lifecycle::Active,
            supersession: None,
        };
        vectors.push(vector);
    }

    vectors.sort_by(|left, right| left.id.cmp(&right.id));
    ambiguous_cells.sort_by(|left, right| {
        (&left.vector_id, &left.engine_build).cmp(&(&right.vector_id, &right.engine_build))
    });
    let source_registry = SourceRegistry {
        schema_version: RC2_SCHEMA_VERSION.into(),
        sources: sources.into_values().collect(),
    };
    let axis_registry = AxisRegistry {
        schema_version: RC2_SCHEMA_VERSION.into(),
        axes: axes
            .into_iter()
            .map(|(axis_id, (axis_kind, values))| AxisEntry {
                description: format!(
                    "Research II semantic axis {axis_id}; values are preserved from RC1."
                ),
                axis_id,
                axis_kind,
                values: values.into_iter().collect(),
            })
            .collect(),
    };
    let vector_ids = VectorIdRegistry {
        schema_version: RC2_SCHEMA_VERSION.into(),
        corpus_version: RC2_CORPUS_VERSION.into(),
        ids: vectors
            .iter()
            .map(|vector| VectorIdEntry {
                id: vector.id.clone(),
                family: vector.family.clone(),
                status: "active".into(),
                superseded_by: None,
            })
            .collect(),
    };
    Ok(MigrationBuild {
        vectors,
        sources: source_registry,
        axes: axis_registry,
        vector_ids,
        incumbent_vectors,
        incumbent_observations,
        ambiguous_cells,
    })
}

fn axis_names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_str)
        .into_iter()
        .flat_map(|value| value.split('|'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn collect_axis_values(
    expectation: &Value,
    axes: &mut BTreeMap<String, (String, BTreeSet<SemanticValue>)>,
) -> Result<()> {
    let mode = expectation
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let inferred_kind = if mode == "per_policy" {
        "policy"
    } else {
        "dialect"
    };
    if let Some(cases) = expectation.get("cases").and_then(Value::as_array) {
        for case in cases {
            if let Some(when) = case.get("when").and_then(Value::as_object) {
                for (axis, value) in when {
                    let entry = axes
                        .entry(axis.clone())
                        .or_insert_with(|| (inferred_kind.into(), BTreeSet::new()));
                    entry.1.insert(semantic_value(value)?);
                }
            }
        }
    }
    Ok(())
}

fn semantic_value(value: &Value) -> Result<SemanticValue> {
    match value {
        Value::String(value) => Ok(SemanticValue::Text(value.clone())),
        Value::Number(value) => value
            .as_i64()
            .map(SemanticValue::Integer)
            .ok_or_else(|| Error::Migration("semantic axes cannot contain floating point".into())),
        Value::Bool(value) => Ok(SemanticValue::Boolean(*value)),
        _ => Err(Error::Migration(
            "semantic axis values must be string, integer, or boolean".into(),
        )),
    }
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Migration(format!("missing or invalid string field {key}")))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn counts(values: impl Iterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

fn canonical_vector_digest(vectors: &[Vector]) -> Result<String> {
    let mut bytes = Vec::new();
    for vector in vectors {
        bytes.extend(canonical_json_line(vector)?);
    }
    Ok(sha256_hex(&bytes))
}

fn canonicalized_sources(registry: &SourceRegistry) -> Value {
    json!(registry)
}

fn canonicalized_axes(registry: &AxisRegistry) -> Value {
    json!(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use occurframe_wire::{Classification, Expectation};

    #[test]
    fn pipe_delimited_axes_become_an_array_without_empty_members() {
        assert_eq!(
            axis_names(Some(&Value::String("a|b||".into()))),
            vec!["a", "b"]
        );
    }

    #[test]
    fn all_classifications_deserialize() {
        for name in [
            "NORMATIVE",
            "POLICY_DEPENDENT",
            "DIALECT_DEPENDENT",
            "AMBIGUOUS_STANDARD",
            "KNOWN_DIVERGENCE",
            "INVALID",
        ] {
            let _: Classification =
                serde_json::from_value(Value::String(name.into())).expect("classification");
        }
    }

    #[test]
    fn all_expectation_modes_deserialize() {
        let examples = [
            json!({"mode":"single","occurrences":[]}),
            json!({"mode":"reject"}),
            json!({"mode":"per_policy","cases":[]}),
            json!({"mode":"per_dialect","cases":[]}),
            json!({"mode":"admissible","cases":[]}),
            json!({"mode":"open"}),
        ];
        for example in examples {
            let _: Expectation = serde_json::from_value(example).expect("expectation mode");
        }
    }
}
