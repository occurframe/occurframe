//! Shared implementation for the `occurframe` and `oframe` conformance clients.
//!
//! The crate is an orchestration and rendering surface. It delegates runner
//! execution, normalization, and scoring to the existing production crates and
//! contains no recurrence parser or evaluator.

#![allow(clippy::missing_errors_doc, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fmt::Write as _,
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use occurframe_conformance::{canonical_pretty_json, load_compatible_corpus};
use occurframe_runner::{
    CaseExecution, DEFAULT_INFRASTRUCTURE_WATCHDOG, ProtocolSchema, ReproducibilityStatus,
    RunnerBuild, RunnerRegistry, run_batch,
};
use occurframe_wire::{
    Classification, ComponentIdentity, ConformanceVerdict, Diagnostic, DialectId, EngineOutcome,
    ExecutionStatus, RUNNER_PROTOCOL_VERSION, RuntimeIdentity, SPECIFICATION_VERSION,
    SemanticValue, TzdbProvenance, TzdbRelease, VerdictStatus,
};
use serde::{Deserialize, Serialize};

/// Tooling/CLI prerelease version.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

const EXIT_SUCCESS: u8 = 0;
const EXIT_CONFORMANCE: u8 = 1;
const EXIT_USAGE: u8 = 3;
const EXIT_ENVIRONMENT: u8 = 4;
const CORPUS_LOCK_JSON: &str = include_str!("../assets/corpus-lock.json");

/// Run the shared command implementation using process arguments and standard I/O.
#[must_use]
pub fn run() -> ExitCode {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let stdout_is_tty = io::stdout().is_terminal();
    let result = execute(&args, stdout_is_tty);
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    if !result.stdout.is_empty() {
        let _ = stdout.write_all(&result.stdout);
    }
    if !result.stderr.is_empty() {
        let _ = stderr.write_all(&result.stderr);
    }
    ExitCode::from(result.exit_code)
}

#[derive(Debug)]
struct CommandResult {
    exit_code: u8,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
    Junit,
}

