use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde_json::json;

static FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static PACKED_CORPUS: OnceLock<PathBuf> = OnceLock::new();
static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn corpus_root() -> PathBuf {
    if let Some(path) = std::env::var_os("OCCURFRAME_TEST_CORPUS") {
        return PathBuf::from(path);
    }
    workspace_root()
        .parent()
        .expect("project root")
        .join("corpus")
}

fn fixture_binary() -> &'static Path {
    FIXTURE.get_or_init(|| {
        let root = workspace_root();
        let output = root.join("target").join(if cfg!(windows) {
            "occurframe-cli-protocol-fixture.exe"
        } else {
            "occurframe-cli-protocol-fixture"
        });
        let status = Command::new("rustc")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/protocol_runner.rs"))
            .args(["--edition", "2024", "-o"])
            .arg(&output)
            .status()
            .expect("launch rustc for protocol fixture");
        assert!(status.success(), "fixture runner must compile");
        output
    })
}

fn packed_corpus() -> &'static Path {
    PACKED_CORPUS.get_or_init(|| {
        let output = workspace_root()
            .join("target/cli-packed-corpus")
            .join(std::process::id().to_string());
        let report = occurframe_conformance::pack_release(&corpus_root(), &output)
            .expect("pack immutable RC2 corpus");
        assert_eq!(report.corpus_version, "1.0.0-rc2");
        assert_eq!(report.vector_count, 184);
        assert_eq!(
            report.canonical_corpus_digest,
            "4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470"
        );
        fs::create_dir_all(output.join("schemas")).expect("packed schema directory");
        fs::copy(
            corpus_root().join("schemas/runner-protocol-v2.schema.json"),
            output.join("schemas/runner-protocol-v2.schema.json"),
        )
        .expect("copy authority schema");
        output
    })
}

fn registry(mode: &str) -> PathBuf {
    let directory = workspace_root()
        .join("target/cli-test-registry")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&directory).expect("registry directory");
    let registry = json!({
        "schema_version": "1.0.0",
        "builds": [{
            "build_id": format!("fixture.{mode}"),
            "protocol_version": "2.0",
            "runner": {"name":"cli-fixture-runner","version":"2.0.0","provenance":"source:occurframe-cli/tests/fixtures/protocol_runner.rs"},
            "engine": {"name":"cli-fixture-engine","version":"1.0.0","provenance":"deterministic test fixture"},
            "language": "Rust",
            "runtime_name": "rustc-fixture",
            "runtime_requirement": "deterministic",
            "launch": {"program":fixture_binary(),"arguments":[mode],"working_directory":".","environment":{}},
            "supported_operations": ["cron.next"],
            "dialect_ids": ["cron.vixie@1"],
            "semantic_profile_claims": {"cron.start_inclusivity":"exclusive","cron.start_truncation":"exact"},
            "tzdb_provenance_acquisition":"deterministic fixture",
            "tzdb_source":"fixture tzdb",
            "allowed_tzdb_release_kinds":["exact"],
            "fallback_tzdb_provenance":{"source":"fixture tzdb","release_kind":"exact","release":"2026a"},
            "representative_vectors":["CRON-ANCH-001"],
            "reproducibility":{"status":"reproducible","setup":"compiled by CLI integration test"},
            "legacy_source":"not applicable: protocol-only CLI fixture"
        }]
    });
    let path = directory.join("runner-builds.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&registry).expect("registry JSON"),
    )
    .expect("write registry");
    path
}

fn invoke(binary: &str, mode: &str, format: &str) -> std::process::Output {
    invoke_with_corpus(binary, mode, format, &corpus_root())
}

