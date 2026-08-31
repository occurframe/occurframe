# Stable-v1 readiness audit

**Question this document answers:** *is the implementation technically ready for
an owner-reviewed public prerelease after CI?*

**Answer: READY FOR OWNER MERGE REVIEW.** Every gate this project defines has
now concluded green in hosted CI against one commit: `cargo fmt`, clippy at
`-D warnings`, the test suite and documentation on Ubuntu, Windows and macOS;
native binaries on all four release targets with alias smoke, path-leak audit
and binary-reproducibility measurement; restoration of the durable certified
evidence by an unprivileged process; two-pass deterministic assembly with
identical trees, `SHA256SUMS` and archives; and the clean-room consumer suite
against the transport archive on Windows x86_64, Linux x86_64, macOS arm64 and
macOS x86_64. Nothing remains that implementation work can close.

Note the scope: this is readiness for an **owner-reviewed public prerelease**, not
for stable `1.0.0`. Stable v1 additionally needs the corpus to leave release-
candidate status, which is a semantic-authority decision and not an
implementation one.

Items are classified **ACCEPTED_LIMITATION** (understood, documented, and not a
defect to fix now), **POST_V1**, or **OWNER_RELEASE_ACTION** (outside
implementation, and deliberately not performed by CI).

---

## The hosted gate, and what it cost to pass

The gate is green. The whole history is recorded, not just the green run: three
of these runs failed, and what they found matters more than the fact that the
last one passed.

| Workflow | Run | Commit | Result |
| --- | --- | --- | --- |
| Release Candidate Packaging | `33342029682` | `83f1a1a` | All four platform jobs failed comparing multi-line alias output. PowerShell's comparison operators *filter* when the left operand is an array, so byte-identical multi-line output compared as unequal. Fixed in `9955aea`. |
| Release Candidate Packaging | `33342714486` | `9955aea` | Four platform jobs passed; assembly failed restoring the locked certification evidence. Fixed in `391e3da`. |
| Fast correctness CI | `33345389784` | `391e3da` | Windows clippy failed: the `cfg(not(unix))` half of the new readability guard is infallible, so `unnecessary_wraps` fired where no Unix job could see it. Fixed in `a61730d`. |
| Fast correctness CI | `33347466238` | `a61730d` | **Success**, all three operating systems. |
| **Fast correctness CI** | **`33348854318`** | **`d5807f2`** | **Success** — Ubuntu, Windows, macOS. |
| **Release Candidate Packaging** | **`33348854321`** | **`d5807f2`** | **Success** — 4 platform jobs, deterministic assembly, 4 clean-room jobs, artifact uploaded. |
| Corpus authority CI | `33342025361` | corpus `5790faa` | Success. |

**The evidence-restore failure was a real defect in the durable archive.**
`certification/rc2-evidence.tar.gz` had been packed with `--mode='u=rw,go=r'`,
which clears the search bit on the archived directory as well as on the files.
`tar` extracts such an archive successfully and reports success; a process
running as `root` then reads every file inside it, because `root` bypasses the
missing bit. An unprivileged consumer — the hosted runner, and any ordinary user
unpacking the published evidence — gets `EACCES` on the first file it opens.
Every local reproduction ran as `root`, which is precisely why the defect
survived to hosted CI. The archive is repacked with `--mode='u=rwX,go=rX'`, its
content byte-identical and its per-file checksums unchanged, and
`verify-evidence-archive --extracted` now checks the restored modes as well as
the checksums, so a developer running as `root` fails exactly where a consumer
would.

**Gate evidence.**

```text
tooling commit                d5807f2e030e66695ca37f27d32f7e12c27d5a43
corpus commit                 5790faa3b886ff9ec3805283e218ea11a8c6dd24
Fast correctness CI           run 33348854318 — success
Release Candidate Packaging   run 33348854321 — success
artifact                      occurframe-0.1.0-rc2 (33.3 MB)
artifact content digest       sha256:b9419ad87cddc683a59f89a600abcbd5431ffe77b879d96b42ffdf5470dce9bb
durable evidence archive      sha256:98d9282ca54f6249bc70be7bce42f5cd637399f64f7d0c276235e9317764e18a
semantic certification digest 1f592fdce4f9641406afb76383ff10585b5764dcfd097e595912c9a01cce98e1
corpus canonical digest       4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470
```

That artifact is **gate evidence, not the publication payload**. It was built
from `d5807f2` on `dev`; the public prerelease must be rebuilt from the commit
actually tagged after the merge, and its `release-manifest.json`
`tooling_commit_sha` must equal that commit.

