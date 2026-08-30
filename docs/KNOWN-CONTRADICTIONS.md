# Known contradictions

## Reserved CLI names versus the no-engine gate

Research II freezes `test`, `explain`, `classify`, and `occurrences` as the only public command names. It also closes the production recurrence-engine gate. The frozen meanings of `explain`, `classify`, and especially `occurrences` appear to require evaluator behavior, so implementing them would conflict with **GO — ORACLE ONLY**.

The `0.1.0-rc1` product implements only `test`, which observes external engines through protocol 2.0 and scores against authored corpus expectations. The remaining names are reserved and return a not-yet-available usage error. No substitute meanings, redirects, or hidden evaluators are supplied.

This contradiction remains unresolved and must be addressed through explicit product doctrine before any reserved command can be implemented.

## Historical Ruby build provenance

`ruby.fugit` and `ruby.ice_cube` remain `unreproducible_provenance` because Phase II did not record the exact historical `concurrent-ruby` dependency required by `tzinfo`. No contemporary dependency, engine, or Ruby version was substituted. This is an evidence-population limitation, not a semantic-authority change.
