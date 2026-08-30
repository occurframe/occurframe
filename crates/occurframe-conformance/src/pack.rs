use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use occurframe_wire::Vector;
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result, canonical_json_line, canonical_pretty_json, load_and_validate_corpus, sha256_hex,
};
use walkdir::WalkDir;

/// A generated release file entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseFile {
    pub path: String,
    pub records: usize,
    pub sha256: String,
}

/// Deterministic release manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: String,
    pub corpus_version: String,
    pub canonical_corpus_digest: String,
    pub files: Vec<ReleaseFile>,
}

/// Summary for a completed deterministic release pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackReport {
    pub corpus_version: String,
    pub output_directory: PathBuf,
    pub vector_count: usize,
    pub canonical_corpus_digest: String,
    pub release_digest: String,
}

/// The on-disk authority representation consumed by a compatible corpus loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusDistributionKind {
    /// An authored corpus checkout containing schemas, registries, and vectors.
    AuthoredCheckout,
    /// A generated, manifest-verified JSONL distribution.
    PackedDistribution,
}

/// A validated corpus view suitable for public conformance execution.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibleCorpus {
    pub root: PathBuf,
    pub corpus_version: String,
    pub canonical_corpus_digest: String,
    pub vectors: Vec<Vector>,
    pub distribution_kind: CorpusDistributionKind,
}

/// Load either an authored corpus checkout or its generated packed distribution.
///
/// Packed distributions are verified against both their SHA-256 manifest and the
/// canonical digest of their parsed vectors. A product may additionally pin that
/// digest to a specific corpus release.
pub fn load_compatible_corpus(root: &Path) -> Result<CompatibleCorpus> {
    if root.join("vectors").is_dir() {
        let (corpus, _) = load_and_validate_corpus(root)?;
        let canonical_corpus_digest = canonical_corpus_digest(&corpus.vectors)?;
        let corpus_version = corpus.vectors.first().map_or_else(
            || "1.0.0-rc2".to_owned(),
            |vector| vector.corpus_version.clone(),
        );
        return Ok(CompatibleCorpus {
            root: root.to_path_buf(),
            corpus_version,
            canonical_corpus_digest,
            vectors: corpus.vectors,
            distribution_kind: CorpusDistributionKind::AuthoredCheckout,
        });
    }
    load_packed_corpus(root)
}

/// Compute the canonical semantic digest used by generated corpus manifests.
pub fn canonical_corpus_digest(vectors: &[Vector]) -> Result<String> {
    let mut sorted = vectors.to_vec();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    let mut corpus_bytes = Vec::new();
    for vector in &sorted {
        corpus_bytes.extend(canonical_json_line(vector)?);
    }
    Ok(sha256_hex(&corpus_bytes))
}

fn load_packed_corpus(root: &Path) -> Result<CompatibleCorpus> {
    verify_manifest(root)?;
    let manifest: ReleaseManifest = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    if manifest.schema_version != "1.0.0" {
        return Err(Error::Validation(format!(
            "unsupported corpus manifest schema {}",
            manifest.schema_version
        )));
    }
    let mut paths = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut vectors = Vec::new();
    for entry in &manifest.files {
        validate_release_path(&entry.path)?;
        if !paths.insert(entry.path.clone()) {
            return Err(Error::Validation(format!(
                "duplicate release file {}",
                entry.path
            )));
        }
        let bytes = fs::read(root.join(&entry.path))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| Error::Validation(format!("{} is not UTF-8: {error}", entry.path)))?;
        let mut records = 0_usize;
        for (index, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let vector: Vector = serde_json::from_str(line).map_err(|error| {
                Error::Validation(format!(
                    "{} record {} is not a vector: {error}",
                    entry.path,
                    index + 1
                ))
            })?;
            if vector.corpus_version != manifest.corpus_version {
                return Err(Error::Validation(format!(
                    "{} has corpus version {}, expected {}",
                    vector.id, vector.corpus_version, manifest.corpus_version
                )));
            }
            if !ids.insert(vector.id.clone()) {
                return Err(Error::Validation(format!(
                    "duplicate packed vector ID {}",
                    vector.id
                )));
            }
            records += 1;
            vectors.push(vector);
        }
        if records != entry.records {
            return Err(Error::Validation(format!(
                "record count mismatch for {}: manifest {}, observed {records}",
                entry.path, entry.records
            )));
        }
    }
    vectors.sort_by(|left, right| left.id.cmp(&right.id));
    let observed_digest = canonical_corpus_digest(&vectors)?;
    if observed_digest != manifest.canonical_corpus_digest {
        return Err(Error::Validation(format!(
            "canonical corpus digest mismatch: manifest {}, observed {observed_digest}",
            manifest.canonical_corpus_digest
        )));
    }
    Ok(CompatibleCorpus {
        root: root.to_path_buf(),
        corpus_version: manifest.corpus_version,
        canonical_corpus_digest: observed_digest,
        vectors,
        distribution_kind: CorpusDistributionKind::PackedDistribution,
    })
}