---

## ACCEPTED_LIMITATION

### A-1 · Two Ruby builds are provenance-blocked

`ruby.fugit` and `ruby.ice_cube` remain `unreproducible_provenance`: Phase II did
not record the exact historical `concurrent-ruby` dependency that `tzinfo`
required, and no contemporary dependency, engine or Ruby version was substituted
to fill the gap.

**Why this is not blocking.** It is a limitation of one historical *measurement
population*, not of the oracle. The corpus, the protocol, the scorer and the CLI
are unaffected; a third party can run `test` against fugit or ice_cube today with
their own runner and get a full result. What is missing is Occurframe's own
ability to reproduce a 2026-vintage build of them for the published differential.

The honest handling is already in place: the population is reported as 23
reproducible of 25 configured rather than as 23 of 23, the two builds are named
in the public report, and Occurframe refuses to execute a provenance-blocked
build at run time, because evidence that cannot be reproduced is not evidence.

**Revisit if** the exact historical dependency set is recovered. Substituting a
different one would not resolve this; it would silently change what was measured.

### A-2 · First-party code is dual-licensed, not Apache-2.0 alone

The workspace declares `Apache-2.0 OR MIT`, and both texts ship in every release.
Some earlier release planning described this code as "Apache-2.0".

**Why this is not blocking.** `OR` is a choice: anyone who requires Apache-2.0
may take Apache-2.0, so no consumer is worse off. The release states the dual
licence accurately in `LICENSES.md`, `release-manifest.json` and the crate
metadata rather than narrowing it in prose while the manifests say otherwise.

**Not decided here.** Narrowing to Apache-2.0 alone would remove an option
existing recipients already have. That is an owner legal decision, and this audit
deliberately does not make it or pre-empt it.

### A-3 · The corpus `legacy/` tree carries no blanket licence

`corpus/LICENSING.md` dedicates the authored semantic data to CC0-1.0 and
licenses the reference matcher and its tests Apache-2.0, following Research II
§7.2. It asserts **no blanket licence** over `legacy/phase2-rc1/`, which mixes
Occurframe-authored research with material derived from third-party engines —
provenance records, raw observations produced by running other projects'
software, and matrices about them.

**Why this is not blocking.** Nothing in the release redistributes `legacy/`; the
release bundle contains generated corpus data, not the corpus repository. The
absence of a claim is the accurate state, and asserting one would purport to
license material that is not Occurframe's to give away.

**Owner decision, per directory, before the corpus is promoted to stable.**

### A-4 · `--no-color` and `NO_COLOR` are accepted but inert

The text renderer emits no ANSI colour, so neither has an observable effect. They
are accepted so that a future coloured renderer cannot break existing
invocations, and the documentation says plainly that they do nothing rather than
implying colour exists.

### A-5 · Binary reproducibility is measured per release, not asserted

CI measures binary reproducibility per target; it is never inferred from
deterministic semantic/package assembly or deterministic transport-archive
assembly. Hosted same-host rebuilds have produced byte-identical binaries on
Linux x86_64, macOS arm64 and macOS x86_64. The Windows x86_64 same-host rebuild
produced different binary bytes. That result is informational and non-gating:
release integrity is established by checksums, manifests, clean-room behaviour
and platform-native build provenance. No broad cross-machine binary
reproducibility claim is made.

---

## POST_V1

### P-1 · The deferred evaluator commands

`explain`, `classify` and `occurrences` are deferred behind the engine gate by
ERRATA-001. They are not v1 scope and not a v1 gap: shipping them requires an
Occurframe recurrence engine, which the ORACLE ONLY verdict does not authorise.

Their frozen semantics are preserved verbatim in the specification so the gate
can be walked without reopening research. The gate is unchanged:

> A named maintainer of a named project commits, in writing and in public, to
> adopt an Occurframe engine at a specified integration seam.

### P-2 · The conceptual scheduling API as a library

`spec/CLI.md` §§1–5 remain a behavioural and conformance specification. Shipping
them as a library is engine-gated in the same way, and would carry the unresolved
`explain`/`classify` boundary question the specification already records.

### P-3 · crates.io publication

`occurframe-wire` and `occurframe-conformance` are publishable;
`occurframe-cli`, `occurframe-report`, `occurframe-runner` and `xtask` are
intentionally `publish = false`. Publication order would be `occurframe-wire`
then `occurframe-conformance`, because packaging the latter resolves its
dependency from the registry rather than the path. Not required for a prerelease.

