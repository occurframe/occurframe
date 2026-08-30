# Releases

## Version identities

Three versions move independently and are never conflated:

```text
Occurframe tooling: 0.1.0-rc1
Corpus:             1.0.0-rc2
Runner protocol:    2.0
```

The certification evidence profile is `rc2.1`. `0.1.0-rc1` is the first public
tooling release candidate; it does not relabel the corpus as stable `1.0.0` and
does not imply that all four reserved commands are implemented.

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

`release/evidence-lock.json` pins the corpus and certification digests plus the
exact required binary inventory. `xtask release-package` refuses to produce a
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