fn validate_release_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.extension().and_then(std::ffi::OsStr::to_str) != Some("jsonl")
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::Validation(format!(
            "unsafe or non-JSONL manifest path {}",
            path.display()
        )));
    }
    Ok(())
}

/// Generate a deterministic RC2 distribution directory.
pub fn pack_release(corpus_root: &Path, output_directory: &Path) -> Result<PackReport> {
    let (corpus, _) = load_and_validate_corpus(corpus_root)?;
    let rendered = render_release(&corpus.vectors)?;
    fs::create_dir_all(output_directory)?;
    for (path, bytes) in &rendered.files {
        fs::write(output_directory.join(path), bytes)?;
    }
    Ok(PackReport {
        corpus_version: "1.0.0-rc2".into(),
        output_directory: output_directory.to_path_buf(),
        vector_count: corpus.vectors.len(),
        canonical_corpus_digest: rendered.canonical_corpus_digest,
        release_digest: rendered.release_digest,
    })
}

/// Render twice from independently loaded corpus state and assert byte identity.
pub fn verify_deterministic_pack(corpus_root: &Path) -> Result<(String, String)> {
    let (first, _) = load_and_validate_corpus(corpus_root)?;
    let first = render_release(&first.vectors)?;
    let (second, _) = load_and_validate_corpus(corpus_root)?;
    let second = render_release(&second.vectors)?;
    if first.files != second.files {
        return Err(Error::Validation(
            "two clean deterministic renders were not byte-identical".into(),
        ));
    }
    Ok((first.release_digest, second.release_digest))
}

/// Verify all hashes in an already generated release manifest and SHA256SUMS.
pub fn verify_manifest(output_directory: &Path) -> Result<()> {
    let manifest: ReleaseManifest =
        serde_json::from_slice(&fs::read(output_directory.join("manifest.json"))?)?;
    for entry in &manifest.files {
        let bytes = fs::read(output_directory.join(&entry.path))?;
        if sha256_hex(&bytes) != entry.sha256 {
            return Err(Error::Validation(format!(
                "manifest digest mismatch for {}",
                entry.path
            )));
        }
    }
    let expected_sums = render_sums(output_directory, &manifest)?;
    let actual_sums = fs::read(output_directory.join("SHA256SUMS"))?;
    if expected_sums != actual_sums {
        return Err(Error::Validation("SHA256SUMS mismatch".into()));
    }
    Ok(())
}

/// Write deterministic SHA-256 sums for every file below `root`, excluding the output itself.
pub fn write_tree_checksums(root: &Path, output: &Path) -> Result<usize> {
    let output = output.to_path_buf();
    let mut paths = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| Error::Io(std::io::Error::other(error)))?;
        if entry.file_type().is_file() && entry.path() != output {
            paths.push(entry.into_path());
        }
    }
    paths.sort_by(|left, right| {
        normalized_relative(root, left).cmp(&normalized_relative(root, right))
    });
    let mut sums = String::new();
    for path in &paths {
        sums.push_str(&sha256_hex(&fs::read(path)?));
        sums.push_str("  ");
        sums.push_str(&normalized_relative(root, path));
        sums.push('\n');
    }
    fs::write(output, sums.as_bytes())?;
    Ok(paths.len())
}

fn normalized_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("walked path is below root")
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedRelease {
    files: BTreeMap<String, Vec<u8>>,
    canonical_corpus_digest: String,
    release_digest: String,
}

