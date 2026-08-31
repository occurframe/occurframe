#!/usr/bin/env python3
"""Fast source-checkout smoke test for the protocol example.

The clean-room suite proves the same things about a packaged release, but it
needs certified evidence and a full assembly, so it cannot run on an ordinary
pull request. This script is the cheap version: it exercises the shipped example
runner end to end against a locally packed corpus, using binaries built from the
checkout, so a broken example or a broken protocol path fails in fast CI rather
than at release time.

The supported developer entry point prepares every required input before
invoking this internal harness:

    cargo run --locked -p xtask -- source-example-smoke --corpus ../corpus
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = REPOSITORY_ROOT / "examples" / "minimal-runner"
ENGINE = "example.minimal"
FAMILIES = ["cron.anchoring", "cron.invalid"]
EXPECTED_SELECTED = 16
EXPECTED_CONFORMANT = 2

failures = []


def check(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        failures.append(name)
        print(f"  FAIL {name}" + (f" -- {detail}" if detail else ""))


def main() -> int:
    parser = argparse.ArgumentParser(description="Smoke the Occurframe protocol example.")
    parser.add_argument("--occurframe", required=True)
    parser.add_argument("--oframe", required=True)
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--workspace")
    arguments = parser.parse_args()

    workspace = (
        Path(arguments.workspace)
        if arguments.workspace
        else Path(tempfile.mkdtemp(prefix="occurframe-example-smoke-"))
    )
    workspace.mkdir(parents=True, exist_ok=True)
    # The launch line names this interpreter explicitly, so the smoke test is
    # about the protocol rather than about whether `python3` is the interpreter's
    # name on this platform.
    registry_document = json.loads(
        (EXAMPLE / "runner-builds.example.json").read_text(encoding="utf-8")
    )
    registry_document["builds"][0]["launch"]["program"] = sys.executable
    registry = workspace / "runner-builds.json"
    registry.write_text(json.dumps(registry_document, indent=2), encoding="utf-8")

    environment = dict(os.environ)
    environment["OCCURFRAME_RUNNER_REGISTRY"] = str(registry)
    environment["OCCURFRAME_RUNNER_ROOT"] = str(EXAMPLE)

    def invoke(binary: str, fmt: str):
        command = [binary, "test", "--engine", ENGINE, "--corpus", arguments.corpus]
        for family in FAMILIES:
            command += ["--family", family]
        command += ["--format", fmt]
        return subprocess.run(
            command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False
        )

    print("protocol example smoke")
    results = {}
    for fmt in ("text", "json", "junit"):
        long_run = invoke(arguments.occurframe, fmt)
        short_run = invoke(arguments.oframe, fmt)
        results[fmt] = long_run
        check(
            f"{fmt} run succeeds",
            long_run.returncode == 0,
            long_run.stderr.decode(errors="replace")[:400],
        )
        check(f"aliases agree on {fmt} output", long_run.stdout == short_run.stdout)
        check(f"aliases agree on {fmt} exit code", long_run.returncode == short_run.returncode)
        check(f"{fmt} output is deterministic", invoke(arguments.occurframe, fmt).stdout == long_run.stdout)

    report = json.loads(results["json"].stdout.decode("utf-8"))
    summary = report["summary"]
    check("expected vector selection", summary["selected_vectors"] == EXPECTED_SELECTED, str(summary))
    check("expected conformant count", summary["conformant"] == EXPECTED_CONFORMANT, str(summary))
    check("unsupported cells are reported, not dropped", summary["unsupported"] == EXPECTED_SELECTED - EXPECTED_CONFORMANT, str(summary))
    check("no runner failure", summary["runner_failures"] == 0, str(summary))
    check("engine identity is recorded", report["engine"]["build_id"] == ENGINE)

    unknown = subprocess.run(
        [arguments.occurframe, "test", "--engine", "no-such-engine", "--corpus", arguments.corpus],
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    check("unknown engine is a usage error", unknown.returncode == 3, str(unknown.returncode))

    if failures:
        print("\nFailed checks:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("\nall example-runner smoke checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
