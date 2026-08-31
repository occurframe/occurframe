use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

use occurframe_wire::{LaunchResolutionMethod, RunnerEnvironmentProvenance};

use crate::RunnerBuild;

pub(crate) const ENVIRONMENT_POLICY: &str = "hermetic_allowlist_v1";

pub(crate) struct PreparedLaunch {
    pub program: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
    pub provenance: RunnerEnvironmentProvenance,
}

pub(crate) fn prepare_launch(build: &RunnerBuild, root: &Path) -> io::Result<PreparedLaunch> {
    let parent_environment = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let (program, method) = resolve_executable(
        &build.launch.program,
        root,
        parent_value(&parent_environment, "PATH"),
        parent_value(&parent_environment, "PATHEXT"),
    )?;
    let (environment, provenance) = build_environment(build, &parent_environment, method);
    Ok(PreparedLaunch {
        program,
        environment,
        provenance,
    })
}

pub(crate) fn planned_environment_provenance(build: &RunnerBuild) -> RunnerEnvironmentProvenance {
    let parent_environment = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let method = classify_resolution(&build.launch.program);
    build_environment(build, &parent_environment, method).1
}

fn classify_resolution(program: &str) -> LaunchResolutionMethod {
    let path = Path::new(program);
    if path.is_absolute() {
        LaunchResolutionMethod::AbsolutePath
    } else if path.components().count() > 1 {
        LaunchResolutionMethod::RepositoryRelative
    } else {
        LaunchResolutionMethod::SearchPath
    }
}

#[cfg(not(windows))]
fn parent_value<'a>(parent: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsString> {
    parent.get(OsStr::new(name))
}

#[cfg(windows)]
fn parent_value<'a>(parent: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsString> {
    parent.iter().find_map(|(key, value)| {
        key.to_string_lossy()
            .eq_ignore_ascii_case(name)
            .then_some(value)
    })
}

fn resolve_executable(
    program: &str,
    root: &Path,
    search_path: Option<&OsString>,
    path_extensions: Option<&OsString>,
) -> io::Result<(PathBuf, LaunchResolutionMethod)> {
    let method = classify_resolution(program);
    let path = Path::new(program);
    match method {
        LaunchResolutionMethod::AbsolutePath => resolve_candidate(path, path_extensions)
            .map(|resolved| (resolved, method))
            .ok_or_else(|| executable_not_found(program)),
        LaunchResolutionMethod::RepositoryRelative => {
            let candidate = root.join(path);
            resolve_candidate(&candidate, path_extensions)
                .map(|resolved| (resolved, method))
                .ok_or_else(|| executable_not_found(program))
        }
        LaunchResolutionMethod::SearchPath => {
            let search_path = search_path.ok_or_else(|| executable_not_found(program))?;
            std::env::split_paths(search_path)
                .find_map(|directory| resolve_candidate(&directory.join(path), path_extensions))
                .map(|resolved| (resolved, method))
                .ok_or_else(|| executable_not_found(program))
        }
    }
}

fn executable_not_found(program: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("could not resolve runner executable {program:?} before clearing the environment"),
    )
}

#[cfg(not(windows))]
fn resolve_candidate(candidate: &Path, _path_extensions: Option<&OsString>) -> Option<PathBuf> {
    candidate.is_file().then(|| candidate.to_path_buf())
}

#[cfg(windows)]
fn resolve_candidate(candidate: &Path, path_extensions: Option<&OsString>) -> Option<PathBuf> {
    if candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    if candidate.extension().is_some() {
        return None;
    }
    let extensions = path_extensions
        .and_then(|value| value.to_str())
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    extensions.split(';').find_map(|extension| {
        let extension = extension.trim().trim_start_matches('.');
        let candidate = candidate.with_extension(extension);
        candidate.is_file().then_some(candidate)
    })
}