#[derive(Debug)]
struct TestOptions {
    engine: String,
    corpus: Option<PathBuf>,
    families: Vec<String>,
    tzdb_requirement: TzdbRequirement,
    format: Option<OutputFormat>,
    _no_color: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TzdbRequirement {
    Any,
    Exact(String),
    Bounded,
    Unknown,
    Known,
    Source(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusLock {
    corpus_repository: String,
    expected_source_revision: String,
    corpus_version: String,
    canonical_digest: String,
    release_digest: String,
    vector_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct CliReport {
    schema_version: String,
    tooling_version: String,
    /// The behavioural specification the verdicts were scored against. It
    /// versions independently of the tool, so a result that omitted it could not
    /// be reproduced.
    specification_version: String,
    runner_protocol_version: String,
    corpus: CorpusIdentity,
    engine: EngineIdentity,
    summary: Summary,
    results: Vec<CaseResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CorpusIdentity {
    version: String,
    canonical_digest: String,
    selected_vectors: usize,
    families: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct EngineIdentity {
    build_id: String,
    runner: ComponentIdentity,
    engine: ComponentIdentity,
    runtime: RuntimeIdentity,
    dialect_ids: Vec<DialectId>,
    semantic_profile_claims: BTreeMap<String, SemanticValue>,
    tzdb_provenance: TzdbProvenance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
struct Summary {
    selected_vectors: usize,
    conformant: usize,
    conformant_admissible: usize,
    non_conformant: usize,
    recorded_unscored: usize,
    unsupported: usize,
    engine_errors: usize,
    timeouts: usize,
    runner_failures: usize,
    warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct CaseResult {
    vector_id: String,
    family: String,
    classification: Classification,
    execution_status: ExecutionStatus,
    engine_outcome: Option<EngineOutcome>,
    verdict: ConformanceVerdict,
    warnings: Vec<Diagnostic>,
}

fn execute(args: &[String], stdout_is_tty: bool) -> CommandResult {
    match dispatch(args, stdout_is_tty) {
        Ok(result) => result,
        Err(error) => CommandResult {
            exit_code: error.exit_code,
            stdout: Vec::new(),
            stderr: format!("occurframe: {}\n", error.message).into_bytes(),
        },
    }
}

fn dispatch(args: &[String], stdout_is_tty: bool) -> Result<CommandResult, CliError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::usage(format!(
            "a command is required\n{}",
            top_level_help()
        )));
    };
    match command {
        "--help" | "-h" => Ok(success_text(top_level_help())),
        "--version" | "-V" => Ok(success_text(version_text())),
        "test" => {
            if args
                .get(1)
                .is_some_and(|argument| argument == "--help" || argument == "-h")
            {
                return Ok(success_text(test_help()));
            }
            let options = parse_test(&args[1..])?;
            run_test(&options, stdout_is_tty)
        }
        // `explain`, `classify` and `occurrences` are deliberately absent from
        // the command tree. Under ERRATA-001 they are deferred behind the engine
        // gate, so they are not part of the v1 contract and are not recognized
        // specially: an ordinary usage error is the honest answer, and a
        // dedicated "reserved" reply would advertise a surface that does not
        // exist.
        unknown => Err(CliError::usage(format!(
            "unknown command '{unknown}'; Occurframe {TOOL_VERSION} implements one command, 'test'"
        ))),
    }
}

fn success_text(text: String) -> CommandResult {
    CommandResult {
        exit_code: EXIT_SUCCESS,
        stdout: text.into_bytes(),
        stderr: Vec::new(),
    }
}

/// The embedded corpus identity, when the shipped lock parses.
fn embedded_corpus_lock() -> Option<CorpusLock> {
    serde_json::from_str(CORPUS_LOCK_JSON).ok()
}

/// `--version` reports every identity a result depends on, not just the binary's.
///
/// A conformance result is meaningless without the specification it was scored
/// against, the corpus it came from and the protocol it was gathered over, and
/// those version independently of the tool.
fn version_text() -> String {
    let mut output = format!(
        "occurframe {TOOL_VERSION}\nspecification {SPECIFICATION_VERSION}\nrunner-protocol {RUNNER_PROTOCOL_VERSION}\n"
    );
    if let Some(lock) = embedded_corpus_lock() {
        let _ = writeln!(output, "corpus {}", lock.corpus_version);
    }
    output
}

/// Default help advertises exactly what v1 ships.
///
/// The deferred commands are absent by design. Listing them here — even as
/// "reserved" — would present a four-command surface that Occurframe cannot
/// implement without the recurrence engine the ORACLE ONLY verdict does not
/// authorise. They are documented in the engine-gated section of the docs
/// instead, where the reader can see why they are not here.
fn top_level_help() -> String {
    format!(
        "Occurframe {TOOL_VERSION} — executable conformance oracle\n\nUSAGE:\n    occurframe test --engine <adapter> [OPTIONS]\n    oframe test --engine <adapter> [OPTIONS]\n\nCOMMANDS:\n    test    Execute corpus vectors against an external engine and score the result\n\nOccurframe observes recurrence implementations. It computes no occurrence, and\nit is not a scheduling engine.\n\nSpecification {SPECIFICATION_VERSION} · runner protocol {RUNNER_PROTOCOL_VERSION}\n"
    )
}

fn test_help() -> String {
    "USAGE:\n    occurframe test --engine <adapter> [OPTIONS]\n\nOPTIONS:\n    --engine <adapter>  Stable configured protocol-v3 runner build ID\n    --corpus <path>     Authored corpus checkout or packed distribution\n    --family <family>   Select a corpus family; may be repeated\n    --tzdb <requirement>  any, exact:<release>, bounded, unknown, known, or source:<name>\n    --format <format>   text, json, or junit\n    --no-color         Disable text color (also honored through NO_COLOR)\n    -h, --help         Print help\n"
        .into()
}

fn parse_test(args: &[String]) -> Result<TestOptions, CliError> {
    let mut engine = None;
    let mut corpus = None;
    let mut families = Vec::new();
    let mut tzdb_requirement = TzdbRequirement::Any;
    let mut format = None;
    let mut no_color = env::var_os("NO_COLOR").is_some();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        match argument.as_str() {
            "--engine" => engine = Some(next_value(args, &mut index, "--engine")?.to_owned()),
            "--corpus" => {
                corpus = Some(PathBuf::from(next_value(args, &mut index, "--corpus")?));
            }
            "--family" => families.push(next_value(args, &mut index, "--family")?.to_owned()),
            "--tzdb" => {
                tzdb_requirement = parse_tzdb(next_value(args, &mut index, "--tzdb")?)?;
            }
            "--format" => {
                let value = next_value(args, &mut index, "--format")?;
                format = Some(match value {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    "junit" => OutputFormat::Junit,
                    _ => {
                        return Err(CliError::usage(format!(
                            "invalid --format '{value}'; expected text, json, or junit"
                        )));
                    }
                });
            }
            "--no-color" => no_color = true,
            "--help" | "-h" => {
                return Err(CliError::usage("--help must immediately follow 'test'"));
            }
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!("unknown option '{value}'")));
            }
            value => return Err(CliError::usage(format!("unexpected argument '{value}'"))),
        }
        index += 1;
    }
    let engine = engine.ok_or_else(|| CliError::usage("'test' requires --engine <adapter>"))?;
    Ok(TestOptions {
        engine,
        corpus,
        families,
        tzdb_requirement,
        format,
        _no_color: no_color,
    })
}

fn next_value<'a>(
    args: &'a [String],
    index: &mut usize,
    option: &str,
) -> Result<&'a str, CliError> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| CliError::usage(format!("{option} requires a value")))
}

fn parse_tzdb(value: &str) -> Result<TzdbRequirement, CliError> {
    match value {
        "any" => Ok(TzdbRequirement::Any),
        "bounded" => Ok(TzdbRequirement::Bounded),
        "unknown" => Ok(TzdbRequirement::Unknown),
        "known" => Ok(TzdbRequirement::Known),
        _ if value.starts_with("exact:") && value.len() > 6 => {
            Ok(TzdbRequirement::Exact(value[6..].to_owned()))
        }
        _ if value.starts_with("source:") && value.len() > 7 => {
            Ok(TzdbRequirement::Source(value[7..].to_owned()))
        }
        _ => Err(CliError::usage(format!(
            "invalid --tzdb requirement '{value}'"
        ))),
    }
}

