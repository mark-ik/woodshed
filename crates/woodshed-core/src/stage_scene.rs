//! The Stage projection: one Set, as a `sceno` scene.
//!
//! Woodshed owns the musical facts and their typed relations; `sceno` owns
//! placement, footprints, representation, and routing. This module is the
//! adapter between the two, the analog of mere's `cartography` graph adapter,
//! and it is the only place in woodshed that knows the scene contract exists.
//!
//! Three properties are deliberate.
//!
//! **A staged card is an instance, not a source.** The material is interned
//! once; staging the same chord twice yields one [`SourceRef`] and two
//! [`ProjectedItem`]s. That is the contract's source-versus-instance
//! separation used as intended, and it is why a Set that drills one voicing
//! four times does not carry four copies of it.
//!
//! **A pair related for several reasons is several relations.** `sceno`
//! deliberately gave relations no channel map, because multi-edge is truth and
//! collapsing to one line is an experience setting. So a chord pair that both
//! shares tones and voice-leads emits two [`RoutedRelation`]s, each with its
//! own kind and weight, and a host chooses whether to fan or collapse them.
//!
//! **Relations here are formula-level, and therefore key-agnostic.**
//! `woodshed-graph` relates catalog formulas (`Major 7` extends `Major`
//! whatever the tonic). Staged cards carry roots, so the Stage layer is where
//! keyed relations (diatonic in, dominant of, resolves to) would eventually be
//! derived. They are not derived here: this founding lifts the formula-level
//! relations onto occurrences, and the keyed layer is its own slice.

use std::collections::BTreeMap;

use sceno::{
    Footprint, InstanceId, ProjectedItem, Rect, Representation, RoutedRelation, Scene, Size2,
    SourceRef, Transform2, Vec2,
};
use woodshed_graph::{
    MaterialRelation, RelationKind, chord_id, exercise_id, relations_between, scale_id,
};
use woodshedding::rehearsal::{CardId, Material, Set};

/// The adapter name every Stage source ref carries, so a viewer with no
/// woodshed access still knows which product minted the id.
pub const ADAPTER: &str = "woodshed.stage";

/// The relation kind for Set order. Namespaced because the scene's `kind` is
/// an open string shared with every other adapter.
pub const SEQUENCE_KIND: &str = "woodshed:sequence";

/// The emphasis channel practice recency rides on. `"heat"` is the name
/// mere's cartography already uses for freshness, and a host that shades by
/// heat needs no woodshed-specific code.
pub const RECENCY_CHANNEL: &str = "heat";

/// How the Stage projection is realized. Defaults render a legible lane
/// without a solver; a caller that runs one can overwrite the transforms.
#[derive(Clone, Debug)]
pub struct StageSceneOptions {
    /// Footprint stamped on each card item. The contract deleted `measure`,
    /// so the host's measured extent belongs here.
    pub card_size: Size2,
    /// Gap between card centres along the lane.
    pub spacing: f32,
    /// Per-occurrence practice recency in `0..=1`, emitted on
    /// [`RECENCY_CHANNEL`]. Supplied by the caller because this crate owns no
    /// history; the evidence layer is the consumer's.
    pub recency: BTreeMap<CardId, f32>,
    /// Which catalog relation families to draw. `None` draws every one.
    /// Filtering is a view operation: it drops projected relations and never
    /// touches Set truth.
    pub relation_kinds: Option<Vec<RelationKind>>,
    /// Whether Set order is drawn as relations.
    pub sequence: bool,
}

impl Default for StageSceneOptions {
    fn default() -> Self {
        Self {
            card_size: Size2::new(160.0, 96.0),
            spacing: 220.0,
            recency: BTreeMap::new(),
            relation_kinds: None,
            sequence: true,
        }
    }
}

/// One Set projected into the scene contract, with the occurrence identities
/// the scene itself cannot carry.
///
/// `sceno` addresses items by position ([`InstanceId`] is an index), so the
/// mapping back to [`CardId`] rides here rather than inside the scene. That
/// keeps the scene product-free while leaving a woodshed host able to answer
/// "which card did I just click".
#[derive(Clone, Debug)]
pub struct StageGraphSnapshot {
    pub scene: Scene,
    /// Occurrence identity per instance, indexed by [`InstanceId`].
    pub cards: Vec<CardId>,
}

impl StageGraphSnapshot {
    pub fn card_of(&self, instance: InstanceId) -> Option<CardId> {
        self.cards.get(instance.0 as usize).copied()
    }

    pub fn instance_of(&self, card: CardId) -> Option<InstanceId> {
        self.cards
            .iter()
            .position(|c| *c == card)
            .map(|i| InstanceId(i as u32))
    }

