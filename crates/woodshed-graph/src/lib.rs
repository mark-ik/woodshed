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

use chartulary::{Author, Container, GraphLog, LogId, Relation, RelationClass};
use woodshedding::{chord, exercise, progression, scale};

/// woodshed's app-private relation family name.
pub const FAMILY: &str = "woodshed";
/// A progression contains a chord (kind 0 of the woodshed family).
pub const CONTAINS: u16 = 0;
/// A chord fits within a scale (kind 1 of the woodshed family).
pub const FITS_IN_SCALE: u16 = 1;
/// An arpeggio is the sequential realization of a chord formula.
pub const REALIZES: u16 = 2;

/// Catalog kind carried by a related-material result. This deliberately names
/// catalog identities rather than a configured practice Card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogKind {
    Scale,
    Chord,
    Arpeggio,
    Progression,
    Exercise,
}

impl CatalogKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scale => "Scale",
            Self::Chord => "Chord",
            Self::Arpeggio => "Arpeggio",
            Self::Progression => "Progression",
            Self::Exercise => "Exercise",
        }
    }
}

/// One material in the catalog graph, projected without its chartulary key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogMaterial {
    pub id: String,
    pub name: String,
    pub kind: CatalogKind,
}

/// Who says so. A relation's authority travels with it, because a filter that
/// hides one layer must not be able to delete another's facts, and a suggestion
/// must never read as catalog truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationAuthority {
    /// Authored in the catalog and read back (a progression's roles).
    Catalog,
    /// Derived from catalog truth by a deterministic rule. Reproducible from
    /// the formulas alone, with no history and no inference.
    Computed,
    /// Observed practice. Produced by the consumer that owns the history, not
    /// by this crate; the kind exists here so one record type spans the layers.
    Evidence,
}

/// The relation vocabulary, root-independent by construction.
///
/// Every kind here relates catalog *formulas*, which is why none of them names
/// a key: `Major 7` extends `Major` whatever the tonic. The keyed relations the
/// plan also wants (diatonic in, borrowed from, dominant of, resolves to,
/// relative/parallel) are contextual — they exist only once a tonic is chosen —
/// so they belong to the keyed-instance layer, beside first-class pitch-class
/// identities, rather than being faked at formula level here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationKind {
    /// A progression calls for this chord.
    ContainsMaterial,
    /// This chord is called for by that progression.
    AppearsIn,
    /// This chord's tones are a subset of that scale's.
    FitsInScale,
    /// That scale admits this chord.
    ScaleAdmits,
    /// An arpeggio is the sequential realization of a chord formula.
    Realizes,
    /// A chord formula is realized by that arpeggio.
    RealizedBy,
    /// This chord's tones are a strict subset of that chord's (Major -> Major 7).
    ExtendedBy,
    /// This chord strictly contains that chord's tones.
    Extends,
    /// Same tone count, one tone moved: an alteration of the same colour.
    Alters,
    /// The two share pitch classes; the count rides on the record.
    SharesTones,
    /// Symmetric voice-leading distance; the distance rides on the record.
    VoiceLeadsTo,
    /// One scale is a rotation of the other's interval set.
    ModeOf,
    /// Same degree count, differing by exactly one tone.
    ScaleNeighbor,
    /// Two chords sit next to each other inside a catalog progression.
    UsedTogether,
    /// Observed: this was practiced before that (consumer-supplied).
    PracticedBefore,
    /// Observed: this was practiced after that (consumer-supplied).
    PracticedAfter,
}

