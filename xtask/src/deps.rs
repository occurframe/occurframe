//! Deterministic dependency inventory and third-party notice generation.
//!
//! The inventory is derived from Cargo's own resolution (`cargo metadata`) and
//! from `Cargo.lock`, so it describes exactly what a released binary links,
//! rather than a hand-maintained list that can drift. License metadata is
//! reported as Cargo records it: where a crate publishes no SPDX expression the
//! field is `null` and the reason is recorded, and nothing is inferred.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use serde_json::Value;

/// How a package enters the released artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    /// Named in a released crate's own `[dependencies]`.
    Direct,
    /// Reached only through another dependency.
    Transitive,
}

/// One redistributed third-party crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyEntry {
    pub name: String,
    pub version: String,
    pub relationship: Relationship,
    /// Cargo's source identifier, for example the crates.io registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The registry checksum recorded in `Cargo.lock`, where one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// The SPDX expression the crate publishes, verbatim. `null` when the crate
    /// publishes none; see `license_status`.
    pub license: Option<String>,
    /// Why `license` is absent, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_status: Option<String>,
    /// Names of license/notice files shipped inside the published crate.
    pub license_files: Vec<String>,
}

/// One workspace crate. First-party code is licensed by this repository, not by
/// a third party, so it is listed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirstPartyEntry {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub published: bool,
}

/// The complete inventory for one released artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    pub schema_version: String,
    /// The crate whose linked dependency closure this inventory describes.
    pub root_package: String,
    pub root_version: String,
    pub generated_by: String,
    pub notes: Vec<String>,
    pub first_party: Vec<FirstPartyEntry>,
    pub third_party_count: usize,
    pub third_party: Vec<DependencyEntry>,
}

/// Text of one crate's notice files, kept out of the JSON inventory.
#[derive(Debug, Clone)]
pub struct NoticeTexts {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub files: Vec<(String, String)>,
}

fn metadata(manifest: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .arg("--manifest-path")
        .arg(manifest)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// Minimal `Cargo.lock` reader for the `checksum` field.
///
/// Only three keys are needed, so a whole TOML parser would be a new
/// redistributed dependency for no gain.
fn lock_checksums(
    lock: &Path,
) -> Result<BTreeMap<(String, String), String>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(lock)?;
    let mut checksums = BTreeMap::new();
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            name = None;
            version = None;
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        match key.trim() {
            "name" => name = Some(value),
            "version" => version = Some(value),
            "checksum" => {
                if let (Some(name), Some(version)) = (name.as_ref(), version.as_ref()) {
                    checksums.insert((name.clone(), version.clone()), value);
                }
            }
            _ => {}
        }
    }
    Ok(checksums)
}

fn string_field(package: &Value, field: &str) -> Option<String> {
    package
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// True when a resolved edge is linked into the artifact, that is, when it is a
/// normal or build dependency rather than a dev-dependency.
fn is_linked_edge(dep: &Value) -> bool {
    let Some(kinds) = dep.get("dep_kinds").and_then(Value::as_array) else {
        return true;
    };
    kinds.iter().any(|kind| {
        !matches!(
            kind.get("kind").and_then(Value::as_str),
            Some("dev" | "development")
        )
    })
}

fn license_file_names(manifest_path: &str) -> Vec<String> {
    let Some(directory) = Path::new(manifest_path).parent() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let upper = name.to_ascii_uppercase();
            upper.starts_with("LICENSE")
                || upper.starts_with("LICENCE")
                || upper.starts_with("COPYING")
                || upper.starts_with("NOTICE")
        })
        .collect();
    names.sort();
    names
}

