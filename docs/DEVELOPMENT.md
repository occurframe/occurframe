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

Fast correctness CI validates authority, fake-runner protocol containment, and deterministic serialization on desktop platforms. The manual/scheduled adapter-certification workflow restores pinned historical builds and runs only the representative migration suite. That is still not the future full 184-vector differential certification, and no smoke artifact is an official engine matrix.