impl RelationKind {
    /// Short human name, for an edge explanation or a relation filter.
    pub fn label(self) -> &'static str {
        match self {
            Self::ContainsMaterial => "contains",
            Self::AppearsIn => "appears in",
            Self::FitsInScale => "fits in scale",
            Self::ScaleAdmits => "admits",
            Self::Realizes => "realizes",
            Self::RealizedBy => "realized by",
            Self::ExtendedBy => "extended by",
            Self::Extends => "extends",
            Self::Alters => "alters",
            Self::SharesTones => "shares tones",
            Self::VoiceLeadsTo => "voice-leads to",
            Self::ModeOf => "mode of",
            Self::ScaleNeighbor => "one tone apart",
            Self::UsedTogether => "used together",
            Self::PracticedBefore => "practiced before",
            Self::PracticedAfter => "practiced after",
        }
    }

    /// Which layer owns it. The Set layer is not here: Set order is the Set's
    /// own truth, not a catalog relation.
    pub fn authority(self) -> RelationAuthority {
        match self {
            Self::ContainsMaterial | Self::AppearsIn | Self::UsedTogether => {
                RelationAuthority::Catalog
            }
            Self::PracticedBefore | Self::PracticedAfter => RelationAuthority::Evidence,
            _ => RelationAuthority::Computed,
        }
    }

    /// The kind naming the same fact from the other end. Symmetric kinds are
    /// their own inverse, which is what lets one computation record both
    /// directions without inventing a second relation.
    pub fn inverse(self) -> Self {
        match self {
            Self::ContainsMaterial => Self::AppearsIn,
            Self::AppearsIn => Self::ContainsMaterial,
            Self::FitsInScale => Self::ScaleAdmits,
            Self::ScaleAdmits => Self::FitsInScale,
            Self::Realizes => Self::RealizedBy,
            Self::RealizedBy => Self::Realizes,
            Self::ExtendedBy => Self::Extends,
            Self::Extends => Self::ExtendedBy,
            Self::PracticedBefore => Self::PracticedAfter,
            Self::PracticedAfter => Self::PracticedBefore,
            symmetric => symmetric,
        }
    }

    pub fn is_symmetric(self) -> bool {
        self.inverse() == self
    }

    /// Every deterministic kind this crate can currently derive, for a relation
    /// filter that wants to name its families rather than count them.
    pub const DETERMINISTIC: [Self; 14] = [
        Self::ContainsMaterial,
        Self::AppearsIn,
        Self::FitsInScale,
        Self::ScaleAdmits,
        Self::Realizes,
        Self::RealizedBy,
        Self::ExtendedBy,
        Self::Extends,
        Self::Alters,
        Self::SharesTones,
        Self::VoiceLeadsTo,
        Self::ModeOf,
        Self::ScaleNeighbor,
        Self::UsedTogether,
    ];
}

/// One typed relation between two catalog materials.
///
/// This replaces the old flattened `{ reason, score }` pair. The difference
/// that matters: several of these may connect the same pair, and ranking
/// chooses what to show first without deleting the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialRelation {
    pub source: String,
    pub target: String,
    pub kind: RelationKind,
    /// This relation's own strength in `0..=100`. Not a ranking of the
    /// neighbor: one pair's several relations each carry their own.
    pub weight: u16,
    /// Continuous distance where the relation has one (voice-leading motion).
    pub distance: Option<f32>,
    /// Shared pitch-class count where the relation is about tones.
    pub shared_tones: Option<usize>,
    /// Why, in the player's words.
    pub explanation: String,
    pub authority: RelationAuthority,
}

impl MaterialRelation {
    fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: RelationKind,
        weight: u16,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            kind,
            weight,
            distance: None,
            shared_tones: None,
            explanation: explanation.into(),
            authority: kind.authority(),
        }
    }

    /// Build an observed relation. Public because the evidence layer is the
    /// consumer's: this crate owns no history and must not pretend to. The kind
    /// must be an [`RelationAuthority::Evidence`] one, so an observation cannot
    /// enter the record set wearing catalog authority.
    pub fn evidence(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: RelationKind,
        weight: u16,
        explanation: impl Into<String>,
    ) -> Self {
        debug_assert_eq!(
            kind.authority(),
            RelationAuthority::Evidence,
            "an observed relation must use an evidence kind"
        );
        Self {
            authority: RelationAuthority::Evidence,
            ..Self::new(source, target, kind, weight, explanation)
        }
    }

    fn with_distance(mut self, distance: f32) -> Self {
        self.distance = Some(distance);
        self
    }

    fn with_shared_tones(mut self, shared: usize) -> Self {
        self.shared_tones = Some(shared);
        self
    }
}

