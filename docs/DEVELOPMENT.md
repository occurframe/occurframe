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

Generated `dist/` content is disposable and must not become normative source. Occurrence arrays are ordered observations: never sort or deduplicate them.

Fast correctness CI validates authority and deterministic serialization. Full differential certification is a separate, slower future workflow and does not run external engines in ordinary pull requests.