    /// Every reason two staged occurrences are related, in display order.
    ///
    /// This is what selecting a fanned edge should show: all applicable
    /// reasons with their own authorities, rather than the one that ranked
    /// highest. Returns empty for a pair with no catalog identity on either
    /// side (a hand-drawn Path relates to nothing yet).
    pub fn reasons(&self, from: CardId, to: CardId, set: &Set) -> Vec<MaterialRelation> {
        let material = |id: CardId| set.cards.iter().find(|c| c.id == id).map(|c| &c.material);
        match (material(from).and_then(catalog_id), material(to).and_then(catalog_id)) {
            (Some(a), Some(b)) => relations_between(&a, &b),
            _ => Vec::new(),
        }
    }
}

/// The catalog identity a material points at, or `None` when it carries its
/// own content.
///
/// A hand-drawn [`Material::Path`] names no catalog formula, so it has no
/// catalog relations. That is a real property of the material rather than a
/// gap: the arranged notes *are* the content.
fn catalog_id(material: &Material) -> Option<String> {
    match material {
        Material::Scale { name, .. } => Some(scale_id(name)),
        Material::Chord { name, .. } => Some(chord_id(name)),
        Material::Riff { name } => Some(exercise_id(name)),
        Material::Path { .. } => None,
    }
}

/// The interned source id for a staged card.
///
/// Catalog materials share a source across occurrences, which is the point.
/// A Path carries its own content, so its source is its occurrence.
fn source_id(material: &Material, card: CardId) -> String {
    catalog_id(material).unwrap_or_else(|| format!("path:{}", card.0))
}

/// A stable wire slug per relation family.
///
/// Written out rather than derived from `RelationKind::label`, because that
/// label is display text a future edit may reword, and this string crosses a
/// wire to viewers that match on it.
fn relation_slug(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::ContainsMaterial => "woodshed:contains",
        RelationKind::AppearsIn => "woodshed:appears-in",
        RelationKind::FitsInScale => "woodshed:fits-in-scale",
        RelationKind::ScaleAdmits => "woodshed:admits",
        RelationKind::Realizes => "woodshed:realizes",
        RelationKind::RealizedBy => "woodshed:realized-by",
        RelationKind::ExtendedBy => "woodshed:extended-by",
        RelationKind::Extends => "woodshed:extends",
        RelationKind::Alters => "woodshed:alters",
        RelationKind::SharesTones => "woodshed:shares-tones",
        RelationKind::VoiceLeadsTo => "woodshed:voice-leads-to",
        RelationKind::ModeOf => "woodshed:mode-of",
        RelationKind::ScaleNeighbor => "woodshed:scale-neighbor",
        RelationKind::UsedTogether => "woodshed:used-together",
        RelationKind::PracticedBefore => "woodshed:practiced-before",
        RelationKind::PracticedAfter => "woodshed:practiced-after",
    }
}

/// Project a Set into the frozen scene contract.
///
/// Placement is a lane along x in Set order, which is honest for material
/// whose order is authoritative and gives the scene real bounds without a
/// solver. A caller running `scenomise` may overwrite the transforms; the
/// relations and identities do not change when it does.
pub fn stage_scene(set: &Set, options: &StageSceneOptions) -> StageGraphSnapshot {
    let mut scene = Scene::new();
    let graph = set.graph();
    let mut cards = Vec::with_capacity(graph.nodes.len());

    // Items, in Set order, one per staged occurrence.
    for node in &graph.nodes {
        let Some(card) = set.cards.iter().find(|c| c.id == node.id) else {
            continue;
        };
        let source = scene.intern_source(SourceRef::new(
            ADAPTER,
            source_id(&card.material, card.id),
        ));
        let channels = options
            .recency
            .get(&card.id)
            .map(|heat| vec![(RECENCY_CHANNEL.to_string(), *heat)])
            .unwrap_or_default();

        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(node.index as f32 * options.spacing, 0.0),
            footprint: Footprint::Rect {
                size: options.card_size,
            },
            representation: Representation::Card,
            layer: 0,
            visible: true,
            hit: None,
            channels,
        });
        cards.push(card.id);
    }

    let instance = |id: CardId| {
        cards
            .iter()
            .position(|c| *c == id)
            .map(|i| InstanceId(i as u32))
    };
    let centre = |i: InstanceId| {
        scene
            .items
            .get(i.0 as usize)
            .map(|item| Vec2::new(item.transform.translate.x, item.transform.translate.y))
            .unwrap_or(Vec2::new(0.0, 0.0))
    };

    // Set order, straight from the Set's own graph so sequence truth stays
    // woodshedding's.
    let mut relations = Vec::new();
    if options.sequence {
        for edge in &graph.edges {
            let (Some(from), Some(to)) = (instance(edge.from), instance(edge.to)) else {
                continue;
            };
            relations.push(RoutedRelation {
                from,
                to,
                space: Scene::WORLD,
                points: vec![centre(from), centre(to)],
                kind: Some(SEQUENCE_KIND.to_string()),
                weight: Some(1.0),
            });
        }
    }

    // Catalog reasons, fanned: one relation per reason per pair, never deduped
    // to the highest-ranked.
    for (i, from_card) in cards.iter().enumerate() {
        let Some(from_material) = set.cards.iter().find(|c| c.id == *from_card) else {
            continue;
        };
        let Some(from_id) = catalog_id(&from_material.material) else {
            continue;
        };
        for to_card in cards.iter().skip(i + 1) {
            let Some(to_material) = set.cards.iter().find(|c| c.id == *to_card) else {
                continue;
            };
            let Some(to_id) = catalog_id(&to_material.material) else {
                continue;
            };
            let (Some(from), Some(to)) = (instance(*from_card), instance(*to_card)) else {
                continue;
            };
            for relation in relations_between(&from_id, &to_id) {
                if let Some(kinds) = &options.relation_kinds {
                    if !kinds.contains(&relation.kind) {
                        continue;
                    }
                }
                relations.push(RoutedRelation {
                    from,
                    to,
                    space: Scene::WORLD,
                    points: vec![centre(from), centre(to)],
                    kind: Some(relation_slug(relation.kind).to_string()),
                    weight: Some(f32::from(relation.weight) / 100.0),
                });
            }
        }
    }
    scene.relations = relations;
    scene.bounds = lane_bounds(&scene, options.card_size);

    StageGraphSnapshot { scene, cards }
}

