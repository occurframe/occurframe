# Interpreting results

Occurframe reports what happened, in categories that are deliberately kept apart.
Collapsing them is the fastest way to draw a wrong conclusion, so this page is
about what each result licenses you to say — and what it does not.

## The categories

| Result | What it means | What it does **not** mean |
| --- | --- | --- |
| **conformant** | The engine's answer equalled the authored expectation exactly — same values, same order, same duplicates. | That the engine is correct in general, or correct outside this vector's dialect, policy and tzdb context. |
| **conformant admissible** | The expectation names several acceptable answers, and the engine produced one of them. The matched case is recorded. | That the other admissible answers are wrong. Admissible means the corpus deliberately declines to pick a winner. |
| **non-conformant** | The engine's answer differed from the expectation selected by its own declared profile. | Automatically a defect. See below. |
| **open / unscored** | The corpus records the case but authors no expectation yet, so the observation is preserved and not scored. | Agreement, disagreement, or a gap in the engine. It is a gap in the corpus, stated honestly. |
| **unsupported** | The engine declared it cannot express this case at all. | A wrong answer. Nothing was measured; there is no semantic content here. |
| **engine error** | The engine failed unexpectedly — a crash, assertion or contract violation. | A semantic disagreement. It is a robustness result. |
| **timeout** | No result arrived before the per-case engine budget. | That the engine would have been wrong. It is a performance/liveness result. |
| **runner / infrastructure failure** | The run itself was not trustworthy: the process would not start, identity did not match, the protocol was violated, or the corpus or registry was invalid. | Anything about the engine. No observation is attributed to it. |

The exit codes preserve the same separation: `0` conformant, `1` semantic
non-conformance or engine error or timeout, `3` usage, `4` environment. `4`
outranks `1`, because a run that did not produce trustworthy evidence is not a
statement about your engine at all.

Unsupported cells, engine errors, timeouts and runner failures are **excluded
from semantic-answer grouping**. When the evidence says 163 vectors show semantic
divergence, that count is over cells where engines actually produced comparable
semantic answers.

## Non-conformant does not mean defective

The corpus classifies every vector, and the classification changes what a
disagreement means:

- **NORMATIVE** — a documented standard or specification determines the answer.
  A disagreement here is the strongest signal the corpus can give, and is the
  closest thing to "probably a bug".
- **POLICY_DEPENDENT** — no document defines the behaviour; it is a library
  choice along a named axis. The corpus records the axis and the expected answer
  *per policy*. A non-conformant result usually means the engine's declared
  policy claim does not match its actual behaviour — a documentation problem as
  often as a code problem.
- **DIALECT_DEPENDENT** — the answer is determined by which cron dialect is being
  implemented. Two engines disagreeing here may both be exactly right for their
  own dialect.
- **AMBIGUOUS_STANDARD** — the governing document is genuinely unclear. The
  corpus records the ambiguity rather than inventing a resolution.
- **KNOWN_DIVERGENCE** — the ecosystem is known to have split, and the split is
  documented.
- **INVALID** — the input is invalid and the correct behaviour is a deliberate
  rejection. An engine that quietly accepts invalid input is non-conformant here,
  and that usually *is* worth fixing.

So the honest reading of a non-conformant cell is: *this engine's behaviour
differs from the expectation the corpus selected for the profile this engine
declared.* Whether that is a defect depends on the classification, on whether the
engine's declared profile is accurate, and on what the engine's own documentation
promises. Occurframe deliberately stops short of that judgement.

There is one case where non-conformance points at the declaration rather than the
behaviour: a `per_policy` or `per_dialect` vector whose declared claims select
zero cases or more than one is non-conformant with an
`unresolved_declared_profile` diagnostic. The scorer will not guess which case
you meant.

## Identity is part of every result

A result is meaningless without the identity that produced it, so every report
carries engine name/version/provenance, runtime language/name/version, declared
dialect IDs, declared semantic-profile claims, and tzdb provenance.

Timezone-database provenance deserves special care. A DST answer is reproducible
only against a known tzdb release. Occurframe distinguishes `exact`, `bounded`
and `unknown`, and treats `unknown` as an honest answer rather than a failure.
Two engines that disagree on a DST vector while reporting different tzdb releases
have not necessarily disagreed about semantics at all — they may have consulted
different data. Use `--tzdb exact:<release>`, `--tzdb bounded`, `--tzdb known` or
`--tzdb source:<name>` to require a provenance class and fail the run as an
environment failure when it is not met, rather than silently accepting evidence
you cannot reproduce.

## Reading the differential matrix

`reports/matrix.json` in the release is the cell-level record of the certified
run: every engine-build × vector pair, with its normalized outcome, verdict,
matched case, warnings and provenance reference.

The certified population is:

| | |
| --- | ---: |
| Configured historical builds | 25 |
| Reproducible builds | 23 |
| Provenance-blocked builds | 2 |
| Vectors | 184 |
| Observations | 4,232 |
| Semantic-divergence vectors | 163 |
| Normative-violation vectors | 76 |
| Engine errors | 8 |
| Timeouts | 1 |
| Unsupported cells | 2,099 |

**163 divergent vectors is not 163 engine defects.** It is the number of vectors
on which the reproducible builds did not all produce the same semantic answer,
across dialects, policies, ambiguity and tzdb provenance combined. The two
provenance-blocked Ruby builds (`ruby.fugit`, `ruby.ice_cube`) were not replaced
with newer dependencies or engine versions, so the population is 23 measured
builds and the shortfall is recorded rather than papered over.

**Do not build a leaderboard.** Ranking engines by conformance count would reward
an engine that reports `unsupported` for everything difficult, punish one that
implements a rich dialect the corpus records many vectors for, and silently
compare implementations that were never trying to do the same thing. Occurframe
publishes no overall score, and the matrix is structured to make per-cell reading
the natural one.

For the same reason, a compatibility claim needs at minimum: corpus version,
dialect or RRULE profile, semantic policy claims, tzdb provenance, engine
identity and provenance, and runtime. "Cron-compatible" on its own is not a
versioned, reproducible identity.

## Using results in CI

The JUnit mapping keeps the distinctions your CI can act on:

| Occurframe result | JUnit |
| --- | --- |
| conformant, conformant admissible | success |
| non-conformant | failure |
| unsupported, open/unscored | skipped |
| engine error, timeout, runner failure | error |

A practical policy is to fail the build on `failure`, investigate `error`
separately as an environment or robustness problem, and track the `skipped` count
over time — a rising skip count means your engine is declaring more of the corpus
out of scope, which is worth noticing even though no test "failed".

JSON output is deterministic with canonical key ordering, so committing a
baseline and diffing subsequent runs is a reliable way to see exactly what
changed when you upgrade an engine or a tzdb release.