/// Build the inventory of everything linked into `root_package`.
#[allow(clippy::too_many_lines)]
pub fn dependency_inventory(
    manifest: &Path,
    lock: &Path,
    root_package: &str,
) -> Result<(Inventory, Vec<NoticeTexts>), Box<dyn std::error::Error>> {
    let metadata = metadata(manifest)?;
    let checksums = lock_checksums(lock)?;

    let packages: BTreeMap<String, &Value> = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("cargo metadata contained no packages")?
        .iter()
        .filter_map(|package| string_field(package, "id").map(|id| (id, package)))
        .collect();
    let workspace_members: BTreeSet<String> = metadata
        .get("workspace_members")
        .and_then(Value::as_array)
        .ok_or("cargo metadata contained no workspace members")?
        .iter()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect();
    let nodes: BTreeMap<String, &Value> = metadata
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(Value::as_array)
        .ok_or("cargo metadata contained no resolve graph")?
        .iter()
        .filter_map(|node| string_field(node, "id").map(|id| (id, node)))
        .collect();

    let root_id = packages
        .iter()
        .find(|(id, package)| {
            workspace_members.contains(*id)
                && string_field(package, "name").as_deref() == Some(root_package)
        })
        .map(|(id, _)| id.clone())
        .ok_or_else(|| format!("workspace does not contain a package named {root_package}"))?;
    let root_version = string_field(packages[&root_id], "version").unwrap_or_default();

    let linked_edges = |id: &str| -> Vec<String> {
        nodes
            .get(id)
            .and_then(|node| node.get("deps"))
            .and_then(Value::as_array)
            .map(|deps| {
                deps.iter()
                    .filter(|dep| is_linked_edge(dep))
                    .filter_map(|dep| string_field(dep, "pkg"))
                    .collect()
            })
            .unwrap_or_default()
    };

    let direct: BTreeSet<String> = linked_edges(&root_id).into_iter().collect();

    // Breadth-first over linked edges only. Workspace crates are traversed so
    // that their own dependencies are captured, but are reported as first party.
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::from([root_id.clone()]);
    while let Some(id) = queue.pop_front() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        for next in linked_edges(&id) {
            if !reachable.contains(&next) {
                queue.push_back(next);
            }
        }
    }

    let mut third_party = Vec::new();
    let mut notices = Vec::new();
    let mut first_party = Vec::new();
    for id in &reachable {
        let Some(package) = packages.get(id) else {
            continue;
        };
        let name = string_field(package, "name").unwrap_or_default();
        let version = string_field(package, "version").unwrap_or_default();
        let license = string_field(package, "license");
        if workspace_members.contains(id) {
            first_party.push(FirstPartyEntry {
                name,
                version,
                license,
                published: package.get("publish").is_none_or(
                    |publish| !matches!(publish, Value::Array(list) if list.is_empty()),
                ),
            });
            continue;
        }
        let manifest_path = string_field(package, "manifest_path").unwrap_or_default();
        let license_files = license_file_names(&manifest_path);
        let license_status = if license.is_some() {
            None
        } else if string_field(package, "license_file").is_some() {
            Some("no SPDX expression published; the crate ships a license file".to_owned())
        } else if license_files.is_empty() {
            Some("no SPDX expression and no license file are published with this crate".to_owned())
        } else {
            Some("no SPDX expression published; see the crate's license files".to_owned())
        };
        // `manifest_path` is deliberately not recorded: it is an absolute path
        // on the build machine and must never enter a published artifact.
        third_party.push(DependencyEntry {
            name: name.clone(),
            version: version.clone(),
            relationship: if direct.contains(id) {
                Relationship::Direct
            } else {
                Relationship::Transitive
            },
            source: string_field(package, "source"),
            checksum: checksums.get(&(name.clone(), version.clone())).cloned(),
            license: license.clone(),
            license_status,
            license_files: license_files.clone(),
        });
        let mut files = Vec::new();
        if let Some(directory) = Path::new(&manifest_path).parent() {
            for file in &license_files {
                if let Ok(text) = fs::read_to_string(directory.join(file)) {
                    files.push((file.clone(), text.replace("\r\n", "\n")));
                }
            }
        }
        notices.push(NoticeTexts {
            name,
            version,
            license,
            files,
        });
    }

    third_party.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });
    first_party.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });
    notices.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });

    let inventory = Inventory {
        schema_version: "1.0.0".into(),
        root_package: root_package.to_owned(),
        root_version,
        generated_by: "cargo metadata --locked, plus Cargo.lock checksums".into(),
        notes: vec![
            "Covers the normal and build dependency closure of the released binaries. Dev-dependencies are excluded because they are not linked into a release."
                .into(),
            "License fields are reproduced exactly as each crate publishes them. A null license means the crate publishes no SPDX expression; nothing is inferred or supplied on a crate's behalf."
                .into(),
            "Checksums are the registry checksums recorded in Cargo.lock and identify the exact published crate archive."
                .into(),
        ],
        first_party,
        third_party_count: third_party.len(),
        third_party,
    };
    Ok((inventory, notices))
}

