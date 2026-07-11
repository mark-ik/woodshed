//! woodshed-graph — woodshed's theory catalog as a chartulary graph.
//!
//! The first real consumer of the chartulary substrate (G3). It reads woodshed's
//! *existing* theory catalog (scales, chords, progressions, exercises) and relates
//! it as a content-addressed container graph, then demonstrates practice lineage
//! over stemma. No data is invented: the relations are read from or computed over
//! the real catalog.
//!
//! - **Nodes** are theory objects. Each is a [`Container`] whose id is
//!   `kind:name` (`chord:Major 7`, `scale:Major`), titled by its name and tagged
//!   with its kind and category.
//! - **Edges** are woodshed's own relation family (the app-private "inner ring",
//!   [`RelationClass::app`]):
//!   - a progression `Contains` a chord (read from the progression's roles), and
//!   - a chord `FitsInScale` a scale (computed: the chord's pitch classes are a
//!     subset of the scale's).
//!
//! Practice lineage lives in stemma, keyed by the same node ids: a practice
//! session is an `Owner` whose visits to theory objects branch and are recorded.
//! The spine does not auto-feed stemma; the consumer records visits with its own
//! "engagement" semantics, keyed by the shared identity. That is how the two
//! layers integrate (substrate plan open question 6, settled here: consumer-side).

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use chartulary::{Container, GraphLog, LogId, Relation, RelationClass};
use woodshedding::{chord, exercise, progression, scale};

/// woodshed's app-private relation family name.
pub const FAMILY: &str = "woodshed";
/// A progression contains a chord (kind 0 of the woodshed family).
pub const CONTAINS: u16 = 0;
/// A chord fits within a scale (kind 1 of the woodshed family).
pub const FITS_IN_SCALE: u16 = 1;

/// Catalog kind carried by a related-material result. This deliberately names
/// catalog identities rather than a configured practice Card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogKind {
    Scale,
    Chord,
    Progression,
    Exercise,
}

/// One explainable neighbor of a catalog object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedMaterial {
    pub id: String,
    pub name: String,
    pub kind: CatalogKind,
    pub reason: String,
}

/// The node id for a scale.
pub fn scale_id(name: &str) -> String {
    format!("scale:{name}")
}
/// The node id for a chord.
pub fn chord_id(name: &str) -> String {
    format!("chord:{name}")
}
/// The node id for a progression.
pub fn progression_id(name: &str) -> String {
    format!("progression:{name}")
}
/// The node id for an exercise.
pub fn exercise_id(name: &str) -> String {
    format!("exercise:{name}")
}

fn relation(kind: u16) -> Relation {
    Relation::new(RelationClass::app(FAMILY, kind))
}

/// The pitch classes (semitones mod 12) an interval list touches.
fn pitch_classes<'a>(intervals: impl IntoIterator<Item = &'a woodshedding::interval::Interval>) -> BTreeSet<i32> {
    intervals
        .into_iter()
        .map(|interval| interval.semitones().rem_euclid(12))
        .collect()
}

/// Build the theory catalog as a chartulary graph, log-backed under `log_id` so a
/// user's fork descends from it with provenance.
pub fn build_catalog_graph(log_id: &str) -> GraphLog<Container, Relation> {
    let mut graph = GraphLog::with_id(LogId::new(log_id));

    // Nodes, one per catalog object. Insert all before connecting.
    for s in scale::catalog() {
        graph.insert_node(
            Container::new(scale_id(s.name))
                .with_title(s.name)
                .with_tag("scale")
                .with_tag(format!("{:?}", s.category)),
        );
    }
    for c in chord::catalog() {
        graph.insert_node(
            Container::new(chord_id(c.name))
                .with_title(c.name)
                .with_tag("chord")
                .with_tag(format!("{:?}", c.category)),
        );
    }
    for p in progression::catalog() {
        graph.insert_node(
            Container::new(progression_id(p.name))
                .with_title(p.name)
                .with_tag("progression")
                .with_tag(format!("{:?}", p.category)),
        );
    }
    for e in exercise::catalog() {
        graph.insert_node(
            Container::new(exercise_id(e.name))
                .with_title(e.name)
                .with_tag("exercise")
                .with_tag(format!("{:?}", e.category)),
        );
    }

    // Edges: a progression contains each distinct chord its roles call for.
    for p in progression::catalog() {
        let mut seen = BTreeSet::new();
        for role in p.roles {
            let chord_name = role.quality.chord_formula_name();
            if seen.insert(chord_name) {
                graph.connect(&progression_id(p.name), &chord_id(chord_name), relation(CONTAINS));
            }
        }
    }

    // Edges: a chord fits a scale when its pitch classes are a subset of the
    // scale's. Computed from the real interval formulas.
    for c in chord::catalog() {
        let chord_pcs = pitch_classes(c.intervals);
        for s in scale::catalog() {
            let scale_pcs = pitch_classes(s.intervals);
            if chord_pcs.is_subset(&scale_pcs) {
                graph.connect(&chord_id(c.name), &scale_id(s.name), relation(FITS_IN_SCALE));
            }
        }
    }

    graph
}

