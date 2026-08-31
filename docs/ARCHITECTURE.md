# Architecture

## Authority boundary

The `occurframe/corpus` repository owns authored, language-neutral authority: specification, schemas, registries, canonical vectors, and the independent Python reference matcher. This repository owns Rust types and deterministic authority operations. A tooling checkout never makes its bundled test data normative, and generated observations, packs, matrices, and reports never override authored corpus source.

## Crate ownership

- `occurframe-wire` owns distinct authority vectors and expectation-blind protocol-v3 case projections, classifications, expectations, versioned dialect IDs, structured tzdb and runner-environment provenance, normalized observations, diagnostics, and verdicts. It performs no evaluation or scoring.
- `occurframe-conformance` owns corpus/schema/registry validation, stable-ID checks, RC1-to-RC2 verification, canonical serialization, release packing, observation normalization, expectation selection, and scoring. It contains no engine-specific logic.
- `occurframe-runner` owns configured discovery, hermetic protocol-v3 subprocess launch, NDJSON, identity validation, separate infrastructure and engine watchdogs, restart, bounded diagnostics, observation construction, batch execution, and deterministic observation output. It contains no recurrence or expectation semantics.
- `occurframe-report` owns derived observation summaries, provenance tables, semantic answer groups, matrices, execution-pathology summaries, and Phase II reconciliation. It consumes immutable observations and conformance-owned verdicts; it does not score or evaluate recurrence.
- `occurframe-cli` owns the shared argument parser, engine/corpus discovery, CLI report model, deterministic JSON/text/JUnit rendering, and exit-code policy for both executable aliases. It delegates execution and scoring and contains no recurrence semantics.
- `xtask` exposes developer-only validation, migration, packing, manifest, determinism, runner-contract, adapter-smoke, full-certification, and release-packaging commands. It is not a public product CLI.

## No-engine gate

Cron remains a predicate-shaped semantic family. RRULE remains an anchor/generator-shaped semantic family. The workspace does not unify them into a recurrence AST and does not parse, expand, resolve, or execute recurrence rules. The Research II engine reopening gate remains closed.

The implemented `test` command invokes only external protocol-v3 runners. It is
the entire v1 semantic surface. `explain`, `classify` and `occurrences` each
require an Occurframe-owned evaluator, so under ERRATA-001 they are deferred
behind the engine gate rather than implemented, delegated or renamed. The
CLI-versus-engine contradiction is resolved in favour of the verdict; nothing in
the certification tooling supplies a hidden evaluator.

## Runner boundary

External adapters speak expectation-blind NDJSON protocol v3. A `started` acknowledgement marks the instant after which the authority may apply the 8,000 ms engine budget. The adapter reports one of five engine outcomes; the supervising authority reports timeout or runner failure separately. Adapters never receive expected answers and never score themselves.

Production adapters live under `runners/`, not in the corpus. The copies under `corpus/legacy/phase2-rc1` remain historical evidence. One launched process represents one immutable engine/configuration identity from `runners/registry/runner-builds.json`.

## Public CLI flow

```text
occurframe/oframe shared parser
        -> digest-locked validated RC3 corpus
        -> authority-owned CaseProjection
        -> configured protocol-v3 runner in a hermetic environment
        -> normalized observation
        -> occurframe-conformance scorer
        -> CLI text / deterministic JSON / JUnit projection
```

Both executable aliases call `occurframe_cli::run()` and cannot acquire separate parsers or command trees. A release carries the generated corpus distribution, but the authored corpus repository remains its source of truth.

## Certification evidence boundary

The authored profile under `certification/` pins the measurement population and
canonical Ubuntu container. `occurframe-runner` produces observations,
`occurframe-conformance` produces verdicts, and `occurframe-report` produces
derived views. The flow is one-way: no observation, consensus, matrix, or
reconciliation category can amend a vector expectation.

The successor profile targets bundles under `dist/certification/rc3/`; no such
authoritative bundle exists until the subsequent recertification task. Existing
RC2 evidence remains immutable. Phase II RC1 is
compared only through the authored exact-identity map; its overloaded `error`
cells remain historical ambiguity rather than being guessed into protocol-v3
outcomes.

## Python reference boundary

`reference/cron_ref.py` remains in the corpus repository as an independent scoring/reference artifact. It is not rewritten in Rust and is never called by Rust product runtime. Its implementation diversity is intentional.