/// Render the redistributable third-party notice file.
pub fn render_notices(inventory: &Inventory, notices: &[NoticeTexts]) -> String {
    let mut output = String::new();
    output.push_str("# Third-party notices\n\n");
    output.push_str(
        "Occurframe's released binaries statically link the Rust crates listed below.\nTheir license and notice texts are reproduced here as published, to satisfy the\nattribution requirements those licenses impose on redistribution.\n\n",
    );
    output.push_str(
        "This file is generated by `cargo run -p xtask -- dependency-inventory` from\n`cargo metadata` and `Cargo.lock`. `DEPENDENCIES.json` in the same release carries\nthe machine-readable form, including versions and registry checksums.\n\n",
    );
    output.push_str("## Summary\n\n");
    output.push_str("| Crate | Version | License |\n| --- | --- | --- |\n");
    for entry in &inventory.third_party {
        let _ = writeln!(
            output,
            "| {} | {} | {} |",
            entry.name,
            entry.version,
            entry
                .license
                .clone()
                .unwrap_or_else(|| "not published by the crate".into())
        );
    }
    output.push_str("\n## Reproduced texts\n\n");
    for notice in notices {
        let _ = writeln!(output, "### {} {}\n", notice.name, notice.version);
        match &notice.license {
            Some(license) => {
                let _ = writeln!(output, "Declared license: `{license}`\n");
            }
            None => output.push_str("Declared license: none published with the crate.\n\n"),
        }
        if notice.files.is_empty() {
            output.push_str(
                "The published crate archive contains no license or notice file. No text is\nreproduced here, and none is invented.\n\n",
            );
            continue;
        }
        for (name, text) in &notice.files {
            let _ = write!(output, "#### `{name}`\n\n```text\n");
            // A license text containing a fence would otherwise break out of the
            // block; indent-safe escaping keeps the reproduction faithful.
            output.push_str(&text.replace("```", "``\u{200b}`"));
            if !text.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n\n");
        }
    }
    output
}

/// Resolve a workspace `Cargo.lock` beside its manifest.
#[must_use]
pub fn lock_beside(manifest: &Path) -> PathBuf {
    manifest.parent().map_or_else(
        || PathBuf::from("Cargo.lock"),
        |parent| parent.join("Cargo.lock"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_checksums_are_read_per_package_block() {
        let directory =
            std::env::temp_dir().join(format!("occurframe-lock-test-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("fixture directory");
        let lock = directory.join("Cargo.lock");
        fs::write(
            &lock,
            "version = 4\n\n[[package]]\nname = \"alpha\"\nversion = \"1.0.0\"\nchecksum = \"aaa\"\n\n[[package]]\nname = \"local\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"beta\"\nversion = \"2.0.0\"\nchecksum = \"bbb\"\n",
        )
        .expect("write lock");
        let checksums = lock_checksums(&lock).expect("parse lock");
        assert_eq!(
            checksums.get(&("alpha".to_owned(), "1.0.0".to_owned())),
            Some(&"aaa".to_owned())
        );
        assert_eq!(
            checksums.get(&("beta".to_owned(), "2.0.0".to_owned())),
            Some(&"bbb".to_owned())
        );
        // A path dependency has no checksum and must not inherit a neighbour's.
        assert_eq!(
            checksums.get(&("local".to_owned(), "0.1.0".to_owned())),
            None
        );
        fs::remove_dir_all(&directory).expect("remove isolated fixture directory");
    }

    #[test]
    fn dev_dependency_edges_are_not_linked() {
        let normal = serde_json::json!({"dep_kinds":[{"kind":null,"target":null}]});
        let build = serde_json::json!({"dep_kinds":[{"kind":"build","target":null}]});
        let dev = serde_json::json!({"dep_kinds":[{"kind":"dev","target":null}]});
        let mixed = serde_json::json!({"dep_kinds":[{"kind":"dev"},{"kind":null}]});
        assert!(is_linked_edge(&normal));
        assert!(is_linked_edge(&build));
        assert!(!is_linked_edge(&dev));
        // A crate that is both a dev-dependency and a real dependency is linked.
        assert!(is_linked_edge(&mixed));
    }
}