fn build_environment(
    build: &RunnerBuild,
    parent: &BTreeMap<OsString, OsString>,
    method: LaunchResolutionMethod,
) -> (BTreeMap<OsString, OsString>, RunnerEnvironmentProvenance) {
    let mut environment = BTreeMap::new();
    environment.insert(OsString::from("TZ"), OsString::from("UTC"));
    environment.insert(OsString::from("LANG"), OsString::from("C"));
    environment.insert(OsString::from("LC_ALL"), OsString::from("C"));

    let mut platform_variable_names = Vec::new();
    #[cfg(windows)]
    {
        if let Some(system_root) = parent_value(parent, "SystemRoot") {
            environment.insert(OsString::from("SystemRoot"), system_root.clone());
            platform_variable_names.push("SystemRoot".to_owned());
        }
        let temporary_directory = std::env::temp_dir().into_os_string();
        environment.insert(OsString::from("TEMP"), temporary_directory.clone());
        environment.insert(OsString::from("TMP"), temporary_directory);
        platform_variable_names.extend(["TEMP".to_owned(), "TMP".to_owned()]);
    }
    #[cfg(not(windows))]
    {
        let _ = parent;
        environment.insert(
            OsString::from("TMPDIR"),
            std::env::temp_dir().into_os_string(),
        );
        platform_variable_names.push("TMPDIR".to_owned());
    }

    for (name, value) in &build.launch.environment {
        environment.insert(OsString::from(name), OsString::from(value));
    }

    let explicit_runner_variable_names = build.launch.environment.keys().cloned().collect();
    let host_timezone_setting = environment
        .get(OsStr::new("TZ"))
        .and_then(|value| value.to_str())
        .unwrap_or("non_utf8")
        .to_owned();
    let locale_policy = if build.launch.environment.contains_key("LC_ALL")
        || build.launch.environment.contains_key("LANG")
        || build.launch.environment.contains_key("LC_TIME")
    {
        "explicit_runner_configuration"
    } else {
        "fixed_c"
    };

    (
        environment,
        RunnerEnvironmentProvenance {
            environment_policy: ENVIRONMENT_POLICY.into(),
            host_timezone_setting,
            locale_policy: locale_policy.into(),
            launch_resolution_method: method,
            platform_variable_names,
            explicit_runner_variable_names,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use occurframe_wire::{ComponentIdentity, TzdbProvenance, TzdbRelease};

    fn build(environment: BTreeMap<String, String>) -> RunnerBuild {
        RunnerBuild {
            build_id: "test".into(),
            protocol_version: "3.0".into(),
            runner: ComponentIdentity {
                name: "test".into(),
                version: "3.0.0".into(),
                provenance: Some("test".into()),
            },
            engine: ComponentIdentity {
                name: "test".into(),
                version: "1".into(),
                provenance: Some("test".into()),
            },
            language: "test".into(),
            runtime_name: "test".into(),
            runtime_requirement: "1".into(),
            launch: crate::LaunchConfig {
                program: "test".into(),
                arguments: Vec::new(),
                working_directory: ".".into(),
                environment,
            },
            supported_operations: vec!["test".into()],
            dialect_ids: Vec::new(),
            semantic_profile_claims: BTreeMap::new(),
            tzdb_provenance_acquisition: "test".into(),
            tzdb_source: "test".into(),
            additional_tzdb_sources: Vec::new(),
            allowed_tzdb_release_kinds: vec![crate::TzdbReleaseKind::Unknown],
            fallback_tzdb_provenance: TzdbProvenance {
                source: "test".into(),
                release: TzdbRelease::Unknown,
                fingerprint: None,
            },
            representative_vectors: vec!["test".into()],
            reproducibility: crate::Reproducibility {
                status: crate::ReproducibilityStatus::Reproducible,
                setup: "test".into(),
                reason_code: None,
                reason: None,
            },
            legacy_source: "test".into(),
        }
    }

    #[test]
    fn arbitrary_parent_environment_is_not_granted() {
        let parent = BTreeMap::from([
            (OsString::from("HOME"), OsString::from("host-home")),
            (OsString::from("TZ"), OsString::from("Pacific/Auckland")),
            (OsString::from("LC_TIME"), OsString::from("tr_TR.UTF-8")),
            (
                OsString::from("CLOUD_SECRET_TOKEN"),
                OsString::from("secret"),
            ),
        ]);
        let (environment, provenance) = build_environment(
            &build(BTreeMap::new()),
            &parent,
            LaunchResolutionMethod::SearchPath,
        );
        assert_eq!(
            environment.get(OsStr::new("TZ")),
            Some(&OsString::from("UTC"))
        );
        assert_eq!(
            environment.get(OsStr::new("LANG")),
            Some(&OsString::from("C"))
        );
        assert_eq!(
            environment.get(OsStr::new("LC_ALL")),
            Some(&OsString::from("C"))
        );
        assert!(!environment.contains_key(OsStr::new("HOME")));
        assert!(!environment.contains_key(OsStr::new("LC_TIME")));
        assert!(!environment.contains_key(OsStr::new("CLOUD_SECRET_TOKEN")));
        assert_eq!(provenance.environment_policy, ENVIRONMENT_POLICY);
    }

    #[test]
    fn explicit_runner_environment_is_granted_and_recorded_by_name_only() {
        let configured = BTreeMap::from([
            ("ADAPTER_MODE".into(), "declared".into()),
            ("TZ".into(), "Europe/London".into()),
        ]);
        let (environment, provenance) = build_environment(
            &build(configured),
            &BTreeMap::new(),
            LaunchResolutionMethod::RepositoryRelative,
        );
        assert_eq!(
            environment.get(OsStr::new("ADAPTER_MODE")),
            Some(&OsString::from("declared"))
        );
        assert_eq!(provenance.host_timezone_setting, "Europe/London");
        assert_eq!(
            provenance.explicit_runner_variable_names,
            vec!["ADAPTER_MODE", "TZ"]
        );
        let serialized = serde_json::to_string(&provenance).unwrap();
        assert!(!serialized.contains("declared"));
    }

    #[test]
    fn symbolic_executable_is_resolved_before_the_child_path_is_cleared() {
        let directory =
            std::env::temp_dir().join(format!("occurframe-resolution-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        #[cfg(windows)]
        let executable = directory.join("fixture.exe");
        #[cfg(not(windows))]
        let executable = directory.join("fixture");
        std::fs::write(&executable, b"fixture").unwrap();
        let search_path = std::env::join_paths([&directory]).unwrap();
        let path_extensions = OsString::from(".EXE");
        let (resolved, method) = resolve_executable(
            "fixture",
            Path::new("unused"),
            Some(&search_path),
            Some(&path_extensions),
        )
        .unwrap();
        #[cfg(windows)]
        assert!(
            resolved
                .to_string_lossy()
                .eq_ignore_ascii_case(&executable.to_string_lossy())
        );
        #[cfg(not(windows))]
        assert_eq!(resolved, executable);
        assert_eq!(method, LaunchResolutionMethod::SearchPath);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