/// One neighbor of a focused material, with **every** relation that connects
/// them. `score` orders neighbors for display; it never decides which relations
/// survive.
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedNeighbor {
    pub id: String,
    pub name: String,
    pub kind: CatalogKind,
    /// Strongest first. Always at least one.
    pub relations: Vec<MaterialRelation>,
    /// Display rank in `0..=100`: the strongest relation, plus a small bonus
    /// for carrying several, so a pair related four ways outranks a pair
    /// related once at the same strength.
    pub score: u16,
}

impl RelatedNeighbor {
    /// The relation shown when there is room for one.
    pub fn primary(&self) -> &MaterialRelation {
        &self.relations[0]
    }

    /// The primary explanation, which is what a one-line row shows.
    pub fn reason(&self) -> &str {
        &self.primary().explanation
    }

    /// Every applicable relation kind, in display order.
    pub fn kinds(&self) -> Vec<RelationKind> {
        self.relations.iter().map(|r| r.kind).collect()
    }

    /// Every explanation, which is what selecting the edge should expose.
    pub fn explanations(&self) -> Vec<&str> {
        self.relations
            .iter()
            .map(|r| r.explanation.as_str())
            .collect()
    }
}

/// The node id for a scale.
pub fn scale_id(name: &str) -> String {
    format!("scale:{name}")
}
/// The node id for a chord.
pub fn chord_id(name: &str) -> String {
    format!("chord:{name}")
}
/// The node id for an arpeggio realization of a chord formula.
pub fn arpeggio_id(name: &str) -> String {
    format!("arpeggio:{name}")
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
fn pitch_classes<'a>(
    intervals: impl IntoIterator<Item = &'a woodshedding::interval::Interval>,
) -> BTreeSet<i32> {
    intervals
        .into_iter()
        .map(|interval| interval.semitones().rem_euclid(12))
        .collect()
}

/// Build the theory catalog as a chartulary graph, log-backed under `log_id` so a
/// user's fork descends from it with provenance.
pub fn build_catalog_graph(log_id: &str) -> GraphLog<Container, Relation> {
    let mut graph = GraphLog::with_id(LogId::new(log_id));
    // chartulary edits are attributed now; the built-in theory catalog is
    // authored by the app itself, so every node and edge carries "catalog".
    let author = Author::new("catalog");

    // Nodes, one per catalog object. Insert all before connecting.
    for s in scale::catalog() {
        graph.insert_node(
            &author,
            Container::new(scale_id(s.name))
                .with_title(s.name)
                .with_tag("scale")
                .with_tag(format!("{:?}", s.category)),
        );
    }
    for c in chord::catalog() {
        graph.insert_node(
            &author,
            Container::new(chord_id(c.name))
                .with_title(c.name)
                .with_tag("chord")
                .with_tag(format!("{:?}", c.category)),
        );
        graph.insert_node(
            &author,
            Container::new(arpeggio_id(c.name))
                .with_title(c.name)
                .with_tag("arpeggio")
                .with_tag(format!("{:?}", c.category)),
        );
    }
    for p in progression::catalog() {
        graph.insert_node(
            &author,
            Container::new(progression_id(p.name))
                .with_title(p.name)
                .with_tag("progression")
                .with_tag(format!("{:?}", p.category)),
        );
    }
    for e in exercise::catalog() {
        graph.insert_node(
            &author,
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
                graph.connect(
                    &author,
                    &progression_id(p.name),
                    &chord_id(chord_name),
                    relation(CONTAINS),
                );
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
                graph.connect(
                    &author,
                    &chord_id(c.name),
                    &scale_id(s.name),
                    relation(FITS_IN_SCALE),
                );
                graph.connect(
                    &author,
                    &arpeggio_id(c.name),
                    &scale_id(s.name),
                    relation(FITS_IN_SCALE),
                );
            }
        }
        graph.connect(
            &author,
            &arpeggio_id(c.name),
            &chord_id(c.name),
            relation(REALIZES),
        );
    }

    graph
}

fn kind_of(node: &Container) -> Option<CatalogKind> {
    if node.tags.iter().any(|tag| tag == "scale") {
        Some(CatalogKind::Scale)
    } else if node.tags.iter().any(|tag| tag == "chord") {
        Some(CatalogKind::Chord)
    } else if node.tags.iter().any(|tag| tag == "arpeggio") {
        Some(CatalogKind::Arpeggio)
    } else if node.tags.iter().any(|tag| tag == "progression") {
        Some(CatalogKind::Progression)
    } else if node.tags.iter().any(|tag| tag == "exercise") {
        Some(CatalogKind::Exercise)
    } else {
        None
    }
}

