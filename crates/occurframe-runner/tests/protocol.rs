use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use occurframe_runner::{
    LaunchConfig, ProtocolSchema, Reproducibility, ReproducibilityStatus, RunnerBuild,
    RunnerSupervisor, TzdbReleaseKind, semantic_observation_digest, semantic_observation_ndjson,
};
use occurframe_wire::{
    Classification, ComponentIdentity, DialectId, EngineOutcome, ExecutionStatus, Expectation,
    Lifecycle, SemanticValue, TzdbProvenance, TzdbRelease, Vector, VerdictStatus,
};
use serde_json::json;

fn schema() -> ProtocolSchema {
    ProtocolSchema::from_value(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["message", "protocol_version"],
        "properties": {
            "message": {"enum": ["hello", "case", "started", "result"]},
            "protocol_version": {"const": "2.0"}
        }
    }))
    .expect("fixture schema")
}

fn build(mode: &str) -> RunnerBuild {
    RunnerBuild {
        build_id: format!("fake-{mode}"),
        protocol_version: "2.0".into(),
        runner: ComponentIdentity {
            name: "fake-runner".into(),
            version: "2.0.0".into(),
            provenance: Some("test fixture".into()),
        },
        engine: ComponentIdentity {
            name: "fake-engine".into(),
            version: "1.0.0".into(),
            provenance: Some("test fixture".into()),
        },
        language: "Rust".into(),
        runtime_name: "rust-test".into(),
        runtime_requirement: "deterministic".into(),
        launch: LaunchConfig {
            program: env!("CARGO_BIN_EXE_occurframe-fake-runner").into(),
            arguments: vec![mode.into()],
            working_directory: ".".into(),
            environment: BTreeMap::new(),
        },
        supported_operations: vec!["cron.next".into(), "cron.parse".into()],
        dialect_ids: vec![DialectId("cron.vixie@1".into())],
        semantic_profile_claims: BTreeMap::from([(
            "cron.start_inclusivity".into(),
            SemanticValue::Text("exclusive".into()),
        )]),
        tzdb_provenance_acquisition: "test fixture".into(),
        tzdb_source: "fake fixture".into(),
        additional_tzdb_sources: Vec::new(),
        allowed_tzdb_release_kinds: vec![TzdbReleaseKind::Exact],
        fallback_tzdb_provenance: TzdbProvenance {
            source: "fake fixture".into(),
            release: TzdbRelease::Exact {
                release: "2026a".into(),
            },
            fingerprint: None,
        },
        representative_vectors: vec!["TEST-001".into()],
        reproducibility: Reproducibility {
            status: ReproducibilityStatus::Reproducible,
            setup: "cargo test".into(),
            reason_code: None,
            reason: None,
        },
        legacy_source: "test fixture".into(),
    }
}

fn vector(id: &str, expectation: Expectation, operation: &str) -> Vector {
    Vector {
        schema_version: "1.0.0".into(),
        corpus_version: "1.0.0-rc2".into(),
        id: id.into(),
        family: "test.fixture".into(),
        title: "runner contract fixture".into(),
        kind: "cron".into(),
        operation: operation.into(),
        input: json!({"kind":"cron","expr":"* * * * *","start":"2026-01-01T00:00:00","count":1,"zone":null}),
        context: json!({}),
        classification: Classification::Normative,
        semantic_axes: Vec::new(),
        normative_evidence: Vec::new(),
        expectation,
        rationale: "transport-only synthetic fixture".into(),
        tags: vec!["fixture".into()],
        lifecycle: Lifecycle::Active,
        supersession: None,
    }
}

fn execute(mode: &str, vector: &Vector, watchdog: Duration) -> occurframe_runner::CaseExecution {
    let mut runner =
        RunnerSupervisor::new(build(mode), PathBuf::from("."), schema(), watchdog, 1024);
    runner.execute(vector, 0)
}

