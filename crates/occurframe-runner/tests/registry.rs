use std::{collections::BTreeMap, path::PathBuf};

use occurframe_runner::{ReproducibilityStatus, RunnerRegistry};

#[test]
fn phase_two_registry_has_25_immutable_build_configurations() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let registry = RunnerRegistry::load(&root.join("runners/registry/runner-builds.json"))
        .expect("load runner registry");
    assert_eq!(registry.builds.len(), 25);
    let counts = registry
        .builds
        .iter()
        .fold(BTreeMap::new(), |mut counts, build| {
            *counts.entry(build.language.as_str()).or_insert(0_usize) += 1;
            counts
        });
    assert_eq!(
        counts,
        BTreeMap::from([
            ("Go", 3),
            ("JavaScript", 6),
            ("PHP", 2),
            ("Python", 12),
            ("Ruby", 2),
        ])
    );
    assert_eq!(
        registry
            .builds
            .iter()
            .filter(|build| build.reproducibility.status == ReproducibilityStatus::Reproducible)
            .count(),
        23
    );
    assert!(
        registry
            .builds
            .iter()
            .filter(|build| build.reproducibility.status == ReproducibilityStatus::Unreproducible)
            .all(|build| build.language == "Ruby")
    );
}

/// The registry is an identity document. A build that does not pin an exact
/// runtime version cannot be certified, because nothing would stop a different
/// interpreter from producing evidence attributed to the pinned one.
#[test]
fn every_configured_build_pins_an_exact_runtime_version() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let registry = RunnerRegistry::load(&root.join("runners/registry/runner-builds.json"))
        .expect("load runner registry");
    let pinned: BTreeMap<&str, &str> = registry
        .builds
        .iter()
        .map(|build| (build.language.as_str(), build.runtime_requirement.as_str()))
        .collect();
    assert_eq!(
        pinned,
        BTreeMap::from([
            ("Go", "go1.24.7"),
            ("JavaScript", "1.3.13"),
            ("PHP", "8.4.21"),
            ("Python", "3.11.15"),
            ("Ruby", "3.3.6"),
        ])
    );
    for build in &registry.builds {
        assert!(
            !build.runtime_requirement.trim().is_empty(),
            "{} does not pin a runtime version",
            build.build_id
        );
    }
}

/// Launch paths must resolve on the canonical Linux certification platform as
/// well as on a developer's Windows checkout. Extension-free program paths let
/// the supervisor append `.exe` on Windows; a hard-coded `.exe` cannot be
/// undone on Linux.
#[test]
fn configured_launch_programs_are_platform_neutral() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let registry = RunnerRegistry::load(&root.join("runners/registry/runner-builds.json"))
        .expect("load runner registry");
    for build in &registry.builds {
        assert!(
            !std::path::Path::new(&build.launch.program)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe")),
            "{} pins a Windows-only launch path: {}",
            build.build_id,
            build.launch.program
        );
    }
}
