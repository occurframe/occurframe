use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use occurframe_conformance::{
    canonical_pretty_json, load_and_validate_corpus, migrate_rc1, pack_release, sha256_hex,
    verify_deterministic_pack, verify_manifest, verify_migration, write_tree_checksums,
};
use occurframe_report::{
    BundleInput, CertificationManifest, DifferentialMatrix, generate_bundle, load_json,
    load_legacy_build_map, load_profile, public_release_report, verify_bundle_checksums,
    verify_deterministic_bundles,
};
use occurframe_runner::{
    CaseExecution, ProtocolSchema, ReproducibilityStatus, RunnerBuild, RunnerRegistry, run_batch,
    semantic_observation_digest, semantic_observation_ndjson,
};
use occurframe_wire::{EngineOutcome, ExecutionStatus, Vector};

mod audit;
mod deps;
mod release;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().ok_or("missing command")?;
    let options = Options::parse(arguments)?;
    match command.as_str() {
        "validate" => {
            let (_, report) = load_and_validate_corpus(&options.required("corpus")?)?;
            print_json(&report)?;
        }
        "migrate" => {
            let report = migrate_rc1(
                &options.required("legacy-vectors")?,
                &options.required("corpus")?,
            )?;
            print_json(&report)?;
        }
        "verify-migration" => {
            let report = verify_migration(
                &options.required("legacy-vectors")?,
                &options.required("corpus")?,
            )?;
            print_json(&report)?;
            if report.semantic_drift_count != 0 || !report.id_equality {
                return Err("migration drift detected".into());
            }
        }
        "pack" => {
            let report = pack_release(&options.required("corpus")?, &options.required("output")?)?;
            print_json(&report)?;
        }
        "verify-deterministic" => {
            let (first_digest, second_digest) =
                verify_deterministic_pack(&options.required("corpus")?)?;
            print_json(&serde_json::json!({
                "byte_equal": true,
                "first_digest": first_digest,
                "second_digest": second_digest
            }))?;
        }
        "verify-manifest" => {
            verify_manifest(&options.required("output")?)?;
            print_json(&serde_json::json!({"manifest_valid": true}))?;
        }
        "checksums" => {
            let files =
                write_tree_checksums(&options.required("root")?, &options.required("output")?)?;
            print_json(&serde_json::json!({"files": files}))?;
        }
        "runner-contract" => runner_contract()?,
        "runner-smoke" => {
            let registry = RunnerRegistry::load(&options.required("registry")?)?;
            let build_id = options.required_string("build")?;
            let build = registry
                .build(&build_id)
                .ok_or_else(|| format!("unknown runner build {build_id}"))?
                .clone();
            let records = execute_smoke(
                &[build],
                &options.required("corpus")?,
                &options.required("schema")?,
                &options.required("root")?,
            )?;
            write_smoke_output(&records, &options.required("output")?)?;
            print_json(&smoke_summary(&records))?;
            if records.iter().any(|record| {
                matches!(
                    record.observation.execution_status,
                    ExecutionStatus::RunnerFailure | ExecutionStatus::Timeout
                )
            }) {
                return Err("adapter smoke contained runner failure or timeout".into());
            }
        }
        "runner-smoke-all" => {
            let registry = RunnerRegistry::load(&options.required("registry")?)?;
            let builds: Vec<_> = registry
                .builds
                .iter()
                .filter(|build| build.reproducibility.status == ReproducibilityStatus::Reproducible)
                .cloned()
                .collect();
            let records = execute_smoke(
                &builds,
                &options.required("corpus")?,
                &options.required("schema")?,
                &options.required("root")?,
            )?;
            write_smoke_output(&records, &options.required("output")?)?;
            write_runner_diagnostics(&records, &options.required("diagnostics")?)?;
            let report = adapter_migration_report(&registry, &records)?;
            fs::write(
                options.required("report")?,
                occurframe_conformance::canonical_pretty_json(&report)?,
            )?;
            print_json(&report)?;
            if records.iter().any(|record| {
                matches!(
                    record.observation.execution_status,
                    ExecutionStatus::RunnerFailure | ExecutionStatus::Timeout
                )
            }) {
                return Err("adapter smoke contained runner failure or timeout".into());
            }
        }
        "differential-certify" => {
            let corpus_path = options.required("corpus")?;
            let (corpus, _) = load_and_validate_corpus(&corpus_path)?;
            let registry = RunnerRegistry::load(&options.required("registry")?)?;
            let profile = load_profile(&options.required("profile")?)?;
            let builds: Vec<_> = registry
                .builds
                .iter()
                .filter(|build| build.reproducibility.status == ReproducibilityStatus::Reproducible)
                .cloned()
                .collect();
            let schema = ProtocolSchema::load(&options.required("schema")?)?;
            let records = run_batch(
                &builds,
                &corpus.vectors,
                &options.required("root")?,
                &schema,
                std::time::Duration::from_millis(profile.execution.infrastructure_watchdog_ms),
            );
            let environment = load_json(&options.required("environment")?)?;
            let legacy_matrix = load_json(&options.required("legacy-matrix")?)?;
            let legacy_build_map = load_legacy_build_map(&options.required("legacy-map")?)?;
            let observation_schema =
                load_json(&corpus_path.join("schemas/normalized-observation.schema.json"))?;
            let conformance_schema =
                load_json(&corpus_path.join("schemas/conformance-result.schema.json"))?;
            let summary = generate_bundle(
                &BundleInput {
                    profile: &profile,
                    registry: &registry,
                    corpus: &corpus,
                    executions: &records,
                    environment: &environment,
                    tooling_source_sha: &options.required_string("tooling-sha")?,
                    legacy_matrix: &legacy_matrix,
                    legacy_build_map: &legacy_build_map,
                    observation_schema: &observation_schema,
                    conformance_schema: &conformance_schema,
                },
                &options.required("output")?,
            )?;
            print_json(&summary)?;
        }
        "differential-verify" => {
            let profile = load_profile(&options.required("profile")?)?;
            let result = verify_deterministic_bundles(
                &profile,
                &options.required("first")?,
                &options.required("second")?,
            )?;
            print_json(&result)?;
        }
        "certification-verify" => {
            verify_bundle_checksums(&options.required("directory")?)?;
            print_json(&serde_json::json!({"checksums_valid": true}))?;
        }
        "release-package" => {
            let report = package_release_candidate(&ReleaseInputs {
                repository_root: options.required("root")?,
                corpus_root: options.required("corpus")?,
                certification_root: options.required("certification")?,
                binaries_root: options.required("binaries")?,
                lock_path: options.required("lock")?,
                output: options.required("output")?,
                commit: options.optional_string("commit"),
            })?;
            print_json(&report)?;
        }
        "dependency-inventory" => {
            let manifest = options.required("manifest")?;
            let (inventory, notices) = deps::dependency_inventory(
                &manifest,
                &deps::lock_beside(&manifest),
                &options
                    .optional_string("package")
                    .unwrap_or_else(|| "occurframe-cli".to_owned()),
            )?;
            fs::write(
                options.required("output")?,
                canonical_pretty_json(&inventory)?,
            )?;
            if let Some(path) = options
                .values
                .get("notices")
                .and_then(|values| values.first())
            {
                fs::write(path, deps::render_notices(&inventory, &notices))?;
            }
            print_json(&serde_json::json!({
                "first_party": inventory.first_party.len(),
                "third_party": inventory.third_party_count
            }))?;
        }
        "audit-paths" => {
            let report =
                audit::audit_paths(&options.required("root")?, &options.all_strings("forbid"))?;
            print_json(&report)?;
            if !report.leaks.is_empty() {
                return Err(format!(
                    "{} absolute developer/CI path leak(s) found in the packaged artifact",
                    report.leaks.len()
                )
                .into());
            }
        }
        "verify-evidence-archive" => {
            let report = verify_evidence_archive(
                &options.required("root")?,
                &options.required("lock")?,
                options.values.get("extracted").and_then(|v| v.first()),
            )?;
            print_json(&report)?;
        }
        "release-attest" => {
            let attestation = release_attestation(
                &options.required("bundle")?,
                options
                    .values
                    .get("archive")
                    .and_then(|values| values.first()),
            )?;
            fs::write(
                options.required("output")?,
                canonical_pretty_json(&attestation)?,
            )?;
            print_json(&attestation)?;
        }
        _ => return Err(format!("unknown command: {command}").into()),
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseEvidenceLock {
    schema_version: String,
    tool_version: String,
    corpus: ReleaseCorpusLock,
    certification: ReleaseCertificationLock,
    specification: ReleaseSpecificationLock,
    evidence_archive: ReleaseEvidenceArchiveLock,
    platform_binaries: Vec<String>,
    provenance_blocked_builds: Vec<String>,
}

/// The behavioural specification this release implements.
///
/// It versions independently of the tooling, the corpus and the runner protocol.
/// The value is pinned here and checked against `occurframe_wire::SPECIFICATION_VERSION`
/// and, when the corpus checkout carries `spec/specification.json`, against that
/// declaration too — so the three cannot drift apart unnoticed.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseSpecificationLock {
    version: String,
    errata_resolving_command_doctrine: String,
    shipped_v1_commands: Vec<String>,
}

