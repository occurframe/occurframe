# Stable-v1 readiness audit

**Question this document answers:** *is the implementation technically ready for
an owner-reviewed public prerelease after CI?*

**Answer: not yet — one hosted-CI condition remains open.** Every locally
executable gate passes at `0.1.0-rc2`, the last doctrine contradiction is
formally resolved, and the release artifact assembles deterministically from
digest-verified inputs. Hosted CI has run against RC2 and found one genuine
defect, in the durable evidence archive rather than in the oracle; that defect is
fixed and its recurrence is now gated. What remains is a hosted run in which the
whole four-platform matrix concludes green at one commit, which requires a push
this environment cannot perform. Nothing found in this audit blocks the *oracle*
itself.

Note the scope: this is readiness for an **owner-reviewed public prerelease**, not
for stable `1.0.0`. Stable v1 additionally needs the corpus to leave release-
candidate status, which is a semantic-authority decision and not an
implementation one.

Items are classified **BLOCKING** (must be fixed before an owner-reviewed
prerelease), **ACCEPTED_LIMITATION** (understood, documented, and not a defect to
fix now), or **POST_V1**.

---

## BLOCKING

### B-1 · The hosted four-platform matrix has not concluded green at RC2

The four-platform release-candidate workflow — platform builds, path-leak audit,
binary-reproducibility measurement, two-pass deterministic assembly, and the
clean-room consumer suite on Windows x86_64, Linux x86_64, macOS arm64 and macOS
x86_64 — has now been executed against RC2, and has not yet concluded green.

**What hosted CI has established so far.**

| Run | Commit | Result |
| --- | --- | --- |
| `33342029682` | `83f1a1a` | All four platform jobs failed comparing multi-line alias output in PowerShell. Fixed in `9955aea`. |
| `33342714486` | `9955aea` | All four platform jobs **passed**; assembly failed restoring the locked certification evidence. |
| `33342025361` (corpus) | `5790faa` | Corpus authority CI **passed**. |

**The assembly failure was a real defect, and it was in the durable evidence
archive itself.** `certification/rc2-evidence.tar.gz` had been packed with
`--mode='u=rw,go=r'`, which clears the search bit on the archived directory as
well as on the files. `tar` extracts such an archive successfully and reports
success; a process running as `root` then reads every file inside it, because
`root` bypasses the missing bit. An unprivileged consumer — the hosted runner,
and any ordinary user unpacking the published evidence — gets `EACCES` on the
first file it opens. Every local reproduction ran as `root`, which is precisely
why the defect survived to hosted CI.

**Resolution.** The archive is repacked with `--mode='u=rwX,go=rX'` (directories
`0755`, files `0644`), its content byte-identical and its per-file checksums
unchanged; its digest is re-pinned in `release/evidence-lock.json`. So that the
class of defect cannot recur silently, `xtask verify-evidence-archive
--extracted` now checks the restored modes as well as the checksums, so a
developer running as `root` fails exactly where a consumer would.

**Still outstanding:** a hosted run in which all four platform jobs, the
deterministic assembly and all four clean-room jobs conclude green at the same
commit. Until that is observed, four-platform status for RC2 is *unverified*,
not *passing*.

This is the only item standing between the current tree and an owner review.

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

Same-host rebuilds with the pinned toolchain and `--remap-path-prefix` produce
byte-identical binaries, and CI measures and reports this per target. Cross-
machine binary reproducibility is **not** claimed and has not been measured. The
release notes claim only what was measured; manifest and archive reproducibility
are separately verified and do hold.

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

## Explicit status of the items this audit was asked to confirm

| Item | Status | Evidence |
| --- | --- | --- |
| Engine gate | **Closed.** No recurrence engine, cron/RRULE evaluator, occurrence generation or scheduling execution exists in the workspace. | `RESEARCH-II.md` §5 unchanged; `spec/specification.json` records `engine_gate_state: closed`; the CLI delegates every computation to an external process over protocol 2.0. |
| Command doctrine | **Resolved by ERRATA-001.** v1 ships `test` alone. | `corpus/spec/ERRATA.md`; enforced by `release-package`, which refuses a lock declaring any other shipped surface, and by CLI and alias tests. |
| Ruby builds | **ACCEPTED_LIMITATION (A-1).** | Public differential report; registry `unreproducible_provenance` entries. |
| Licensing | **ACCEPTED_LIMITATION (A-2, A-3).** Recorded accurately; no owner legal decision made here. | `LICENSES.md`, `corpus/LICENSING.md`, `DEPENDENCIES.json`. |
| Publication | **Owner-controlled, outside implementation.** | `docs/RELEASE-CHECKLIST.md`; CI publishes nothing, merges nothing and tags nothing. |

## Evidence for the readiness answer

Locally executed at the RC2 commits, all passing: `cargo fmt --check`; `cargo
clippy --workspace --all-targets -D warnings`; `cargo test --workspace`; `cargo
doc --workspace --no-deps` with warnings denied; corpus validation and
deterministic pack (184 vectors, canonical digest unchanged); durable
certified-evidence archive verification, including per-file checksums and
unprivileged readability after extraction; two-pass deterministic release
assembly with identical file
manifests, `SHA256SUMS` and archives; absolute-path audit clean; dependency and
licence inventory regenerated and compared; the clean-room consumer suite against
the packaged release on Linux.

Unverified pending owner push: the same clean-room suite on Windows, macOS arm64
and macOS x86_64, and the per-target binary-reproducibility measurement (B-1).
