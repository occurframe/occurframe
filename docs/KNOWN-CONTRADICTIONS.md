# Known contradictions

Tensions are recorded here rather than smoothed over. Resolved entries keep their
place, pointing at the formal decision, so that a reader who remembers the
contradiction can find out how it was settled.

## Reserved CLI names versus the no-engine gate — RESOLVED

**Resolved by ERRATA-001**, recorded in the corpus at
[`spec/ERRATA.md`](https://github.com/occurframe/corpus/blob/dev/spec/ERRATA.md)
and carried by specification version `1.0.0-rc1`.

Research II froze `test`, `explain`, `classify` and `occurrences` as the public
command names while its own decision gate explicitly did not authorise a
production recurrence engine in any language. Three of the four could not be
implemented without one: `occurrences` emits occurrences outright, `classify`
needs a parser and evaluator for each cron dialect, and `explain` must decide
what a schedule denotes and which policy axes are reachable. `test` needs none of
that — it measures an external engine's answers against authored expectations.

The precedence rule applied is that a final verdict and its explicit prohibition
govern lower-level interface text that cannot be implemented without violating
it. Occurframe v1 therefore ships one semantic command, `test`, and the other
three are deferred behind the unchanged engine gate: not implemented, not
advertised in help, not redefined into corpus or report commands to preserve
their names, and not backed by `cron_ref.py`, an incumbent engine, an arbitrary
adapter or a partial evaluator. Their frozen semantics are preserved verbatim in
the specification so the gate can be walked without reopening research.

What remains open is not a contradiction but a condition: the engine gate itself.
It is unchanged and closed, and reads *"a named maintainer of a named project
commits, in writing and in public, to adopt an Occurframe engine at a specified
integration seam."*

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
