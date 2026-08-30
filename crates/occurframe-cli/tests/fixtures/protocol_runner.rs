use std::{env, io::{self, BufRead, Write}, process, thread, time::Duration};

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "success".into());
    if mode == "runner-failure" {
        process::exit(17);
    }
    println!("{{\"message\":\"hello\",\"protocol_version\":\"2.0\",\"runner\":{{\"name\":\"cli-fixture-runner\",\"version\":\"2.0.0\",\"provenance\":\"source:occurframe-cli/tests/fixtures/protocol_runner.rs\"}},\"engine\":{{\"name\":\"cli-fixture-engine\",\"version\":\"1.0.0\",\"provenance\":\"deterministic test fixture\"}},\"runtime\":{{\"language\":\"Rust\",\"runtime\":\"rustc-fixture\",\"version\":\"deterministic\"}},\"capabilities\":[\"cron.next\"],\"dialect_ids\":[\"cron.vixie@1\"],\"semantic_profile_claims\":{{\"cron.start_inclusivity\":\"exclusive\",\"cron.start_truncation\":\"exact\"}},\"tzdb_provenance\":{{\"source\":\"fixture tzdb\",\"release_kind\":\"exact\",\"release\":\"2026a\"}}}}");
    io::stdout().flush().expect("flush hello");
    for line in io::stdin().lock().lines() {
        let line = line.expect("case line");
        let request_id = extract(&line, "\"request_id\":\"").unwrap_or("missing");
        let vector_id = extract(&line, "\"id\":\"").unwrap_or("missing");
        println!("{{\"message\":\"started\",\"protocol_version\":\"2.0\",\"request_id\":\"{request_id}\"}}");
        io::stdout().flush().expect("flush started");
        if mode == "timeout" {
            thread::sleep(Duration::from_secs(60));
            return;
        }
        let outcome = match mode.as_str() {
            "unsupported" => "{\"type\":\"unsupported\",\"diagnostic\":{\"code\":\"fixture_unsupported\",\"message\":\"fixture unsupported\"}}".into(),
            "engine-error" => "{\"type\":\"engine_error\",\"diagnostic\":{\"code\":\"fixture_error\",\"message\":\"fixture engine error\"}}".into(),
            "rejection" => "{\"type\":\"rejection\",\"diagnostic\":{\"code\":\"fixture_rejection\",\"message\":\"fixture rejection\"}}".into(),
            "mixed" if vector_id != "CRON-ANCH-001" => "{\"type\":\"rejection\",\"diagnostic\":{\"code\":\"fixture_rejection\",\"message\":\"fixture rejection\"}}".into(),
            _ => occurrence_outcome(vector_id),
        };
        let warnings = if mode == "warnings" {
            "[{\"code\":\"fixture_warning\",\"message\":\"successful warning\"}]"
        } else {
            "[]"
        };
        println!("{{\"message\":\"result\",\"protocol_version\":\"2.0\",\"request_id\":\"{request_id}\",\"outcome\":{outcome},\"warnings\":{warnings}}}");
        io::stdout().flush().expect("flush result");
    }
}

fn extract<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.split_once(marker)?.1;
    rest.split_once('\"').map(|(value, _)| value)
}

fn occurrence_outcome(id: &str) -> String {
    let occurrences = match id {
        "CRON-ANCH-001" => "[\"2026-01-02T12:00:00\",\"2026-01-03T12:00:00\",\"2026-01-04T12:00:00\"]",
        "CRON-ANCH-002" => "[\"2026-01-02T12:00:00\",\"2026-01-03T12:00:00\"]",
        "CRON-ANCH-003" => "[\"2026-01-01T12:00:30\",\"2026-01-01T12:01:30\",\"2026-01-01T12:02:30\"]",
        "CRON-ANCH-004" => "[\"2026-01-08T00:00:00\",\"2026-01-15T00:00:00\",\"2026-01-22T00:00:00\",\"2026-01-29T00:00:00\",\"2026-02-01T00:00:00\",\"2026-02-08T00:00:00\",\"2026-02-15T00:00:00\",\"2026-02-22T00:00:00\",\"2026-03-01T00:00:00\",\"2026-03-08T00:00:00\"]",
        _ => "[]",
    };
    format!("{{\"type\":\"occurrences\",\"occurrences\":{occurrences}}}")
}