fn run_test(options: &TestOptions, stdout_is_tty: bool) -> Result<CommandResult, CliError> {
    let corpus_lock: CorpusLock = serde_json::from_str(CORPUS_LOCK_JSON)
        .map_err(|error| CliError::environment(format!("invalid embedded corpus lock: {error}")))?;
    let corpus_path = discover_corpus(options.corpus.as_deref())?;
    let corpus = load_compatible_corpus(&corpus_path)
        .map_err(|error| CliError::environment(format!("corpus validation failed: {error}")))?;
    validate_corpus_lock(&corpus_lock, &corpus)?;

    let all_families: BTreeSet<_> = corpus
        .vectors
        .iter()
        .map(|vector| vector.family.clone())
        .collect();
    for family in &options.families {
        if !all_families.contains(family) {
            return Err(CliError::usage(format!(
                "unknown corpus family '{family}'; available families: {}",
                all_families.iter().cloned().collect::<Vec<_>>().join(", ")
            )));
        }
    }
    let selected_family_set: BTreeSet<_> = options.families.iter().cloned().collect();
    let vectors = corpus
        .vectors
        .iter()
        .filter(|vector| {
            selected_family_set.is_empty() || selected_family_set.contains(&vector.family)
        })
        .cloned()
        .collect::<Vec<_>>();

    let registry_path = discover_runner_registry()?;
    let registry = RunnerRegistry::load(&registry_path)
        .map_err(|error| CliError::environment(format!("runner registry is invalid: {error}")))?;
    let build = resolve_build(&registry, &options.engine)?;
    if build.reproducibility.status != ReproducibilityStatus::Reproducible {
        return Err(CliError::environment(format!(
            "engine '{}' is configured but provenance-blocked: {}",
            build.build_id,
            build
                .reproducibility
                .reason
                .as_deref()
                .unwrap_or("historical dependencies are not reproducible")
        )));
    }
    let repository_root = discover_runner_root(&registry_path)?;
    let schema_path = discover_protocol_schema(&corpus_path, &repository_root)?;
    let schema = ProtocolSchema::load(&schema_path)
        .map_err(|error| CliError::environment(format!("protocol schema is invalid: {error}")))?;
    let records = run_batch(
        std::slice::from_ref(build),
        &vectors,
        &repository_root,
        &schema,
        DEFAULT_INFRASTRUCTURE_WATCHDOG,
    );
    if records.len() != vectors.len() {
        return Err(CliError::environment(format!(
            "runner produced {} records for {} selected vectors",
            records.len(),
            vectors.len()
        )));
    }
    let report = build_report(&corpus, &vectors, build, &records)?;
    enforce_tzdb(&options.tzdb_requirement, &report.engine.tzdb_provenance)?;

    let format = options.format.unwrap_or(if stdout_is_tty {
        OutputFormat::Text
    } else {
        OutputFormat::Json
    });
    let stdout = match format {
        OutputFormat::Text => render_text(&report).into_bytes(),
        OutputFormat::Json => canonical_pretty_json(&report)
            .map_err(|error| CliError::environment(format!("JSON rendering failed: {error}")))?,
        OutputFormat::Junit => render_junit(&report).into_bytes(),
    };
    Ok(CommandResult {
        exit_code: exit_code_for(&report),
        stdout,
        stderr: Vec::new(),
    })
}

fn validate_corpus_lock(
    lock: &CorpusLock,
    corpus: &occurframe_conformance::CompatibleCorpus,
) -> Result<(), CliError> {
    if corpus.corpus_version != lock.corpus_version
        || corpus.canonical_corpus_digest != lock.canonical_digest
        || corpus.vectors.len() != lock.vector_count
        || lock.corpus_repository != "https://github.com/occurframe/corpus"
        || lock.expected_source_revision.len() != 40
        || lock.release_digest.len() != 64
    {
        return Err(CliError::environment(format!(
            "corpus identity does not match the shipped authority lock (expected {} / {} / {} vectors; observed {} / {} / {})",
            lock.corpus_version,
            lock.canonical_digest,
            lock.vector_count,
            corpus.corpus_version,
            corpus.canonical_corpus_digest,
            corpus.vectors.len()
        )));
    }
    Ok(())
}

fn resolve_build<'a>(registry: &'a RunnerRegistry, id: &str) -> Result<&'a RunnerBuild, CliError> {
    registry.build(id).ok_or_else(|| {
        let ids = registry
            .builds
            .iter()
            .map(|build| build.build_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        CliError::usage(format!(
            "unknown engine ID '{id}'; available configured IDs: {ids}"
        ))
    })
}

/// The root of an installed/extracted release, derived only from the running
/// executable.
///
/// Discovery must never depend on the current working directory, a Cargo
/// workspace layout, or a source checkout: an extracted release has to work from
/// an arbitrary directory. Symlinked installs are resolved so that
/// `/usr/local/bin/occurframe -> /opt/occurframe-0.1.0-rc1/bin/occurframe`
/// still finds `/opt/occurframe-0.1.0-rc1`.
fn bundle_root() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let executable = fs::canonicalize(&executable).unwrap_or(executable);
    executable.parent()?.parent().map(Path::to_path_buf)
}

/// The bundled corpus layout produced by `xtask release-package`.
const BUNDLED_CORPUS: &str = "corpus";
/// The bundled adapter-identity registry produced by `xtask release-package`.
const BUNDLED_REGISTRY: &str = "adapters/runner-builds.json";
const PROTOCOL_SCHEMA: &str = "schemas/runner-protocol-v3.schema.json";

fn is_corpus_directory(path: &Path) -> bool {
    path.join("manifest.json").is_file() || path.join("vectors").is_dir()
}

fn discover_corpus(explicit: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = env::var_os("OCCURFRAME_CORPUS") {
        return Ok(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    // Release layout first: an extracted bundle always answers for itself.
    if let Some(bundle) = bundle_root() {
        candidates.push(bundle.join(BUNDLED_CORPUS));
    }
    // Source-checkout convenience only, and only after the release layout.
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join(BUNDLED_CORPUS));
        candidates.push(cwd.join("../corpus"));
    }
    candidates
        .into_iter()
        .find(|path| is_corpus_directory(path))
        .ok_or_else(|| {
            CliError::environment(
                "compatible corpus not found; pass --corpus or set OCCURFRAME_CORPUS",
            )
        })
}

