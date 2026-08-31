use std::{path::Path, process::Command};

use occurframe_wire::{SourceRevision, SourceRevisionMethod};

use crate::{Error, Result};

/// Observe the source checkout that supplies corpus or tooling bytes.
///
/// A Git revision is accepted only for a clean checkout rooted exactly at
/// `root`. Material without repository metadata requires a trusted, explicit
/// attestation; a directory name, version, or content digest is never promoted
/// into source identity.
pub fn observe_source_revision(
    root: &Path,
    expected_revision: Option<&str>,
    attested_revision: Option<&str>,
) -> Result<SourceRevision> {
    let source = if root.join(".git").exists() {
        if attested_revision.is_some() {
            return Err(Error::SourceProvenance(
                "attested source revision cannot override an available Git checkout".into(),
            ));
        }
        observe_git_checkout(root)?
    } else {
        let revision = attested_revision.ok_or_else(|| {
            Error::SourceProvenance(
                "source has no .git metadata; --attested-source-revision is required".into(),
            )
        })?;
        validate_git_object_id(revision)?;
        SourceRevision {
            revision: revision.into(),
            method: SourceRevisionMethod::AttestedInput,
        }
    };

    if let Some(expected) = expected_revision {
        validate_git_object_id(expected)?;
        if source.revision != expected {
            return Err(Error::SourceProvenance(format!(
                "expected source revision {expected}, observed {} via {:?}",
                source.revision, source.method
            )));
        }
    }
    Ok(source)
}

fn observe_git_checkout(root: &Path) -> Result<SourceRevision> {
    let canonical_root = root.canonicalize()?;
    let top_level = git_output(root, &["rev-parse", "--show-toplevel"])?;
    let canonical_top_level = Path::new(top_level.trim()).canonicalize()?;
    if canonical_top_level != canonical_root {
        return Err(Error::SourceProvenance(format!(
            "source root {} is inside Git checkout {}, not its root",
            canonical_root.display(),
            canonical_top_level.display()
        )));
    }
    let revision = git_output(root, &["rev-parse", "--verify", "HEAD"])?
        .trim()
        .to_owned();
    validate_git_object_id(&revision)?;
    let status = git_output(
        root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    if !status.trim().is_empty() {
        return Err(Error::SourceProvenance(
            "source checkout is dirty; HEAD does not identify the executed bytes".into(),
        ));
    }
    Ok(SourceRevision {
        revision,
        method: SourceRevisionMethod::GitCheckout,
    })
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(Error::SourceProvenance(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| Error::SourceProvenance(format!("Git output is not UTF-8: {error}")))
}

fn validate_git_object_id(revision: &str) -> Result<()> {
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::SourceProvenance(
            "source revision must be a full 40-character Git object ID".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    fn fixture(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "occurframe-source-{name}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn git(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(arguments)
            .status()
            .expect("run Git fixture command");
        assert!(status.success(), "git {}", arguments.join(" "));
    }

    fn repository(name: &str, message: &str) -> std::path::PathBuf {
        let root = fixture(name);
        fs::create_dir_all(&root).expect("create fixture");
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.name", "Occurframe Test"]);
        git(&root, &["config", "user.email", "test@occurframe.invalid"]);
        fs::write(root.join("canonical.json"), b"{\"same\":true}\n").expect("fixture content");
        git(&root, &["add", "canonical.json"]);
        git(&root, &["commit", "--quiet", "-m", message]);
        root
    }

    #[test]
    fn actual_checkout_revision_is_observed_and_expected_revision_is_enforced() {
        let root = repository("positive", "positive");
        let observed = observe_source_revision(&root, None, None).expect("observe Git checkout");
        assert_eq!(observed.method, SourceRevisionMethod::GitCheckout);
        assert!(observe_source_revision(&root, Some(&observed.revision), None).is_ok());
        let mismatch = "0000000000000000000000000000000000000000";
        assert!(
            observe_source_revision(&root, Some(mismatch), None)
                .expect_err("mismatch must fail closed")
                .to_string()
                .contains("expected source revision")
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn equal_content_does_not_collapse_distinct_source_identity() {
        let first = repository("identity-a", "first history");
        let second = repository("identity-b", "second history");
        assert_eq!(
            fs::read(first.join("canonical.json")).unwrap(),
            fs::read(second.join("canonical.json")).unwrap()
        );
        let first_source = observe_source_revision(&first, None, None).unwrap();
        let second_source = observe_source_revision(&second, None, None).unwrap();
        assert_ne!(first_source.revision, second_source.revision);
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn non_git_input_requires_an_explicit_attestation() {
        let root = fixture("attested");
        fs::create_dir_all(&root).unwrap();
        assert!(observe_source_revision(&root, None, None).is_err());
        let revision = "1234567890abcdef1234567890abcdef12345678";
        let observed = observe_source_revision(&root, Some(revision), Some(revision)).unwrap();
        assert_eq!(observed.method, SourceRevisionMethod::AttestedInput);
        assert_eq!(observed.revision, revision);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dirty_checkout_is_not_misattributed_to_head() {
        let root = repository("dirty", "clean commit");
        fs::write(root.join("untracked"), b"not represented by HEAD").unwrap();
        assert!(
            observe_source_revision(&root, None, None)
                .expect_err("dirty source must fail")
                .to_string()
                .contains("dirty")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
