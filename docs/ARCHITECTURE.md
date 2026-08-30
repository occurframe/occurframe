# Architecture

## Authority boundary

The `occurframe/corpus` repository owns authored, language-neutral authority: specification, schemas, registries, canonical vectors, and the independent Python reference matcher. This repository owns Rust types and deterministic authority operations. A tooling checkout never makes its bundled test data normative, and generated observations, packs, matrices, and reports never override authored corpus source.

## Crate ownership

- `occurframe-wire` owns typed wire models: vectors, classifications, expectations, versioned dialect IDs, structured tzdb provenance, protocol-v2 messages, normalized observations, diagnostics, and verdicts. It performs no evaluation or scoring.
- `occurframe-conformance` owns corpus/schema/registry validation, stable-ID checks, RC1-to-RC2 verification, canonical serialization, release packing, observation normalization, expectation selection, and scoring. It contains no engine-specific logic.
- `occurframe-runner` owns configured discovery, child-process containment, protocol-v2 NDJSON, identity validation, separate infrastructure and engine watchdogs, restart, bounded diagnostics, observation construction, batch execution, and deterministic observation output. It contains no recurrence or expectation semantics.
- `xtask` exposes developer-only validation, migration, packing, manifest, determinism, runner-contract, and adapter-smoke commands. It is not a public product CLI.

## No-engine gate

Cron remains a predicate-shaped semantic family. RRULE remains an anchor/generator-shaped semantic family. The workspace does not unify them into a recurrence AST and does not parse, expand, resolve, or execute recurrence rules. The Research II engine reopening gate remains closed.

## Runner boundary

External adapters speak NDJSON protocol v2. A `started` acknowledgement marks the instant after which the authority may apply the 8,000 ms engine budget. The adapter reports one of five engine outcomes; the supervising authority reports timeout or runner failure separately. Adapters never score themselves.

Production adapters live under `runners/`, not in the corpus. The copies under `corpus/legacy/phase2-rc1` remain historical evidence. One launched process represents one immutable engine/configuration identity from `runners/registry/runner-builds.json`.

## Python reference boundary

`reference/cron_ref.py` remains in the corpus repository as an independent scoring/reference artifact. It is not rewritten in Rust and is never called by Rust product runtime. Its implementation diversity is intentional.
