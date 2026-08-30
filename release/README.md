# Occurframe 0.1.0-rc1

**Occurframe is an executable conformance oracle for recurring-time semantics.
It is not a scheduling engine.**

This is a **prerelease**. It is not Occurframe v1.0.

```text
Occurframe tooling: 0.1.0-rc1
Corpus:             1.0.0-rc2
Runner protocol:    2.0
```

## Start here

1. Verify the archive checksum, then verify `SHA256SUMS` inside it.
2. Run `bin/occurframe-<your-target> --version`.
3. Read `docs/GETTING-STARTED.md`.

```sh
export OCCURFRAME_RUNNER_REGISTRY="$PWD/examples/minimal-runner/runner-builds.example.json"
./bin/occurframe-x86_64-unknown-linux-gnu test \
    --engine example.minimal --family cron.anchoring --family cron.invalid
```

That runs the bundled protocol example — a protocol example only, not a
recurrence engine and not a conformance reference — end to end against the
bundled corpus. To test a real engine, write a runner:
`docs/WRITING-A-RUNNER.md`.

## What is in here

```text
bin/            occurframe and oframe for four targets; two names, one program
corpus/         the generated RC2 corpus this release is pinned to
docs/           the full public documentation set
examples/       runnable protocol example and an example registry
adapters/       engine identity registry (no engine runtimes are bundled)
reports/        candidate public differential report and cell-level matrix
certification/  certification manifest and the observed certification environment
VERSION         the three version identities and the released commit
release-manifest.json   machine-readable release identity and provenance
DEPENDENCIES.json / THIRD-PARTY-NOTICES.md   what the binaries link, and under what licence
LICENSES.md     which licence covers which part of this release
SHA256SUMS      digest of every packaged file
```

Nothing here resolves relative to your working directory. Extract the release
anywhere and run it from anywhere; the bundled corpus is found from the location
of the executable. `--corpus` overrides it, and `OCCURFRAME_RUNNER_REGISTRY` /
`OCCURFRAME_RUNNER_ROOT` accept paths anywhere on the filesystem.

## The evidence in `reports/`

`reports/differential-report.md` describes the certified RC2 differential run and
`reports/matrix.json` retains every cell. The population is 25 configured
historical engine builds, 23 of them reproducible, over 184 vectors for 4,232
observations.

Read `docs/INTERPRETING-RESULTS.md` before drawing conclusions. In particular,
163 semantically divergent vectors is **not** 163 engine defects, unsupported
cells and errors and timeouts are excluded from semantic-answer grouping, and no
engine ranking is published or derivable.

## Scope of this prerelease

Only `test` is implemented. `explain`, `classify` and `occurrences` remain
reserved and return a usage error, because their frozen meanings imply evaluator
behaviour that conflicts with the no-engine boundary. See
`docs/KNOWN-CONTRADICTIONS.md`.

Occurframe's own code is `Apache-2.0 OR MIT`; the corpus's authored semantic data
is `CC0-1.0`. See `LICENSES.md`.
