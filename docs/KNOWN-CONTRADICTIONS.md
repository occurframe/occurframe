# Known contradictions

Unresolved tensions are recorded here rather than smoothed over. Each one is
still open.

## Reserved CLI names versus the no-engine gate

Research II freezes `test`, `explain`, `classify` and `occurrences` as the only
public command names. It also closes the production recurrence-engine gate. The
frozen meanings of `explain`, `classify` and especially `occurrences` appear to
require evaluator behaviour, so implementing them would conflict with
**GO — ORACLE ONLY**.

`0.1.0-rc1` implements only `test`, which observes external engines through
protocol `2.0` and scores against authored corpus expectations. The remaining
names are reserved and return a not-yet-available usage error. No substitute
meanings, redirects or hidden evaluators are supplied.

This contradiction remains unresolved and must be addressed through explicit
product doctrine before any reserved command can be implemented. A user reading
the help text will see four names and find one that works; that is the honest
presentation of the current state, not a plan.

## Historical Ruby build provenance

`ruby.fugit` and `ruby.ice_cube` remain `unreproducible_provenance` because
Phase II did not record the exact historical `concurrent-ruby` dependency
required by `tzinfo`. No contemporary dependency, engine or Ruby version was
substituted.

The consequence is that the certified population is 23 measured builds out of 25
configured. This is an evidence-population limitation, not a semantic-authority
change, and it is stated in the public differential report rather than hidden by
quietly reporting 23 as the configured total.

## Dual licensing versus a single-license statement

The Occurframe Rust and tooling code is published under `Apache-2.0 OR MIT`, and
both texts ship in every release. Release planning has at times described this
code as "Apache-2.0".

These are not in conflict for a consumer — `OR` is a choice, so anyone who
requires Apache-2.0 may simply take Apache-2.0 — but they are not the same
statement, and a downstream licence scanner will report the dual expression.
The release therefore states the dual licence accurately in `LICENSES.md` and
`release-manifest.json` rather than narrowing it in documentation while the crate
metadata says otherwise. Narrowing to Apache-2.0 alone would be a deliberate
licensing decision affecting existing recipients, and is an owner decision, not a
packaging one. It is recorded here as unresolved.

## `--no-color` accepted but inert

`--no-color` and `NO_COLOR` are parsed and accepted, but the current text
renderer emits no ANSI colour, so neither has an observable effect. Accepting
them now means a future coloured renderer cannot break existing invocations;
until then the documentation says plainly that they do nothing rather than
implying colour exists.