fn discover_runner_registry() -> Result<PathBuf, CliError> {
    if let Some(path) = env::var_os("OCCURFRAME_RUNNER_REGISTRY") {
        return Ok(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    // Release layout first, for the same reason as the corpus.
    if let Some(bundle) = bundle_root() {
        candidates.push(bundle.join(BUNDLED_REGISTRY));
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("runners/registry/runner-builds.json"));
    }
    candidates.into_iter().find(|path| path.is_file()).ok_or_else(|| {
        CliError::environment(
            "runner registry not found; set OCCURFRAME_RUNNER_REGISTRY, or run from a tooling checkout",
        )
    })
}

/// Resolve the base directory that relative `launch.program`, `launch.arguments`
/// and `launch.working_directory` entries in a registry are interpreted against.
///
/// The rule is deterministic and never falls back to the process working
/// directory: a registry supplied through `OCCURFRAME_RUNNER_REGISTRY` may live
/// anywhere on the filesystem, inside or outside the release, and the paths it
/// declares are resolved relative to the registry itself.
fn discover_runner_root(registry_path: &Path) -> Result<PathBuf, CliError> {
    if let Some(path) = env::var_os("OCCURFRAME_RUNNER_ROOT") {
        return Ok(PathBuf::from(path));
    }
    // A bare `runner-builds.json` names the current directory explicitly rather
    // than silently inheriting an arbitrary absolute working directory.
    let Some(parent) = registry_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(PathBuf::from("."));
    };
    let parent_name = parent.file_name().and_then(|name| name.to_str());
    let grandparent_name = parent
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    // Source checkout: <root>/runners/registry/runner-builds.json -> <root>
    if parent_name == Some("registry") && grandparent_name == Some("runners") {
        return parent
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| CliError::environment("cannot resolve tooling repository root"));
    }
    // Release bundle: <bundle>/adapters/runner-builds.json -> <bundle>
    if parent_name == Some("adapters") {
        if let Some(bundle) = parent.parent() {
            return Ok(bundle.to_path_buf());
        }
    }
    // Any other registry: resolve relative launch paths beside the registry.
    Ok(parent.to_path_buf())
}

fn discover_protocol_schema(corpus: &Path, root: &Path) -> Result<PathBuf, CliError> {
    if let Some(path) = env::var_os("OCCURFRAME_RUNNER_PROTOCOL_SCHEMA") {
        return Ok(PathBuf::from(path));
    }
    let mut candidates = vec![corpus.join(PROTOCOL_SCHEMA)];
    if let Some(bundle) = bundle_root() {
        candidates.push(bundle.join(BUNDLED_CORPUS).join(PROTOCOL_SCHEMA));
    }
    candidates.push(root.join("../corpus").join(PROTOCOL_SCHEMA));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            CliError::environment(
                "runner protocol schema not found; set OCCURFRAME_RUNNER_PROTOCOL_SCHEMA",
            )
        })
}