/// The selected material's neighbors, each carrying every relation that
/// connects them. Deterministic: derived from catalog truth alone, with no
/// history and no inference. Practice evidence is layered by the consumer that
/// owns it, onto these same records.
pub fn related_neighbors(selected_id: &str, limit: usize) -> Vec<RelatedNeighbor> {
    relation_index()
        .get(selected_id)
        .into_iter()
        .flatten()
        .take(limit)
        .cloned()
        .collect()
}

/// Every relation between two materials, in display order. This is what
/// selecting an edge should show: all applicable reasons, each with its own
/// authority, rather than the one that happened to rank highest.
pub fn relations_between(source_id: &str, target_id: &str) -> Vec<MaterialRelation> {
    relation_index()
        .get(source_id)
        .into_iter()
        .flatten()
        .find(|neighbor| neighbor.id == target_id)
        .map(|neighbor| neighbor.relations.clone())
        .unwrap_or_default()
}

/// A catalog object's display name, if it exists.
pub fn material_name(id: &str) -> Option<String> {
    node_names().get(id).cloned()
}

/// Every material in the catalog graph, in stable id order. This is the
/// whole-mere source; focused views should use [`related_neighbors`] instead.
pub fn catalog_materials() -> Vec<CatalogMaterial> {
    static MATERIALS: OnceLock<Vec<CatalogMaterial>> = OnceLock::new();
    MATERIALS
        .get_or_init(|| {
            let log = build_catalog_graph("woodshed:catalog:materials");
            let mut materials = log
                .graph()
                .nodes()
                .filter_map(|(_, node)| {
                    Some(CatalogMaterial {
                        id: node.id.clone(),
                        name: if node.title.is_empty() {
                            node.id.clone()
                        } else {
                            node.title.clone()
                        },
                        kind: kind_of(node)?,
                    })
                })
                .collect::<Vec<_>>();
            materials.sort_by(|a, b| a.id.cmp(&b.id));
            materials
        })
        .clone()
}

fn relation_index() -> &'static HashMap<String, Vec<RelatedNeighbor>> {
    static INDEX: OnceLock<HashMap<String, Vec<RelatedNeighbor>>> = OnceLock::new();
    INDEX.get_or_init(build_relation_index)
}

fn node_names() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let log = build_catalog_graph("woodshed:catalog:names");
        log.graph()
            .nodes()
            .map(|(_, node)| {
                (
                    node.id.clone(),
                    if node.title.is_empty() {
                        node.id.clone()
                    } else {
                        node.title.clone()
                    },
                )
            })
            .collect()
    })
}

/// Accumulates relation records per (source, target) pair before ranking, so a
/// pair related four ways arrives at ranking as four records, not one.
#[derive(Default)]
struct RelationBuilder {
    by_source: HashMap<String, HashMap<String, Vec<MaterialRelation>>>,
}

impl RelationBuilder {
    /// Record `relation` and its inverse, so both ends see the pair.
    fn add(&mut self, relation: MaterialRelation) {
        let inverse = MaterialRelation {
            source: relation.target.clone(),
            target: relation.source.clone(),
            kind: relation.kind.inverse(),
            explanation: relation.explanation.clone(),
            ..relation.clone()
        };
        for record in [relation, inverse] {
            self.by_source
                .entry(record.source.clone())
                .or_default()
                .entry(record.target.clone())
                .or_default()
                .push(record);
        }
    }

    /// Record a relation whose two directions read differently.
    fn add_directed(
        &mut self,
        source: &str,
        target: &str,
        kind: RelationKind,
        weight: u16,
        forward: String,
        reverse: String,
    ) {
        let forward_record = MaterialRelation::new(source, target, kind, weight, forward);
        let reverse_record = MaterialRelation::new(target, source, kind.inverse(), weight, reverse);
        for record in [forward_record, reverse_record] {
            self.by_source
                .entry(record.source.clone())
                .or_default()
                .entry(record.target.clone())
                .or_default()
                .push(record);
        }
    }

