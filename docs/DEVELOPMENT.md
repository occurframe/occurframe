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
cargo run -p xtask -- pack --corpus ../corpus --output ../corpus/dist/occurframe-corpus-1.0.0-rc3
cargo run -p xtask -- verify-deterministic --corpus ../corpus
```

`verify-migration` proves the historical RC1-to-RC2 zero-drift migration and
must be run against an immutable RC2 checkout, not the deliberately corrected
RC3 authority. Corpus CI checks out RC2 separately for that purpose.

Runner development uses only internal `xtask` surfaces:

```text
cargo run -p xtask -- runner-contract
cargo run -p xtask -- runner-smoke --root . --corpus ../corpus --schema ../corpus/schemas/runner-protocol-v3.schema.json --registry runners/registry/runner-builds.json --build go.robfig-cron --output smoke-artifacts/go.ndjson
cargo run -p xtask -- runner-smoke-all --root . --corpus ../corpus --schema ../corpus/schemas/runner-protocol-v3.schema.json --registry runners/registry/runner-builds.json --output smoke-artifacts/all.ndjson --report adapter-migration-report.json --diagnostics smoke-artifacts/diagnostics.json
```

Prepare each runtime from the pinned files below `runners/<language>/` before a
real smoke run. Dependency restoration may use the network; once prepared,
differential execution must be offline-capable. `runner-smoke-all` skips only
builds explicitly marked unreproducible, and fails on any timeout or runner
failure among the reproducible builds.

Generated `dist/` content is disposable and must not become normative source. Occurrence arrays are ordered observations: never sort or deduplicate them.

Public CLI development uses the shared implementation for both aliases:

```text
cargo run -p occurframe-cli --bin occurframe -- test --engine <build-id> --corpus ../corpus --format json
cargo run -p occurframe-cli --bin oframe -- test --engine <build-id> --corpus ../corpus --format junit
```

The checked-in registry is found at `runners/registry/runner-builds.json` when running from the repository root. Tests or separately prepared third-party adapters may set `OCCURFRAME_RUNNER_REGISTRY`, `OCCURFRAME_RUNNER_ROOT`, and `OCCURFRAME_RUNNER_PROTOCOL_SCHEMA`; this changes only transport/configuration locations and does not introduce another adapter contract.

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

The certified RC2 evidence lives in the repository at
`certification/rc2-evidence.tar.gz`, with its digest pinned in the evidence lock,
so release assembly never depends on an expiring CI artifact:

```text
cargo run -p xtask -- verify-evidence-archive --root . --lock release/evidence-lock.json
mkdir -p dist/certification && tar -xzf certification/rc2-evidence.tar.gz -C dist/certification
cargo run -p xtask -- verify-evidence-archive --root . --lock release/evidence-lock.json --extracted dist/certification/rc2
```

The archive is rebuilt — if it ever has to be — with modes normalised the way
`X` normalises them, so directories keep their search bit:

```text
tar --sort=name --mtime='@0' --owner=0 --group=0 --numeric-owner \
    --mode='u=rwX,go=rX' -cf - rc2 | gzip -9n > certification/rc2-evidence.tar.gz
```

`--mode='u=rw,go=r'` looks equivalent and is not: it clears the search bit on
the directory too. Extraction still succeeds, `root` still reads every file
through the unsearchable directory, and every unprivileged consumer — hosted CI
included — gets `EACCES` instead. `verify-evidence-archive --extracted` checks
the restored modes for exactly this reason, so a developer running as `root`
fails where a consumer would.

Release-candidate packaging is deterministic and refuses evidence, corpus, or binaries that differ from `release/evidence-lock.json`:

```text
cargo run -p xtask -- release-package --root . --corpus ../corpus --certification dist/certification/rc2 --binaries dist/platform-binaries --lock release/evidence-lock.json --output dist/release/occurframe-0.1.0-rc2
```

The output path must not already exist. Full platform assembly is performed by the manual release-candidate CI workflow; it uploads artifacts but never publishes GitHub Releases or crates.

Release builds must remap the builder's absolute paths, or panic metadata ships
the machine's `CARGO_HOME`. Cargo's `trim-paths` profile option is not stable on
the pinned toolchain, so the remap goes through `RUSTFLAGS`:

```text
RUSTFLAGS="--remap-path-prefix=$CARGO_HOME=/cargo --remap-path-prefix=$PWD=/occurframe" cargo build --locked --release -p occurframe-cli --bins
```

Packaging refuses to write `SHA256SUMS` until an audit proves the bundle carries
no absolute developer or CI path, and the same audit is available directly:

```text
cargo run -p xtask -- audit-paths --root dist/release/occurframe-0.1.0-rc2
cargo run -p xtask -- audit-paths --root dist/platform-binaries --forbid /srv/build
```

The dependency inventory and third-party notices are generated, never
hand-maintained:

```text
cargo run -p xtask -- dependency-inventory --manifest Cargo.toml --output DEPENDENCIES.json --notices THIRD-PARTY-NOTICES.md
```

The digest of a transport archive cannot live inside the archive it describes, so
it is recorded beside the release:

```text
cargo run -p xtask -- release-attest --bundle dist/release/occurframe-0.1.0-rc2 --archive dist/release/occurframe-0.1.0-rc2.tar.gz --output dist/release/release-attestation.json
```

Two consumer-facing suites complement the Rust tests. The fast one runs in
ordinary CI against a source checkout; the clean-room one runs against a packaged
release and deliberately assumes no Cargo, Rust, Git or checkout on the consumer
machine:

```text
cargo run --locked -p xtask -- source-example-smoke --corpus ../corpus
python3 tests/clean-room/verify_release.py --bundle dist/release/occurframe-0.1.0-rc2.tar.gz --target x86_64-unknown-linux-gnu
```

`source-example-smoke` validates and deterministically packs the authored
corpus, stages the runner-protocol schema required only by the source smoke,
builds both CLI aliases, configures the bundled minimal runner, and executes the
same end-to-end checks used by Fast CI. A separate schema-copy recipe is neither
required nor supported.

Source-package readiness: `occurframe-wire` and `occurframe-conformance` are
publishable; `occurframe-cli`, `occurframe-report`, `occurframe-runner` and
`xtask` are `publish = false` and intentionally implementation-private. Internal
path dependencies carry a version requirement, because Cargo refuses to package
a crate whose dependency has none.

```text
cargo package -p occurframe-wire --list
cargo package -p occurframe-wire
```

`cargo package -p occurframe-conformance` cannot complete until
`occurframe-wire` exists on crates.io, because packaging resolves that
dependency from the registry rather than from the path. That is publication
ordering, not a defect.
