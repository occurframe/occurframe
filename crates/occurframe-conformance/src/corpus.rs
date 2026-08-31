use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use occurframe_wire::{Expectation, Lifecycle, SemanticValue, Vector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use walkdir::WalkDir;

use crate::{Error, Result};

/// A validated in-memory corpus and its authored registries.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub root: PathBuf,
    pub vectors: Vec<Vector>,
    pub sources: SourceRegistry,
    pub axes: AxisRegistry,
    pub dialects: DialectRegistry,
    pub vector_ids: VectorIdRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRegistry {
    pub schema_version: String,
    pub sources: Vec<SourceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub source_id: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisRegistry {
    pub schema_version: String,
    pub axes: Vec<AxisEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisEntry {
    pub axis_id: String,
    pub axis_kind: String,
    pub description: String,
    pub values: Vec<SemanticValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialectRegistry {
    pub schema_version: String,
    pub dialects: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIdRegistry {
    pub schema_version: String,
    pub corpus_version: String,
    pub ids: Vec<VectorIdEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIdEntry {
    pub id: String,
    pub family: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

/// Summary emitted by semantic and registry validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub corpus_version: String,
    pub vectors: usize,
    pub sources_referenced: usize,
    pub sources_resolved: usize,
    pub axes_referenced: usize,
    pub axes_resolved: usize,
    pub dialect_ids_referenced: usize,
    pub dialect_ids_resolved: usize,
    pub retired_ids: usize,
    pub superseded_ids: usize,
}

/// Load authored files, validate schemas, then enforce cross-file semantic invariants.
pub fn load_and_validate_corpus(root: &Path) -> Result<(Corpus, ValidationReport)> {
    load_and_validate_corpus_version(root, "1.0.0-rc3")
}

pub(crate) fn load_and_validate_corpus_version(
    root: &Path,
    expected_version: &str,
) -> Result<(Corpus, ValidationReport)> {
    validate_schemas(root)?;
    let mut vectors = load_vectors(&root.join("vectors"))?;
    vectors.sort_by(|left, right| left.id.cmp(&right.id));

    let sources: SourceRegistry = read_json(&root.join("registry/sources.json"))?;
    let axes: AxisRegistry = read_json(&root.join("registry/semantic-axes.json"))?;
    let dialects: DialectRegistry = read_json(&root.join("registry/dialects.json"))?;
    let vector_ids: VectorIdRegistry = read_json(&root.join("registry/vector-ids.json"))?;

    let corpus = Corpus {
        root: root.to_path_buf(),
        vectors,
        sources,
        axes,
        dialects,
        vector_ids,
    };
    let report = semantic_validate(&corpus, expected_version)?;
    Ok((corpus, report))
}

/// Validate every authored document against its Draft 2020-12 schema.
pub fn validate_schemas(root: &Path) -> Result<()> {
    let mut schema_paths: Vec<_> = fs::read_dir(root.join("schemas"))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    schema_paths.sort();
    for path in schema_paths {
        let schema = read_value(&path)?;
        jsonschema::validator_for(&schema)
            .map_err(|error| Error::Schema(format!("{}: {error}", path.display())))?;
    }
    let vector_schema = read_value(&root.join("schemas/vector.schema.json"))?;
    let source_schema = read_value(&root.join("schemas/source-registry.schema.json"))?;
    let axis_schema = read_value(&root.join("schemas/semantic-axis-registry.schema.json"))?;
    let dialect_schema = read_value(&root.join("schemas/dialect-registry.schema.json"))?;
    let vector_id_schema = read_value(&root.join("schemas/vector-id-registry.schema.json"))?;

    for path in vector_paths(&root.join("vectors"))? {
        validate_document(&vector_schema, &read_value(&path)?, &path)?;
    }
    validate_document(
        &source_schema,
        &read_value(&root.join("registry/sources.json"))?,
        &root.join("registry/sources.json"),
    )?;
    validate_document(
        &axis_schema,
        &read_value(&root.join("registry/semantic-axes.json"))?,
        &root.join("registry/semantic-axes.json"),
    )?;
    validate_document(
        &dialect_schema,
        &read_value(&root.join("registry/dialects.json"))?,
        &root.join("registry/dialects.json"),
    )?;
    validate_document(
        &vector_id_schema,
        &read_value(&root.join("registry/vector-ids.json"))?,
        &root.join("registry/vector-ids.json"),
    )?;
    Ok(())
}

fn validate_document(schema: &Value, instance: &Value, path: &Path) -> Result<()> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| Error::Schema(format!("{}: {error}", path.display())))?;
    let errors: Vec<_> = validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::Schema(format!(
            "{}: {}",
            path.display(),
            errors.join("; ")
        )))
    }
}

fn semantic_validate(corpus: &Corpus, expected_version: &str) -> Result<ValidationReport> {
    if corpus.vectors.is_empty() {
        return Err(Error::Validation("corpus contains no vectors".into()));
    }
    let corpus_version = corpus.vectors[0].corpus_version.clone();
    if corpus_version != expected_version {
        return Err(Error::Validation(format!(
            "expected corpus {expected_version}, found {corpus_version}"
        )));
    }

    let source_ids: BTreeSet<_> = corpus
        .sources
        .sources
        .iter()
        .map(|entry| entry.source_id.as_str())
        .collect();
    let axis_ids: BTreeSet<_> = corpus
        .axes
        .axes
        .iter()
        .map(|entry| entry.axis_id.as_str())
        .collect();
    let dialect_ids: BTreeSet<_> = corpus
        .dialects
        .dialects
        .iter()
        .filter_map(|entry| entry.get("dialect_id").and_then(Value::as_str))
        .collect();
    let stable_ids: BTreeMap<_, _> = corpus
        .vector_ids
        .ids
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();

    reject_duplicates(
        "vector",
        corpus.vectors.iter().map(|vector| vector.id.as_str()),
    )?;
    reject_duplicates(
        "source",
        corpus
            .sources
            .sources
            .iter()
            .map(|entry| entry.source_id.as_str()),
    )?;
    reject_duplicates(
        "axis",
        corpus.axes.axes.iter().map(|entry| entry.axis_id.as_str()),
    )?;
    reject_duplicates(
        "dialect",
        corpus
            .dialects
            .dialects
            .iter()
            .filter_map(|entry| entry.get("dialect_id").and_then(Value::as_str)),
    )?;

    let authored_ids: BTreeSet<_> = corpus
        .vectors
        .iter()
        .map(|vector| vector.id.as_str())
        .collect();
    let retired_ids = validate_stable_id_entries(&corpus.vector_ids.ids, &authored_ids)?;

    let mut referenced_sources = BTreeSet::new();
    let mut referenced_axes = BTreeSet::new();
    let mut referenced_dialects = BTreeSet::new();
    let mut superseded = 0;
    let axis_entries: BTreeMap<_, _> = corpus
        .axes
        .axes
        .iter()
        .map(|entry| (entry.axis_id.as_str(), entry))
        .collect();
    for vector in &corpus.vectors {
        if vector.corpus_version != corpus_version || vector.schema_version != "1.0.0" {
            return Err(Error::Validation(format!(
                "{} has inconsistent schema/corpus version",
                vector.id
            )));
        }
        if vector.lifecycle != Lifecycle::Active {
            return Err(Error::Validation(format!(
                "authored vector {} is not active",
                vector.id
            )));
        }
        for evidence in &vector.normative_evidence {
            referenced_sources.insert(evidence.source_id.as_str());
            if !source_ids.contains(evidence.source_id.as_str()) {
                return Err(Error::Validation(format!(
                    "{} references unknown source {}",
                    vector.id, evidence.source_id
                )));
            }
        }
        for axis in &vector.semantic_axes {
            referenced_axes.insert(axis.as_str());
            if !axis_ids.contains(axis.as_str()) {
                return Err(Error::Validation(format!(
                    "{} references unknown semantic axis {axis}",
                    vector.id
                )));
            }
        }
        validate_expectation_axes(vector, &axis_entries)?;
        if let Some(dialect) = vector
            .context
            .get("dialect")
            .and_then(Value::as_str)
            .filter(|dialect| dialect.contains('@'))
        {
            referenced_dialects.insert(dialect);
            if !dialect_ids.contains(dialect) {
                return Err(Error::Validation(format!(
                    "{} references unknown dialect {dialect}",
                    vector.id
                )));
            }
        }
        if let Some(supersession) = &vector.supersession {
            superseded += 1;
            for id in [
                supersession.supersedes.as_deref(),
                supersession.superseded_by.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if !stable_ids.contains_key(id) {
                    return Err(Error::Validation(format!(
                        "{} references unknown supersession ID {id}",
                        vector.id
                    )));
                }
            }
        }
    }

    for dialect in &corpus.dialects.dialects {
        let dialect_id = dialect
            .get("dialect_id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Validation("dialect is missing dialect_id".into()))?;
        for source_id in string_array(dialect, "source_references")? {
            referenced_sources.insert(source_id);
            if !source_ids.contains(source_id) {
                return Err(Error::Validation(format!(
                    "dialect {dialect_id} references unknown source {source_id}"
                )));
            }
        }
        for vector_id in string_array(dialect, "pinning_vectors")? {
            if !authored_ids.contains(vector_id) {
                return Err(Error::Validation(format!(
                    "dialect {dialect_id} references unknown pinning vector {vector_id}"
                )));
            }
        }
    }

    Ok(ValidationReport {
        corpus_version,
        vectors: corpus.vectors.len(),
        sources_referenced: referenced_sources.len(),
        sources_resolved: referenced_sources.len(),
        axes_referenced: referenced_axes.len(),
        axes_resolved: referenced_axes.len(),
        dialect_ids_referenced: referenced_dialects.len(),
        dialect_ids_resolved: referenced_dialects.len(),
        retired_ids,
        superseded_ids: superseded,
    })
}

fn validate_stable_id_entries(
    entries: &[VectorIdEntry],
    authored_ids: &BTreeSet<&str>,
) -> Result<usize> {
    reject_duplicates(
        "stable vector",
        entries.iter().map(|entry| entry.id.as_str()),
    )?;
    let by_id: BTreeMap<_, _> = entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    let active_ids: BTreeSet<_> = entries
        .iter()
        .filter(|entry| entry.status == "active")
        .map(|entry| entry.id.as_str())
        .collect();
    if authored_ids != &active_ids {
        return Err(Error::Validation(
            "active stable-ID registry set differs from authored vector ID set".into(),
        ));
    }

    let mut retired = 0;
    for entry in entries {
        match entry.status.as_str() {
            "active" => {
                if entry.superseded_by.is_some() {
                    return Err(Error::Validation(format!(
                        "active ID {} cannot be superseded",
                        entry.id
                    )));
                }
            }
            "retired" => {
                retired += 1;
                let successor = entry.superseded_by.as_deref().ok_or_else(|| {
                    Error::Validation(format!(
                        "retired ID {} must identify its successor",
                        entry.id
                    ))
                })?;
                let successor_entry = by_id.get(successor).ok_or_else(|| {
                    Error::Validation(format!(
                        "retired ID {} has unknown successor {successor}",
                        entry.id
                    ))
                })?;
                if successor_entry.status != "active" {
                    return Err(Error::Validation(format!(
                        "retired ID {} must point to an active successor",
                        entry.id
                    )));
                }
            }
            status => {
                return Err(Error::Validation(format!(
                    "stable ID {} has invalid status {status}",
                    entry.id
                )));
            }
        }
    }
    Ok(retired)
}

fn validate_expectation_axes(vector: &Vector, axes: &BTreeMap<&str, &AxisEntry>) -> Result<()> {
    let cases = match &vector.expectation {
        Expectation::PerPolicy { cases, .. }
        | Expectation::PerDialect { cases, .. }
        | Expectation::Admissible { cases, .. } => cases.as_slice(),
        _ => &[],
    };
    for case in cases {
        for (axis_id, value) in &case.when {
            if !vector.semantic_axes.contains(axis_id) {
                return Err(Error::Validation(format!(
                    "{} expectation references axis {axis_id} absent from semantic_axes",
                    vector.id
                )));
            }
            let registry_entry = axes.get(axis_id.as_str()).ok_or_else(|| {
                Error::Validation(format!("{} references unknown axis {axis_id}", vector.id))
            })?;
            if !registry_entry.values.contains(value) {
                return Err(Error::Validation(format!(
                    "{} uses unregistered value for axis {axis_id}",
                    vector.id
                )));
            }
        }
    }
    if let Some(policy) = vector.context.get("policy").and_then(Value::as_object) {
        for (axis_id, value) in policy {
            if !vector.semantic_axes.contains(axis_id) {
                return Err(Error::Validation(format!(
                    "{} context references axis {axis_id} absent from semantic_axes",
                    vector.id
                )));
            }
            let semantic_value: SemanticValue = serde_json::from_value(value.clone())?;
            let registry_entry = axes.get(axis_id.as_str()).ok_or_else(|| {
                Error::Validation(format!("{} references unknown axis {axis_id}", vector.id))
            })?;
            if !registry_entry.values.contains(&semantic_value) {
                return Err(Error::Validation(format!(
                    "{} uses unregistered context value for axis {axis_id}",
                    vector.id
                )));
            }
        }
    }
    Ok(())
}

fn string_array<'a>(value: &'a Value, field: &str) -> Result<Vec<&'a str>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Validation(format!("missing array field {field}")))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| Error::Validation(format!("{field} contains a non-string")))
        })
        .collect()
}