    fn finish(
        self,
        names: &HashMap<String, String>,
        kinds: &HashMap<String, CatalogKind>,
    ) -> HashMap<String, Vec<RelatedNeighbor>> {
        let mut index: HashMap<String, Vec<RelatedNeighbor>> = HashMap::new();
        for (source, targets) in self.by_source {
            let mut neighbors: Vec<RelatedNeighbor> = targets
                .into_iter()
                .filter_map(|(target, mut relations)| {
                    let kind = *kinds.get(&target)?;
                    relations.sort_by(|a, b| {
                        (std::cmp::Reverse(a.weight), a.kind)
                            .cmp(&(std::cmp::Reverse(b.weight), b.kind))
                    });
                    relations.dedup_by(|a, b| a.kind == b.kind && a.explanation == b.explanation);
                    let best = relations.first().map(|r| r.weight).unwrap_or_default();
                    // Multiplicity is evidence of relevance, so it lifts the
                    // rank a little. It never merges the relations: the bonus
                    // is capped precisely so it cannot become a reason to keep
                    // only one of them.
                    let bonus = ((relations.len() as u16).saturating_sub(1) * 2).min(8);
                    Some(RelatedNeighbor {
                        name: names
                            .get(&target)
                            .cloned()
                            .unwrap_or_else(|| target.clone()),
                        id: target,
                        kind,
                        score: best.saturating_add(bonus).min(100),
                        relations,
                    })
                })
                .collect();
            neighbors.sort_by(|a, b| {
                (std::cmp::Reverse(a.score), a.kind, a.name.as_str()).cmp(&(
                    std::cmp::Reverse(b.score),
                    b.kind,
                    b.name.as_str(),
                ))
            });
            index.insert(source, neighbors);
        }
        index
    }
}

fn build_relation_index() -> HashMap<String, Vec<RelatedNeighbor>> {
    let log = build_catalog_graph("woodshed:catalog:related");
    let graph = log.graph();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut kinds: HashMap<String, CatalogKind> = HashMap::new();
    for (_, node) in graph.nodes() {
        names.insert(
            node.id.clone(),
            if node.title.is_empty() {
                node.id.clone()
            } else {
                node.title.clone()
            },
        );
        if let Some(kind) = kind_of(node) {
            kinds.insert(node.id.clone(), kind);
        }
    }

    let mut builder = RelationBuilder::default();

    // The authored and computed edges the catalog graph already carries.
    for (source, source_node) in graph.nodes() {
        let source_name = names.get(&source_node.id).cloned().unwrap_or_default();
        for (_, target, edge) in graph.out_edges(source) {
            let Some(target_node) = graph.node(target) else {
                continue;
            };
            let target_name = names.get(&target_node.id).cloned().unwrap_or_default();
            let (kind, weight, forward, reverse) = match &edge.class {
                class if class == &RelationClass::app(FAMILY, CONTAINS) => (
                    RelationKind::ContainsMaterial,
                    96,
                    format!("Calls for {target_name}"),
                    format!("Appears in {source_name}"),
                ),
                class if class == &RelationClass::app(FAMILY, FITS_IN_SCALE) => (
                    RelationKind::FitsInScale,
                    92,
                    format!("Fits {target_name}"),
                    format!("Admits {source_name}"),
                ),
                class if class == &RelationClass::app(FAMILY, REALIZES) => (
                    RelationKind::Realizes,
                    100,
                    format!("Arpeggiates {target_name}"),
                    format!("Practice {source_name} as an arpeggio"),
                ),
                _ => continue,
            };
            builder.add_directed(
                &source_node.id,
                &target_node.id,
                kind,
                weight,
                forward,
                reverse,
            );
        }
    }

    add_chord_relations(&mut builder);
    add_scale_relations(&mut builder);
    add_progression_adjacency(&mut builder);

    builder.finish(&names, &kinds)
}

fn circular_distance(a: i32, b: i32) -> i32 {
    let d = (a - b).abs().rem_euclid(12);
    d.min(12 - d)
}

