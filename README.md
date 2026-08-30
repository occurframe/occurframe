# Occurframe

> Occurframe is an executable conformance oracle for recurring-time semantics. It is not a scheduling engine.

Occurframe v1 is **GO — ORACLE ONLY**. It consumes versioned, language-neutral conformance vectors and normalized runner observations, then reports whether an external implementation is normative, admissible, unsupported, erroneous, timed out, or infrastructure-failed.

This repository contains Rust authority tooling and wire types. The separate [`occurframe/corpus`](https://github.com/occurframe/corpus) repository owns the specification, schemas, registries, vectors, independent Python reference matcher, and historical evidence.

The public commands reserved by Research II—`occurframe test`, `explain`, `classify`, and `occurrences`, plus the `oframe` executable alias—are not implemented in this milestone. `xtask` is developer tooling, not a public Occurframe CLI.

No recurrence evaluator, scheduler, runner adapter, execution runtime, or engine crate exists here. See [Architecture](docs/ARCHITECTURE.md) and [Development](docs/DEVELOPMENT.md).

