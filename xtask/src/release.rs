//! Release provenance: the machine-readable identity carried inside every
//! Occurframe release bundle.
//!
//! Everything recorded here is derived from pinned inputs — the evidence lock,
//! the certified bundle, the resolved dependency graph, and the toolchain that
//! actually produced the binaries. No wall-clock value is recorded: a build
//! timestamp would make two otherwise identical assemblies differ, so the only
//! time-like field is `source_date_epoch`, and it is populated only when the
//! release environment supplies one.

use std::{path::Path, process::Command};

use serde::Serialize;

/// The toolchain that compiled the released binaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Toolchain {
    pub rustc_version: String,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
}

impl Toolchain {
    /// Read `rustc -vV`, which is stable, machine-readable and clock-free.
    pub fn detect() -> Result<Self, Box<dyn std::error::Error>> {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(rustc).arg("-vV").output()?;
        if !output.status.success() {
            return Err("rustc -vV failed while recording release provenance".into());
        }
        let text = String::from_utf8(output.stdout)?;
        let field = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}: ")))
                .map(str::trim)
                .map(str::to_owned)
        };
        Ok(Self {
            rustc_version: field("release").ok_or("rustc -vV did not report a release version")?,
            host: field("host").ok_or("rustc -vV did not report a host triple")?,
            commit_hash: field("commit-hash"),
        })
    }
}

/// One released executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinaryEntry {
    pub path: String,
    pub alias: String,
    pub target: String,
    pub sha256: String,
}

/// Split `occurframe-x86_64-pc-windows-msvc.exe` into its alias and target.
///
/// The two executable names are aliases for one implementation, so the release
/// records which alias and which target triple each file is, rather than
/// leaving that to be inferred from a file name by a consumer.
pub fn split_binary_name(file_name: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let stem = file_name.strip_suffix(".exe").unwrap_or(file_name);
    for alias in ["occurframe", "oframe"] {
        if let Some(target) = stem.strip_prefix(&format!("{alias}-"))
            && !target.is_empty()
        {
            return Ok(((*alias).to_owned(), target.to_owned()));
        }
    }
    Err(format!("release binary {file_name} does not name an alias and target triple").into())
}

/// Corpus identity, restated inside the release so that a consumer can check the
/// bundled corpus without a network fetch or a corpus checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorpusIdentity {
    pub version: String,
    pub repository: String,
    pub sha: String,
    pub canonical_digest: String,
    pub release_digest: String,
    pub vectors: usize,
}

/// The certified differential evidence this release republishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CertificationIdentity {
    pub artifact_name: String,
    pub profile_version: String,
    pub tooling_source_sha: String,
    /// Digest of the certification manifest document itself.
    pub certification_manifest_sha256: String,
    /// Digest of the certified semantic evidence bundle.
    pub semantic_bundle_digest: String,
    /// Digest of the derived differential matrix.
    pub matrix_sha256: String,
    pub population: EvidencePopulation,
}

/// The certified evidence population, restated verbatim from the lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidencePopulation {
    pub configured_builds: usize,
    pub reproducible_builds: usize,
    pub provenance_blocked_builds: usize,
    pub vectors: usize,
    pub observations: usize,
    pub semantic_divergence_vectors: usize,
    pub normative_violation_vectors: usize,
}

/// Licensing statement for everything the bundle redistributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Licensing {
    pub occurframe_code: String,
    pub reference_and_tooling_code: String,
    pub corpus_semantic_data: String,
    pub third_party: String,
}

/// Digest of a generated inventory file carried in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
}

/// The complete release identity document written to `release-manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseManifest {
    pub schema_version: String,
    pub artifact_kind: String,
    pub tool_version: String,
    /// The behavioural specification this release implements. It versions
    /// independently of the tool, the corpus and the runner protocol.
    pub specification_version: String,
    /// The erratum that fixed the shipped command doctrine this release obeys.
    pub specification_errata: Vec<String>,
    /// The complete set of semantic commands this release ships.
    pub shipped_commands: Vec<String>,
    pub tooling_repository: String,
    pub tooling_commit_sha: String,
    pub toolchain: Toolchain,
    pub target_triples: Vec<String>,
    pub binaries: Vec<BinaryEntry>,
    pub corpus: CorpusIdentity,
    pub runner_protocol_version: String,
    pub certification: CertificationIdentity,
    pub licensing: Licensing,
    pub inventories: Vec<FileDigest>,
    /// Populated only from `SOURCE_DATE_EPOCH`. A release never records the
    /// wall clock of the machine that assembled it.
    pub source_date_epoch: Option<u64>,
    pub notes: Vec<String>,
}

/// Read a reproducible release timestamp, if the environment supplies one.
#[must_use]
pub fn source_date_epoch() -> Option<u64> {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

/// Resolve the commit being released.
///
/// An explicit value wins, so a CI job can pass the exact SHA it checked out;
/// otherwise the working checkout is asked. Packaging fails rather than
/// recording an unknown provenance.
pub fn commit_sha(
    repository_root: &Path,
    explicit: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(value) = explicit {
        let value = value.trim();
        if value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(value.to_ascii_lowercase());
        }
        return Err(
            format!("--commit must be a full 40-character git SHA, found {value:?}").into(),
        );
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repository_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("cannot record the released commit: {error}"))?;
    if !output.status.success() {
        return Err(
            "cannot record the released commit; pass --commit with the exact 40-character SHA"
                .into(),
        );
    }
    let sha = String::from_utf8(output.stdout)?
        .trim()
        .to_ascii_lowercase();
    if sha.len() != 40 {
        return Err("git rev-parse HEAD did not return a full commit SHA".into());
    }
    Ok(sha)
}

/// The human-readable identity card placed at the root of the bundle.
#[must_use]
pub fn render_version_file(manifest: &ReleaseManifest) -> String {
    format!(
        "Occurframe tooling: {}\nSpecification:      {}\nCorpus:             {}\nRunner protocol:    {}\nCertification:      {}\nCommit:             {}\n",
        manifest.tool_version,
        manifest.specification_version,
        manifest.corpus.version,
        manifest.runner_protocol_version,
        manifest.certification.profile_version,
        manifest.tooling_commit_sha
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_names_split_into_alias_and_target() {
        assert_eq!(
            split_binary_name("occurframe-x86_64-pc-windows-msvc.exe").expect("split"),
            ("occurframe".to_owned(), "x86_64-pc-windows-msvc".to_owned())
        );
        assert_eq!(
            split_binary_name("oframe-aarch64-apple-darwin").expect("split"),
            ("oframe".to_owned(), "aarch64-apple-darwin".to_owned())
        );
        assert!(split_binary_name("occurframe").is_err());
        assert!(split_binary_name("something-else-x86_64").is_err());
    }

    #[test]
    fn an_explicit_commit_must_be_a_full_sha() {
        let root = Path::new(".");
        assert_eq!(
            commit_sha(root, Some("FE0B3A868FE0F618CCDDB01878DDE8D3A2553E8E")).expect("accepted"),
            "fe0b3a868fe0f618ccddb01878dde8d3a2553e8e"
        );
        assert!(commit_sha(root, Some("fe0b3a8")).is_err());
        assert!(commit_sha(root, Some("not-a-sha-at-all")).is_err());
    }
}