fn voice_leading_distance(a: &BTreeSet<i32>, b: &BTreeSet<i32>) -> f32 {
    let one_way = |from: &BTreeSet<i32>, to: &BTreeSet<i32>| -> f32 {
        from.iter()
            .map(|tone| {
                to.iter()
                    .map(|other| circular_distance(*tone, *other))
                    .min()
                    .unwrap_or(6)
            })
            .sum::<i32>() as f32
            / from.len().max(1) as f32
    };
    (one_way(a, b) + one_way(b, a)) * 0.5
}

/// Chord-to-chord relations. One pair may earn several of these at once, which
/// is the whole point: `Major 7` both extends `Major` and shares three of its
/// tones, and a player is owed both facts rather than the higher-scoring one.
fn add_chord_relations(builder: &mut RelationBuilder) {
    let chords = chord::catalog();
    for (i, source) in chords.iter().enumerate() {
        let source_pcs = pitch_classes(source.intervals);
        for target in chords.iter().skip(i + 1) {
            let target_pcs = pitch_classes(target.intervals);
            let shared = source_pcs.intersection(&target_pcs).count();
            let motion = voice_leading_distance(&source_pcs, &target_pcs);
            let (from_id, to_id) = (chord_id(source.name), chord_id(target.name));

            // Containment: strictly more tones over the same core.
            if source_pcs.len() < target_pcs.len() && source_pcs.is_subset(&target_pcs) {
                let added = target_pcs.len() - source_pcs.len();
                builder.add_directed(
                    &from_id,
                    &to_id,
                    RelationKind::ExtendedBy,
                    90,
                    format!("{} extends it (+{added})", target.name),
                    format!("Extends {} (+{added})", source.name),
                );
            } else if target_pcs.len() < source_pcs.len() && target_pcs.is_subset(&source_pcs) {
                let added = source_pcs.len() - target_pcs.len();
                builder.add_directed(
                    &to_id,
                    &from_id,
                    RelationKind::ExtendedBy,
                    90,
                    format!("{} extends it (+{added})", source.name),
                    format!("Extends {} (+{added})", target.name),
                );
            }

            // Alteration: same size, one tone moved.
            if source_pcs.len() == target_pcs.len() && source_pcs.len().saturating_sub(shared) == 1
            {
                builder.add(
                    MaterialRelation::new(
                        &from_id,
                        &to_id,
                        RelationKind::Alters,
                        76,
                        "One tone apart".to_string(),
                    )
                    .with_shared_tones(shared),
                );
            }

            // Shared tones and voice-leading are distinct facts about the same
            // pair, so they are distinct records, not one blended score.
            if shared > 0 {
                let weight = (44 + shared * 11).min(88) as u16;
                builder.add(
                    MaterialRelation::new(
                        &from_id,
                        &to_id,
                        RelationKind::SharesTones,
                        weight,
                        format!("Shares {shared} tone{}", if shared == 1 { "" } else { "s" }),
                    )
                    .with_shared_tones(shared),
                );
            }
            if motion <= 1.5 {
                let weight = (86.0 - motion * 20.0).round().clamp(0.0, 86.0) as u16;
                builder.add(
                    MaterialRelation::new(
                        &from_id,
                        &to_id,
                        RelationKind::VoiceLeadsTo,
                        weight,
                        format!("Voice-leading {motion:.1}"),
                    )
                    .with_distance(motion),
                );
            }
        }
    }
}

/// The rotations of an interval set, as normalized pitch-class sets. Two scales
/// are modes of each other when one appears among the other's rotations.
fn rotations(pcs: &BTreeSet<i32>) -> Vec<BTreeSet<i32>> {
    pcs.iter()
        .map(|root| {
            pcs.iter()
                .map(|pc| (pc - root).rem_euclid(12))
                .collect::<BTreeSet<i32>>()
        })
        .collect()
}