fn invoke_with_corpus(
    binary: &str,
    mode: &str,
    format: &str,
    corpus: &Path,
) -> std::process::Output {
    let registry = registry(mode);
    Command::new(binary)
        .args([
            "test",
            "--engine",
            &format!("fixture.{mode}"),
            "--corpus",
            &corpus.to_string_lossy(),
            "--family",
            "cron.anchoring",
            "--format",
            format,
            "--tzdb",
            "exact:2026a",
        ])
        .env("OCCURFRAME_RUNNER_REGISTRY", registry)
        .env("OCCURFRAME_RUNNER_ROOT", workspace_root())
        .output()
        .expect("run CLI")
}

#[test]
fn packed_corpus_version_digest_count_and_cli_execution_are_verified() {
    let packed = occurframe_conformance::load_compatible_corpus(packed_corpus())
        .expect("load packed corpus");
    assert_eq!(packed.corpus_version, "1.0.0-rc2");
    assert_eq!(packed.vectors.len(), 184);
    assert_eq!(
        packed.canonical_corpus_digest,
        "4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470"
    );
    let output = invoke_with_corpus(
        env!("CARGO_BIN_EXE_occurframe"),
        "success",
        "json",
        packed_corpus(),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn aliases_have_identical_json_junit_and_exit_codes() {
    for format in ["json", "junit"] {
        let long = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", format);
        let short = invoke(env!("CARGO_BIN_EXE_oframe"), "success", format);
        assert_eq!(
            long.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&long.stderr)
        );
        assert_eq!(long.status.code(), short.status.code());
        assert_eq!(long.stdout, short.stdout);
        assert_eq!(long.stderr, short.stderr);
    }
}

#[test]
fn deterministic_json_and_junit_are_byte_identical() {
    for format in ["json", "junit"] {
        let first = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", format);
        let second = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", format);
        assert_eq!(first.stdout, second.stdout);
        assert_eq!(first.stderr, second.stderr);
        assert_eq!(first.status.code(), second.status.code());
    }
}

#[test]
fn semantic_and_infrastructure_exit_codes_are_distinct() {
    let semantic = invoke(env!("CARGO_BIN_EXE_occurframe"), "mixed", "json");
    assert_eq!(semantic.status.code(), Some(1));
    let infrastructure = invoke(env!("CARGO_BIN_EXE_occurframe"), "runner-failure", "json");
    assert_eq!(infrastructure.status.code(), Some(4));
    let usage = Command::new(env!("CARGO_BIN_EXE_occurframe"))
        .args(["test", "--engine", "not-configured", "--corpus"])
        .output()
        .expect("run usage fixture");
    assert_eq!(usage.status.code(), Some(3));
}

/// The shipped v1 command surface, exercised through the real executables.
///
/// ERRATA-001 defers `explain`, `classify` and `occurrences` behind the engine
/// gate. They must behave exactly as any other unknown word: no special
/// recognition, no "reserved" reply, and no appearance in default help — a
/// surface Occurframe cannot implement must not be advertised as if it could.
#[test]
fn both_aliases_ship_exactly_one_semantic_command() {
    for binary in [
        env!("CARGO_BIN_EXE_occurframe"),
        env!("CARGO_BIN_EXE_oframe"),
    ] {
        let help = Command::new(binary).arg("--help").output().expect("help");
        assert_eq!(help.status.code(), Some(0));
        let help_text = String::from_utf8(help.stdout).expect("UTF-8");
        assert!(help_text.contains("occurframe test --engine"));
        for deferred in ["explain", "classify", "occurrences"] {
            assert!(
                !help_text.contains(deferred),
                "{binary} advertises the engine-gated command {deferred}"
            );
        }

        let version = Command::new(binary)
            .arg("--version")
            .output()
            .expect("version");
        assert_eq!(version.status.code(), Some(0));
        let version_text = String::from_utf8(version.stdout).expect("UTF-8");
        assert!(
            version_text.starts_with("occurframe 0.1.0-rc2\n"),
            "{version_text}"
        );
        assert!(version_text.contains("specification 1.0.0-rc1"));
        assert!(version_text.contains("runner-protocol 2.0"));
        assert!(version_text.contains("corpus 1.0.0-rc2"));

        let baseline = Command::new(binary)
            .arg("definitely-not-a-command")
            .output()
            .expect("unknown");
        assert_eq!(baseline.status.code(), Some(3));
        for deferred in ["explain", "classify", "occurrences"] {
            let deferred_run = Command::new(binary)
                .arg(deferred)
                .output()
                .expect("deferred");
            assert_eq!(
                deferred_run.status.code(),
                baseline.status.code(),
                "{deferred} must be an ordinary usage error"
            );
            let stderr = String::from_utf8(deferred_run.stderr).expect("UTF-8");
            assert!(
                stderr.contains(&format!("unknown command '{deferred}'")),
                "{stderr}"
            );
            assert!(!stderr.contains("reserved"), "{stderr}");
        }
    }
}

#[test]
fn aliases_agree_on_help_version_and_the_deferred_command_surface() {
    for arguments in [
        vec!["--help"],
        vec!["--version"],
        vec!["explain"],
        vec!["classify"],
        vec!["occurrences"],
        vec!["definitely-not-a-command"],
    ] {
        let long = Command::new(env!("CARGO_BIN_EXE_occurframe"))
            .args(&arguments)
            .output()
            .expect("occurframe");
        let short = Command::new(env!("CARGO_BIN_EXE_oframe"))
            .args(&arguments)
            .output()
            .expect("oframe");
        assert_eq!(
            long.stdout, short.stdout,
            "stdout differs for {arguments:?}"
        );
        assert_eq!(
            long.stderr, short.stderr,
            "stderr differs for {arguments:?}"
        );
        assert_eq!(
            long.status.code(),
            short.status.code(),
            "exit code differs for {arguments:?}"
        );
    }
}

/// The specification version is part of every structured result, in all formats.
#[test]
fn every_output_format_carries_the_specification_version() {
    let json = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", "json");
    assert_eq!(json.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("deterministic JSON report");
    assert_eq!(report["specification_version"], "1.0.0-rc1");
    assert_eq!(report["tooling_version"], "0.1.0-rc2");
    assert_eq!(report["runner_protocol_version"], "2.0");
    assert_eq!(report["corpus"]["version"], "1.0.0-rc2");

    let junit = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", "junit");
    let junit_text = String::from_utf8(junit.stdout).expect("UTF-8");
    assert!(
        junit_text.contains(r#"<property name="occurframe.specification" value="1.0.0-rc1"/>"#)
    );

    let text = invoke(env!("CARGO_BIN_EXE_occurframe"), "success", "text");
    let rendered = String::from_utf8(text.stdout).expect("UTF-8");
    assert!(rendered.contains("specification: 1.0.0-rc1"), "{rendered}");
}

#[test]
fn unknown_engine_family_and_tzdb_mismatch_have_exact_domains() {
    let registry = registry("success");
    let common = |extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_occurframe"));
        command
            .args(["test", "--engine", "fixture.success", "--corpus"])
            .arg(corpus_root())
            .args(extra)
            .env("OCCURFRAME_RUNNER_REGISTRY", &registry)
            .env("OCCURFRAME_RUNNER_ROOT", workspace_root());
        command.output().expect("run CLI domain fixture")
    };
    let unknown_engine = {
        let mut command = Command::new(env!("CARGO_BIN_EXE_occurframe"));
        command
            .args(["test", "--engine", "unknown", "--corpus"])
            .arg(corpus_root())
            .env("OCCURFRAME_RUNNER_REGISTRY", &registry)
            .env("OCCURFRAME_RUNNER_ROOT", workspace_root());
        command.output().expect("run unknown engine")
    };
    assert_eq!(unknown_engine.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&unknown_engine.stderr).contains("fixture.success"));
    assert_eq!(common(&["--family", "not.a.family"]).status.code(), Some(3));
    assert_eq!(
        common(&["--family", "cron.anchoring", "--tzdb", "exact:1900a"])
            .status
            .code(),
        Some(4)
    );
}