/// The durable, in-repository copy of the certified RC2 evidence.
///
/// Release assembly must not depend on a CI artifact: hosted artifacts expire,
/// and once one does, the release can never be reassembled from its own inputs.
/// The evidence therefore lives in the repository, and its digest lives in the
/// immutable lock, so a future assembly restores exactly the bytes that were
/// certified or fails loudly.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseEvidenceArchiveLock {
    path: String,
    sha256: String,
    extracts_to: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseCorpusLock {
    version: String,
    sha: String,
    vectors: usize,
    canonical_digest: String,
    release_digest: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseCertificationLock {
    artifact_name: String,
    tooling_source_sha: String,
    profile_version: String,
    semantic_bundle_digest: String,
    matrix_sha256: String,
    configured_builds: usize,
    reproducible_builds: usize,
    unreproducible_builds: usize,
    vectors: usize,
    observations: usize,
    semantic_divergence_vectors: usize,
    normative_violation_vectors: usize,
}

#[derive(Debug, serde::Serialize)]
struct ReleasePackageReport {
    tool_version: String,
    corpus_version: String,
    corpus_digest: String,
    certification_digest: String,
    binary_count: usize,
    bundle_directory: String,
    bundle_digest: String,
    certification_artifact_name: String,
}

/// Everything `release-package` needs. Grouped so that adding a release input
/// does not silently reorder a long positional argument list.
struct ReleaseInputs {
    repository_root: PathBuf,
    corpus_root: PathBuf,
    certification_root: PathBuf,
    binaries_root: PathBuf,
    lock_path: PathBuf,
    output: PathBuf,
    /// The exact commit being released. CI passes the SHA it checked out;
    /// otherwise the working checkout is asked.
    commit: Option<String>,
}

/// Copy a directory tree, deterministically and without following symlinks.
///
/// `filter` selects which files are published, so that developer-only material
/// in a source directory never reaches the distribution by accident.
fn copy_tree(
    source: &Path,
    destination: &Path,
    filter: &dyn Fn(&Path) -> bool,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut entries: Vec<_> = walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| filter(path))
        .collect();
    entries.sort();
    for path in &entries {
        let relative = path.strip_prefix(source)?;
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &target)?;
    }
    Ok(entries.len())
}

