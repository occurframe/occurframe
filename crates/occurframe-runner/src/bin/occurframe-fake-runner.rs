//! Deterministic protocol fault injector used only by contract tests.

use std::{
    env,
    io::{self, BufRead, Write},
    process, thread,
    time::Duration,
};

use serde_json::{Value, json};

#[allow(clippy::too_many_lines)]
fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "valid".into());
    if mode == "exit-before-hello" {
        process::exit(17);
    }
    if mode == "malformed-hello" {
        emit_raw("{not-json");
        return;
    }
    let mut hello = json!({
        "message": "hello",
        "protocol_version": "2.0",
        "runner": {"name": "fake-runner", "version": "2.0.0", "provenance": "test fixture"},
        "engine": {"name": "fake-engine", "version": "1.0.0", "provenance": "test fixture"},
        "runtime": {"language": "Rust", "runtime": "rust-test", "version": "deterministic"},
        "capabilities": ["cron.next", "cron.parse"],
        "dialect_ids": ["cron.vixie@1"],
        "semantic_profile_claims": {"cron.start_inclusivity": "exclusive"},
        "tzdb_provenance": {"source": "fake fixture", "release_kind": "exact", "release": "2026a"}
    });
    match mode.as_str() {
        "wrong-protocol" => hello["protocol_version"] = json!("1.0"),
        "wrong-runner" => hello["runner"]["name"] = json!("impostor"),
        "wrong-engine" => hello["engine"]["name"] = json!("impostor"),
        _ => {}
    }
    emit(&hello);

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(case): Result<Value, _> = serde_json::from_str(&line) else {
            return;
        };
        let request_id = case["request_id"].as_str().unwrap_or("missing");
        let vector_id = case["vector"]["id"].as_str().unwrap_or("missing");

        if mode == "exit-before-started"
            || (mode == "restart-failure" && vector_id.ends_with("001"))
        {
            process::exit(18);
        }
        if mode == "stderr-failure" {
            eprint!("{}", "diagnostic-tail-".repeat(1000));
            process::exit(20);
        }
        if mode == "never-started" {
            thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode == "stdout-contamination" {
            emit_raw("debug output does not belong on stdout");
            return;
        }
        emit(&json!({
            "message": "started",
            "protocol_version": "2.0",
            "request_id": request_id
        }));

        if mode == "exit-after-started" {
            process::exit(19);
        }
        if mode == "started-then-hangs" || (mode == "restart-timeout" && vector_id.ends_with("001"))
        {
            thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode == "malformed-result" {
            emit_raw("{malformed-result");
            return;
        }

        let (outcome, warnings) = match mode.as_str() {
            "rejection" => (
                diagnostic_outcome("rejection", "deliberate_rejection"),
                vec![],
            ),
            "engine-error" => (
                diagnostic_outcome("engine_error", "unexpected_exception"),
                vec![],
            ),
            "unsupported" => (
                diagnostic_outcome("unsupported", "unsupported_capability"),
                vec![],
            ),
            "accepted" => (json!({"type": "accepted"}), vec![]),
            "empty" => (json!({"type": "occurrences", "occurrences": []}), vec![]),
            "warnings" => (
                json!({"type": "occurrences", "occurrences": []}),
                vec![json!({"code": "fixture_warning", "message": "successful warning"})],
            ),
            "non-monotonic" => (
                json!({"type": "occurrences", "occurrences": ["later", "earlier", "earlier"]}),
                vec![],
            ),
            _ => (
                json!({"type": "occurrences", "occurrences": ["observed"]}),
                vec![],
            ),
        };
        emit(&json!({
            "message": "result",
            "protocol_version": "2.0",
            "request_id": request_id,
            "outcome": outcome,
            "warnings": warnings
        }));
    }
}

fn diagnostic_outcome(kind: &str, code: &str) -> Value {
    json!({
        "type": kind,
        "diagnostic": {"code": code, "message": "deterministic fake-runner diagnostic"}
    })
}

fn emit(value: &Value) {
    emit_raw(&serde_json::to_string(value).expect("serialize fixture message"));
}

fn emit_raw(line: &str) {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").expect("write fixture message");
    stdout.flush().expect("flush fixture message");
}
