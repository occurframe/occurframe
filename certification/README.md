# Certification configuration and historical evidence

This directory holds the **authored** certification configuration. It is
configuration and definition only: it contains no measurements, and nothing in
here is normative about recurrence semantics.

Authority runs one way:

```
corpus vectors + specification   ->  normative authority
occurframe-conformance           ->  scoring
observations                     ->  evidence
matrices / reports               ->  derived views
```

An observation never amends an expectation. A report never re-decides a verdict.

## What the profile pins

`profile.json` is the immutable RC2 configuration. `profile-rc3.json` is the
certified successor input with status `certified`. It declares an expected corpus revision for
comparison, but the run derives the actual clean checkout revision and records
it independently from the canonical digest. A mismatch fails closed. If Git
metadata is unavailable, only an explicit trusted `attested_input` is accepted;
directory names, versions, and content digests are never invented as source
identity.

The current profile also pins runner protocol 3.0, the engine configuration set
(including builds declared unreproducible *before* execution), every runner
version, the official engine timeout, the infrastructure watchdog, the
canonical platform, and the provenance fields every observation and environment
capture must carry.

Two things in the profile deserve emphasis, because they are the parts most
easily faked:

**Declared unreproducibility is a pre-commitment.** A build may be absent from
the observation set only if the profile already says it cannot be restored. A
build that is configured, declared reproducible, and then produces no
observation for some vector is a certification failure — not a gap to be
explained afterwards.

**Naming a platform is not pinning an environment.** `Ubuntu 24.04 x86_64` is
the canonical platform, but that string guarantees nothing about a future tzdb
release or interpreter patchlevel. The profile therefore also pins the exact
runtime versions and the exact tzdb release, and the run captures what was
*actually* observed. Where a component cannot be pinned perfectly, its observed
provenance is preserved verbatim rather than rounded to the pinned value.

## The historical RC2 environment

The official RC2 evidence set is measured **only** inside the container defined
by `docker/Dockerfile`. The host operating system is a host: it starts the
container and nothing else. This is deliberate. Phase II's adapter smoke run was
measured on a developer workstation, and the recorded evidence shows what that
costs — `CPython 3.13.5` observations attributed to a configuration pinning
`3.11.15`, and 51 observations whose system tzdb provenance degraded to
`unknown` because the host had no `tzdata.zi`.

Every measured runtime is pinned to the version the runner registry configures:

| language | runtime | pinned |
|---|---|---|
| Python | CPython | 3.11.15 |
| JavaScript | Bun | 1.3.13 |
| Go | gc | go1.24.7 |
| PHP | PHP | 8.4.21 |
| Ruby | MRI | 3.3.6 |

None of these are the versions Ubuntu 24.04 ships, so each is built or fetched
at its exact release rather than resolved from a distribution package. The
system timezone database is built from the IANA `2026a` release in `rearguard`
dataform for the same reason: Ubuntu's `tzdata` package rolls forward, which
would make the evidence set silently depend on the day the image was built.

That pin is not cosmetic. `.system` and `.vendored` Python builds exist
precisely to measure two different timezone databases, and the Phase II builds
they are compared against are identified as `@tz2026a` and `@tz2026c`. If the
container's system tzdb were 2026c, `.system` and `.vendored` would be measuring
the same database and the reconciliation mapping would be manufactured rather
than observed.

### Runtime identity is enforced, not merely recorded

`RunnerBuild::validate_hello` compares the observed `runtime.version` against
the configured pinned version and fails the handshake on a mismatch. CPython
3.13.5 cannot satisfy a 3.11.15 configuration; PHP 8.4.24 cannot satisfy 8.4.21.
The failure surfaces as a runner identity mismatch, which the certification
workflow treats as an infrastructure failure of the certification itself —
distinct from any engine's legitimate rejection, timeout, or non-conformance.

### Offline execution

The image is built **with** network access; the certification runs **without**
it. Every engine dependency is restored during the build from the exact commit
or release recorded in the lockfiles, and staged outside the working tree. The
run therefore executes with `--network none`, which makes "this certification
does not reach the network" a demonstrated property rather than an assurance.

## Prepared RC3 run

```powershell
# once, with network: build the canonical image
pwsh -File .\certification\docker\certify.ps1 -Action build

# capture the observed environment (no network)
pwsh -File .\certification\docker\certify.ps1 -Action probe

# run all 184 vectors across all 23 reproducible builds twice, then verify
# byte-identical semantic artifacts (no network)
pwsh -File .\certification\docker\certify.ps1 -Action run `
  -Script certification/docker/tasks/full-certification.sh
```

The durable RC3 evidence was produced with this command and is separately
digest-locked; the command's mere availability is not evidence. The driver first
requires clean Git worktrees. It bind-mounts the tooling repository read-only at
`/src`, the corpus
read-only at `/src-corpus`, and a writable output directory at `/out`. The
working tree is copied inside the container rather than edited in place, so the
corpus is provably unmodified and the host's own dependency directories cannot
shadow the pinned ones.

`phase2-build-map.json` is the authored identity bridge used for historical
reconciliation. It maps all 25 configured builds to RC1 engine keys only where
identity is exact; it does not assert semantic equivalence. The two Ruby builds
remain present in that map so all 814 RC1 generic-error cells stay in the
historical inventory even though the missing `concurrent-ruby` identity keeps
those builds out of the new observation population. See
`RUBY-PROVENANCE.md` for the bounded investigation.

Runner subprocesses do not inherit the container environment. Their executable
is resolved before `env_clear`; they receive deliberate `TZ=UTC`, the C locale,
the required platform/temp minimum, and declared build variables only. Each
observation records the safe policy facts and declared variable names, not
values.

## Artifacts

Historical RC2 bundles remain immutable. A fresh run lands in
`dist/certification/rc3/`. The normalized durable copy is committed as
`certification/rc3-evidence.tar.gz` and pinned by `release/evidence-lock.json`;
the historical lock is preserved as `release/evidence-lock-rc2.json`.

Committing a regenerated observation set to make the repository "contain the
latest run" would turn evidence into source, which is the one thing this
architecture exists to prevent.