/// Out-of-bundle attestation.
///
/// The digest of an archive cannot live inside the archive it describes, so the
/// bundle digest and the transport-archive digest are recorded beside the
/// release rather than within it.
/// Result of checking the durable evidence archive against the immutable lock.
#[derive(Debug, serde::Serialize)]
struct EvidenceArchiveReport {
    archive: String,
    sha256: String,
    bytes: u64,
    extracts_to: String,
    /// Present once an extracted directory has also been checksum-verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    extracted_checksums_valid: Option<bool>,
}

/// Verify the in-repository certified evidence against `release/evidence-lock.json`.
///
/// This is what replaces a dependency on an expiring CI artifact. The archive is
/// checked by digest before anything is unpacked, and — when an already-extracted
/// directory is supplied — every file inside it is re-verified against the
/// certification bundle's own `SHA256SUMS`.
fn verify_evidence_archive(
    repository_root: &Path,
    lock_path: &Path,
    extracted: Option<&PathBuf>,
) -> Result<EvidenceArchiveReport, Box<dyn std::error::Error>> {
    let lock: ReleaseEvidenceLock = serde_json::from_slice(&fs::read(lock_path)?)?;
    let archive_path = repository_root.join(&lock.evidence_archive.path);
    let bytes = fs::read(&archive_path).map_err(|error| {
        format!(
            "durable certification evidence is missing at {}: {error}",
            archive_path.display()
        )
    })?;
    let digest = sha256_hex(&bytes);
    if digest != lock.evidence_archive.sha256 {
        return Err(format!(
            "certification evidence archive digest mismatch: lock {}, found {digest}",
            lock.evidence_archive.sha256
        )
        .into());
    }
    let extracted_checksums_valid = match extracted {
        Some(directory) => {
            verify_bundle_checksums(directory)?;
            Some(true)
        }
        None => None,
    };
    Ok(EvidenceArchiveReport {
        archive: lock.evidence_archive.path,
        sha256: digest,
        bytes: bytes.len() as u64,
        extracts_to: lock.evidence_archive.extracts_to,
        extracted_checksums_valid,
    })
}

/// Prove the specification version agrees everywhere it is recorded.
///
/// Three places can state it: the compiled-in constant, the immutable release
/// lock, and the corpus's own `spec/specification.json`. A release whose
/// components disagreed about which specification it implements would be
/// unreproducible, so packaging refuses rather than picking one.
fn validate_specification_identity(
    lock: &ReleaseSpecificationLock,
    corpus_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if lock.version != occurframe_wire::SPECIFICATION_VERSION {
        return Err(format!(
            "specification version mismatch: lock {}, tooling {}",
            lock.version,
            occurframe_wire::SPECIFICATION_VERSION
        )
        .into());
    }
    if lock.shipped_v1_commands != ["test"] {
        return Err(format!(
            "the shipped v1 command surface is `test` alone; the lock names {:?}",
            lock.shipped_v1_commands
        )
        .into());
    }
    // The corpus is the authoring side. Older pinned checkouts predate the
    // declaration, so its absence is not an error; disagreement is.
    let declaration = corpus_root.join("spec/specification.json");
    if declaration.is_file() {
        let declared: serde_json::Value = serde_json::from_slice(&fs::read(&declaration)?)?;
        let declared_version = declared
            .get("specification_version")
            .and_then(serde_json::Value::as_str)
            .ok_or("corpus spec/specification.json has no specification_version")?;
        if declared_version != lock.version {
            return Err(format!(
                "specification version mismatch: corpus declares {declared_version}, release lock pins {}",
                lock.version
            )
            .into());
        }
    }
    Ok(())
}