fn build_report(
    corpus: &occurframe_conformance::CompatibleCorpus,
    vectors: &[occurframe_wire::Vector],
    build: &RunnerBuild,
    records: &[CaseExecution],
) -> Result<CliReport, CliError> {
    let first = records.first().ok_or_else(|| {
        CliError::environment("no observations were produced for the selected corpus")
    })?;
    let vector_by_id: BTreeMap<_, _> = vectors
        .iter()
        .map(|vector| (vector.id.as_str(), vector))
        .collect();
    let mut results = Vec::new();
    let mut summary = Summary {
        selected_vectors: vectors.len(),
        ..Summary::default()
    };
    for record in records {
        let vector = vector_by_id
            .get(record.observation.vector_id.as_str())
            .ok_or_else(|| {
                CliError::environment(format!(
                    "observation references unknown selected vector {}",
                    record.observation.vector_id
                ))
            })?;
        update_summary(&mut summary, record);
        results.push(CaseResult {
            vector_id: vector.id.clone(),
            family: vector.family.clone(),
            classification: vector.classification,
            execution_status: record.observation.execution_status,
            engine_outcome: record.observation.engine_outcome.clone(),
            verdict: record.verdict.clone(),
            warnings: record.observation.warnings.clone(),
        });
    }
    results.sort_by(|left, right| left.vector_id.cmp(&right.vector_id));
    let mut families = vectors
        .iter()
        .map(|vector| vector.family.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    families.sort();
    Ok(CliReport {
        schema_version: "1.0.0".into(),
        tooling_version: TOOL_VERSION.into(),
        specification_version: SPECIFICATION_VERSION.into(),
        runner_protocol_version: RUNNER_PROTOCOL_VERSION.into(),
        corpus: CorpusIdentity {
            version: corpus.corpus_version.clone(),
            canonical_digest: corpus.canonical_corpus_digest.clone(),
            selected_vectors: vectors.len(),
            families,
        },
        engine: EngineIdentity {
            build_id: build.build_id.clone(),
            runner: first.observation.runner.clone(),
            engine: first.observation.engine.clone(),
            runtime: first.observation.runtime.clone(),
            dialect_ids: first.observation.dialect_ids.clone(),
            semantic_profile_claims: first.observation.semantic_profile_claims.clone(),
            tzdb_provenance: first.observation.tzdb_provenance.clone(),
        },
        summary,
        results,
    })
}

fn update_summary(summary: &mut Summary, record: &CaseExecution) {
    summary.warnings += record.observation.warnings.len();
    match record.verdict.status {
        VerdictStatus::Conformant => summary.conformant += 1,
        VerdictStatus::ConformantAdmissible => summary.conformant_admissible += 1,
        VerdictStatus::NonConformant => summary.non_conformant += 1,
        VerdictStatus::RecordedUnscored => summary.recorded_unscored += 1,
        VerdictStatus::Unsupported => summary.unsupported += 1,
        VerdictStatus::Timeout => summary.timeouts += 1,
        VerdictStatus::InfrastructureFailure => summary.runner_failures += 1,
    }
    if matches!(
        record.observation.engine_outcome,
        Some(EngineOutcome::EngineError { .. })
    ) {
        summary.engine_errors += 1;
    }
}

fn enforce_tzdb(
    requirement: &TzdbRequirement,
    provenance: &TzdbProvenance,
) -> Result<(), CliError> {
    let matches = match requirement {
        TzdbRequirement::Any => true,
        TzdbRequirement::Exact(required) => {
            matches!(&provenance.release, TzdbRelease::Exact { release } if release == required)
        }
        TzdbRequirement::Bounded => matches!(provenance.release, TzdbRelease::Bounded { .. }),
        TzdbRequirement::Unknown => matches!(provenance.release, TzdbRelease::Unknown),
        TzdbRequirement::Known => !matches!(provenance.release, TzdbRelease::Unknown),
        TzdbRequirement::Source(required) => provenance.source == *required,
    };
    if matches {
        Ok(())
    } else {
        Err(CliError::environment(format!(
            "observed tzdb provenance does not satisfy --tzdb requirement: {}",
            compact_json(provenance)
        )))
    }
}

fn exit_code_for(report: &CliReport) -> u8 {
    if report.summary.runner_failures > 0 {
        EXIT_ENVIRONMENT
    } else if report.summary.non_conformant > 0
        || report.summary.engine_errors > 0
        || report.summary.timeouts > 0
    {
        EXIT_CONFORMANCE
    } else {
        EXIT_SUCCESS
    }
}

fn render_text(report: &CliReport) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "Occurframe conformance result");
    let _ = writeln!(output, "specification: {}", report.specification_version);
    let _ = writeln!(output, "engine: {}", report.engine.engine.name);
    let _ = writeln!(output, "engine version: {}", report.engine.engine.version);
    let _ = writeln!(output, "corpus version: {}", report.corpus.version);
    let _ = writeln!(output, "corpus digest: {}", report.corpus.canonical_digest);
    let _ = writeln!(
        output,
        "runtime: {} {} {}",
        report.engine.runtime.language,
        report.engine.runtime.runtime,
        report.engine.runtime.version
    );
    let _ = writeln!(
        output,
        "dialect/profile: {} / {}",
        report
            .engine
            .dialect_ids
            .iter()
            .map(|dialect| dialect.0.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        compact_json(&report.engine.semantic_profile_claims)
    );
    let _ = writeln!(
        output,
        "tzdb provenance: {}",
        compact_json(&report.engine.tzdb_provenance)
    );
    let _ = writeln!(
        output,
        "selected vectors: {}",
        report.summary.selected_vectors
    );
    let _ = writeln!(output, "conformant: {}", report.summary.conformant);
    let _ = writeln!(
        output,
        "admissible: {}",
        report.summary.conformant_admissible
    );
    let _ = writeln!(output, "non-conformant: {}", report.summary.non_conformant);
    let _ = writeln!(output, "unsupported: {}", report.summary.unsupported);
    let _ = writeln!(output, "errors: {}", report.summary.engine_errors);
    let _ = writeln!(output, "timeouts: {}", report.summary.timeouts);
    let _ = writeln!(
        output,
        "runner failures: {}",
        report.summary.runner_failures
    );
    let actionable = report
        .results
        .iter()
        .filter(|result| {
            !matches!(
                result.verdict.status,
                VerdictStatus::Conformant
                    | VerdictStatus::ConformantAdmissible
                    | VerdictStatus::RecordedUnscored
            )
        })
        .collect::<Vec<_>>();
    if !actionable.is_empty() {
        let _ = writeln!(output, "\nActionable results:");
        for result in actionable {
            let _ = writeln!(
                output,
                "- {} [{:?}] {}",
                result.vector_id,
                result.classification,
                verdict_name(result.verdict.status)
            );
        }
    }
    output
}

fn render_junit(report: &CliReport) -> String {
    let failures = report
        .results
        .iter()
        .filter(|result| junit_kind(result) == JunitKind::Failure)
        .count();
    let errors = report
        .results
        .iter()
        .filter(|result| junit_kind(result) == JunitKind::Error)
        .count();
    let skipped = report
        .results
        .iter()
        .filter(|result| junit_kind(result) == JunitKind::Skipped)
        .count();
    let mut output = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"occurframe\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\">\n",
        report.results.len()
    );
    output.push_str("  <properties>\n");
    for (name, value) in [
        ("occurframe.version", report.tooling_version.as_str()),
        (
            "occurframe.specification",
            report.specification_version.as_str(),
        ),
        (
            "occurframe.protocol",
            report.runner_protocol_version.as_str(),
        ),
        ("occurframe.corpus.version", report.corpus.version.as_str()),
        (
            "occurframe.corpus.digest",
            report.corpus.canonical_digest.as_str(),
        ),
        ("occurframe.engine", report.engine.engine.name.as_str()),
        (
            "occurframe.engine.version",
            report.engine.engine.version.as_str(),
        ),
    ] {
        let _ = writeln!(
            output,
            "    <property name=\"{}\" value=\"{}\"/>",
            xml_escape(name),
            xml_escape(value)
        );
    }
    output.push_str("  </properties>\n");
    for result in &report.results {
        let _ = writeln!(
            output,
            "  <testcase classname=\"{}\" name=\"{}\">",
            xml_escape(&result.family),
            xml_escape(&result.vector_id)
        );
        let message = format!(
            "{} ({:?})",
            verdict_name(result.verdict.status),
            result.classification
        );
        match junit_kind(result) {
            JunitKind::Success => {}
            JunitKind::Failure => {
                let _ = writeln!(
                    output,
                    "    <failure message=\"{}\"/>",
                    xml_escape(&message)
                );
            }
            JunitKind::Error => {
                let _ = writeln!(output, "    <error message=\"{}\"/>", xml_escape(&message));
            }
            JunitKind::Skipped => {
                let _ = writeln!(
                    output,
                    "    <skipped message=\"{}\"/>",
                    xml_escape(&message)
                );
            }
        }
        if !result.warnings.is_empty() {
            let _ = writeln!(
                output,
                "    <system-out>{}</system-out>",
                xml_escape(&compact_json(&result.warnings))
            );
        }
        output.push_str("  </testcase>\n");
    }
    output.push_str("</testsuite>\n");
    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JunitKind {
    Success,
    Failure,
    Error,
    Skipped,
}

