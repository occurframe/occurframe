# Occurframe

> **Occurframe is an executable conformance oracle for recurring-time semantics.
> It is not a scheduling engine.**

Occurframe answers one question precisely: *does this recurrence implementation
behave the way the corpus says it should, under the policy, dialect and
timezone-database provenance it declares?* It does not compute occurrences, does
not parse cron or RRULE, and does not run anything on a schedule. It sends
authored test vectors to your engine, records exactly what your engine answered,
and scores that answer against a versioned expectation.

```text
occurframe test --engine <configured-build-id>
oframe    test --engine <configured-build-id>
```

`occurframe` and `oframe` are two names for one implementation; every option,
byte of output and exit code is identical.

New here? [Getting started](docs/GETTING-STARTED.md) takes you from a downloaded
archive to a JUnit report in CI.

## The corpus

A **vector** is one language-neutral recurring-time behaviour case: an input, the
semantic context it must be read in, a classification, the normative evidence
behind it, and an authored expectation. Occurframe `0.1.0-rc1` ships corpus
`1.0.0-rc2` — 184 vectors across cron anchoring, day fields, day-of-week
numbering, DST, extensions, field counts, invalid input, names and steps, plus
RRULE core/BY-rules/sets/DST and timezone-database provenance.

The corpus lives in its own repository, [`occurframe/corpus`](https://github.com/occurframe/corpus),
and is the semantic authority. Generated observations, matrices, reports and
packed corpus data are derived artifacts; incumbent behaviour never defines an
expectation. Vector IDs are permanent, and a corrected expectation must carry
reviewed evidence. The tooling pins the corpus by version, canonical digest and
vector count, and refuses to run against anything else.

## Why dialect, policy and tzdb provenance are part of every result

"Cron-compatible" is not a testable claim, because there is no single cron. The
same expression legitimately produces different answers depending on:

- **Dialect** — Vixie, POSIX, Quartz, AWS EventBridge, robfig, croniter and
  others differ on day-of-month/day-of-week combination, sixth-field meaning,
  extensions such as `L`, `W`, `#` and `?`, and name handling. Occurframe records
  a permanent, versioned dialect ID rather than an impression.
- **Policy** — some behaviour is not a dialect difference but an undocumented
  library choice. Whether "next occurrence after T" includes T is the classic
  one: no cron document defines that function at all, so the corpus makes the
  axis explicit instead of picking a winner and calling everyone else wrong.
- **Timezone-database provenance** — a DST answer is reproducible only if you
  know which tzdb release produced it. Occurframe distinguishes an exact release,
  a bounded range, and genuinely unknown, and refuses to silently treat a system
  zoneinfo of unknown vintage as a pinned release.

Every result therefore carries engine name and version with provenance, runtime
language/name/version, declared dialect IDs, declared semantic-profile claims,
and tzdb provenance. Occurframe validates those declarations against the runner's
`hello` before it will attribute a single observation to your engine.

## The differential evidence

The release republishes a certified differential run: 25 configured historical
engine builds, 23 of them reproducible, executed over all 184 vectors for 4,232
observations. 163 vectors show semantic divergence between engines and 76 show a
normative violation.

**A divergence is not automatically a defect.** The corpus distinguishes
normative answers from named policy differences, named dialect differences,
standard ambiguity, known implementation divergence, and tzdb-provenance
differences. A cell where two engines disagree because they implement two
documented dialects is a difference, not a bug in either. There is no engine
leaderboard and no overall quality score, and none should be derived from this
data. [Interpreting results](docs/INTERPRETING-RESULTS.md) is the guide to
reading a matrix without over-claiming.

## What this prerelease is not

`0.1.0-rc1` is a **prerelease**. It is not Occurframe v1.0, and the corpus is
`1.0.0-rc2`, not stable `1.0.0`.

Only `test` is implemented. `explain`, `classify` and `occurrences` are reserved
names that return a usage error: their frozen meanings appear to require
evaluator behaviour, which conflicts with Occurframe's no-engine boundary. They
are not redirected, faked or partially implemented. See
[Known contradictions](docs/KNOWN-CONTRADICTIONS.md).

## Documentation

| Document | For |
| --- | --- |
| [Getting started](docs/GETTING-STARTED.md) | Verify an archive, run a first test, wire it into CI |
| [CLI](docs/CLI.md) | Options, discovery, exit codes, environment variables |
| [Writing a runner](docs/WRITING-A-RUNNER.md) | Integrating an engine over protocol `2.0` |
| [Interpreting results](docs/INTERPRETING-RESULTS.md) | What each verdict does and does not mean |
| [Releases](docs/RELEASES.md) | Release contents, provenance, reproducibility |
| [Known contradictions](docs/KNOWN-CONTRADICTIONS.md) | Unresolved product tensions, stated plainly |
| [Architecture](docs/ARCHITECTURE.md) · [Runner architecture](docs/RUNNER-ARCHITECTURE.md) · [Differential certification](docs/DIFFERENTIAL-CERTIFICATION.md) · [Development](docs/DEVELOPMENT.md) | Internals and contribution |

`examples/minimal-runner/` is a complete, runnable protocol example — a protocol
example only, not a recurrence engine and not a conformance reference.

## Licensing

Occurframe's Rust and reference/tooling code is `Apache-2.0 OR MIT`; take either.
The corpus's authored semantic data is `CC0-1.0`. Released binaries statically
link third-party crates whose licenses and notices are reproduced in the release
as `THIRD-PARTY-NOTICES.md`, with a machine-readable inventory in
`DEPENDENCIES.json`. No third-party recurrence engine or engine runtime is
redistributed.