/// Scale-to-scale relations: modes of one another, or one tone apart.
fn add_scale_relations(builder: &mut RelationBuilder) {
    let scales = scale::catalog();
    for (i, source) in scales.iter().enumerate() {
        let source_pcs = pitch_classes(source.intervals);
        for target in scales.iter().skip(i + 1) {
            let target_pcs = pitch_classes(target.intervals);
            let (from_id, to_id) = (scale_id(source.name), scale_id(target.name));

            if source_pcs.len() == target_pcs.len() && rotations(&source_pcs).contains(&target_pcs)
            {
                builder.add(MaterialRelation::new(
                    &from_id,
                    &to_id,
                    RelationKind::ModeOf,
                    88,
                    "Same tones, different centre".to_string(),
                ));
                continue;
            }

            let shared = source_pcs.intersection(&target_pcs).count();
            if source_pcs.len() == target_pcs.len() && source_pcs.len().saturating_sub(shared) == 1
            {
                builder.add(
                    MaterialRelation::new(
                        &from_id,
                        &to_id,
                        RelationKind::ScaleNeighbor,
                        70,
                        "One degree apart".to_string(),
                    )
                    .with_shared_tones(shared),
                );
            }
        }
    }
}

/// Chords the catalog itself puts next to each other. Authored, not computed:
/// this is a progression saying these two follow one another in practice, which
/// is a different claim from any tone-counting rule and stays a separate record.
fn add_progression_adjacency(builder: &mut RelationBuilder) {
    for progression in progression::catalog() {
        for pair in progression.roles.windows(2) {
            let (a, b) = (
                pair[0].quality.chord_formula_name(),
                pair[1].quality.chord_formula_name(),
            );
            if a == b {
                continue;
            }
            builder.add(MaterialRelation::new(
                chord_id(a),
                chord_id(b),
                RelationKind::UsedTogether,
                84,
                format!("Adjacent in {}", progression.name),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graph_holds_the_whole_catalog() {
        let graph = build_catalog_graph("woodshed:catalog");
        let expected = scale::catalog().len()
            + chord::catalog().len() * 2
            + progression::catalog().len()
            + exercise::catalog().len();
        assert_eq!(graph.graph().node_count(), expected);
        // A well-known object is present, found by its stable id.
        assert!(graph.graph().get(&chord_id("Major 7")).is_some());
        assert!(graph.graph().get(&arpeggio_id("Major 7")).is_some());
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
        assert!(
            chords.contains(&chord_id("Dominant 7")),
            "V is a dominant 7"
        );
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
        assert!(
            scales.contains(&scale_id("Major")),
            "Major 7 fits the Major scale"
        );
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
        let author = Author::new("mark");
        mine.insert_node(
            &author,
            Container::new(chord_id("Mark's Cluster"))
                .with_title("Mark's Cluster")
                .with_tag("chord"),
        );
        mine.connect(
            &author,
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
                .all(|q| q.predicate == scholia::SCHEMA_NAME
                    || q.predicate == scholia::SCHEMA_KEYWORDS),
            "only schema.org literals project; the app ring stays private"
        );
        // A known chord's title reaches RDF as schema:name.
        assert!(quads.iter().any(|q| q.predicate == scholia::SCHEMA_NAME
            && matches!(&q.object, scholia::Term::Literal { value, .. } if value == "Major 7")));
    }

    #[test]
    fn a_practice_session_records_lineage_over_the_same_node_ids() {
        use chartulary::stemma::{EntryPrivacy, Stemma, TransitionKind};

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
    fn a_neighbor_explains_both_directions() {
        let chord = related_neighbors(&chord_id("Major 7"), 64);
        let scale = chord
            .iter()
            .find(|n| n.id == scale_id("Major"))
            .expect("the chord fits the major scale");
        assert!(scale.kinds().contains(&RelationKind::FitsInScale));

        let progression = chord
            .iter()
            .find(|n| n.id == progression_id("ii-V-I (Jazz)"))
            .expect("the chord appears in the progression");
        assert!(progression.kinds().contains(&RelationKind::AppearsIn));

        // The same fact from the scale's end names the inverse kind.
        let from_scale = related_neighbors(&scale_id("Major"), 64);
        let back = from_scale
            .iter()
            .find(|n| n.id == chord_id("Major 7"))
            .expect("the scale admits the chord");
        assert!(back.kinds().contains(&RelationKind::ScaleAdmits));
    }

    #[test]
    fn chord_and_arpeggio_are_distinct_related_identities() {
        let chord = related_neighbors(&chord_id("Minor 7"), 64);
        let arpeggio = chord
            .iter()
            .find(|n| n.id == arpeggio_id("Minor 7"))
            .expect("chord offers its arpeggio realization");
        assert_eq!(arpeggio.kind, CatalogKind::Arpeggio);
        assert!(arpeggio.kinds().contains(&RelationKind::RealizedBy));

        let reverse = related_neighbors(&arpeggio_id("Minor 7"), 64);
        let source = reverse
            .iter()
            .find(|n| n.id == chord_id("Minor 7"))
            .expect("the arpeggio points back at its chord");
        assert!(source.kinds().contains(&RelationKind::Realizes));
    }

    #[test]
    fn one_pair_keeps_every_applicable_relation() {
        // Major and Major 7 are related several ways at once. Ranking picks the
        // order; it must not pick a winner and drop the rest.
        let relations = relations_between(&chord_id("Major"), &chord_id("Major 7"));
        let kinds: Vec<RelationKind> = relations.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&RelationKind::ExtendedBy), "{kinds:?}");
        assert!(kinds.contains(&RelationKind::SharesTones), "{kinds:?}");
        assert!(kinds.contains(&RelationKind::VoiceLeadsTo), "{kinds:?}");
        assert!(
            relations.len() >= 3,
            "several relations survive ranking: {kinds:?}"
        );

        // Each carries its own measurement, rather than one blended score.
        let shared = relations
            .iter()
            .find(|r| r.kind == RelationKind::SharesTones)
            .unwrap();
        assert_eq!(shared.shared_tones, Some(3));
        let motion = relations
            .iter()
            .find(|r| r.kind == RelationKind::VoiceLeadsTo)
            .unwrap();
        assert!(motion.distance.is_some());
    }

    #[test]
    fn relations_carry_the_authority_that_produced_them() {
        // Authored by a progression...
        let together = relations_between(&chord_id("Minor 7"), &chord_id("Dominant 7"));
        let authored = together
            .iter()
            .find(|r| r.kind == RelationKind::UsedTogether)
            .expect("ii-V puts them next to each other");
        assert_eq!(authored.authority, RelationAuthority::Catalog);
        assert!(authored.explanation.starts_with("Adjacent in"));

        // ...versus derived from the formulas.
        let computed = relations_between(&chord_id("Major"), &chord_id("Major 7"));
        assert!(computed
            .iter()
            .filter(|r| r.kind != RelationKind::UsedTogether)
            .all(|r| r.authority == RelationAuthority::Computed));
    }

    #[test]
    fn modes_of_one_scale_relate_as_modes() {
        let dorian = related_neighbors(&scale_id("Dorian"), 128);
        let major = dorian
            .iter()
            .find(|n| n.id == scale_id("Major"))
            .expect("Dorian is a rotation of the major scale");
        assert!(major.kinds().contains(&RelationKind::ModeOf));
        assert_eq!(major.primary().authority, RelationAuthority::Computed);
    }

    #[test]
    fn ranking_prefers_multiplicity_without_merging_it() {
        let neighbors = related_neighbors(&chord_id("Major"), 128);
        let major_7 = neighbors
            .iter()
            .find(|n| n.id == chord_id("Major 7"))
            .expect("closely related chord");
        assert!(major_7.score > 60);
        assert!(major_7.relations.len() > 1, "multiplicity is retained");
        // The score reflects the strongest relation plus a bounded bonus, so it
        // can never exceed a scale of 100 or reorder by count alone.
        let best = major_7.relations.iter().map(|r| r.weight).max().unwrap();
        assert!(major_7.score >= best && major_7.score <= best.saturating_add(8));
        // Neighbors arrive strongest-first.
        assert!(neighbors.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn deterministic_relations_need_no_history() {
        // Nothing in this crate reads practice history: every relation it
        // produces is Catalog or Computed. The Evidence kinds exist for the
        // consumer that owns the history to add to the same records.
        let neighbors = related_neighbors(&chord_id("Major 7"), 128);
        assert!(!neighbors.is_empty());
        assert!(neighbors.iter().flat_map(|n| &n.relations).all(|r| {
            r.authority == RelationAuthority::Catalog || r.authority == RelationAuthority::Computed
        }));
    }
}
