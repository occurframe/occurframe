use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use occurframe_conformance::{
    load_and_validate_corpus, migrate_rc1, pack_release, verify_deterministic_pack,
    verify_manifest, verify_migration, write_tree_checksums,
};
use occurframe_report::{
    BundleInput, generate_bundle, load_json, load_legacy_build_map, load_profile,
    verify_bundle_checksums, verify_deterministic_bundles,
};
use occurframe_runner::{
    CaseExecution, ProtocolSchema, ReproducibilityStatus, RunnerBuild, RunnerRegistry, run_batch,
    semantic_observation_digest, semantic_observation_ndjson,
};
use occurframe_wire::{EngineOutcome, ExecutionStatus, Vector};

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
        _ => return Err(format!("unknown command: {command}").into()),
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
    values: std::collections::BTreeMap<String, PathBuf>,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let arguments: Vec<_> = arguments.collect();
        if arguments.len() % 2 != 0 {
            return Err("options must be provided as --name PATH pairs".into());
        }
        let mut values = std::collections::BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            let name = pair[0]
                .strip_prefix("--")
                .ok_or_else(|| format!("expected --option, found {}", pair[0]))?;
            values.insert(name.to_owned(), PathBuf::from(&pair[1]));
        }
        Ok(Self { values })
    }

    fn required(&self, name: &str) -> Result<PathBuf, String> {
        self.values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing --{name}"))
    }

    fn required_string(&self, name: &str) -> Result<String, String> {
        self.values
            .get(name)
            .map(|value| value.to_string_lossy().into_owned())
            .ok_or_else(|| format!("missing --{name}"))
    }
}