### P-4 · Corpus promotion to stable `1.0.0`

A semantic-authority decision about vector expectations and open-expectation
coverage, not an implementation task.

---

## OWNER_RELEASE_ACTION

Everything below is deliberately outside CI and outside implementation. CI
merges nothing, tags nothing and publishes nothing, by design.

### O-1 · Merge review

Draft pull requests `dev → main` in both repositories, corpus first: the corpus
is the semantic authority the tooling is measured against, so a tooling `main`
that referenced an unmerged corpus would be provenance-incoherent.

### O-2 · Rebuild the public artifact from the tagged commit

The gate artifact was built from `d5807f2` on `dev`. After the merges, run
Release Candidate Packaging from merged `main` and confirm the resulting
`release-manifest.json` records that commit as `tooling_commit_sha`. A published
archive whose manifest names a commit that was never tagged cannot be verified
by a consumer, so the `dev` artifact must not be published.

### O-3 · Tag and prerelease

Tag `corpus-1.0.0-rc2` and `v0.1.0-rc2` — prerelease tags only, no stable tag —
then a **draft** GitHub prerelease with the archive, its checksum and the
attestation from the merged-`main` build, re-downloaded into a clean directory
and clean-room verified before publication.

### O-4 · The Apache-2.0-alone question

A-2 records the dual licence accurately rather than narrowing it. Narrowing is
an owner legal decision this audit does not make.

### O-5 · Per-directory licensing of the corpus `legacy/` tree

A-3, before the corpus is promoted to stable.

---

## Explicit status of the items this audit was asked to confirm

| Item | Status | Evidence |
| --- | --- | --- |
| Engine gate | **Closed.** No recurrence engine, cron/RRULE evaluator, occurrence generation or scheduling execution exists in the workspace. | `RESEARCH-II.md` §5 unchanged; `spec/specification.json` records `engine_gate_state: closed`; the CLI delegates every computation to an external process over protocol 2.0. |
| Command doctrine | **Resolved by ERRATA-001.** v1 ships `test` alone. | `corpus/spec/ERRATA.md`; enforced by `release-package`, which refuses a lock declaring any other shipped surface, and by CLI and alias tests. |
| Ruby builds | **ACCEPTED_LIMITATION (A-1).** | Public differential report; registry `unreproducible_provenance` entries. |
| Licensing | **ACCEPTED_LIMITATION (A-2, A-3).** Recorded accurately; no owner legal decision made here. | `LICENSES.md`, `corpus/LICENSING.md`, `DEPENDENCIES.json`. |
| Publication | **OWNER_RELEASE_ACTION (O-1…O-3).** | `docs/RELEASE-CHECKLIST.md`; CI publishes nothing, merges nothing and tags nothing. |
| Four-platform hosted gate | **Green at `d5807f2`.** | Release Candidate Packaging run `33348854321`: 4 platform jobs, deterministic assembly, 4 clean-room jobs, artifact `occurframe-0.1.0-rc2`. |

## Evidence for the readiness answer

**Hosted, at `d5807f2`, all green.** Fast correctness CI (run `33348854318`) on
Ubuntu, Windows and macOS: `cargo fmt --check`; `cargo clippy --workspace
--all-targets -- -D warnings`; `cargo test --workspace`; `cargo doc --workspace
--no-deps` with warnings denied; bundled-corpus identity and deterministic pack;
the protocol-example smoke suite; dependency and licence inventory generation.
Release Candidate Packaging (run `33348854321`): native binaries for
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-apple-darwin` and
`x86_64-apple-darwin`, each with alias smoke, help/version equivalence, path-leak
audit, staging and binary-reproducibility measurement; restoration of the durable
certified evidence by an unprivileged runner, including its permission check,
extracted `SHA256SUMS` and `certification-verify`; two-pass assembly with
identical trees, `SHA256SUMS` and gzip archives; licence and dependency inventory
validation against the packaged copies; the out-of-bundle attestation; and the
clean-room consumer suite run against the transport archive on all four targets,
in jobs that install no Rust toolchain and check out no corpus.

**Locally, at the same tree:** the same Rust gates, corpus validation and
deterministic pack (184 vectors, canonical digest unchanged), evidence
restoration including unprivileged readability, two-pass assembly, path audit,
inventory comparison, and the clean-room suite on Linux — 120 of 120 checks.

Not claimed: cross-machine binary reproducibility (A-5), which is measured and
reported per target rather than asserted.