fn junit_kind(result: &CaseResult) -> JunitKind {
    if result.execution_status == ExecutionStatus::RunnerFailure
        || result.execution_status == ExecutionStatus::Timeout
        || matches!(
            result.engine_outcome,
            Some(EngineOutcome::EngineError { .. })
        )
    {
        JunitKind::Error
    } else {
        match result.verdict.status {
            VerdictStatus::Conformant | VerdictStatus::ConformantAdmissible => JunitKind::Success,
            VerdictStatus::NonConformant => JunitKind::Failure,
            VerdictStatus::Unsupported | VerdictStatus::RecordedUnscored => JunitKind::Skipped,
            VerdictStatus::Timeout | VerdictStatus::InfrastructureFailure => JunitKind::Error,
        }
    }
}

fn verdict_name(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Conformant => "conformant",
        VerdictStatus::ConformantAdmissible => "conformant_admissible",
        VerdictStatus::NonConformant => "non_conformant",
        VerdictStatus::Unsupported => "unsupported",
        VerdictStatus::Timeout => "timeout",
        VerdictStatus::InfrastructureFailure => "runner_failure",
        VerdictStatus::RecordedUnscored => "recorded_unscored",
    }
}

fn compact_json(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unrenderable>".into())
}

fn xml_escape(value: &str) -> String {
    value
        .chars()
        .filter(|character| matches!(*character, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}'))
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '\"' => "&quot;".chars().collect(),
            '\'' => "&apos;".chars().collect(),
            other => vec![other],
        })
        .collect()
}

