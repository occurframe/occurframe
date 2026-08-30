# Release checklist (owner-controlled)

Publication is owner-controlled. Nothing in this repository publishes anything:
CI builds, verifies and uploads artifacts, and stops. This checklist is the
sequence a repository owner performs by hand.

Every step below is reversible up to the point where a tag or a GitHub Release is
created. Do not proceed past a failing verification.

**Never paste a token, password or secret into a command recorded in an issue,
PR or commit.** The steps below use `gh` with its own stored credentials and the
Actions-provided `GITHUB_TOKEN`; no additional secret is required for a
prerelease, and crates.io publication is not required at all.

## 0. Preconditions

- [ ] `dev` is green on the fast correctness workflow.
- [ ] The release-candidate workflow has completed with all four platform jobs
      and all four clean-room jobs passing.
- [ ] The uploaded artifact's two assemblies produced byte-identical
      `SHA256SUMS`, and the recorded binary-reproducibility result has been read
      and accepted (it is a measurement, not an assumption).
- [ ] `release/evidence-lock.json` still matches the certified evidence.
- [ ] `release-notes/0.1.0-rc1.md` has been read end to end and is accurate.

## 1. Review `dev` → `main` for the tooling repository

```sh
gh pr create --repo occurframe/occurframe --base main --head dev --draft \
  --title "Release candidate review: Occurframe 0.1.0-rc1" \
  --body-file release-notes/0.1.0-rc1.md
```

- [ ] The PR is marked **draft** and titled as a release-candidate review.
- [ ] Auto-merge is **not** enabled.
- [ ] Review the diff in full, in particular anything under `release/`,
      `crates/occurframe-cli/assets/corpus-lock.json` and the workflows.

## 2. Review `dev` → `main` for the corpus repository

```sh
gh pr create --repo occurframe/corpus --base main --head dev --draft \
  --title "Corpus 1.0.0-rc2 publication review" \
  --body-file release-notes/1.0.0-rc2.md
```

- [ ] Confirm the diff contains **no** change to any vector, expectation, schema
      semantics, registry semantics or canonical digest.
- [ ] Confirm the canonical corpus digest is still
      `4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470` over 184
      vectors. Any change here stops the release and is investigated before
      anything else happens.

## 3. Merge the tooling repository to `main`

- [ ] Undraft the tooling PR only after both reviews are complete.
- [ ] Merge `dev` → `main` with a merge commit (no squash: the release commit SHA
      recorded in `release-manifest.json` must remain reachable).
- [ ] Record the resulting `main` SHA.

## 4. Merge the corpus repository to `main`

- [ ] Undraft and merge the corpus PR.
- [ ] Record the resulting `main` SHA.

Order matters: the corpus is the semantic authority the tooling pins, so a
published tooling release must not reference an unpublished corpus state.

## 5. Tag the corpus prerelease

```sh
git -C <corpus checkout> fetch origin
git -C <corpus checkout> checkout main && git -C <corpus checkout> pull --ff-only
git -C <corpus checkout> tag -a corpus-1.0.0-rc2 -m "Occurframe corpus 1.0.0-rc2 (prerelease)"
git -C <corpus checkout> push origin corpus-1.0.0-rc2
```

- [ ] The tag is a prerelease identity. Do **not** create a `1.0.0` tag.

## 6. Tag the tooling prerelease

```sh
git -C <tooling checkout> fetch origin
git -C <tooling checkout> checkout main && git -C <tooling checkout> pull --ff-only
git -C <tooling checkout> tag -a v0.1.0-rc1 -m "Occurframe 0.1.0-rc1 (prerelease)"
git -C <tooling checkout> push origin v0.1.0-rc1
```

- [ ] The tag SHA equals the `tooling_commit_sha` in the artifact's
      `release-manifest.json`. If it does not, re-run the release-candidate
      workflow on the merged `main` and use that artifact instead.

## 7. Create the GitHub prerelease

```sh
gh release create v0.1.0-rc1 \
  --repo occurframe/occurframe \
  --title "Occurframe 0.1.0-rc1" \
  --notes-file release-notes/0.1.0-rc1.md \
  --prerelease \
  --draft
```

- [ ] `--prerelease` is set. This is not v1.0.
- [ ] Create it as a **draft** first, attach and verify artifacts, then publish.

## 8. Attach verified artifacts

Download the artifact from the release-candidate run, verify it locally *before*
uploading, and upload only what you verified.

```sh
gh run download <run-id> --repo occurframe/occurframe --name occurframe-0.1.0-rc1 --dir ./staging
cd ./staging

sha256sum -c occurframe-0.1.0-rc1.tar.gz.sha256
tar -xzf occurframe-0.1.0-rc1.tar.gz
( cd occurframe-0.1.0-rc1 && sha256sum -c SHA256SUMS )

python3 <tooling checkout>/tests/clean-room/verify_release.py \
  --bundle occurframe-0.1.0-rc1 --target x86_64-unknown-linux-gnu
```

- [ ] Archive checksum verifies.
- [ ] Every packaged file matches `SHA256SUMS`.
- [ ] The clean-room suite passes on at least the platform you are on.
- [ ] `release-manifest.json` names the tag's commit and the expected corpus and
      certification digests.

```sh
gh release upload v0.1.0-rc1 --repo occurframe/occurframe \
  occurframe-0.1.0-rc1.tar.gz \
  occurframe-0.1.0-rc1.tar.gz.sha256 \
  release-attestation.json
```

- [ ] Publish the draft release only after the uploads are listed and correct.

## 9. Verify the published checksums

From a clean directory, as an outside consumer would:

```sh
gh release download v0.1.0-rc1 --repo occurframe/occurframe --dir ./published
cd ./published
sha256sum -c occurframe-0.1.0-rc1.tar.gz.sha256
sha256sum occurframe-0.1.0-rc1.tar.gz
```

- [ ] The published archive digest equals `archive_sha256` in
      `release-attestation.json`.
- [ ] Extract and re-run the clean-room suite against the published artifact.

## 10. crates.io (optional, later)

crates.io publication is **not required** for this prerelease and should not be
part of it.

`occurframe-wire` and `occurframe-conformance` are the only crates currently
publishable; `occurframe-cli`, `occurframe-report`, `occurframe-runner` and
`xtask` are `publish = false` and intentionally implementation-private. If a
future release publishes, do it in dependency order (`occurframe-wire`, then
`occurframe-conformance`) and only after `cargo package` succeeds for each on a
clean checkout of the tagged commit.

## What must not happen

- [ ] No stable `v1.0.0` or `corpus-1.0.0` tag is created.
- [ ] No release is published without `--prerelease`.
- [ ] No auto-merge is enabled on either review PR.
- [ ] No corpus vector, expectation or canonical digest changes as part of
      publication.
- [ ] CI publishes nothing automatically; if a workflow ever gains a publish
      step, that is a change to review, not a convenience to accept.
