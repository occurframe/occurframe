#!/usr/bin/env python3
"""Occurframe runner protocol 3.0 — minimal integration example.

PROTOCOL EXAMPLE ONLY — NOT A RECURRENCE ENGINE AND NOT A CONFORMANCE
REFERENCE. This program computes nothing. It replays a tiny, documented table
of fixed answers (``fixtures.json``) so that an engine maintainer can see the
exact NDJSON traffic Occurframe expects, end to end, without first standing up
a real engine. Nothing it emits is evidence about any implementation's
correctness, and it must never be cited in a conformance claim.

The wire contract, in full:

  * The runner writes exactly one ``hello`` line to stdout at startup, before
    reading anything.
  * Occurframe writes one ``case`` line per vector to the runner's stdin.
  * For each ``case`` the runner writes exactly one ``started`` line carrying
    the same ``request_id``, then performs the native operation, then writes
    exactly one terminal ``result`` line carrying that same ``request_id``.
  * Every message is one line of UTF-8 JSON on stdout, flushed immediately.
    stdout carries protocol traffic and nothing else; logging goes to stderr.
  * A ``result`` outcome is exactly one of five kinds: ``occurrences``,
    ``accepted``, ``rejection``, ``unsupported``, ``engine_error``.

Read ``docs/WRITING-A-RUNNER.md`` for the normative description.
"""

from __future__ import annotations

import argparse
import json
import platform
import sys
from pathlib import Path
from typing import Any, Dict

PROTOCOL_VERSION = "3.0"

# The identity below is asserted to Occurframe in `hello`, and Occurframe
# refuses to attribute any observation to a build whose `hello` disagrees with
# the registry entry that launched it. Runner identity, engine identity and
# provenance, runtime language/name/version, capability list, dialect IDs,
# semantic profile claims and timezone-database provenance must all match
# exactly, including list order. Keep this block and
# `runner-builds.example.json` in step.
RUNNER_IDENTITY = {
    "name": "occurframe-minimal-example-runner",
    "version": "3.0.0",
    "provenance": "source:examples/minimal-runner/runner.py",
}
ENGINE_IDENTITY = {
    "name": "occurframe-example-fixture",
    "version": "0.0.0-example",
    "provenance": (
        "not a recurrence engine: fixed answers replayed from "
        "examples/minimal-runner/fixtures.json"
    ),
}
CAPABILITIES = ["cron.next", "cron.parse"]
DIALECT_IDS = ["cron.vixie@1"]
SEMANTIC_PROFILE_CLAIMS = {"cron.start_inclusivity": "exclusive"}
# This example resolves no timezones at all, so it declares a named, obviously
# non-real provenance rather than implying a timezone-database release it never
# consulted. A real runner reports the release its engine actually used.
TZDB_PROVENANCE = {
    "source": "example fixture (no timezone database is consulted)",
    "release_kind": "exact",
    "release": "none",
}


def emit(message: Dict[str, Any]) -> None:
    """Write one protocol message as a single flushed NDJSON line."""
    sys.stdout.write(json.dumps(message, separators=(",", ":"), sort_keys=True))
    sys.stdout.write("\n")
    sys.stdout.flush()


def hello(runtime_version: str) -> Dict[str, Any]:
    return {
        "message": "hello",
        "protocol_version": PROTOCOL_VERSION,
        "runner": RUNNER_IDENTITY,
        "engine": ENGINE_IDENTITY,
        "runtime": {
            "language": "Python",
            "runtime": "CPython",
            "version": runtime_version,
        },
        "capabilities": CAPABILITIES,
        "dialect_ids": DIALECT_IDS,
        "semantic_profile_claims": SEMANTIC_PROFILE_CLAIMS,
        "tzdb_provenance": TZDB_PROVENANCE,
    }


def diagnostic(code: str, message: str) -> Dict[str, str]:
    return {"code": code, "message": message}


def outcome_for(vector: Dict[str, Any], fixtures: Dict[str, Any]) -> Dict[str, Any]:
    """Look one vector up in the fixture table.

    A real runner calls its engine here and translates the engine's native
    behaviour into exactly one of the five outcomes. Anything the engine cannot
    express is `unsupported`; a deliberate, engine-defined refusal of the input
    is `rejection`; an unexpected crash or contract violation inside the engine
    is `engine_error`. Never guess: `unsupported` and `rejection` mean different
    things to the scorer and are never interchangeable.
    """
    entry = fixtures.get(vector.get("id"))
    if entry is None:
        return {
            "type": "unsupported",
            "diagnostic": diagnostic(
                "outside_example_fixture_subset",
                "this protocol example only answers the documented fixture subset; "
                "it computes nothing and claims nothing about this vector",
            ),
        }
    kind = entry["outcome"]
    if kind == "occurrences":
        return {"type": "occurrences", "occurrences": entry["occurrences"]}
    if kind == "accepted":
        return {"type": "accepted"}
    return {
        "type": kind,
        "diagnostic": diagnostic(entry["code"], entry["message"]),
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Occurframe protocol-v3 integration example (not an engine)."
    )
    parser.add_argument(
        "--fixtures",
        default=str(Path(__file__).resolve().parent / "fixtures.json"),
        help="fixture table; resolved beside this script by default",
    )
    parser.add_argument(
        "--runtime-version",
        default=platform.python_version(),
        help=(
            "runtime version to declare in hello. Defaults to the running "
            "interpreter. Override it to demonstrate Occurframe rejecting a "
            "build whose observed runtime differs from its configuration."
        ),
    )
    arguments = parser.parse_args()

    with open(arguments.fixtures, "r", encoding="utf-8") as handle:
        fixtures = json.load(handle)["vectors"]

    emit(hello(arguments.runtime_version))

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            print(f"minimal-runner: unreadable input line: {error}", file=sys.stderr)
            return 1
        if message.get("message") != "case":
            print(
                f"minimal-runner: unexpected inbound message {message.get('message')!r}",
                file=sys.stderr,
            )
            return 1

        request_id = message["request_id"]
        # `started` is the attributable acknowledgement: it tells Occurframe the
        # case was received and the engine budget, rather than the startup
        # watchdog, now governs. Emit it before doing any work.
        emit(
            {
                "message": "started",
                "protocol_version": PROTOCOL_VERSION,
                "request_id": request_id,
            }
        )
        emit(
            {
                "message": "result",
                "protocol_version": PROTOCOL_VERSION,
                "request_id": request_id,
                "outcome": outcome_for(
                    {
                        "id": message["vector_id"],
                        "family": message["family"],
                        "operation": message["operation"],
                        "input": message["input"],
                        "context": message["semantic_context"],
                    },
                    fixtures,
                ),
                "warnings": [],
            }
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
