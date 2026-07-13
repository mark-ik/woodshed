# woodshed-graph

woodshed's theory catalog as a [chartulary](https://github.com/mark-ik/chartulary)
graph. The first real consumer of the container-graph substrate (G3): it reads
woodshed's *existing* catalog (scales, chords, progressions, exercises) and relates
it, then records practice lineage over
[stemma](https://github.com/mark-ik/stemma). No data is invented.

- **Nodes** are theory objects, one `Container` each (`chord:Major 7`,
  `scale:Major`), titled and tagged by kind and category.
- **Edges** are woodshed's own relation family (the app-private inner ring):
  - a progression `Contains` a chord, read from the progression's roles, and
  - a chord `FitsInScale` a scale, computed from the real interval formulas (the
    chord's pitch classes are a subset of the scale's).

```rust
use woodshed_graph::{build_catalog_graph, chord_id, scale_id, FAMILY, FITS_IN_SCALE};
use chartulary::RelationClass;

let graph = build_catalog_graph("woodshed:catalog");
let g = graph.graph();

// Which scales does a major-seventh chord fit in?
let cmaj7 = g.key_of(&chord_id("Major 7")).unwrap();
let fits = RelationClass::app(FAMILY, FITS_IN_SCALE);
for (_, scale) in g.out_edges_of_class(cmaj7, &fits) {
    println!("{}", g.node(scale).unwrap().title.as_deref().unwrap());
}
```

A user can fork the shared catalog into their own graph, add a personal chord, and
the fork carries provenance back to the catalog while leaving it untouched. A
practice session is an `Owner` in stemma whose visits branch, keyed by the same
node ids as the graph, which is how the two layers integrate: by shared identity,
consumer-recorded, not by coupling the graph spine to the lineage.

Depends on the pure `woodshedding` theory crate (no UI, audio, or genet), plus
chartulary and stemma. See [`design_docs/`](design_docs/).

Part of the [woodshed](https://github.com/mark-ik/woodshed) workspace; dual MIT OR Apache-2.0.