fn kind_of(node: &Container) -> Option<CatalogKind> {
    if node.tags.iter().any(|tag| tag == "scale") {
        Some(CatalogKind::Scale)
    } else if node.tags.iter().any(|tag| tag == "chord") {
        Some(CatalogKind::Chord)
    } else if node.tags.iter().any(|tag| tag == "progression") {
        Some(CatalogKind::Progression)
    } else if node.tags.iter().any(|tag| tag == "exercise") {
        Some(CatalogKind::Exercise)
    } else {
        None
    }
}

/// Return the selected catalog object's immediate musical neighbors with a
/// human-readable explanation. Results are deterministic and derive only from
/// catalog truth; personalized practice-history ranking is layered on later.
pub fn related_material(selected_id: &str, limit: usize) -> Vec<RelatedMaterial> {
    static INDEX: OnceLock<HashMap<String, Vec<RelatedMaterial>>> = OnceLock::new();
    INDEX
        .get_or_init(build_related_index)
        .get(selected_id)
        .into_iter()
        .flatten()
        .take(limit)
        .cloned()
        .collect()
}

fn build_related_index() -> HashMap<String, Vec<RelatedMaterial>> {
    let log = build_catalog_graph("woodshed:catalog:related");
    let graph = log.graph();
    let mut index: HashMap<String, Vec<RelatedMaterial>> = HashMap::new();

    for (source, source_node) in graph.nodes() {
        let source_name = source_node
            .title
            .clone()
            .unwrap_or_else(|| source_node.id.clone());
        let Some(source_kind) = kind_of(source_node) else { continue };
        for (_, target, edge) in graph.out_edges(source) {
            let Some(target_node) = graph.node(target) else { continue };
            let Some(target_kind) = kind_of(target_node) else { continue };
            let target_name = target_node
                .title
                .clone()
                .unwrap_or_else(|| target_node.id.clone());
            let (forward_reason, reverse_reason) = match edge.class {
                ref class if class == &RelationClass::app(FAMILY, CONTAINS) => {
                    (format!("Used in {source_name}"), format!("Appears in {source_name}"))
                }
                ref class if class == &RelationClass::app(FAMILY, FITS_IN_SCALE) => {
                    (
                        format!("{source_name} fits this scale"),
                        format!("{source_name} fits {target_name}"),
                    )
                }
                _ => continue,
            };
            index.entry(source_node.id.clone()).or_default().push(RelatedMaterial {
                id: target_node.id.clone(),
                name: target_name,
                kind: target_kind,
                reason: forward_reason,
            });
            index.entry(target_node.id.clone()).or_default().push(RelatedMaterial {
                id: source_node.id.clone(),
                name: source_name.clone(),
                kind: source_kind,
                reason: reverse_reason,
            });
        }
    }

    for related in index.values_mut() {
        related.sort_by(|a, b| {
            (a.kind, a.name.as_str(), a.reason.as_str())
                .cmp(&(b.kind, b.name.as_str(), b.reason.as_str()))
        });
        related.dedup_by(|a, b| a.id == b.id && a.reason == b.reason);
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graph_holds_the_whole_catalog() {
        let graph = build_catalog_graph("woodshed:catalog");
        let expected = scale::catalog().len()
            + chord::catalog().len()
            + progression::catalog().len()
            + exercise::catalog().len();
        assert_eq!(graph.graph().node_count(), expected);
        // A well-known object is present, found by its stable id.
        assert!(graph.graph().get(&chord_id("Major 7")).is_some());
        assert!(graph.graph().get(&scale_id("Major")).is_some());
    }

    #[test]
    fn a_progression_contains_its_chords() {
        let graph = build_catalog_graph("woodshed:catalog");
        let g = graph.graph();
        // ii-V-I calls for a minor 7, a dominant 7, and a major 7.
        let prog = g
            .key_of(&progression_id("ii-V-I (Jazz)"))
            .expect("the ii-V-I progression is in the catalog");
        let contains = RelationClass::app(FAMILY, CONTAINS);
        let chords: BTreeSet<String> = g
            .out_edges_of_class(prog, &contains)
            .map(|(_, target)| g.node(target).unwrap().id.clone())
            .collect();
        assert!(chords.contains(&chord_id("Major 7")), "I is a major 7");
        assert!(chords.contains(&chord_id("Dominant 7")), "V is a dominant 7");
        assert!(chords.contains(&chord_id("Minor 7")), "ii is a minor 7");
    }

    #[test]
    fn a_major_seventh_fits_the_major_scale() {
        let graph = build_catalog_graph("woodshed:catalog");
        let g = graph.graph();
        let cmaj7 = g.key_of(&chord_id("Major 7")).unwrap();
        let fits = RelationClass::app(FAMILY, FITS_IN_SCALE);
        let scales: BTreeSet<String> = g
            .out_edges_of_class(cmaj7, &fits)
            .map(|(_, target)| g.node(target).unwrap().id.clone())
            .collect();
        // 1-3-5-7 sits inside the major scale.
        assert!(scales.contains(&scale_id("Major")), "Major 7 fits the Major scale");
    }

    #[test]
    fn tags_partition_the_catalog_by_kind() {
        let graph = build_catalog_graph("woodshed:catalog");
        let g = graph.graph();
        assert_eq!(g.nodes_tagged("chord").count(), chord::catalog().len());
        assert_eq!(g.nodes_tagged("scale").count(), scale::catalog().len());
    }

    #[test]
    fn a_user_forks_the_catalog_to_add_a_personal_chord() {
        let catalog = build_catalog_graph("woodshed:catalog");
        let before = catalog.graph().node_count();

        // A user forks the shared catalog into their own graph and adds a chord.
        let mut mine = catalog.fork(LogId::new("mark:private"));
        mine.insert_node(
            Container::new(chord_id("Mark's Cluster"))
                .with_title("Mark's Cluster")
                .with_tag("chord"),
        );
        mine.connect(
            &chord_id("Mark's Cluster"),
            &scale_id("Chromatic"),
            relation(FITS_IN_SCALE),
        );

        // The fork has the addition and knows it descends from the catalog.
        assert!(mine.graph().get(&chord_id("Mark's Cluster")).is_some());
        let provenance = mine.provenance().expect("a fork carries provenance");
        assert_eq!(provenance.source, Some(LogId::new("woodshed:catalog")));

        // The shared catalog is untouched.
        assert_eq!(catalog.graph().node_count(), before);
        assert!(catalog.graph().get(&chord_id("Mark's Cluster")).is_none());
    }

    #[test]
    fn the_catalog_projects_names_but_keeps_private_relations_private() {
        let graph = build_catalog_graph("woodshed:catalog");
        let quads = scholia::to_quads(graph.graph());

        // woodshed's relations (Contains, FitsInScale) are its private family, so
        // every projected triple is a curated literal; not one music relation
        // leaks into RDF.
        assert!(
            quads
                .iter()
                .all(|q| q.predicate == scholia::SCHEMA_NAME || q.predicate == scholia::SCHEMA_KEYWORDS),
            "only schema.org literals project; the app ring stays private"
        );
        // A known chord's title reaches RDF as schema:name.
        assert!(quads.iter().any(|q| q.predicate == scholia::SCHEMA_NAME
            && matches!(&q.object, scholia::Term::Literal { value, .. } if value == "Major 7")));
    }

    #[test]
    fn a_practice_session_records_lineage_over_the_same_node_ids() {
        use stemma::{EntryPrivacy, Stemma, TransitionKind};

        // The lineage is keyed by the SAME identities as the chartulary graph, so
        // the two layers integrate by shared id, not by coupling. The consumer
        // decides what a "visit" is (here, a practice engagement) and records it;
        // the graph spine does not auto-feed stemma.
        let mut lineage: Stemma<String, String, String, ()> = Stemma::new();
        let session = lineage.ensure_owner("practice:2026-07-08".to_string(), None);

        let drilled = [
            (chord_id("Major 7"), "Major 7"),
            (scale_id("Major"), "Major"),
            (chord_id("Minor 7"), "Minor 7"),
        ];
        for (at, (id, title)) in drilled.iter().enumerate() {
            let entry = lineage.resolve_or_create_entry(
                id.clone(),
                title.to_string(),
                at as u64,
                EntryPrivacy::LocalOnly,
            );
            lineage
                .visit_entry(session, entry, (), TransitionKind::LinkClick, at as u64)
                .expect("the session and entry exist");
        }

        assert_eq!(lineage.visit_count(), 3, "three practice engagements");
        assert_eq!(lineage.entry_count(), 3);
        // The lineage entry is the graph node, by identity.
        assert!(lineage.entry_id_by_key(&chord_id("Major 7")).is_some());
    }

    #[test]
    fn related_material_explains_both_directions() {
        let chord = related_material(&chord_id("Major 7"), 64);
        assert!(chord.iter().any(|item| {
            item.id == scale_id("Major") && item.reason.contains("fits this scale")
        }));
        assert!(chord.iter().any(|item| {
            item.id == progression_id("ii-V-I (Jazz)")
                && item.reason.contains("Appears in")
        }));

        let scale = related_material(&scale_id("Major"), 64);
        assert!(scale.iter().any(|item| {
            item.id == chord_id("Major 7") && item.reason.contains("fits Major")
        }));
    }
}
