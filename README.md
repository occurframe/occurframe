# Occurframe

> Occurframe is an executable conformance oracle for recurring-time semantics. It is not a scheduling engine.

Occurframe `0.1.0-rc1` tests an external recurrence implementation against the versioned RC2 conformance corpus:

```text
occurframe test --engine <configured-build-id>
oframe test --engine <configured-build-id>
```

A vector is a language-neutral recurring-time behavior case with classification, evidence, semantic context, and an authored expectation. Occurframe sends validated vectors to a protocol-v2 adapter, preserves the native engine output, and delegates scoring to the conformance layer. It does not calculate occurrences itself.

Engine version, declared dialect/profile, runtime, and timezone-database provenance are part of every result. Without those identities, a broad claim such as “cron-compatible” is not reproducible. Unsupported capabilities and infrastructure failures are reported separately from semantic disagreement.

Use `--format json` for deterministic automation output or `--format junit` for ordinary CI ingestion. Text is the default on a terminal; JSON is the default when stdout is redirected. See [CLI](docs/CLI.md) for options, engine discovery, exit codes, and third-party protocol-v2 adapters.

The separate [`occurframe/corpus`](https://github.com/occurframe/corpus) repository remains semantic authority. This repository contains Rust wire, conformance, runner, reporting, and shared CLI crates plus small external adapters. Generated observations, matrices, reports, packed corpus data, and release bundles are derived artifacts—not normative source.

Only `test` is implemented in this prerelease. `explain`, `classify`, and `occurrences` remain reserved because their frozen Research II meanings imply evaluator behavior that conflicts with **GO — ORACLE ONLY**. No scheduler or recurrence engine is present.

Start with [CLI](docs/CLI.md), [Architecture](docs/ARCHITECTURE.md), [Runner architecture](docs/RUNNER-ARCHITECTURE.md), [Differential certification](docs/DIFFERENTIAL-CERTIFICATION.md), or [Development](docs/DEVELOPMENT.md).
