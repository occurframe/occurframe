#!/usr/bin/env python3
"""Verify an Occurframe release the way a first-time consumer would.

This harness deliberately owns nothing but itself. It takes a packaged release,
moves it somewhere unrelated to any Occurframe checkout, and drives it from a
working directory that is unrelated to both. Everything it asserts has to hold
for a person who downloaded an archive and has no Cargo, no Rust, no Git and no
source tree on the machine.

Run it against a directory or a `.tar.gz`:

    python3 tests/clean-room/verify_release.py \\
        --bundle dist/release/occurframe-0.1.0-rc3 \\
        --target x86_64-unknown-linux-gnu

Exit status is 0 when every check passes and 1 otherwise; every check reports
its own line so a CI log shows which one failed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import xml.etree.ElementTree as ElementTree
from pathlib import Path

CORPUS_VERSION = "1.0.0-rc3"
CORPUS_CANONICAL_DIGEST = (
    "c0a9cf0587c02ce5022cbb94d060e14d5b9d6f99c3210e512965f35062c4dfe0"
)
CORPUS_VECTORS = 184
TOOL_VERSION = "0.1.0-rc3"
SPECIFICATION_VERSION = "1.0.0-rc1"
RUNNER_PROTOCOL_VERSION = "3.0"
# ERRATA-001: v1 ships one semantic command. These three require a recurrence
# evaluator the ORACLE ONLY verdict does not authorise, so they must behave as
# any other unknown word rather than as recognized-but-unavailable commands.
DEFERRED_COMMANDS = ["classify", "explain", "occurrences"]
EXAMPLE_ENGINE = "example.minimal"
EXAMPLE_FAMILIES = ["cron.anchoring", "cron.invalid"]
# The example fixture answers two vectors and reports everything else as
# unsupported; see examples/minimal-runner/fixtures.json.
EXPECTED_CONFORMANT = 2
EXPECTED_SELECTED = 16

REQUIRED_ENTRIES = [
    "bin",
    "corpus",
    "docs",
    "examples",
    "reports",
    "certification",
    "README.md",
    "VERSION",
    "SHA256SUMS",
    "LICENSES.md",
    "DEPENDENCIES.json",
    "THIRD-PARTY-NOTICES.md",
    "release-manifest.json",
]

# Absolute paths belonging to a build machine. Kept in step with
# `xtask/src/audit.rs`; this copy exists so the harness needs no Rust.
FORBIDDEN_PATH_PATTERNS = [
    b"/home/",
    b"/Users/",
    b"/root/",
    b"C:\\Users\\",
    b"C:/Users/",
    b"/github/workspace",
    b"D:\\a\\",
    b"d:\\a\\",
    b"/private/var/folders/",
]

FAILURES = []
CHECKS = 0


def check(name: str, condition: bool, detail: str = "") -> bool:
    global CHECKS
    CHECKS += 1
    if condition:
        print(f"  ok   {name}")
        return True
    FAILURES.append(name if not detail else f"{name}: {detail}")
    print(f"  FAIL {name}" + (f" -- {detail}" if detail else ""))
    return False


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def run(executable: Path, arguments, cwd: Path, env_overrides=None):
    """Run a release binary with a deliberately sparse environment.

    Only the variables a documented consumer would set are passed through, so a
    stray OCCURFRAME_* value in the CI environment cannot make a broken release
    look working.
    """
    environment = {
        key: value
        for key, value in os.environ.items()
        if key in ("PATH", "SYSTEMROOT", "SystemRoot", "COMSPEC", "TMP", "TEMP", "HOME", "USERPROFILE", "LANG", "LC_ALL", "PATHEXT")
    }
    environment.update(env_overrides or {})
    return subprocess.run(
        [str(executable), *arguments],
        cwd=str(cwd),
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def extract(bundle: Path, destination: Path) -> Path:
    """Place the release somewhere unrelated to any source checkout."""
    if bundle.is_dir():
        target = destination / bundle.name
        shutil.copytree(bundle, target)
        return target
    with tarfile.open(bundle, "r:gz") as archive:
        roots = {name.split("/")[0] for name in archive.getnames() if name.strip("./")}
        if len(roots) != 1:
            raise SystemExit(f"archive must contain exactly one root directory, found {sorted(roots)}")
        archive.extractall(destination)
        return destination / roots.pop()


def verify_checksums(root: Path) -> None:
    """Every file the release claims, at the digest it claims."""
    lines = (root / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
    mismatched = []
    missing = []
    for line in lines:
        if not line.strip():
            continue
        digest, _, relative = line.partition("  ")
        path = root / relative
        if not path.is_file():
            missing.append(relative)
        elif sha256_file(path) != digest:
            mismatched.append(relative)
    check("SHA256SUMS lists every packaged file", len(lines) > 0)
    check("SHA256SUMS references no missing file", not missing, ", ".join(missing[:3]))
    check("every packaged file matches its recorded digest", not mismatched, ", ".join(mismatched[:3]))

    listed = {line.partition("  ")[2] for line in lines if line.strip()}
    # Only the top-level SHA256SUMS is excluded: the corpus distribution has its
    # own SHA256SUMS, and that one is a packaged file like any other.
    on_disk = {
        str(path.relative_to(root)).replace("\\", "/")
        for path in root.rglob("*")
        if path.is_file()
    } - {"SHA256SUMS"}
    check("no unrecorded file is present in the release", listed == on_disk, ", ".join(sorted(on_disk - listed)[:3]))


def verify_corpus(root: Path) -> None:
    manifest = json.loads((root / "corpus" / "manifest.json").read_text(encoding="utf-8"))
    check("bundled corpus reports version " + CORPUS_VERSION, manifest["corpus_version"] == CORPUS_VERSION, str(manifest.get("corpus_version")))
    check(
        "bundled corpus reports the certified canonical digest",
        manifest["canonical_corpus_digest"] == CORPUS_CANONICAL_DIGEST,
        manifest.get("canonical_corpus_digest", ""),
    )
    records = sum(entry["records"] for entry in manifest["files"])
    check(f"bundled corpus contains {CORPUS_VECTORS} vectors", records == CORPUS_VECTORS, str(records))
    bad = [
        entry["path"]
        for entry in manifest["files"]
        if sha256_file(root / "corpus" / entry["path"]) != entry["sha256"]
    ]
    check("every corpus file matches the corpus manifest", not bad, ", ".join(bad[:3]))


def verify_manifest(root: Path, target: str) -> None:
    """The release must be able to describe itself, precisely."""
    manifest = json.loads((root / "release-manifest.json").read_text(encoding="utf-8"))

    check("manifest names the tooling version", manifest["tool_version"] == TOOL_VERSION, str(manifest.get("tool_version")))
    commit = manifest.get("tooling_commit_sha", "")
    check(
        "manifest records a full 40-character commit SHA",
        len(commit) == 40 and all(character in "0123456789abcdef" for character in commit),
        commit,
    )
    toolchain = manifest.get("toolchain", {})
    check("manifest records the Rust toolchain", bool(toolchain.get("rustc_version")) and bool(toolchain.get("host")), str(toolchain))
    check("manifest records this target triple", target in manifest.get("target_triples", []), str(manifest.get("target_triples")))
    check("manifest records the runner protocol version", manifest["runner_protocol_version"] == RUNNER_PROTOCOL_VERSION)
    check(
        "manifest records the specification version",
        manifest.get("specification_version") == SPECIFICATION_VERSION,
        str(manifest.get("specification_version")),
    )
    check(
        "manifest records the errata fixing the command doctrine",
        "ERRATA-001" in manifest.get("specification_errata", []),
        str(manifest.get("specification_errata")),
    )
    check(
        "manifest declares exactly one shipped semantic command",
        manifest.get("shipped_commands") == ["test"],
        str(manifest.get("shipped_commands")),
    )

    corpus = manifest.get("corpus", {})
    check("manifest records the corpus version", corpus.get("version") == CORPUS_VERSION)
    check("manifest records the corpus canonical digest", corpus.get("canonical_digest") == CORPUS_CANONICAL_DIGEST)
    check("manifest records the corpus vector count", corpus.get("vectors") == CORPUS_VECTORS)
    check("manifest records the corpus source revision", len(corpus.get("source_revision", "")) == 40)
    check("manifest records the corpus source method", corpus.get("source_revision_method") == "git_checkout")

    certification = manifest.get("certification", {})
    for field in ("artifact_name", "profile_version", "tooling_source_revision", "tooling_source_revision_method", "certification_manifest_sha256", "semantic_bundle_digest", "matrix_sha256"):
        check(f"manifest records certification {field}", bool(certification.get(field)), field)
    check(
        "manifest distinguishes attested tooling source provenance",
        certification.get("tooling_source_revision_method") == "attested_input",
        str(certification.get("tooling_source_revision_method")),
    )
    check(
        "manifest's certification manifest digest matches the packaged document",
        certification.get("certification_manifest_sha256")
        == sha256_file(root / "certification" / "certification-manifest.json"),
    )
    population = certification.get("population", {})
    check("manifest records the certified population", population.get("vectors") == CORPUS_VECTORS and population.get("observations", 0) > 0, str(population))
    check(
        "manifest records provenance-blocked builds explicitly",
        population.get("configured_builds", 0)
        == population.get("reproducible_builds", 0) + population.get("provenance_blocked_builds", -1),
        str(population),
    )

    # No wall-clock value may enter the artifact's identity.
    check("manifest records no build timestamp", "source_date_epoch" in manifest, "field absent")
    epoch = manifest["source_date_epoch"]
    check("source_date_epoch is null or an integer", epoch is None or isinstance(epoch, int), str(epoch))

    binaries = {entry["path"]: entry for entry in manifest.get("binaries", [])}
    check("manifest records every binary", len(binaries) == 8, str(len(binaries)))
    bad = [
        path
        for path, entry in binaries.items()
        if not (root / path).is_file() or sha256_file(root / path) != entry["sha256"]
    ]
    check("every binary matches its recorded digest", not bad, ", ".join(bad[:3]))
    check(
        "each binary records an alias and target",
        all(entry.get("alias") in ("occurframe", "oframe") and entry.get("target") for entry in binaries.values()),
    )

    inventories = {entry["path"]: entry["sha256"] for entry in manifest.get("inventories", [])}
    check("manifest records the dependency inventory digest", "DEPENDENCIES.json" in inventories)
    check("manifest records the third-party notices digest", "THIRD-PARTY-NOTICES.md" in inventories)
    for path, digest in inventories.items():
        check(f"{path} matches its recorded digest", sha256_file(root / path) == digest)

    licensing = manifest.get("licensing", {})
    for field in ("occurframe_code", "corpus_semantic_data", "third_party"):
        check(f"manifest states licensing for {field}", bool(licensing.get(field)), field)

    inventory = json.loads((root / "DEPENDENCIES.json").read_text(encoding="utf-8"))
    third_party = inventory["third_party"]
    check("dependency inventory is non-empty", len(third_party) > 0, str(len(third_party)))
    check("dependency inventory count matches its own entries", inventory["third_party_count"] == len(third_party))
    check(
        "dependency inventory is sorted deterministically",
        [(entry["name"], entry["version"]) for entry in third_party]
        == sorted((entry["name"], entry["version"]) for entry in third_party),
    )
    check(
        "every dependency records a version and relationship",
        all(entry.get("version") and entry.get("relationship") in ("direct", "transitive") for entry in third_party),
    )
    unexplained = [
        entry["name"]
        for entry in third_party
        if entry.get("license") is None and not entry.get("license_status")
    ]
    check(
        "no dependency has an unexplained missing license",
        not unexplained,
        ", ".join(unexplained[:5]),
    )
    # An absolute build path in the inventory would defeat the point of it.
    leaked = [entry["name"] for entry in third_party if "manifest_path" in entry]
    check("dependency inventory records no build-machine path", not leaked, ", ".join(leaked[:3]))
    check(
        "workspace crates are listed as first party, not as third-party dependencies",
        all(not entry["name"].startswith("occurframe-") for entry in third_party)
        and len(inventory["first_party"]) > 0,
    )


def verify_no_developer_paths(root: Path) -> None:
    leaks = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        data = path.read_bytes()
        for pattern in FORBIDDEN_PATH_PATTERNS:
            if pattern in data:
                leaks.append(f"{path.relative_to(root)} contains {pattern.decode('latin-1')}")
    check("release contains no absolute developer or CI path", not leaks, "; ".join(leaks[:3]))


def make_registry(
    source: Path,
    destination: Path,
    build_id: str,
    runtime_requirement: str,
    runtime_declared: str,
    program: str = None,
) -> Path:
    """Write a registry variant derived from the one the release ships.

    `program` defaults to the interpreter running this harness. The shipped
    example names `python3`, which is right on POSIX but is not guaranteed to
    exist under that name on Windows; naming the interpreter explicitly keeps
    the harness about the protocol rather than about interpreter naming. The
    shipped file is still exercised verbatim, separately, wherever `python3`
    resolves.
    """
    registry = json.loads(source.read_text(encoding="utf-8"))
    build = registry["builds"][0]
    build["build_id"] = build_id
    build["runtime_requirement"] = runtime_requirement
    build["launch"]["program"] = program or sys.executable
    build["launch"]["arguments"] = ["runner.py", "--runtime-version", runtime_declared]
    destination.write_text(json.dumps(registry, indent=2), encoding="utf-8")
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description="Clean-room verification of an Occurframe release.")
    parser.add_argument("--bundle", required=True, help="release directory or .tar.gz")
    parser.add_argument("--target", required=True, help="target triple of the binaries to exercise")
    parser.add_argument("--keep", action="store_true", help="keep the temporary clean room for inspection")
    arguments = parser.parse_args()

    workspace = Path(tempfile.mkdtemp(prefix="occurframe-clean-room-"))
    install_root = workspace / "install"
    install_root.mkdir()
    # The working directory is deliberately empty and unrelated to both the
    # release and any checkout: nothing may be resolved relative to it.
    neutral_cwd = workspace / "neutral"
    neutral_cwd.mkdir()
    scratch = workspace / "scratch"
    scratch.mkdir()

    try:
        root = extract(Path(arguments.bundle).resolve(), install_root)
        print(f"clean room: {root}")
        print(f"working directory: {neutral_cwd}")

        print("\n[1] release layout")
        for entry in REQUIRED_ENTRIES:
            check(f"release contains {entry}", (root / entry).exists())
        check(
            "VERSION names the specification",
            f"Specification:      {SPECIFICATION_VERSION}"
            in (root / "VERSION").read_text(encoding="utf-8"),
        )

        print("\n[2] packaged integrity")
        verify_checksums(root)

        print("\n[3] bundled corpus identity")
        verify_corpus(root)

        print("\n[4] no build-machine paths")
        verify_no_developer_paths(root)

        print("\n[4b] release manifest and inventories")
        verify_manifest(root, arguments.target)

        suffix = ".exe" if arguments.target.endswith("windows-msvc") else ""
        occurframe = root / "bin" / f"occurframe-{arguments.target}{suffix}"
        oframe = root / "bin" / f"oframe-{arguments.target}{suffix}"
        if not occurframe.is_file() or not oframe.is_file():
            print(f"  FAIL binaries for {arguments.target} are absent from the release")
            FAILURES.append("platform binaries absent")
            return 1
        if os.name != "nt":
            # Archive transports routinely drop the executable bit.
            for binary in (occurframe, oframe):
                binary.chmod(binary.stat().st_mode | 0o755)

        print("\n[5] both aliases start with no checkout present")
        version = run(occurframe, ["--version"], neutral_cwd)
        short_version = run(oframe, ["--version"], neutral_cwd)
        check("occurframe --version succeeds", version.returncode == 0, version.stderr.decode(errors="replace")[:200])
        check("oframe --version succeeds", short_version.returncode == 0, short_version.stderr.decode(errors="replace")[:200])
        check(f"version reports {TOOL_VERSION}", TOOL_VERSION.encode() in version.stdout, version.stdout.decode(errors="replace").strip())
        check("aliases report identical version output", version.stdout == short_version.stdout)
        long_help = run(occurframe, ["--help"], neutral_cwd)
        short_help = run(oframe, ["--help"], neutral_cwd)
        check("occurframe --help succeeds", long_help.returncode == 0)
        check("oframe --help succeeds", short_help.returncode == 0)
        check("aliases print identical help", long_help.stdout == short_help.stdout)
        check(
            "version reports the specification it scores against",
            f"specification {SPECIFICATION_VERSION}".encode() in version.stdout,
            version.stdout.decode(errors="replace").strip(),
        )
        check(
            "version reports the bundled corpus",
            f"corpus {CORPUS_VERSION}".encode() in version.stdout,
        )

        # ERRATA-001: exactly one semantic command ships, and the deferred ones
        # are neither implemented nor advertised.
        help_text = long_help.stdout.decode(errors="replace")
        check(
            "help advertises exactly one semantic command",
            "occurframe test --engine" in help_text
            and not any(command in help_text for command in DEFERRED_COMMANDS),
            help_text,
        )
        check(
            "help claims no recurrence engine",
            "computes no occurrence" in help_text and "not a scheduling engine" in help_text,
        )
        baseline = run(occurframe, ["definitely-not-a-command"], neutral_cwd)
        check("an unknown command is a usage error (exit 3)", baseline.returncode == 3, str(baseline.returncode))
        for command in DEFERRED_COMMANDS:
            deferred = run(occurframe, [command], neutral_cwd)
            short_deferred = run(oframe, [command], neutral_cwd)
            check(
                f"deferred command '{command}' is an ordinary usage error",
                deferred.returncode == baseline.returncode
                and f"unknown command '{command}'".encode() in deferred.stderr
                and b"reserved" not in deferred.stderr,
                deferred.stderr.decode(errors="replace")[:200],
            )
            check(
                f"aliases agree on deferred command '{command}'",
                deferred.stdout == short_deferred.stdout
                and deferred.stderr == short_deferred.stderr
                and deferred.returncode == short_deferred.returncode,
            )

        print("\n[6] external protocol-v3 runner")
        example_directory = root / "examples" / "minimal-runner"
        example_registry = example_directory / "runner-builds.example.json"
        check("release ships an example runner registry", example_registry.is_file())
        check("release ships the example runner itself", (example_directory / "runner.py").is_file())
        # The launch line names this harness's own interpreter, so the check is
        # about the protocol rather than about whether `python3` happens to be
        # the interpreter's name on this platform.
        portable_registry = make_registry(
            example_registry, scratch / "runner-builds.json", EXAMPLE_ENGINE, "example", "example"
        )
        registry_env = {
            "OCCURFRAME_RUNNER_REGISTRY": str(portable_registry),
            "OCCURFRAME_RUNNER_ROOT": str(example_directory),
        }
        arguments_common = ["test", "--engine", EXAMPLE_ENGINE]
        for family in EXAMPLE_FAMILIES:
            arguments_common += ["--family", family]

        outputs = {}
        for name, binary in (("occurframe", occurframe), ("oframe", oframe)):
            for fmt in ("text", "json", "junit"):
                outputs[(name, fmt)] = run(binary, arguments_common + ["--format", fmt], neutral_cwd, registry_env)

        for fmt in ("text", "json", "junit"):
            long_run = outputs[("occurframe", fmt)]
            short_run = outputs[("oframe", fmt)]
            check(
                f"{fmt} run succeeds through the bundled corpus and external runner",
                long_run.returncode == 0,
                long_run.stderr.decode(errors="replace")[:300],
            )
            check(f"aliases produce identical {fmt} output", long_run.stdout == short_run.stdout)
            check(f"aliases produce identical {fmt} exit code", long_run.returncode == short_run.returncode)

        print("\n[7] deterministic output")
        for fmt in ("json", "junit", "text"):
            again = run(occurframe, arguments_common + ["--format", fmt], neutral_cwd, registry_env)
            check(f"repeated {fmt} run is byte-identical", again.stdout == outputs[("occurframe", fmt)].stdout)

        print("\n[8] result content")
        report = json.loads(outputs[("occurframe", "json")].stdout.decode("utf-8"))
        check("JSON reports the tooling version", report["tooling_version"] == TOOL_VERSION)
        check(
            "JSON reports the specification version",
            report.get("specification_version") == SPECIFICATION_VERSION,
            str(report.get("specification_version")),
        )
        check("JSON reports runner protocol " + RUNNER_PROTOCOL_VERSION, report["runner_protocol_version"] == RUNNER_PROTOCOL_VERSION)
        check("JSON reports the certified corpus digest", report["corpus"]["canonical_digest"] == CORPUS_CANONICAL_DIGEST)
        check("JSON records engine identity", report["engine"]["build_id"] == EXAMPLE_ENGINE)
        check("JSON records tzdb provenance", "tzdb_provenance" in report["engine"])
        summary = report["summary"]
        check(f"{EXPECTED_SELECTED} vectors were selected", summary["selected_vectors"] == EXPECTED_SELECTED, str(summary["selected_vectors"]))
        check(f"{EXPECTED_CONFORMANT} vectors are conformant", summary["conformant"] == EXPECTED_CONFORMANT, str(summary["conformant"]))
        check("unsupported cells are reported separately", summary["unsupported"] == EXPECTED_SELECTED - EXPECTED_CONFORMANT, str(summary["unsupported"]))
        check("no runner failure occurred", summary["runner_failures"] == 0)

        text = outputs[("occurframe", "text")].stdout.decode("utf-8")
        check("text output states the corpus digest", CORPUS_CANONICAL_DIGEST in text)
        check(
            "text output states the specification version",
            f"specification: {SPECIFICATION_VERSION}" in text,
        )
        check("text output separates unsupported from disagreement", "unsupported:" in text and "non-conformant:" in text)

        junit = ElementTree.fromstring(outputs[("occurframe", "junit")].stdout.decode("utf-8"))
        check("JUnit root is a testsuite", junit.tag == "testsuite")
        check(f"JUnit reports {EXPECTED_SELECTED} tests", junit.get("tests") == str(EXPECTED_SELECTED), str(junit.get("tests")))
        check("JUnit reports no failure", junit.get("failures") == "0", str(junit.get("failures")))
        check("JUnit reports no error", junit.get("errors") == "0", str(junit.get("errors")))
        check(
            "JUnit skips unsupported cells",
            junit.get("skipped") == str(EXPECTED_SELECTED - EXPECTED_CONFORMANT),
            str(junit.get("skipped")),
        )

        print("\n[9] error domains are distinct")
        unknown = run(occurframe, ["test", "--engine", "no-such-engine"], neutral_cwd, registry_env)
        check("unknown engine is a usage error (exit 3)", unknown.returncode == 3, str(unknown.returncode))
        check("unknown engine names the configured IDs", EXAMPLE_ENGINE.encode() in unknown.stderr)

        mismatched = make_registry(
            example_registry,
            scratch / "identity-mismatch.json",
            "example.minimal.mismatch",
            "a-version-the-runner-will-not-report",
            "example",
        )
        identity = run(
            occurframe,
            ["test", "--engine", "example.minimal.mismatch", "--family", "cron.anchoring"],
            neutral_cwd,
            {
                "OCCURFRAME_RUNNER_REGISTRY": str(mismatched),
                "OCCURFRAME_RUNNER_ROOT": str(root / "examples" / "minimal-runner"),
            },
        )
        check(
            "runner identity/provenance failure is an environment failure (exit 4)",
            identity.returncode == 4,
            str(identity.returncode),
        )

        print("\n[10] registries outside the release are accepted")
        # No OCCURFRAME_RUNNER_ROOT here: the runner root must be derived from
        # the registry's own location, so a registry that lives nowhere near the
        # release still resolves its relative launch paths.
        external = scratch / "external-registry"
        external.mkdir()
        shutil.copy(example_directory / "runner.py", external / "runner.py")
        shutil.copy(example_directory / "fixtures.json", external / "fixtures.json")
        external_registry = make_registry(
            example_registry, external / "runner-builds.json", EXAMPLE_ENGINE, "example", "example"
        )
        detached = run(
            occurframe,
            arguments_common + ["--format", "json"],
            neutral_cwd,
            {"OCCURFRAME_RUNNER_REGISTRY": str(external_registry)},
        )
        check(
            "a registry outside the release resolves its own launch paths",
            detached.returncode == 0,
            detached.stderr.decode(errors="replace")[:300],
        )
        check(
            "an external registry produces the same result as the bundled one",
            detached.stdout == outputs[("occurframe", "json")].stdout,
        )

        print("\n[11] the shipped example registry, used exactly as documented")
        shipped_python = shutil.which("python3")
        try:
            shipped_python_works = shipped_python is not None and subprocess.run(
                [shipped_python, "--version"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            ).returncode == 0
        except OSError:
            shipped_python_works = False
        if not shipped_python_works:
            print("  skip python3 is not executable under that name on this platform")
        else:
            # Documented path: set one variable, run. No OCCURFRAME_RUNNER_ROOT,
            # so the bundled registry's own location must supply the base.
            shipped = run(
                occurframe,
                arguments_common + ["--format", "json"],
                neutral_cwd,
                {"OCCURFRAME_RUNNER_REGISTRY": str(example_registry)},
            )
            check(
                "the shipped example registry works with only OCCURFRAME_RUNNER_REGISTRY set",
                shipped.returncode == 0,
                shipped.stderr.decode(errors="replace")[:300],
            )
            check(
                "the shipped registry produces the same result",
                shipped.stdout == outputs[("occurframe", "json")].stdout,
            )

        print(f"\n{CHECKS - len(FAILURES)}/{CHECKS} checks passed")
        if FAILURES:
            print("\nFailed checks:")
            for failure in FAILURES:
                print(f"  - {failure}")
            return 1
        return 0
    finally:
        if arguments.keep:
            print(f"clean room kept at {workspace}")
        else:
            shutil.rmtree(workspace, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