fn reject_duplicates<'a>(label: &str, values: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(Error::Validation(format!(
                "duplicate {label} identifier: {value}"
            )));
        }
    }
    Ok(())
}

fn load_vectors(root: &Path) -> Result<Vec<Vector>> {
    vector_paths(root)?
        .into_iter()
        .map(|path| read_json(&path))
        .collect()
}

fn vector_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| Error::Io(std::io::Error::other(error)))?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_value(path: &Path) -> Result<Value> {
    read_json(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_identifier_is_invalid() {
        let error =
            reject_duplicates("test", ["A", "A"].into_iter()).expect_err("duplicate must fail");
        assert!(error.to_string().contains("duplicate test identifier: A"));
    }

    #[test]
    fn retired_and_superseded_ids_are_validated() {
        let entries = vec![
            VectorIdEntry {
                id: "OLD".into(),
                family: "test".into(),
                status: "retired".into(),
                superseded_by: Some("NEW".into()),
            },
            VectorIdEntry {
                id: "NEW".into(),
                family: "test".into(),
                status: "active".into(),
                superseded_by: None,
            },
        ];
        assert_eq!(
            validate_stable_id_entries(&entries, &BTreeSet::from(["NEW"]))
                .expect("valid retirement"),
            1
        );

        let broken = vec![VectorIdEntry {
            id: "OLD".into(),
            family: "test".into(),
            status: "retired".into(),
            superseded_by: Some("MISSING".into()),
        }];
        assert!(
            validate_stable_id_entries(&broken, &BTreeSet::new())
                .expect_err("unknown successor must fail")
                .to_string()
                .contains("unknown successor")
        );
    }
}