#[test]
fn valid_results_cover_all_engine_outcomes_and_warning_orthogonality() {
    let single = vector(
        "TEST-001",
        Expectation::Single {
            occurrences: vec!["observed".into()],
            note: None,
        },
        "cron.next",
    );
    let valid = execute("valid", &single, Duration::from_secs(2));
    assert_eq!(
        valid.verdict.status,
        VerdictStatus::Conformant,
        "{valid:#?}"
    );

    let empty = vector(
        "TEST-002",
        Expectation::Single {
            occurrences: Vec::new(),
            note: None,
        },
        "cron.next",
    );
    let empty_result = execute("empty", &empty, Duration::from_secs(2));
    assert_eq!(empty_result.verdict.status, VerdictStatus::Conformant);
    assert!(matches!(
        empty_result.observation.engine_outcome,
        Some(EngineOutcome::Occurrences { ref occurrences }) if occurrences.is_empty()
    ));

    let warning = execute("warnings", &empty, Duration::from_secs(2));
    assert_eq!(warning.verdict.status, VerdictStatus::Conformant);
    assert_eq!(warning.observation.warnings.len(), 1);

    let reject = vector(
        "TEST-003",
        Expectation::Reject {
            error_class: None,
            note: None,
        },
        "cron.parse",
    );
    assert_eq!(
        execute("rejection", &reject, Duration::from_secs(2))
            .verdict
            .status,
        VerdictStatus::Conformant
    );
    assert_eq!(
        execute("engine-error", &reject, Duration::from_secs(2))
            .verdict
            .status,
        VerdictStatus::NonConformant
    );
    assert_eq!(
        execute("unsupported", &single, Duration::from_secs(2))
            .verdict
            .status,
        VerdictStatus::Unsupported
    );

    let open = vector("TEST-004", Expectation::Open { note: None }, "cron.parse");
    assert_eq!(
        execute("accepted", &open, Duration::from_secs(2))
            .verdict
            .status,
        VerdictStatus::RecordedUnscored
    );
}

#[test]
fn protocol_and_identity_faults_are_runner_failures() {
    let case = vector("TEST-001", Expectation::Open { note: None }, "cron.next");
    for mode in [
        "wrong-protocol",
        "wrong-runner",
        "wrong-engine",
        "wrong-runtime-version",
        "malformed-hello",
        "malformed-result",
        "stdout-contamination",
        "exit-before-hello",
        "exit-before-started",
        "exit-after-started",
    ] {
        let record = execute(mode, &case, Duration::from_secs(2));
        assert_eq!(
            record.observation.execution_status,
            ExecutionStatus::RunnerFailure,
            "mode {mode}"
        );
        assert_eq!(record.verdict.status, VerdictStatus::InfrastructureFailure);
        assert!(record.observation.engine_outcome.is_none());
    }
}

#[test]
fn pre_started_watchdog_is_not_an_engine_timeout() {
    let case = vector(
        "TEST-001",
        Expectation::Reject {
            error_class: None,
            note: None,
        },
        "cron.parse",
    );
    let record = execute("never-started", &case, Duration::from_millis(100));
    assert_eq!(
        record.observation.execution_status,
        ExecutionStatus::RunnerFailure
    );
    assert_eq!(record.verdict.status, VerdictStatus::InfrastructureFailure);
}

#[test]
fn timeout_and_runner_failure_force_restart() {
    let first = vector(
        "TEST-001",
        Expectation::Reject {
            error_class: None,
            note: None,
        },
        "cron.parse",
    );
    let second = vector("TEST-002", Expectation::Open { note: None }, "cron.next");

    let mut timeout_runner = RunnerSupervisor::new(
        build("restart-timeout"),
        PathBuf::from("."),
        schema(),
        Duration::from_secs(2),
        1024,
    );
    let timed_out = timeout_runner.execute(&first, 0);
    assert_eq!(
        timed_out.observation.execution_status,
        ExecutionStatus::Timeout,
        "{timed_out:#?}"
    );
    assert_eq!(timed_out.verdict.status, VerdictStatus::Timeout);
    let recovered = timeout_runner.execute(&second, 1);
    assert_eq!(
        recovered.observation.execution_status,
        ExecutionStatus::Completed
    );

    let mut failed_runner = RunnerSupervisor::new(
        build("restart-failure"),
        PathBuf::from("."),
        schema(),
        Duration::from_secs(2),
        1024,
    );
    let failed = failed_runner.execute(&first, 0);
    assert_eq!(
        failed.observation.execution_status,
        ExecutionStatus::RunnerFailure
    );
    let recovered = failed_runner.execute(&second, 1);
    assert_eq!(
        recovered.observation.execution_status,
        ExecutionStatus::Completed
    );
}