/// Human-readable licensing map for the bundle root.
fn render_licenses(licensing: &release::Licensing, third_party_count: usize) -> String {
    format!(
        "# Licensing\n\nThis release redistributes material under more than one license.\n\n| Part of the release | License |\n| --- | --- |\n| Occurframe Rust code (`bin/`) | {} |\n| Reference and tooling code | {} |\n| Corpus semantic data (`corpus/`) | {} |\n| Third-party crates linked into the binaries ({third_party_count}) | {} |\n\nFull texts of the Occurframe licenses are in `LICENSE-APACHE` and `LICENSE-MIT`\nat the root of this bundle. `Apache-2.0 OR MIT` is a choice: a consumer may take\neither license, so anyone requiring Apache-2.0 alone may simply use Apache-2.0.\n\nThe corpus is authored in the separate `occurframe/corpus` repository and its\nvectors, registries and schemas are dedicated to the public domain under\nCC0-1.0. Generated observations, matrices and reports are derived artifacts, not\nnormative source.\n\n`THIRD-PARTY-NOTICES.md` reproduces the license and notice texts published with\neach linked crate, and `DEPENDENCIES.json` records name, version, source,\nregistry checksum and declared license for each. Where a crate publishes no\nlicense expression, that fact is recorded rather than a license being assumed.\n\nNo third-party recurrence engine or engine runtime is redistributed here. The\nadapter registry in `adapters/` records engine identity and provenance only.\n",
        licensing.occurframe_code,
        licensing.reference_and_tooling_code,
        licensing.corpus_semantic_data,
        licensing.third_party
    )
}

#[derive(Debug, serde::Serialize)]
struct ReleaseAttestation {
    schema_version: String,
    artifact_kind: String,
    tool_version: String,
    /// SHA-256 of the bundle's `SHA256SUMS`, which in turn covers every file.
    bundle_checksums_digest: String,
    bundle_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_sha256: Option<String>,
}

fn release_attestation(
    bundle: &Path,
    archive: Option<&PathBuf>,
) -> Result<ReleaseAttestation, Box<dyn std::error::Error>> {
    let checksums = fs::read(bundle.join("SHA256SUMS"))?;
    // One line per packaged file; the manifest is newline-terminated.
    let bundle_files = checksums
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .count();
    let (archive_file, archive_sha256) = match archive {
        Some(path) => (
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Some(sha256_hex(&fs::read(path)?)),
        ),
        None => (None, None),
    };
    Ok(ReleaseAttestation {
        schema_version: "1.0.0".into(),
        artifact_kind: "occurframe_release_attestation".into(),
        tool_version: env!("CARGO_PKG_VERSION").into(),
        bundle_checksums_digest: sha256_hex(&checksums),
        bundle_files,
        archive_file,
        archive_sha256,
    })
}

