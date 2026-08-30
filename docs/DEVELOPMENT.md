# Development

Use a Rust toolchain supporting edition 2024. The ordinary correctness gate is:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Corpus operations are developer-only `xtask` commands:

```text
cargo run -p xtask -- validate --corpus ../corpus
cargo run -p xtask -- verify-migration --legacy-vectors ../corpus/legacy/phase2-rc1/vectors --corpus ../corpus
cargo run -p xtask -- pack --corpus ../corpus --output ../corpus/dist/occurframe-corpus-1.0.0-rc2
cargo run -p xtask -- verify-deterministic --corpus ../corpus
```

Runner development uses only internal `xtask` surfaces:

```text
cargo run -p xtask -- runner-contract
cargo run -p xtask -- runner-smoke --root . --corpus ../corpus --schema ../corpus/schemas/runner-protocol-v2.schema.json --registry runners/registry/runner-builds.json --build go.robfig-cron --output smoke-artifacts/go.ndjson
cargo run -p xtask -- runner-smoke-all --root . --corpus ../corpus --schema ../corpus/schemas/runner-protocol-v2.schema.json --registry runners/registry/runner-builds.json --output smoke-artifacts/all.ndjson --report adapter-migration-report.json --diagnostics smoke-artifacts/diagnostics.json
```

Prepare each runtime from the pinned files below `runners/<language>/` before a
real smoke run. Dependency restoration may use the network; once prepared,
differential execution must be offline-capable. `runner-smoke-all` skips only
builds explicitly marked unreproducible, and fails on any timeout or runner
failure among the reproducible builds.

Generated `dist/` content is disposable and must not become normative source. Occurrence arrays are ordered observations: never sort or deduplicate them.

Fast correctness CI validates authority, fake-runner protocol containment, and
deterministic serialization on desktop platforms. The manual/scheduled adapter
certification workflow restores pinned historical builds and runs only the
representative migration suite; its smoke artifact is not the full evidence
set. The separate Full Differential Certification workflow runs the complete
184-vector population twice in the canonical environment and uploads the
candidate RC2 evidence bundle. Neither workflow changes corpus authority.

Full RC2 certification is a separate, long-running developer workflow. Build
the canonical image once, then execute with networking disabled:

```text
pwsh -File certification/docker/certify.ps1 -Action build
pwsh -File certification/docker/certify.ps1 -Action run -Script certification/docker/tasks/full-certification.sh
```

The task validates all 184 vectors, runs every reproducible build, emits one
observation per build/vector pair, scores with `occurframe-conformance`, runs a
second certification, and verifies byte equality of all seven semantic
artifacts. For direct internal use, the corresponding commands are
`differential-certify`, `differential-verify`, and `certification-verify`.

Do not run a candidate official certification on a developer interpreter. The
runtime version is part of runner identity and is enforced during `hello`.
Dependency restoration happens during image construction; the actual
certification container runs with `--network none`.