/// World bounds covering every placed card.
fn lane_bounds(scene: &Scene, card: Size2) -> Rect {
    let mut bounds: Option<Rect> = None;
    for item in &scene.items {
        let origin = Vec2::new(
            item.transform.translate.x - card.w * 0.5,
            item.transform.translate.y - card.h * 0.5,
        );
        let rect = Rect::new(origin, card);
        bounds = Some(match bounds {
            Some(acc) => acc.union(rect),
            None => rect,
        });
    }
    bounds.unwrap_or(Rect::new(Vec2::new(0.0, 0.0), Size2::new(0.0, 0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use woodshedding::pitch::PitchClass;
    use woodshedding::rehearsal::{Card, LoopMode, Setting, Timing, Touch};

    fn card(label: &str, material: Material) -> Card {
        Card {
            id: CardId::UNASSIGNED,
            label: label.to_string(),
            material,
            setting: Setting::default(),
            touch: Touch::default(),
            timing: Timing::default(),
            from: None,
        }
    }

    fn chord(name: &str) -> Material {
        Material::Chord {
            name: name.to_string(),
            root: PitchClass::new(0),
        }
    }

    fn set_of(materials: &[(&str, Material)]) -> Set {
        Set::from_cards(
            materials.iter().map(|(l, m)| card(l, m.clone())),
            LoopMode::Off,
        )
    }

    #[test]
    fn each_staged_occurrence_is_its_own_instance() {
        let set = set_of(&[("one", chord("Major")), ("two", chord("Major"))]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        assert_eq!(snapshot.scene.items.len(), 2, "two occurrences, two items");
        assert_eq!(
            snapshot.scene.sources.len(),
            1,
            "one material, interned once"
        );
        assert_eq!(
            snapshot.scene.items[0].source, snapshot.scene.items[1].source,
            "both instances point at the same source"
        );
        assert_ne!(
            snapshot.cards[0], snapshot.cards[1],
            "occurrence identities stay distinct"
        );
    }

    #[test]
    fn set_order_is_drawn_and_can_be_withheld() {
        let set = set_of(&[
            ("one", chord("Major")),
            ("two", chord("Minor")),
            ("three", chord("Major 7")),
        ]);
        let with = stage_scene(&set, &StageSceneOptions::default());
        let sequence = |s: &StageGraphSnapshot| {
            s.scene
                .relations
                .iter()
                .filter(|r| r.kind.as_deref() == Some(SEQUENCE_KIND))
                .count()
        };
        assert_eq!(sequence(&with), 2, "three cards, two Next edges");

        let without = stage_scene(
            &set,
            &StageSceneOptions {
                sequence: false,
                ..Default::default()
            },
        );
        assert_eq!(sequence(&without), 0);
        assert_eq!(
            without.scene.items.len(),
            3,
            "withholding a relation family never drops items"
        );
    }

    #[test]
    fn a_pair_related_for_several_reasons_fans_into_several_relations() {
        // Major and Major 7 both share tones and stand in an extension
        // relation, which is the multi-reason case the contract exists to
        // carry without a channel map.
        let set = set_of(&[("plain", chord("Major")), ("seventh", chord("Major 7"))]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        let catalog: Vec<&RoutedRelation> = snapshot
            .scene
            .relations
            .iter()
            .filter(|r| r.kind.as_deref() != Some(SEQUENCE_KIND))
            .collect();
        assert!(
            catalog.len() > 1,
            "expected a fan, got {:?}",
            catalog.iter().map(|r| &r.kind).collect::<Vec<_>>()
        );

        let kinds: Vec<&str> = catalog.iter().filter_map(|r| r.kind.as_deref()).collect();
        let unique = kinds.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            unique.len(),
            kinds.len(),
            "each reason appears once, not deduped to the best one"
        );
        assert!(
            catalog.iter().all(|r| r.from != r.to),
            "no self-relations"
        );
    }

    #[test]
    fn selecting_a_pair_exposes_every_reason_with_its_authority() {
        let set = set_of(&[("plain", chord("Major")), ("seventh", chord("Major 7"))]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());
        let reasons = snapshot.reasons(snapshot.cards[0], snapshot.cards[1], &set);

        assert!(reasons.len() > 1, "the fan is readable back off the pair");
        assert!(
            reasons.iter().all(|r| !r.explanation.is_empty()),
            "every reason explains itself in the player's words"
        );
    }

    #[test]
    fn a_relation_filter_is_a_view_operation() {
        let set = set_of(&[("plain", chord("Major")), ("seventh", chord("Major 7"))]);
        let only_tones = stage_scene(
            &set,
            &StageSceneOptions {
                relation_kinds: Some(vec![RelationKind::SharesTones]),
                sequence: false,
                ..Default::default()
            },
        );
        assert!(
            only_tones
                .scene
                .relations
                .iter()
                .all(|r| r.kind.as_deref() == Some(relation_slug(RelationKind::SharesTones))),
            "only the requested family survives"
        );
        assert_eq!(only_tones.scene.items.len(), 2, "Set truth is untouched");
    }

    #[test]
    fn a_hand_drawn_path_is_placed_but_relates_to_nothing() {
        let set = set_of(&[
            ("drawn", Material::Path {
                positions: vec![(0, 3), (1, 5)],
                root: PitchClass::new(0),
            }),
            ("chord", chord("Major")),
        ]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        assert_eq!(snapshot.scene.items.len(), 2);
        assert_eq!(
            snapshot.scene.sources.len(),
            2,
            "a path carries its own content, so it interns its own source"
        );
        let catalog = snapshot
            .scene
            .relations
            .iter()
            .filter(|r| r.kind.as_deref() != Some(SEQUENCE_KIND))
            .count();
        assert_eq!(catalog, 0, "a path names no catalog formula");
    }

    #[test]
    fn recency_rides_the_scene_rather_than_a_host_side_lookup() {
        let set = set_of(&[("one", chord("Major")), ("two", chord("Minor"))]);
        let mut recency = BTreeMap::new();
        recency.insert(set.cards[0].id, 0.75);
        let snapshot = stage_scene(
            &set,
            &StageSceneOptions {
                recency,
                ..Default::default()
            },
        );

        assert_eq!(
            snapshot.scene.items[0].channels,
            vec![(RECENCY_CHANNEL.to_string(), 0.75)],
            "a remote viewer shades from the scene, not from woodshed's store"
        );
        assert!(snapshot.scene.items[1].channels.is_empty());
    }

    #[test]
    fn instances_and_occurrences_map_both_ways() {
        let set = set_of(&[("one", chord("Major")), ("two", chord("Minor"))]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        for (i, card) in snapshot.cards.iter().enumerate() {
            let instance = InstanceId(i as u32);
            assert_eq!(snapshot.card_of(instance), Some(*card));
            assert_eq!(snapshot.instance_of(*card), Some(instance));
        }
        assert_eq!(snapshot.card_of(InstanceId(99)), None);
    }

    #[test]
    fn an_empty_set_projects_an_empty_scene_not_a_broken_one() {
        let snapshot = stage_scene(&Set::default(), &StageSceneOptions::default());
        assert!(snapshot.scene.items.is_empty());
        assert!(snapshot.scene.relations.is_empty());
        assert_eq!(snapshot.scene.spaces.len(), 1, "the world space still exists");
    }

    #[test]
    fn the_lane_grows_with_the_set_and_every_card_is_inside_it() {
        let set = set_of(&[
            ("one", chord("Major")),
            ("two", chord("Minor")),
            ("three", chord("Major 7")),
        ]);
        let options = StageSceneOptions::default();
        let snapshot = stage_scene(&set, &options);
        let bounds = snapshot.scene.bounds;

        assert!(bounds.size.w >= options.spacing * 2.0);
        for item in &snapshot.scene.items {
            let x = item.transform.translate.x;
            assert!(
                x >= bounds.origin.x && x <= bounds.origin.x + bounds.size.w,
                "card at {x} fell outside the framed bounds"
            );
        }
    }
}