#[allow(clippy::too_many_lines)]
fn package_release_candidate(
    inputs: &ReleaseInputs,
) -> Result<ReleasePackageReport, Box<dyn std::error::Error>> {
    let ReleaseInputs {
        repository_root,
        corpus_root,
        certification_root,
        binaries_root,
        lock_path,
        output,
        commit,
    } = inputs;
    let (repository_root, corpus_root, certification_root, binaries_root, lock_path, output) = (
        repository_root.as_path(),
        corpus_root.as_path(),
        certification_root.as_path(),
        binaries_root.as_path(),
        lock_path.as_path(),
        output.as_path(),
    );
    if output.exists() {
        return Err(format!(
            "release output already exists: {}; use a new empty path",
            output.display()
        )
        .into());
    }
    let lock: ReleaseEvidenceLock = serde_json::from_slice(&fs::read(lock_path)?)?;
    if lock.schema_version != "1.0.0" || lock.tool_version != env!("CARGO_PKG_VERSION") {
        return Err("release evidence lock does not match the tooling prerelease".into());
    }
    validate_specification_identity(&lock.specification, corpus_root)?;
    verify_bundle_checksums(certification_root)?;
    let manifest: CertificationManifest = serde_json::from_slice(&fs::read(
        certification_root.join("certification-manifest.json"),
    )?)?;
    let matrix_bytes = fs::read(certification_root.join("matrix.json"))?;
    let matrix: DifferentialMatrix = serde_json::from_slice(&matrix_bytes)?;
    validate_release_evidence(&lock, &manifest, &matrix, &matrix_bytes)?;

    fs::create_dir_all(output.join("bin"))?;
    fs::create_dir_all(output.join("reports"))?;
    fs::create_dir_all(output.join("certification"))?;
    fs::create_dir_all(output.join("adapters"))?;
    let mut binaries = Vec::new();
    let mut target_triples = BTreeSet::new();
    for binary in &lock.platform_binaries {
        let source = binaries_root.join(binary);
        if !source.is_file() {
            return Err(format!("missing required platform binary {}", source.display()).into());
        }
        fs::copy(&source, output.join("bin").join(binary))?;
        let (alias, target) = release::split_binary_name(binary)?;
        target_triples.insert(target.clone());
        binaries.push(release::BinaryEntry {
            path: format!("bin/{binary}"),
            alias,
            target,
            sha256: sha256_hex(&fs::read(&source)?),
        });
    }
    binaries.sort_by(|left, right| left.path.cmp(&right.path));

    let corpus_report = pack_release(corpus_root, &output.join("corpus"))?;
    if corpus_report.corpus_version != lock.corpus.version
        || corpus_report.vector_count != lock.corpus.vectors
        || corpus_report.canonical_corpus_digest != lock.corpus.canonical_digest
        || corpus_report.release_digest != lock.corpus.release_digest
    {
        return Err("generated corpus distribution differs from the release lock".into());
    }
    fs::create_dir_all(output.join("corpus/schemas"))?;
    fs::copy(
        corpus_root.join("schemas/runner-protocol-v2.schema.json"),
        output.join("corpus/schemas/runner-protocol-v2.schema.json"),
    )?;
    fs::copy(
        repository_root.join("runners/registry/runner-builds.json"),
        output.join("adapters/runner-builds.json"),
    )?;
    fs::copy(
        certification_root.join("matrix.json"),
        output.join("reports/matrix.json"),
    )?;
    fs::write(
        output.join("reports/differential-report.md"),
        public_release_report(&matrix, &manifest, &lock.provenance_blocked_builds)?,
    )?;
    for name in ["certification-manifest.json", "environment.json"] {
        fs::copy(
            certification_root.join(name),
            output.join("certification").join(name),
        )?;
    }
    fs::copy(
        repository_root.join("release/README.md"),
        output.join("README.md"),
    )?;

    // Public documentation travels with the release: a consumer who downloaded
    // an archive has no checkout to read docs from.
    let documentation = copy_tree(
        &repository_root.join("docs"),
        &output.join("docs"),
        &|path| path.extension().is_some_and(|extension| extension == "md"),
    )?;
    if documentation == 0 {
        return Err("no public documentation was available for release packaging".into());
    }
    // The protocol example must be runnable straight from an extracted release,
    // because that is exactly the clean-room path a new integrator takes.
    let examples = copy_tree(
        &repository_root.join("examples"),
        &output.join("examples"),
        &|path| {
            !path
                .components()
                .any(|component| component.as_os_str() == "__pycache__")
        },
    )?;
    if examples == 0 {
        return Err("no protocol example was available for release packaging".into());
    }
    // The release notes belong with the artifact, not only on a release page: a
    // consumer working offline should be able to read what they downloaded.
    let notes = repository_root
        .join("release-notes")
        .join(format!("{}.md", lock.tool_version));
    if !notes.is_file() {
        return Err(format!("release notes are missing: {}", notes.display()).into());
    }
    fs::create_dir_all(output.join("release-notes"))?;
    fs::copy(
        &notes,
        output
            .join("release-notes")
            .join(format!("{}.md", lock.tool_version)),
    )?;

    let mut license_count = 0_usize;
    for entry in fs::read_dir(repository_root)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.file_name().to_string_lossy().starts_with("LICENSE")
        {
            fs::copy(entry.path(), output.join(entry.file_name()))?;
            license_count += 1;
        }
    }
    if license_count == 0 {
        return Err("no LICENSE file was available for release packaging".into());
    }
    // A corpus checkout that carries its own license file publishes it with the
    // corpus data it covers.
    for name in ["LICENSE", "LICENSE-CC0", "LICENSE.txt", "COPYING"] {
        let candidate = corpus_root.join(name);
        if candidate.is_file() {
            fs::copy(&candidate, output.join("corpus").join(name))?;
        }
    }

    // Dependency inventory and third-party notices for everything the binaries
    // statically link.
    let workspace_manifest = repository_root.join("Cargo.toml");
    let (inventory, notices) = deps::dependency_inventory(
        &workspace_manifest,
        &deps::lock_beside(&workspace_manifest),
        "occurframe-cli",
    )?;
    let inventory_bytes = canonical_pretty_json(&inventory)?;
    fs::write(output.join("DEPENDENCIES.json"), &inventory_bytes)?;
    let notice_bytes = deps::render_notices(&inventory, &notices).into_bytes();
    fs::write(output.join("THIRD-PARTY-NOTICES.md"), &notice_bytes)?;
    let licensing = release::Licensing {
        occurframe_code: "Apache-2.0 OR MIT (see LICENSE-APACHE and LICENSE-MIT)".into(),
        reference_and_tooling_code: "Apache-2.0 OR MIT (see LICENSE-APACHE and LICENSE-MIT)".into(),
        corpus_semantic_data:
            "CC0-1.0 (occurframe/corpus authored vectors, registries and schemas)".into(),
        third_party: "per crate; see THIRD-PARTY-NOTICES.md and DEPENDENCIES.json".into(),
    };
    fs::write(
        output.join("LICENSES.md"),
        render_licenses(&licensing, inventory.third_party_count),
    )?;

    let release_manifest = release::ReleaseManifest {
        schema_version: "2.0.0".into(),
        artifact_kind: "occurframe_release_candidate".into(),
        tool_version: lock.tool_version.clone(),
        specification_version: lock.specification.version.clone(),
        specification_errata: vec![lock.specification.errata_resolving_command_doctrine.clone()],
        shipped_commands: lock.specification.shipped_v1_commands.clone(),
        tooling_repository: "https://github.com/occurframe/occurframe".into(),
        tooling_commit_sha: release::commit_sha(repository_root, commit.as_deref())?,
        toolchain: release::Toolchain::detect()?,
        target_triples: target_triples.into_iter().collect(),
        binaries,
        corpus: release::CorpusIdentity {
            version: corpus_report.corpus_version.clone(),
            repository: "https://github.com/occurframe/corpus".into(),
            sha: lock.corpus.sha.clone(),
            canonical_digest: corpus_report.canonical_corpus_digest.clone(),
            release_digest: corpus_report.release_digest.clone(),
            vectors: corpus_report.vector_count,
        },
        runner_protocol_version: occurframe_wire::RUNNER_PROTOCOL_VERSION.into(),
        certification: release::CertificationIdentity {
            artifact_name: lock.certification.artifact_name.clone(),
            profile_version: lock.certification.profile_version.clone(),
            tooling_source_sha: lock.certification.tooling_source_sha.clone(),
            certification_manifest_sha256: sha256_hex(&fs::read(
                certification_root.join("certification-manifest.json"),
            )?),
            semantic_bundle_digest: lock.certification.semantic_bundle_digest.clone(),
            matrix_sha256: lock.certification.matrix_sha256.clone(),
            population: release::EvidencePopulation {
                configured_builds: lock.certification.configured_builds,
                reproducible_builds: lock.certification.reproducible_builds,
                provenance_blocked_builds: lock.certification.unreproducible_builds,
                vectors: lock.certification.vectors,
                observations: lock.certification.observations,
                semantic_divergence_vectors: lock.certification.semantic_divergence_vectors,
                normative_violation_vectors: lock.certification.normative_violation_vectors,
            },
        },
        licensing,
        inventories: vec![
            release::FileDigest {
                path: "DEPENDENCIES.json".into(),
                sha256: sha256_hex(&inventory_bytes),
            },
            release::FileDigest {
                path: "THIRD-PARTY-NOTICES.md".into(),
                sha256: sha256_hex(&notice_bytes),
            },
        ],
        source_date_epoch: release::source_date_epoch(),
        notes: vec![
            "No build timestamp is recorded. Two assemblies of the same inputs produce the same manifest."
                .into(),
            "The digest of the transport archive cannot be carried inside the archive; `xtask release-attest` records it beside the release."
                .into(),
            "Third-party engine runtimes are deliberately not redistributed. Only the adapter identity registry ships."
                .into(),
            "Occurframe ships one semantic command, `test`. `explain`, `classify` and `occurrences` are deferred behind the engine gate by ERRATA-001 because each requires a recurrence evaluator that the ORACLE ONLY verdict does not authorise. No evaluator is present in this release."
                .into(),
        ],
    };
    fs::write(
        output.join("release-manifest.json"),
        canonical_pretty_json(&release_manifest)?,
    )?;
    fs::write(
        output.join("VERSION"),
        release::render_version_file(&release_manifest),
    )?;

    // Nothing is checksummed until the bundle is proven free of build-machine
    // paths, so a leaking artifact can never acquire a valid SHA256SUMS.
    let leaks = audit::audit_paths(output, &[])?;
    if !leaks.leaks.is_empty() {
        return Err(format!(
            "release bundle contains {} absolute developer/CI path leak(s); first: {} in {}",
            leaks.leaks.len(),
            leaks.leaks[0].pattern,
            leaks.leaks[0].file
        )
        .into());
    }

    let checksum_path = output.join("SHA256SUMS");
    write_tree_checksums(output, &checksum_path)?;
    let bundle_digest = sha256_hex(&fs::read(&checksum_path)?);
    Ok(ReleasePackageReport {
        tool_version: lock.tool_version,
        corpus_version: corpus_report.corpus_version,
        corpus_digest: corpus_report.canonical_corpus_digest,
        certification_digest: manifest.semantic_bundle_digest,
        binary_count: lock.platform_binaries.len(),
        bundle_directory: output.to_string_lossy().into_owned(),
        bundle_digest,
        certification_artifact_name: lock.certification.artifact_name,
    })
}