#[derive(Debug)]
struct CliError {
    exit_code: u8,
    message: String,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_USAGE,
            message: message.into(),
        }
    }

    fn environment(message: impl Into<String>) -> Self {
        Self {
            exit_code: EXIT_ENVIRONMENT,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use occurframe_conformance::sha256_hex;

    #[test]
    fn parser_requires_engine_and_accepts_families() {
        assert_eq!(execute(&["test".into()], false).exit_code, EXIT_USAGE);
        let parsed = parse_test(&[
            "--engine".into(),
            "python.dateutil.system".into(),
            "--family".into(),
            "rrule.core".into(),
            "--family".into(),
            "cron.invalid".into(),
            "--format".into(),
            "junit".into(),
        ])
        .expect("parse");
        assert_eq!(parsed.families, ["rrule.core", "cron.invalid"]);
        assert_eq!(parsed.format, Some(OutputFormat::Junit));
    }

    #[test]
    fn parser_accepts_corpus_and_tzdb_override() {
        let parsed = parse_test(&[
            "--engine".into(),
            "fixture".into(),
            "--corpus".into(),
            "packed".into(),
            "--tzdb".into(),
            "exact:2026a".into(),
        ])
        .expect("parse");
        assert_eq!(parsed.corpus, Some(PathBuf::from("packed")));
        assert_eq!(
            parsed.tzdb_requirement,
            TzdbRequirement::Exact("2026a".into())
        );
    }

    #[test]
    fn parser_rejects_unknown_flags_and_formats() {
        for args in [
            vec!["--engine".into(), "x".into(), "--wat".into()],
            vec![
                "--engine".into(),
                "x".into(),
                "--format".into(),
                "yaml".into(),
            ],
        ] {
            assert_eq!(
                parse_test(&args).expect_err("invalid").exit_code,
                EXIT_USAGE
            );
        }
    }

    #[test]
    fn the_v1_command_surface_is_exactly_one_semantic_command() {
        // ERRATA-001: the deferred evaluator commands are not part of the v1
        // contract, so they behave as any other unknown word — not as a
        // recognized-but-unavailable command, which would advertise a surface
        // Occurframe cannot implement without a recurrence engine.
        let unknown = execute(&["not-a-command".into()], false);
        for command in ["explain", "classify", "occurrences"] {
            let result = execute(&[command.into()], false);
            assert_eq!(result.exit_code, EXIT_USAGE);
            let stderr = String::from_utf8(result.stderr).expect("UTF-8");
            assert!(
                stderr.contains(&format!("unknown command '{command}'")),
                "{command} must be an ordinary usage error, found: {stderr}"
            );
            assert!(
                !stderr.contains("reserved"),
                "{command} must not be advertised as reserved"
            );
            assert_eq!(
                result.exit_code, unknown.exit_code,
                "{command} must not be distinguishable from any other unknown command"
            );
        }
    }

    #[test]
    fn help_and_version_advertise_one_command_and_claim_no_engine() {
        let help = String::from_utf8(execute(&["--help".into()], false).stdout).expect("UTF-8");
        assert!(help.contains("occurframe test --engine"));
        assert!(help.contains("oframe test --engine"));
        for deferred in ["explain", "classify", "occurrences"] {
            assert!(
                !help.contains(deferred),
                "default help must not advertise the engine-gated command {deferred}"
            );
        }
        assert!(help.contains("computes no occurrence"));
        assert!(help.contains("not a scheduling engine"));
        assert!(help.contains(SPECIFICATION_VERSION));

        let version =
            String::from_utf8(execute(&["--version".into()], false).stdout).expect("UTF-8");
        assert!(version.starts_with(&format!("occurframe {TOOL_VERSION}\n")));
        assert!(version.contains(&format!("specification {SPECIFICATION_VERSION}")));
        assert!(version.contains(&format!("runner-protocol {RUNNER_PROTOCOL_VERSION}")));
        assert!(version.contains("corpus 1.0.0-rc3"));
    }

    #[test]
    fn the_embedded_corpus_lock_pins_the_corrected_rc3_identity() {
        let lock = embedded_corpus_lock().expect("embedded corpus lock parses");
        assert_eq!(lock.corpus_version, "1.0.0-rc3");
        assert_eq!(
            lock.canonical_digest,
            "c0a9cf0587c02ce5022cbb94d060e14d5b9d6f99c3210e512965f35062c4dfe0"
        );
        assert_eq!(lock.vector_count, 184);
        assert_eq!(
            lock.corpus_repository,
            "https://github.com/occurframe/corpus"
        );
    }

    #[test]
    fn executable_independent_help_and_version() {
        assert_eq!(
            execute(&["--help".into()], true).stdout,
            execute(&["--help".into()], false).stdout
        );
        assert_eq!(execute(&["--version".into()], false).exit_code, 0);
    }

    #[test]
    fn output_contract_goldens_cover_success_failure_and_pathologies() {
        let conformant = synthetic_case(
            "GOLDEN-001",
            ExecutionStatus::Completed,
            Some(EngineOutcome::Occurrences {
                occurrences: vec!["later".into(), "earlier".into(), "earlier".into()],
            }),
            VerdictStatus::Conformant,
        );
        let admissible = CaseResult {
            verdict: ConformanceVerdict {
                status: VerdictStatus::ConformantAdmissible,
                matched_case: Some("named-policy".into()),
                diagnostic: None,
            },
            ..conformant.clone()
        };
        let non_conformant = synthetic_case(
            "GOLDEN-002",
            ExecutionStatus::Completed,
            Some(EngineOutcome::Rejection {
                diagnostic: diagnostic("native_rejection"),
            }),
            VerdictStatus::NonConformant,
        );
        let unscored = synthetic_case(
            "GOLDEN-003",
            ExecutionStatus::Completed,
            Some(EngineOutcome::Accepted),
            VerdictStatus::RecordedUnscored,
        );
        let unsupported = synthetic_case(
            "GOLDEN-004",
            ExecutionStatus::Completed,
            Some(EngineOutcome::Unsupported {
                diagnostic: diagnostic("unsupported_capability"),
            }),
            VerdictStatus::Unsupported,
        );
        let engine_error = synthetic_case(
            "GOLDEN-005",
            ExecutionStatus::Completed,
            Some(EngineOutcome::EngineError {
                diagnostic: diagnostic("unexpected_exception"),
            }),
            VerdictStatus::NonConformant,
        );
        let timeout = synthetic_case(
            "GOLDEN-006",
            ExecutionStatus::Timeout,
            None,
            VerdictStatus::Timeout,
        );
        let runner_failure = synthetic_case(
            "GOLDEN-007",
            ExecutionStatus::RunnerFailure,
            None,
            VerdictStatus::InfrastructureFailure,
        );

        let outputs = BTreeMap::from([
            (
                "text_success",
                render_text(&synthetic_report(vec![conformant.clone()])).into_bytes(),
            ),
            (
                "text_failure",
                render_text(&synthetic_report(vec![non_conformant.clone()])).into_bytes(),
            ),
            (
                "json_success",
                canonical_pretty_json(&synthetic_report(vec![conformant.clone()])).expect("JSON"),
            ),
            (
                "json_mixed",
                canonical_pretty_json(&synthetic_report(vec![
                    conformant.clone(),
                    non_conformant.clone(),
                ]))
                .expect("JSON"),
            ),
            (
                "junit",
                render_junit(&synthetic_report(vec![
                    conformant,
                    admissible,
                    non_conformant,
                    unscored,
                    unsupported.clone(),
                    engine_error.clone(),
                    timeout.clone(),
                    runner_failure.clone(),
                ]))
                .into_bytes(),
            ),
            (
                "json_unsupported",
                canonical_pretty_json(&synthetic_report(vec![unsupported])).expect("JSON"),
            ),
            (
                "json_engine_error",
                canonical_pretty_json(&synthetic_report(vec![engine_error])).expect("JSON"),
            ),
            (
                "json_timeout",
                canonical_pretty_json(&synthetic_report(vec![timeout])).expect("JSON"),
            ),
            (
                "json_runner_failure",
                canonical_pretty_json(&synthetic_report(vec![runner_failure])).expect("JSON"),
            ),
        ]);
        let expected = BTreeMap::from([
            (
                "text_success",
                "9e91079bdc8007f2029a23514c633337b43f68fd62a52a12183b907b02c648ea",
            ),
            (
                "text_failure",
                "2c6411c2c61e6f66673f864bad66a74f171a8bfd04d41fc82011e5697ca58d63",
            ),
            (
                "json_success",
                "3647d744dc8a9f42fadb3ffe629b94d7745a3011ba706c646256bf2cca22c1de",
            ),
            (
                "json_mixed",
                "737837760081fe2724e4e2f18be73809572d776f45996e2c998741b639c7a470",
            ),
            (
                "junit",
                "821a4f8e90da645b82623e3cf60c6bf8fbfc5eb5a037a98c287abcc625516387",
            ),
            (
                "json_unsupported",
                "ae1030c91ac55023fca800d1de835e80fd32822fc2312762bf5ffa5e99086dd3",
            ),
            (
                "json_engine_error",
                "1fff5669556dcd538f1299b396f46c548dd36818b3b485186446f9b60591ddcb",
            ),
            (
                "json_timeout",
                "5d19c0c7bf8e0b528378f8dbcbc7801de487e9af784db7df48ef320afccc0fe9",
            ),
            (
                "json_runner_failure",
                "6acb4b1edd3138fdb500236742f1d3d8b15ac963056568c0acae6d05ead2daac",
            ),
        ]);
        let actual = outputs
            .iter()
            .map(|(name, bytes)| (*name, sha256_hex(bytes)))
            .collect::<BTreeMap<_, _>>();
        let expected = expected
            .into_iter()
            .map(|(name, digest)| (name, digest.to_owned()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn runner_root_is_derived_from_the_registry_and_never_from_the_working_directory() {
        // Source checkout: <root>/runners/registry/runner-builds.json
        assert_eq!(
            discover_runner_root(Path::new(
                "/anywhere/tooling/runners/registry/runner-builds.json"
            ))
            .expect("checkout root"),
            PathBuf::from("/anywhere/tooling")
        );
        // Extracted release bundle: <bundle>/adapters/runner-builds.json
        assert_eq!(
            discover_runner_root(Path::new(
                "/opt/occurframe-0.1.0-rc1/adapters/runner-builds.json"
            ))
            .expect("bundle root"),
            PathBuf::from("/opt/occurframe-0.1.0-rc1")
        );
        // An external maintainer's registry, outside any Occurframe layout: the
        // base directory is the registry's own directory, not the process CWD.
        assert_eq!(
            discover_runner_root(Path::new("/srv/my-engine/occurframe-registry.json"))
                .expect("external root"),
            PathBuf::from("/srv/my-engine")
        );
        // A bare file name resolves explicitly, rather than silently inheriting
        // an arbitrary absolute working directory.
        assert_eq!(
            discover_runner_root(Path::new("runner-builds.json")).expect("bare registry"),
            PathBuf::from(".")
        );
    }

    #[test]
    fn exit_code_contract_has_documented_precedence() {
        assert_eq!(
            exit_code_for(&synthetic_report(vec![synthetic_case(
                "A",
                ExecutionStatus::Completed,
                None,
                VerdictStatus::Conformant,
            )])),
            0
        );
        assert_eq!(
            exit_code_for(&synthetic_report(vec![synthetic_case(
                "B",
                ExecutionStatus::Completed,
                None,
                VerdictStatus::NonConformant,
            )])),
            1
        );
        assert_eq!(
            exit_code_for(&synthetic_report(vec![synthetic_case(
                "C",
                ExecutionStatus::RunnerFailure,
                None,
                VerdictStatus::InfrastructureFailure,
            )])),
            4
        );
    }

    fn synthetic_report(results: Vec<CaseResult>) -> CliReport {
        let mut summary = Summary {
            selected_vectors: results.len(),
            ..Summary::default()
        };
        for result in &results {
            summary.warnings += result.warnings.len();
            match result.verdict.status {
                VerdictStatus::Conformant => summary.conformant += 1,
                VerdictStatus::ConformantAdmissible => summary.conformant_admissible += 1,
                VerdictStatus::NonConformant => summary.non_conformant += 1,
                VerdictStatus::RecordedUnscored => summary.recorded_unscored += 1,
                VerdictStatus::Unsupported => summary.unsupported += 1,
                VerdictStatus::Timeout => summary.timeouts += 1,
                VerdictStatus::InfrastructureFailure => summary.runner_failures += 1,
            }
            if matches!(
                result.engine_outcome,
                Some(EngineOutcome::EngineError { .. })
            ) {
                summary.engine_errors += 1;
            }
        }
        CliReport {
            schema_version: "1.0.0".into(),
            tooling_version: TOOL_VERSION.into(),
            specification_version: SPECIFICATION_VERSION.into(),
            runner_protocol_version: "3.0".into(),
            corpus: CorpusIdentity {
                version: "1.0.0-rc3".into(),
                canonical_digest:
                    "c0a9cf0587c02ce5022cbb94d060e14d5b9d6f99c3210e512965f35062c4dfe0".into(),
                selected_vectors: results.len(),
                families: vec!["fixture.family".into()],
            },
            engine: EngineIdentity {
                build_id: "fixture.engine".into(),
                runner: ComponentIdentity {
                    name: "fixture-runner".into(),
                    version: "3.0.0".into(),
                    provenance: Some("fixture".into()),
                },
                engine: ComponentIdentity {
                    name: "fixture-engine".into(),
                    version: "1.0.0".into(),
                    provenance: Some("fixture".into()),
                },
                runtime: RuntimeIdentity {
                    language: "Rust".into(),
                    runtime: "fixture".into(),
                    version: "1".into(),
                },
                dialect_ids: vec![DialectId("fixture@1".into())],
                semantic_profile_claims: BTreeMap::from([(
                    "fixture.policy".into(),
                    SemanticValue::Text("named".into()),
                )]),
                tzdb_provenance: TzdbProvenance {
                    source: "fixture".into(),
                    release: TzdbRelease::Exact {
                        release: "2026a".into(),
                    },
                    fingerprint: None,
                },
            },
            summary,
            results,
        }
    }

    fn synthetic_case(
        id: &str,
        execution_status: ExecutionStatus,
        engine_outcome: Option<EngineOutcome>,
        status: VerdictStatus,
    ) -> CaseResult {
        CaseResult {
            vector_id: id.into(),
            family: "fixture.family".into(),
            classification: Classification::Normative,
            execution_status,
            engine_outcome,
            verdict: ConformanceVerdict {
                status,
                matched_case: None,
                diagnostic: None,
            },
            warnings: Vec::new(),
        }
    }

    fn diagnostic(code: &str) -> Diagnostic {
        Diagnostic {
            code: code.into(),
            message: "deterministic fixture diagnostic".into(),
            details: None,
        }
    }
}
