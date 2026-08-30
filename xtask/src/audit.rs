//! Absolute developer/CI path leak detection for packaged artifacts.
//!
//! A published release must be consumable by someone who has never seen the
//! machine that built it. Any absolute path belonging to a developer home
//! directory, a hosted CI workspace, or a build cache is both an information
//! leak and evidence that the artifact is not relocatable, so packaging fails
//! rather than shipping one.

use std::{fs, path::Path};

use serde::Serialize;
use walkdir::WalkDir;

/// Byte patterns that identify an absolute path belonging to the build machine.
///
/// These are matched against raw file bytes so that compiled binaries are
/// covered as well as text: without `--remap-path-prefix`, panic location
/// strings in a release binary embed the builder's `CARGO_HOME`.
const FORBIDDEN: &[&str] = &[
    // POSIX user and administrative home directories.
    "/home/",
    "/Users/",
    "/root/",
    // Windows user profiles, in both separator conventions, plus the extended
    // path prefix that tooling sometimes emits.
    "C:\\Users\\",
    "C:/Users/",
    "\\\\?\\C:\\Users\\",
    // Hosted CI workspaces. `/github/workspace` is the container action mount,
    // and `D:\a\` is the Windows runner's checkout drive.
    "/github/workspace",
    "D:\\a\\",
    "d:\\a\\",
    // Common macOS build scratch space.
    "/private/var/folders/",
];

/// One offending occurrence, reported with enough context to fix it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathLeak {
    pub file: String,
    pub pattern: String,
    pub occurrences: usize,
    /// A short, redacted excerpt around the first occurrence. Only bytes that
    /// are already present in the published artifact are echoed.
    pub sample: String,
}

/// Summary of one audit pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditReport {
    pub root: String,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub patterns: Vec<String>,
    pub leaks: Vec<PathLeak>,
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut index = 0;
    while index + needle.len() <= haystack.len() {
        if &haystack[index..index + needle.len()] == needle {
            hits.push(index);
            index += needle.len();
        } else {
            index += 1;
        }
    }
    hits
}

/// Render a short printable excerpt starting at `offset`.
fn sample(bytes: &[u8], offset: usize) -> String {
    const WINDOW: usize = 64;
    let end = bytes.len().min(offset + WINDOW);
    bytes[offset..end]
        .iter()
        .map(|byte| {
            let character = char::from(*byte);
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '.'
            }
        })
        .collect()
}

/// Scan every regular file below `root` for absolute build-machine paths.
///
/// `extra` adds caller-supplied literals, so a build can forbid its own
/// checkout and cache directories in addition to the generic patterns.
pub fn audit_paths(
    root: &Path,
    extra: &[String],
) -> Result<AuditReport, Box<dyn std::error::Error>> {
    let mut patterns: Vec<String> = FORBIDDEN
        .iter()
        .map(|pattern| (*pattern).to_owned())
        .collect();
    for value in extra {
        let trimmed = value.trim();
        // An empty or single-character literal would match everything.
        if trimmed.len() > 1 && !patterns.iter().any(|existing| existing == trimmed) {
            patterns.push(trimmed.to_owned());
        }
    }
    patterns.sort();

    let mut files_scanned = 0_usize;
    let mut bytes_scanned = 0_u64;
    let mut leaks = Vec::new();
    let mut entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .collect();
    entries.sort();

    for path in entries {
        let bytes = fs::read(&path)?;
        files_scanned += 1;
        bytes_scanned += bytes.len() as u64;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for pattern in &patterns {
            let hits = find_all(&bytes, pattern.as_bytes());
            if let Some(first) = hits.first() {
                leaks.push(PathLeak {
                    file: relative.clone(),
                    pattern: pattern.clone(),
                    occurrences: hits.len(),
                    sample: sample(&bytes, *first),
                });
            }
        }
    }
    leaks.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.pattern.cmp(&right.pattern))
    });

    Ok(AuditReport {
        root: root.to_string_lossy().replace('\\', "/"),
        files_scanned,
        bytes_scanned,
        patterns,
        leaks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_and_repeated_occurrences_are_counted_once_each() {
        assert_eq!(find_all(b"aXbXc", b"X"), vec![1, 3]);
        assert_eq!(find_all(b"abc", b"zzz"), Vec::<usize>::new());
        assert_eq!(find_all(b"abc", b""), Vec::<usize>::new());
    }

    #[test]
    fn samples_stay_printable_for_binary_input() {
        let rendered = sample(&[b'/', b'h', 0x00, 0xff, b'e'], 0);
        assert_eq!(rendered, "/h..e");
    }

    #[test]
    fn short_extra_literals_are_rejected_so_they_cannot_match_everything() {
        let directory = std::env::temp_dir().join(format!(
            "occurframe-audit-test-{}-{}",
            std::process::id(),
            "empty"
        ));
        fs::create_dir_all(&directory).expect("fixture directory");
        fs::write(directory.join("clean.txt"), b"nothing to see").expect("fixture file");
        let report = audit_paths(&directory, &["/".into(), String::new(), "/opt/x".into()])
            .expect("audit runs");
        assert!(report.leaks.is_empty());
        assert!(report.patterns.contains(&"/opt/x".to_owned()));
        assert!(!report.patterns.contains(&"/".to_owned()));
        fs::remove_dir_all(&directory).expect("remove isolated fixture directory");
    }

    #[test]
    fn a_planted_home_directory_path_is_detected() {
        let directory = std::env::temp_dir().join(format!(
            "occurframe-audit-leak-{}-{}",
            std::process::id(),
            "leak"
        ));
        fs::create_dir_all(&directory).expect("fixture directory");
        fs::write(
            directory.join("binary.bin"),
            b"\x7fELF...panicked at /home/runner/.cargo/registry/src/x.rs",
        )
        .expect("fixture file");
        let report = audit_paths(&directory, &[]).expect("audit runs");
        assert_eq!(report.leaks.len(), 1);
        assert_eq!(report.leaks[0].file, "binary.bin");
        assert_eq!(report.leaks[0].pattern, "/home/");
        fs::remove_dir_all(&directory).expect("remove isolated fixture directory");
    }
}
