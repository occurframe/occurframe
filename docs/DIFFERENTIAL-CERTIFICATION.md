# Differential certification

## Purpose and authority

The RC2 differential certification measures the behavior of the preserved
Phase II engine builds through runner protocol 2.0. It produces the candidate
evidence base for Occurframe v1. It does not create semantic authority.

The authority and evidence chain is:

```text
RC2 specification, registries, vectors
        -> validated vector
        -> protocol-v2 external runner
        -> normalized observation (evidence)
        -> occurframe-conformance verdict
        -> occurframe-report derived views
```

Only the corpus can define expected behavior. An engine majority cannot alter
an expectation, and a report cannot rescore a cell.

## Canonical environment

The canonical environment is Ubuntu 24.04 x86_64, materialized by
`certification/docker/Dockerfile`. Its measured runtimes are pinned to the
versions in `runners/registry/runner-builds.json`: CPython 3.11.15, Bun 1.3.13,
Go 1.24.7, PHP 8.4.21, and MRI 3.3.6. System zoneinfo is built from IANA tzdb
2026a in rearguard form.

The platform label is not treated as sufficient provenance. Every run records
the actual image ID, base image digest where available, OS release,
architecture, kernel, runtime versions, system-zoneinfo digest, package tzdb,
runtime ICU fingerprint, installed engine commits, and dependency inventories
in `environment.json`. Exact, bounded, and unknown tzdb claims remain distinct.

The image is built with network access. Execution uses `--network none` after
all dependencies and Cargo/Go caches have been prepared.

## Population and completeness

No vector is prefiltered. Every build declared reproducible before execution
receives all 184 vectors, including operations it cannot perform. Missing
capabilities yield explicit `unsupported` observations.

For the current profile, 23 builds are reproducible and two Ruby builds are
blocked, so completeness requires exactly 4,232 unique build/vector cells.
Every reproducible build must have 184 terminal observations. Duplicates,
missing cells, schema-invalid observations, identity mismatches, and
`runner_failure` fail the certification infrastructure. Legitimate rejection,
unsupported, engine error, timeout, or non-conformance remains measured
evidence and does not fail infrastructure.

## Outcomes, scoring, and matrices

The canonical `matrix.json` retains the execution status, native outcome,
conformance verdict, matched expected case, warnings, semantic-answer
identifier, and provenance reference for every cell. `matrix.csv` is a
convenience projection with the same pathology distinctions.

Scoring is performed once by `occurframe-conformance`. The report crate does
not duplicate expectation selection. In particular:

- `unsupported` is not disagreement;
- `runner_failure` is infrastructure failure;
- `timeout` is not rejection;
- `engine_error` cannot satisfy an expected rejection;
- policy/dialect cases are selected only from declared profiles;
- occurrence arrays retain native order and duplicates.

## Semantic divergence

Semantic answer groups include only occurrence arrays, accepted parses, and
deliberate rejections. Unsupported, engine errors, timeouts, and runner
failures are grouped as execution pathologies and never increase the distinct
semantic-answer count.

Each multi-answer vector is described using its authored classification and
registered axes. Derived categories distinguish normative violations,
documented policy differences, documented dialect differences, standard
ambiguity, known implementation divergence, and tzdb-provenance differences.
A multi-answer vector is not automatically called a defect, and engines are
not assigned an overall rank.

## Phase II reconciliation

Phase II RC1 remains immutable historical evidence. The authored
`certification/phase2-build-map.json` maps only exact engine/configuration
identities. Reconciliation categories are:

- `exact_match`
- `protocol_refinement`
- `tzdb_provenance_change`
- `runtime_environment_change`
- `adapter_correction` (requires authored concrete evidence)
- `fresh_engine_behavior_difference`
- `legacy_ambiguous_error`
- `unresolved_difference`

RC1's 814 generic `error` cells remain explicit ambiguity unless cell-specific
evidence supports a narrower statement. The new certification does not need to
reproduce Research II's `157/184` headline: that number measured a particular
Phase II engine/environment set, not an acceptance threshold.

## Determinism and artifacts

The complete execution runs twice in the same prepared image. These files must
be byte-identical and have identical SHA-256 digests:

```text
observations.ndjson
conformance-results.ndjson
matrix.json
matrix.csv
differential-report.md
reconciliation.json
reconciliation.md
```

The native occurrence sequence is never sorted to manufacture stability.
Volatile environment/container wrapper metadata is retained in
`environment.json` but excluded from the semantic comparison.

The canonical bundle is generated under `dist/certification/rc2/` with a
manifest and `SHA256SUMS`. It is uploaded by the dedicated Full Differential
Certification workflow and is not routinely committed.

## Ruby status

`ruby.fugit` and `ruby.ice_cube` remain
`unreproducible_provenance`. The bounded investigation is recorded in
`certification/RUBY-PROVENANCE.md`. No contemporary concurrent-ruby version,
newer engine, or alternate Ruby runtime may be substituted merely to fill the
two missing build rows.