#[test]
fn sequential_cases_and_non_monotonic_order_are_preserved() {
    let expected = vec!["later".into(), "earlier".into(), "earlier".into()];
    let first = vector(
        "TEST-001",
        Expectation::Single {
            occurrences: expected.clone(),
            note: None,
        },
        "cron.next",
    );
    let second = vector(
        "TEST-002",
        Expectation::Single {
            occurrences: expected.clone(),
            note: None,
        },
        "cron.next",
    );
    let mut runner = RunnerSupervisor::new(
        build("non-monotonic"),
        PathBuf::from("."),
        schema(),
        Duration::from_secs(2),
        1024,
    );
    for (index, case) in [&first, &second].into_iter().enumerate() {
        let record = runner.execute(case, index);
        assert_eq!(
            record.verdict.status,
            VerdictStatus::Conformant,
            "{record:#?}"
        );
        assert!(matches!(
            record.observation.engine_outcome,
            Some(EngineOutcome::Occurrences { occurrences }) if occurrences == expected
        ));
    }
}

#[test]
fn semantic_bundle_is_byte_stable_and_ignores_transport_metadata() {
    let case = vector(
        "TEST-001",
        Expectation::Single {
            occurrences: vec!["observed".into()],
            note: None,
        },
        "cron.next",
    );
    let first = execute("valid", &case, Duration::from_secs(2));
    let mut second = execute("valid", &case, Duration::from_secs(2));
    second.runner_diagnostic = Some(occurframe_runner::RunnerDiagnostic {
        diagnostic: occurframe_wire::Diagnostic {
            code: "machine_specific".into(),
            message: "ignored transport metadata".into(),
            details: None,
        },
        stderr_tail: Some("C:\\machine\\volatile\\path".into()),
    });
    let first_bytes = semantic_observation_ndjson(std::slice::from_ref(&first)).unwrap();
    let second_bytes = semantic_observation_ndjson(std::slice::from_ref(&second)).unwrap();
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(
        semantic_observation_digest(&[first]).unwrap(),
        semantic_observation_digest(&[second]).unwrap()
    );
}

#[test]
fn bounded_stderr_tail_is_diagnostic_only() {
    let case = vector("TEST-001", Expectation::Open { note: None }, "cron.next");
    let record = execute("stderr-failure", &case, Duration::from_secs(2));
    assert_eq!(
        record.observation.execution_status,
        ExecutionStatus::RunnerFailure
    );
    let tail = record
        .runner_diagnostic
        .expect("transport diagnostic")
        .stderr_tail
        .expect("stderr tail");
    assert!(tail.len() <= 1024);
}

#[test]
fn the_same_fake_runner_suite_has_an_identical_digest() {
    fn suite() -> Vec<occurframe_runner::CaseExecution> {
        let cases = [
            (
                "accepted",
                vector("TEST-003", Expectation::Open { note: None }, "cron.parse"),
            ),
            (
                "empty",
                vector(
                    "TEST-002",
                    Expectation::Single {
                        occurrences: Vec::new(),
                        note: None,
                    },
                    "cron.next",
                ),
            ),
            (
                "valid",
                vector(
                    "TEST-001",
                    Expectation::Single {
                        occurrences: vec!["observed".into()],
                        note: None,
                    },
                    "cron.next",
                ),
            ),
        ];
        cases
            .iter()
            .map(|(mode, case)| execute(mode, case, Duration::from_secs(2)))
            .collect()
    }
    let first = suite();
    let second = suite();
    assert_eq!(
        semantic_observation_ndjson(&first).unwrap(),
        semantic_observation_ndjson(&second).unwrap()
    );
    assert_eq!(
        semantic_observation_digest(&first).unwrap(),
        semantic_observation_digest(&second).unwrap()
    );
}