fn validate_release_evidence(
    lock: &ReleaseEvidenceLock,
    manifest: &CertificationManifest,
    matrix: &DifferentialMatrix,
    matrix_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let certification = &lock.certification;
    let manifest_matches = manifest.tooling_source_sha == certification.tooling_source_sha
        && manifest.certification_profile_version == certification.profile_version
        && manifest.semantic_bundle_digest == certification.semantic_bundle_digest
        && manifest.corpus_sha == lock.corpus.sha
        && manifest.corpus_version == lock.corpus.version
        && manifest.configured_builds == certification.configured_builds
        && manifest.reproducible_builds == certification.reproducible_builds
        && manifest.unreproducible_builds == certification.unreproducible_builds
        && manifest.vectors == certification.vectors
        && manifest.observations == certification.observations;
    let matrix_matches = sha256_hex(matrix_bytes) == certification.matrix_sha256
        && matrix.summary.semantic_divergence_vectors == certification.semantic_divergence_vectors
        && matrix.summary.normative_violation_vectors == certification.normative_violation_vectors
        && matrix.summary.actual_observations == certification.observations
        && matrix.summary.vectors == certification.vectors;
    if !manifest_matches || !matrix_matches {
        return Err("certification evidence differs from the immutable release lock".into());
    }
    Ok(())
}

