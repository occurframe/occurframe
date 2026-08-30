# Ruby provenance investigation

The RC2 full-certification milestone made one bounded attempt to recover the
historical `concurrent-ruby` checkout used by the Phase II fugit and ice_cube
measurements. The attempt did not recover evidence strong enough to reproduce
that dependency without substitution.

Evidence inspected:

- `legacy/phase2-rc1/engines/PROVENANCE.md`, the RC1 Ruby runner, and all RC1
  Ruby raw observations;
- the exact recorded commits for fugit (`efda655251c2…`), et-orbi
  (`4725bc964c76…`), raabro (`af88c0117167…`), ice_cube
  (`32ff145baf15…`), and tzinfo (`ca5752c4b175…`);
- gemspecs, Gemfiles, repository trees, tags, and commit history at those exact
  revisions;
- the concurrent-ruby tag and commit history that existed at the Phase II run
  date.

The exact tzinfo revision declares only `concurrent-ruby ~> 1.0`. None of the
recorded engine/dependency revisions contains a lockfile identifying the
resolved version or commit. RC1 preserved neither the dependency checkout's
Git metadata nor a runtime version string. Concurrent-ruby `v1.3.8`
(`0b88d5ff75f6…`) was the newest upstream tag at the mission date, but choosing
it would infer that the historical workspace followed the then-current default
branch. That is not reproducible historical provenance.

Accordingly, `ruby.fugit` and `ruby.ice_cube` remain
`unreproducible_provenance`. No concurrent-ruby version, newer engine, or
alternate Ruby runtime has been substituted.
