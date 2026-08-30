use std::{env, path::PathBuf, process::ExitCode};

use occurframe_conformance::{
    load_and_validate_corpus, migrate_rc1, pack_release, verify_deterministic_pack,
    verify_manifest, verify_migration, write_tree_checksums,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

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
        _ => return Err(format!("unknown command: {command}").into()),
    }
    Ok(())
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
}