fn render_release(vectors: &[Vector]) -> Result<RenderedRelease> {
    let mut sorted = vectors.to_vec();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    let mut groups: BTreeMap<String, Vec<&Vector>> = BTreeMap::new();
    let mut corpus_bytes = Vec::new();
    for vector in &sorted {
        corpus_bytes.extend(canonical_json_line(vector)?);
        groups.entry(file_name(vector)).or_default().push(vector);
    }
    let canonical_corpus_digest = sha256_hex(&corpus_bytes);

    let mut files = BTreeMap::new();
    let mut manifest_files = Vec::new();
    for (path, vectors) in groups {
        let mut bytes = Vec::new();
        for vector in &vectors {
            bytes.extend(canonical_json_line(*vector)?);
        }
        manifest_files.push(ReleaseFile {
            path: path.clone(),
            records: vectors.len(),
            sha256: sha256_hex(&bytes),
        });
        files.insert(path, bytes);
    }
    let manifest = ReleaseManifest {
        schema_version: "1.0.0".into(),
        corpus_version: "1.0.0-rc2".into(),
        canonical_corpus_digest: canonical_corpus_digest.clone(),
        files: manifest_files,
    };
    let manifest_bytes = canonical_pretty_json(&manifest)?;
    files.insert("manifest.json".into(), manifest_bytes);

    let sums = render_sums_from_files(&files);
    let release_digest = sha256_hex(&sums);
    files.insert("SHA256SUMS".into(), sums);
    Ok(RenderedRelease {
        files,
        canonical_corpus_digest,
        release_digest,
    })
}

fn file_name(vector: &Vector) -> String {
    format!("{}.jsonl", vector.family.replace('.', "-"))
}

fn render_sums_from_files(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut output = String::new();
    for (path, bytes) in files {
        output.push_str(&sha256_hex(bytes));
        output.push_str("  ");
        output.push_str(path);
        output.push('\n');
    }
    output.into_bytes()
}

fn render_sums(output_directory: &Path, manifest: &ReleaseManifest) -> Result<Vec<u8>> {
    let mut files = BTreeMap::new();
    files.insert(
        "manifest.json".into(),
        fs::read(output_directory.join("manifest.json"))?,
    );
    for entry in &manifest.files {
        files.insert(
            entry.path.clone(),
            fs::read(output_directory.join(&entry.path))?,
        );
    }
    Ok(render_sums_from_files(&files))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use occurframe_wire::{Classification, Expectation, Lifecycle};
    use serde_json::json;

    use super::*;

    fn vector(id: &str, occurrences: Vec<&str>) -> Vector {
        Vector {
            schema_version: "1.0.0".into(),
            corpus_version: "1.0.0-rc2".into(),
            id: id.into(),
            family: "cron.test".into(),
            title: "test".into(),
            kind: "cron".into(),
            operation: "cron.next".into(),
            input: json!({}),
            context: json!({}),
            classification: Classification::Normative,
            semantic_axes: Vec::new(),
            normative_evidence: Vec::new(),
            expectation: Expectation::Single {
                occurrences: occurrences.into_iter().map(str::to_owned).collect(),
                note: None,
            },
            rationale: "test".into(),
            tags: Vec::new(),
            lifecycle: Lifecycle::Active,
            supersession: None,
        }
    }

    #[test]
    fn record_order_is_by_id_but_occurrences_are_untouched() {
        let rendered = render_release(&[
            vector("Z", vec!["2", "1"]),
            vector("A", vec!["b", "a", "a"]),
        ])
        .expect("render");
        let records: Vec<serde_json::Value> = String::from_utf8(
            rendered
                .files
                .get("cron-test.jsonl")
                .expect("JSONL")
                .clone(),
        )
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect();
        assert_eq!(records[0]["id"], "A");
        assert_eq!(
            records[0]["expectation"]["occurrences"],
            json!(["b", "a", "a"])
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let vectors = [vector("B", vec![]), vector("A", vec!["x"])];
        assert_eq!(
            render_release(&vectors).expect("first"),
            render_release(&vectors).expect("second")
        );
    }

    #[test]
    fn packed_distribution_is_manifest_verified_and_loadable() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "occurframe-pack-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).expect("fixture directory");
        let rendered =
            render_release(&[vector("B", vec!["later", "earlier"]), vector("A", vec![])])
                .expect("render");
        for (path, bytes) in rendered.files {
            fs::write(directory.join(path), bytes).expect("write fixture");
        }
        let loaded = load_compatible_corpus(&directory).expect("load packed corpus");
        assert_eq!(
            loaded.distribution_kind,
            CorpusDistributionKind::PackedDistribution
        );
        assert_eq!(loaded.vectors.len(), 2);
        assert_eq!(loaded.vectors[0].id, "A");
        assert_eq!(
            loaded.vectors[1].expectation,
            Expectation::Single {
                occurrences: vec!["later".into(), "earlier".into()],
                note: None
            }
        );
        fs::remove_dir_all(&directory).expect("remove isolated fixture directory");
    }
}
