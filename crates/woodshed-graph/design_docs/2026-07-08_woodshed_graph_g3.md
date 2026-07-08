# woodshed-graph: G3, the first consumer

**Date:** 2026-07-08
**Status:** G3 landed. The first real consumer of the chartulary substrate.
Canonical program plan in mere:
`design_docs/mere_docs/technical_architecture/2026-07-08_generic_graph_substrate_plan.md`.

## What G3 is

The plan's G3: put real user data on the substrate and prove the stack end to end,
which also settles the stemma-wiring question. Done (6 tests). woodshed was chosen
over isometry deliberately: relating *existing* catalog data validates the model
more honestly than demo data shaped to fit it.

The catalog is woodshed's real theory data, read from the pure `woodshedding` crate:

- **Nodes:** every scale, chord, progression, and exercise in the catalog, one
  `Container` each (`kind:name` id, titled, tagged by kind and category).
- **Edges (woodshed's app-private family):**
  - `Contains`: a progression to each distinct chord its roles call for, read
    straight from the progression's `ChordRole` list.
  - `FitsInScale`: a chord to a scale, computed from the real interval formulas
    (the chord's pitch classes mod 12 are a subset of the scale's).

Neither relation is invented: one is read from the data, one is computed over it.
The tests assert real facts (ii-V-I contains a major 7, a major 7 fits the major
scale).

## What it proved about the substrate

- **The container model fits a real domain** without bending. A theory object maps
  cleanly to a `Container` (id, title, tags); nothing was forced.
- **The app-private ring is the right home** for `Contains` / `FitsInScale`. These
  are woodshed's own relations, not shared-semantic knowledge relations, so they
  live in the `RelationClass::app("woodshed", kind)` inner ring and do not project
  to RDF. The two-ring split earned its keep on first contact.
- **Fork lineage is meaningful on real data.** A user forks the shared catalog into
  a private graph, adds a chord, and the fork carries provenance back to
  `woodshed:catalog` while the catalog is untouched. This is the "shared reference
  data plus personal additions" pattern, exactly what a catalog wants.

## Open question 6, settled: consumer-side wiring

The plan asked whether the graph spine should auto-feed stemma (a visit per edit)
or whether wiring is consumer-side. G3 settles it: **consumer-side.** A practice
session is a stemma `Owner`; its visits to theory objects are recorded by the
consumer with its own "engagement" semantics (drilling a chord, not editing the
graph), keyed by the *same node ids* as the chartulary graph. The two layers
integrate by shared identity, not by coupling the spine to lineage. An edit to the
graph is not a visit; a practice engagement is. Only the consumer knows which is
which, so only the consumer records visits.

This keeps chartulary and stemma decoupled (chartulary has no stemma dependency)
and lets each app define what lineage means for it. mere's browser navigation,
woodshed's practice, isometry's session play would each feed stemma differently,
over the same shared node identities.

## Housing

A standalone crate, not a member of woodshed's workspace, so it entangles no build.
It path-deps the pure `woodshedding` crate (whose workspace deps resolve against
woodshed's own workspace), plus chartulary and stemma. If it ever becomes a shipped
woodshed feature, it moves into that workspace; as a substrate proof it stands
alone.

## Not yet

G4 is scholia, the RDF projection over the semantic ring. G5 is mere's re-base onto
the substrate.