fn write_runner_diagnostics(
    records: &[CaseExecution],
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let diagnostics: Vec<_> = records
        .iter()
        .filter_map(|record| {
            record.runner_diagnostic.as_ref().map(|diagnostic| {
                serde_json::json!({
                    "build_id": record.build_id,
                    "vector_id": record.observation.vector_id,
                    "diagnostic": diagnostic
                })
            })
        })
        .collect();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        output,
        occurframe_conformance::canonical_pretty_json(&diagnostics)?,
    )?;
    Ok(())
}

fn runner_contract() -> Result<(), Box<dyn std::error::Error>> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["test", "-p", "occurframe-runner", "--test", "protocol"])
        .status()?;
    if !status.success() {
        return Err("fake-runner protocol contract suite failed".into());
    }
    Ok(())
}

fn execute_smoke(
    builds: &[RunnerBuild],
    corpus_path: &Path,
    schema_path: &Path,
    root: &Path,
) -> Result<Vec<CaseExecution>, Box<dyn std::error::Error>> {
    let (corpus, _) = load_and_validate_corpus(corpus_path)?;
    let by_id: BTreeMap<_, _> = corpus
        .vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let schema = ProtocolSchema::load(schema_path)?;
    let mut records = Vec::new();
    for build in builds {
        let vectors: Vec<Vector> = build
            .representative_vectors
            .iter()
            .map(|id| {
                by_id.get(id.as_str()).copied().cloned().ok_or_else(|| {
                    format!("{} references missing smoke vector {id}", build.build_id)
                })
            })
            .collect::<Result<_, _>>()?;
        records.extend(run_batch(
            std::slice::from_ref(build),
            &vectors,
            root,
            &schema,
            occurframe_runner::DEFAULT_INFRASTRUCTURE_WATCHDOG,
        ));
    }
    records.sort_by(|left, right| {
        left.observation
            .vector_id
            .cmp(&right.observation.vector_id)
            .then_with(|| left.build_id.cmp(&right.build_id))
    });
    Ok(records)
}

fn write_smoke_output(
    records: &[CaseExecution],
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, semantic_observation_ndjson(records)?)?;
    Ok(())
}

fn smoke_summary(records: &[CaseExecution]) -> serde_json::Value {
    serde_json::json!({
        "digest": semantic_observation_digest(records).expect("serialize validated observations"),
        "records": records.len(),
        "runner_failures": records.iter().filter(|record| record.observation.execution_status == ExecutionStatus::RunnerFailure).count(),
        "timeouts": records.iter().filter(|record| record.observation.execution_status == ExecutionStatus::Timeout).count()
    })
}

fn adapter_migration_report(
    registry: &RunnerRegistry,
    records: &[CaseExecution],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let languages: BTreeSet<_> = registry
        .builds
        .iter()
        .map(|build| build.language.clone())
        .collect();
    let reproducible: BTreeSet<_> = registry
        .builds
        .iter()
        .filter(|build| build.reproducibility.status == ReproducibilityStatus::Reproducible)
        .map(|build| build.build_id.clone())
        .collect();
    let unreproducible: Vec<_> = registry
        .builds
        .iter()
        .filter(|build| build.reproducibility.status == ReproducibilityStatus::Unreproducible)
        .map(|build| {
            serde_json::json!({
                "build_id": build.build_id,
                "reason": build.reproducibility.reason
            })
        })
        .collect();
    let mut outcome_counts = BTreeMap::<String, usize>::new();
    let mut failures = BTreeSet::new();
    let mut launched = BTreeSet::new();
    let mut handshaken = BTreeSet::new();
    let mut vectors = BTreeSet::new();
    let mut protocol_failures = 0_usize;
    for record in records {
        vectors.insert(record.observation.vector_id.clone());
        let startup_failed = record.runner_diagnostic.as_ref().is_some_and(|diagnostic| {
            matches!(
                diagnostic.diagnostic.code.as_str(),
                "startup_failure" | "hello_failure" | "identity_mismatch" | "malformed_hello"
            )
        });
        if !startup_failed {
            launched.insert(record.build_id.clone());
            handshaken.insert(record.build_id.clone());
        }
        if record.observation.execution_status == ExecutionStatus::RunnerFailure {
            failures.insert(record.build_id.clone());
        }
        if record.runner_diagnostic.as_ref().is_some_and(|diagnostic| {
            diagnostic.diagnostic.code.contains("protocol")
                || diagnostic.diagnostic.code.contains("identity")
                || diagnostic.diagnostic.code.contains("started")
                || diagnostic.diagnostic.code.contains("terminal")
                || diagnostic.diagnostic.code.contains("hello")
        }) {
            protocol_failures += 1;
        }
        let outcome = match &record.observation.engine_outcome {
            Some(EngineOutcome::Occurrences { .. }) => "occurrences",
            Some(EngineOutcome::Accepted) => "accepted",
            Some(EngineOutcome::Rejection { .. }) => "rejection",
            Some(EngineOutcome::Unsupported { .. }) => "unsupported",
            Some(EngineOutcome::EngineError { .. }) => "engine_error",
            None if record.observation.execution_status == ExecutionStatus::Timeout => "timeout",
            None => "runner_failure",
        };
        *outcome_counts.entry(outcome.into()).or_default() += 1;
    }
    let adapter_paths = BTreeMap::from([
        ("Go", "runners/go/main.go"),
        ("JavaScript", "runners/javascript/runner.mjs"),
        ("PHP", "runners/php/runner.php"),
        ("Python", "runners/python/runner.py"),
        ("Ruby", "runners/ruby/runner.rb"),
    ]);
    Ok(serde_json::json!({
        "artifact_kind": "adapter_migration_report_not_differential_matrix",
        "protocol_version": "2.0",
        "language_runners_migrated": languages,
        "engine_builds_configured": registry.builds.len(),
        "engine_builds_reproducible": reproducible.len(),
        "engine_builds_unreproducible": unreproducible,
        "engine_builds_successfully_launched": launched,
        "engine_builds_successfully_handshaken": handshaken,
        "representative_vectors_executed": vectors,
        "outcome_counts": outcome_counts,
        "protocol_failures": protocol_failures,
        "runner_failures": records.iter().filter(|record| record.observation.execution_status == ExecutionStatus::RunnerFailure).count(),
        "timeouts": records.iter().filter(|record| record.observation.execution_status == ExecutionStatus::Timeout).count(),
        "unsupported_cases": outcome_counts.get("unsupported").copied().unwrap_or(0),
        "legacy_runner_source": "occurframe/corpus legacy/phase2-rc1/runners",
        "new_adapter_paths": adapter_paths,
        "semantic_observation_digest": semantic_observation_digest(records)?,
        "failed_builds": failures
    }))
}

