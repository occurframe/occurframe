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
