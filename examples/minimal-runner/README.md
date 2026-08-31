# Minimal protocol-v3 runner

**Protocol example only — not a recurrence engine and not a conformance
reference.**

This directory is the smallest complete demonstration of Occurframe's runner
protocol `3.0`. It exists so that an engine maintainer can watch the whole
handshake — `hello`, `case`, `started`, `result` — happen against the real
`occurframe test` command before wiring up an actual engine.

`runner.py` computes nothing. It replays a two-entry table of fixed answers
from `fixtures.json` and reports `unsupported` for every other vector. Its
results are not measurements, are not evidence, and must never be quoted in a
compatibility or conformance claim about any implementation. It is written in
Python only because a Python 3 interpreter is already present on every hosted CI
image; the protocol is line-oriented JSON over stdin/stdout and is equally
implementable in any language that can read a line and write a line.

## Run it

The example needs no build step and no third-party packages — only `python3` on
`PATH` and an extracted Occurframe release (or a source checkout).

```sh
export OCCURFRAME_RUNNER_REGISTRY=/path/to/examples/minimal-runner/runner-builds.example.json

occurframe test --engine example.minimal --family cron.anchoring --family cron.invalid
oframe    test --engine example.minimal --family cron.anchoring --family cron.invalid --format json
```

Expected result: two conformant vectors and fourteen unsupported ones, exit
code `0`.

* `CRON-ANCH-001` answers `occurrences`. The fixture replays the authored RC3
  expectation for the `exclusive` start-inclusivity policy, which is the policy
  this example declares in its `hello`, so the scorer records it as conformant.
* `CRON-INV-001` answers `rejection`, matching an authored `reject` expectation.
* Everything else answers `unsupported`, which is scored as unsupported rather
  than as agreement or disagreement.

Nothing above requires `OCCURFRAME_RUNNER_ROOT`. Relative paths in a registry's
`launch` block are resolved against the registry's own directory, so a registry
may live anywhere on the filesystem — inside the release, beside your engine, or
in a scratch directory. Set `OCCURFRAME_RUNNER_ROOT` only when you deliberately
want the launch paths interpreted against some other base:

```sh
export OCCURFRAME_RUNNER_ROOT=/srv/engines/my-engine
```

To run against a corpus other than the one bundled with the release, pass
`--corpus /path/to/corpus` or set `OCCURFRAME_CORPUS`. Either way the corpus must
still match the RC3 identity the tooling pins.

## What the registry entry has to line up with

Occurframe refuses to attribute an observation to a build whose `hello`
disagrees with the registry entry that launched it. The check is exact, and it
covers list order:

| Registry field | `hello` field |
| --- | --- |
| `protocol_version` | `protocol_version` |
| `runner` | `runner` (name, version, provenance) |
| `engine` | `engine` (name, version, provenance) |
| `language`, `runtime_name`, `runtime_requirement` | `runtime` |
| `supported_operations` | `capabilities` |
| `dialect_ids` | `dialect_ids` |
| `semantic_profile_claims` | `semantic_profile_claims` |
| `tzdb_source` / `additional_tzdb_sources` | `tzdb_provenance.source` |
| `allowed_tzdb_release_kinds` | `tzdb_provenance.release_kind` |

A mismatch is an environment failure (exit `4`), not a semantic disagreement.
You can see that for yourself by changing `runtime_requirement` in a copy of the
registry and rerunning: the run fails as infrastructure, and no observation is
recorded against the engine.

## The one thing this example does that a real runner must not

`runtime_requirement` here is the literal string `example`, and the launch line
passes `--runtime-version example` so the runner declares that same value. That
makes the example run unchanged on any Python 3 interpreter, which is the point
of an example.

A real integration must not do this. The runtime version is part of the build's
identity: a configuration pinning CPython 3.11.15 is not satisfied by CPython
3.13.5, and recording an observed version without enforcing it would let an
unpinned interpreter silently produce evidence attributed to a pinned one. A
real runner reports the interpreter or runtime it is actually executing on, and
the registry pins that exact version:

```sh
python3 -c 'import platform; print(platform.python_version())'
```

Put that value in `runtime_requirement`, drop the `--runtime-version` argument
from the launch line, and the identity check then means something.

## Writing your own

`docs/WRITING-A-RUNNER.md` is the normative description of protocol `3.0`:
message shapes, the five terminal outcomes and when each one is correct, the
identity and provenance rules, and the budget and watchdog boundaries. Read that
before adapting this file. `runner.py` is deliberately short enough to read in
one sitting and is commented at each protocol decision point.