fn print_json(value: &impl serde::Serialize) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[derive(Debug, Default)]
struct Options {
    /// Repeatable by name, so that an option such as `--forbid` can be given
    /// more than once without a bespoke parser.
    values: std::collections::BTreeMap<String, Vec<PathBuf>>,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let arguments: Vec<_> = arguments.collect();
        if arguments.len() % 2 != 0 {
            return Err("options must be provided as --name VALUE pairs".into());
        }
        let mut values: std::collections::BTreeMap<String, Vec<PathBuf>> =
            std::collections::BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            let name = pair[0]
                .strip_prefix("--")
                .ok_or_else(|| format!("expected --option, found {}", pair[0]))?;
            values
                .entry(name.to_owned())
                .or_default()
                .push(PathBuf::from(&pair[1]));
        }
        Ok(Self { values })
    }

    fn required(&self, name: &str) -> Result<PathBuf, String> {
        self.values
            .get(name)
            .and_then(|values| values.first())
            .cloned()
            .ok_or_else(|| format!("missing --{name}"))
    }

    fn required_string(&self, name: &str) -> Result<String, String> {
        self.optional_string(name)
            .ok_or_else(|| format!("missing --{name}"))
    }

    fn optional_string(&self, name: &str) -> Option<String> {
        self.values
            .get(name)
            .and_then(|values| values.first())
            .map(|value| value.to_string_lossy().into_owned())
    }

    fn all_strings(&self, name: &str) -> Vec<String> {
        self.values
            .get(name)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specification_lock(version: &str, commands: &[&str]) -> ReleaseSpecificationLock {
        ReleaseSpecificationLock {
            version: version.to_owned(),
            errata_resolving_command_doctrine: "ERRATA-001".into(),
            shipped_v1_commands: commands.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn the_specification_version_must_agree_everywhere_it_is_recorded() {
        let empty = Path::new("does-not-exist");
        // The pinned value, the compiled-in constant and the shipped surface all agree.
        assert!(
            validate_specification_identity(
                &specification_lock(occurframe_wire::SPECIFICATION_VERSION, &["test"]),
                empty,
            )
            .is_ok()
        );
        // A lock that drifts from the tooling constant is refused.
        assert!(
            validate_specification_identity(&specification_lock("9.9.9", &["test"]), empty)
                .expect_err("drift must fail")
                .to_string()
                .contains("specification version mismatch")
        );
        // A lock that claims a deferred evaluator command is shipped is refused:
        // ERRATA-001 fixes the v1 surface at `test` alone.
        assert!(
            validate_specification_identity(
                &specification_lock(occurframe_wire::SPECIFICATION_VERSION, &["test", "explain"]),
                empty,
            )
            .expect_err("a deferred command must not be declared shipped")
            .to_string()
            .contains("shipped v1 command surface")
        );
    }

    #[test]
    fn a_corpus_declaring_a_different_specification_version_is_refused() {
        let directory =
            std::env::temp_dir().join(format!("occurframe-spec-identity-{}", std::process::id()));
        fs::create_dir_all(directory.join("spec")).expect("fixture directory");
        fs::write(
            directory.join("spec/specification.json"),
            br#"{"specification_version": "0.0.1-other"}"#,
        )
        .expect("fixture declaration");
        let error = validate_specification_identity(
            &specification_lock(occurframe_wire::SPECIFICATION_VERSION, &["test"]),
            &directory,
        )
        .expect_err("cross-repository drift must fail");
        assert!(error.to_string().contains("corpus declares 0.0.1-other"));
        fs::remove_dir_all(&directory).expect("remove isolated fixture directory");
    }
}
