# Releases

## Version identities

Four versions move independently and are never conflated:

```text
Occurframe tooling: 0.1.0-rc2
Specification:      1.0.0-rc1
Corpus:             1.0.0-rc2
Runner protocol:    2.0
```

The certification evidence profile is `rc2.1`.

The **behavioural specification** versions separately from everything else
because it is the semantics an implementation is measured against, not the
artefact that measures. It is authored in `occurframe/corpus` and declared in
`spec/specification.json`; the tooling pins the same value once in
`release/evidence-lock.json`, and packaging fails if the two disagree.
Specification `1.0.0-rc1` is the version that carries ERRATA-001, under which
Occurframe v1 ships one semantic command.

`0.1.0-rc2` does not relabel the corpus as stable `1.0.0`, does not claim a
four-command CLI, and does not imply that an Occurframe recurrence engine
exists.

## What a release contains

```text
bin/                      both aliases for four target triples
corpus/                   generated RC2 distribution plus the protocol schema
docs/                     the public documentation set
examples/minimal-runner/  runnable protocol example (not an engine)
adapters/                 engine identity registry (no engine runtimes)
reports/                  candidate public differential report and matrix
certification/            certification manifest and observed environment
README.md  VERSION  LICENSES.md  LICENSE-APACHE  LICENSE-MIT
DEPENDENCIES.json  THIRD-PARTY-NOTICES.md
release-manifest.json     machine-readable release identity and provenance
SHA256SUMS                digest of every packaged file
```

Third-party engine runtimes are deliberately not bundled. A release records what
an engine build *is* — identity, provenance, runtime pin, declared semantics — not
a redistributed copy of it.

## Provenance

`release-manifest.json` records, for every archive:

- tooling version, repository, and the exact 40-character commit released;
- the Rust toolchain that built the binaries (version, host triple, commit hash);
- every target triple, and every binary with its alias, target and SHA-256;
- corpus version, repository, pinned corpus SHA, canonical digest, release
  digest and vector count;
- the specification version, the errata that fix its command doctrine, and the
  complete set of semantic commands the release ships;
- runner protocol version;
- certification artifact name, profile version, the tooling SHA that produced the
  evidence, the digest of the certification manifest itself, the certified
  semantic bundle digest, the matrix digest, and the certified population;
- the licensing map and the digests of `DEPENDENCIES.json` and
  `THIRD-PARTY-NOTICES.md`;
- `source_date_epoch`, populated only when the release environment supplies one.

**No wall-clock value is recorded.** A build timestamp would make two otherwise
identical assemblies differ, so the only time-like field comes from
`SOURCE_DATE_EPOCH`.

The digest of a transport archive cannot live inside the archive it describes, so
`xtask release-attest` writes a `release-attestation.json` beside the release,
recording the bundle checksum digest and the archive's SHA-256.

## Locked evidence

The certified RC2 differential evidence is republished by every release, never
re-measured at release time. It is stored **in this repository** as
`certification/rc2-evidence.tar.gz` (a deterministic, `gzip -n` archive of the
certified bundle), not fetched from a hosted CI artifact. Hosted artifacts
expire; once one does, a release can no longer be reassembled from its own
inputs, and the evidence behind a published claim becomes unreachable.

The archive's SHA-256 is pinned in `release/evidence-lock.json` under
`evidence_archive`, so restoring it is digest-verified in three steps:

```text
cargo run -p xtask -- verify-evidence-archive --root . --lock release/evidence-lock.json
tar -xzf certification/rc2-evidence.tar.gz -C dist/certification
cargo run -p xtask -- verify-evidence-archive --root . --lock release/evidence-lock.json --extracted dist/certification/rc2
```

The first call refuses a modified archive before anything is unpacked; the third
re-verifies every extracted file against the certification bundle's own
`SHA256SUMS` and checks that the restored tree is readable without privilege —
an archive that recorded its directory without a search bit passes every
checksum and is still unusable to anyone who is not `root`.
`certification.artifact_name` in the lock remains as provenance —
it records which certification run produced the evidence — and is no longer a
fetch location.

`release/evidence-lock.json` pins the corpus and certification digests, the
evidence-archive digest, plus the exact required binary inventory. `xtask release-package` refuses to produce a
bundle when a required binary is missing, the corpus differs from the lock, the
certified matrix differs, a certification checksum fails, or the output directory
already exists.

Packaging additionally refuses to write `SHA256SUMS` until an audit proves the
bundle carries no absolute developer or CI path, so a leaking artifact can never
acquire a valid checksum manifest.

## Reproducibility

Three distinct properties, measured separately and never conflated:

- **Semantic/package manifest reproducibility** — two assemblies from the same
  inputs produce the identical file list and identical `release-manifest.json`.
- **Archive reproducibility** — the deterministic `tar` (sorted names, zeroed
  mtime, numeric owner) and `gzip -n` produce byte-identical archives, and
  therefore identical archive digests.
- **Binary reproducibility** — whether recompiling the same source with the same
  pinned toolchain yields byte-identical executables.

The release workflow measures the first two by assembling twice from clean state
and comparing `SHA256SUMS` and archive digests, and measures the third by
rebuilding the binaries and comparing digests. Release builds set
`--remap-path-prefix` for `CARGO_HOME` and the checkout, which both removes the
builder's absolute paths from panic metadata and removes a source of
machine-to-machine variation. Any claim about binary reproducibility must come
from that measurement; it is not assumed.

## Dependency and licence inventory

`xtask dependency-inventory` derives `DEPENDENCIES.json` and
`THIRD-PARTY-NOTICES.md` from `cargo metadata` and `Cargo.lock`. It covers the
normal and build dependency closure of the released binaries — dev-dependencies
are excluded because they are not linked — and records for each crate its name,
version, direct or transitive relationship, source, registry checksum, declared
SPDX expression and the names of the licence files published inside the crate.

License fields are reproduced exactly as each crate publishes them. Where a crate
publishes no expression the field is `null` and the reason is recorded. Nothing
is inferred, and no licence is supplied on a crate's behalf.

## Publication

Generated bundles belong under `dist/release/` or CI artifact storage and are not
routine source-controlled content. The release-candidate workflow builds and
smokes each native binary, assembles twice, checks byte-identical `SHA256SUMS`,
runs the clean-room consumer suite on all four platforms, and uploads the
candidate artifact. It does not publish a GitHub Release or a crates.io package,
and it does not merge to `main`.

Publication is owner-controlled. See [Release checklist](RELEASE-CHECKLIST.md).
