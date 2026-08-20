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
use scenotime::{RelationId, Revision, SceneEpoch, SceneSnapshot};
use woodshed_graph::{
    chord_id, exercise_id, relations_between, scale_id, MaterialRelation, RelationAuthority,
    RelationKind,
};
use woodshedding::rehearsal::{CardId, Material, Set};

use crate::arrangement::{arrange_graph, GraphArrangement};

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
    /// Product-selected arrangement. It changes placement only; occurrence and
    /// relation identity are assigned after the arrangement is resolved.
    pub arrangement: GraphArrangement,
}

impl Default for StageSceneOptions {
    fn default() -> Self {
        Self {
            card_size: Size2::new(160.0, 96.0),
            spacing: 220.0,
            recency: BTreeMap::new(),
            relation_kinds: None,
            sequence: true,
            arrangement: GraphArrangement::Snake,
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
    pub snapshot: SceneSnapshot,
    /// Occurrence identity per instance, indexed by [`InstanceId`].
    pub cards: Vec<CardId>,
}

/// Epoch-qualified item identity used by Cambium callbacks. A stale event from
/// a previous dense projection cannot name a new occupant of the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StageInstanceRef {
    pub epoch: SceneEpoch,
    pub instance: InstanceId,
}

/// Epoch-qualified relation identity. Cambium currently carries relation ids as
/// strings, so [`Self::key`] is the lossless adapter at that boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StageRelationRef {
    pub epoch: SceneEpoch,
    pub relation: RelationId,
}

/// Stable, view-local identity for one derivable relation between staged
/// occurrences. Unlike [`StageRelationRef`], this survives a new dense scene
/// epoch: occurrence ids and the open relation kind carry the semantic key.
/// It is suitable for visibility preferences, never for changing relation
/// truth.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StageRelationKey {
    pub from: CardId,
    pub to: CardId,
    pub kind: String,
}

impl StageRelationKey {
    /// A DOM/scenario-safe diagnostic key. Consumers should retain the typed
    /// value when they can; this string is only an adapter at text boundaries.
    pub fn wire_key(&self) -> String {
        format!("{}:{}:{}", self.from.0, self.to.0, self.kind)
    }
}

/// Human-facing disclosure for one routed Stage relation. The route remains
/// in Scenograph; this record restores the Woodshed meaning the product UI is
/// responsible for explaining.
#[derive(Clone, Debug, PartialEq)]
pub struct StageRelationDetail {
    pub reference: StageRelationRef,
    pub key: StageRelationKey,
    pub label: String,
    pub explanation: String,
    pub authority: &'static str,
    pub weight: u16,
}

impl StageRelationRef {
    pub fn key(self) -> String {
        format!("{}:{}", self.epoch.0, self.relation.0)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let (epoch, relation) = key.split_once(':')?;
        Some(Self {
            epoch: SceneEpoch(epoch.parse().ok()?),
            relation: RelationId(relation.parse().ok()?),
        })
    }
}

impl StageGraphSnapshot {
    pub fn epoch(&self) -> SceneEpoch {
        self.snapshot.epoch
    }

    pub fn card_of(&self, instance: InstanceId) -> Option<CardId> {
        self.cards.get(instance.0 as usize).copied()
    }

    pub fn card_of_ref(&self, reference: StageInstanceRef) -> Option<CardId> {
        (reference.epoch == self.epoch())
            .then(|| self.card_of(reference.instance))
            .flatten()
    }

    pub fn instance_of(&self, card: CardId) -> Option<InstanceId> {
        self.cards
            .iter()
            .position(|c| *c == card)
            .map(|i| InstanceId(i as u32))
    }

    pub fn instance_ref_of(&self, card: CardId) -> Option<StageInstanceRef> {
        Some(StageInstanceRef {
            epoch: self.epoch(),
            instance: self.instance_of(card)?,
        })
    }

    pub fn relation_ref(&self, relation: RelationId) -> Option<StageRelationRef> {
        self.snapshot.active_relation(relation)?;
        Some(StageRelationRef {
            epoch: self.epoch(),
            relation,
        })
    }

    pub fn relation(&self, reference: StageRelationRef) -> Option<&RoutedRelation> {
        (reference.epoch == self.epoch())
            .then(|| self.snapshot.active_relation(reference.relation))
            .flatten()
    }

    /// Stable semantic identity for a relation in this snapshot.
    pub fn relation_key(&self, reference: StageRelationRef) -> Option<StageRelationKey> {
        let relation = self.relation(reference)?;
        Some(StageRelationKey {
            from: self.card_of(relation.from)?,
            to: self.card_of(relation.to)?,
            kind: relation.kind.clone().unwrap_or_else(|| "relation".into()),
        })
    }

    /// Explain one relation using the owning Woodshed layer. Set sequence is
    /// authored Set truth; catalog relations recover their typed reason and
    /// authority from `woodshed-graph` rather than asking the scene to carry
    /// product vocabulary.
    pub fn relation_detail(
        &self,
        reference: StageRelationRef,
        set: &Set,
    ) -> Option<StageRelationDetail> {
        let key = self.relation_key(reference)?;
        if key.kind == SEQUENCE_KIND {
            return Some(StageRelationDetail {
                reference,
                key,
                label: "sequence".into(),
                explanation: "Earlier card precedes later card in this Set.".into(),
                authority: "Set",
                weight: 100,
            });
        }

        let reason = self
            .reasons(key.from, key.to, set)
            .into_iter()
            .find(|reason| relation_slug(reason.kind) == key.kind)?;
        Some(StageRelationDetail {
            reference,
            key,
            label: reason.kind.label().into(),
            explanation: reason.explanation,
            authority: match reason.authority {
                RelationAuthority::Catalog => "Catalog",
                RelationAuthority::Computed => "Computed",
                RelationAuthority::Evidence => "Evidence",
            },
            weight: reason.weight,
        })
    }

    pub fn items(&self) -> Vec<(StageInstanceRef, &ProjectedItem)> {
        self.snapshot
            .active_items_in_order()
            .into_iter()
            .map(|(instance, item)| {
                (
                    StageInstanceRef {
                        epoch: self.epoch(),
                        instance,
                    },
                    item,
                )
            })
            .collect()
    }

    pub fn relations(&self) -> Vec<(StageRelationRef, &RoutedRelation)> {
        self.snapshot
            .tables
            .relations
            .iter()
            .enumerate()
            .filter_map(|(index, relation)| {
                Some((
                    StageRelationRef {
                        epoch: self.epoch(),
                        relation: RelationId(index as u32),
                    },
                    relation.as_ref()?,
                ))
            })
            .collect()
    }

    /// Every reason two staged occurrences are related, in display order.
    ///
    /// This is what selecting a fanned edge should show: all applicable
    /// reasons with their own authorities, rather than the one that ranked
    /// highest. Returns empty for a pair with no catalog identity on either
    /// side (a hand-drawn Path relates to nothing yet).
    pub fn reasons(&self, from: CardId, to: CardId, set: &Set) -> Vec<MaterialRelation> {
        let material = |id: CardId| set.cards.iter().find(|c| c.id == id).map(|c| &c.material);
        match (
            material(from).and_then(catalog_id),
            material(to).and_then(catalog_id),
        ) {
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

/// Project a Set into the portable scene contract.
///
/// Placement is a lane along x in Set order, which is honest for material
/// whose order is authoritative and gives the scene real bounds without a
/// solver. A caller running `scenomise` may overwrite the transforms; the
/// relations and identities do not change when it does.
pub fn stage_scene(set: &Set, options: &StageSceneOptions) -> StageGraphSnapshot {
    let mut scene = Scene::new();
    let graph = set.graph();
    let mut cards = Vec::with_capacity(graph.nodes.len());
    let graph_edges = graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some((
                graph.nodes.iter().position(|node| node.id == edge.from)?,
                graph.nodes.iter().position(|node| node.id == edge.to)?,
            ))
        })
        .collect::<Vec<_>>();
    let positions = arrange_graph(
        options.arrangement,
        graph.nodes.len(),
        &graph_edges,
        if graph.nodes.is_empty() {
            None
        } else {
            Some(set.cursor.min(graph.nodes.len() - 1))
        },
    );
    let span = options.spacing * (graph.nodes.len() as f32).sqrt().ceil().max(1.0);

    // Items, in Set order, one per staged occurrence.
    for node in &graph.nodes {
        let Some(card) = set.cards.iter().find(|c| c.id == node.id) else {
            continue;
        };
        let source =
            scene.intern_source(SourceRef::new(ADAPTER, source_id(&card.material, card.id)));
        let channels = options
            .recency
            .get(&card.id)
            .map(|heat| vec![(RECENCY_CHANNEL.to_string(), *heat)])
            .unwrap_or_default();

        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(
                (positions[node.index].0 - 0.5) * span,
                (positions[node.index].1 - 0.5) * span,
            ),
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
    let item_bounds = lane_bounds(&scene, options.card_size);
    let floor_padding = 48.0;
    let floor_bounds = Rect::new(
        Vec2::new(
            item_bounds.origin.x - floor_padding,
            item_bounds.origin.y - floor_padding,
        ),
        Size2::new(
            item_bounds.size.w + floor_padding * 2.0,
            item_bounds.size.h + floor_padding * 2.0,
        ),
    );
    let floor_source = scene.intern_source(SourceRef::new(ADAPTER, "stage-floor"));
    scene.backdrops.push(sceno::Backdrop {
        source: floor_source,
        space: Scene::WORLD,
        transform: Transform2::translation(
            floor_bounds.origin.x + floor_bounds.size.w * 0.5,
            floor_bounds.origin.y + floor_bounds.size.h * 0.5,
        ),
        footprint: Footprint::Rect {
            size: floor_bounds.size,
        },
        kind: "woodshed:stage-floor".into(),
        visible: true,
        collidable: false,
    });
    scene.bounds = floor_bounds;
    let epoch = dense_epoch(&scene, &cards);
    let snapshot = SceneSnapshot::from_dense(epoch, Revision(0), scene)
        .unwrap_or_else(|error| panic!("Woodshed produced an invalid Stage scene: {error:?}"));

    StageGraphSnapshot { snapshot, cards }
}

/// A dense projection starts a fresh, content-addressed epoch whenever any
/// table occupant or route changes. Woodshed does not emit diffs yet, so this
/// coarse lifetime is safer than reinterpreting an `InstanceId` after reorder.
fn dense_epoch(scene: &Scene, cards: &[CardId]) -> SceneEpoch {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut write = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    for card in cards {
        write(&card.0.to_le_bytes());
    }
    for source in &scene.sources {
        write(&(source.adapter.len() as u64).to_le_bytes());
        write(source.adapter.as_bytes());
        write(&(source.id.len() as u64).to_le_bytes());
        write(source.id.as_bytes());
    }
    for item in &scene.items {
        write(&item.source.0.to_le_bytes());
        write(&item.transform.translate.x.to_bits().to_le_bytes());
        write(&item.transform.translate.y.to_bits().to_le_bytes());
        write(&item.transform.scale.to_bits().to_le_bytes());
        write(&item.transform.rotate.to_bits().to_le_bytes());
        if let Footprint::Rect { size } = &item.footprint {
            write(&size.w.to_bits().to_le_bytes());
            write(&size.h.to_bits().to_le_bytes());
        }
        for (channel, value) in &item.channels {
            write(&(channel.len() as u64).to_le_bytes());
            write(channel.as_bytes());
            write(&value.to_bits().to_le_bytes());
        }
    }
    for backdrop in &scene.backdrops {
        write(&backdrop.source.0.to_le_bytes());
        write(&backdrop.space.0.to_le_bytes());
        write(&backdrop.transform.translate.x.to_bits().to_le_bytes());
        write(&backdrop.transform.translate.y.to_bits().to_le_bytes());
        write(&backdrop.transform.scale.to_bits().to_le_bytes());
        write(&backdrop.transform.rotate.to_bits().to_le_bytes());
        write(&(backdrop.kind.len() as u64).to_le_bytes());
        write(backdrop.kind.as_bytes());
        write(&[backdrop.visible.into(), backdrop.collidable.into()]);
        if let Footprint::Rect { size } = &backdrop.footprint {
            write(&size.w.to_bits().to_le_bytes());
            write(&size.h.to_bits().to_le_bytes());
        }
    }
    for relation in &scene.relations {
        write(&relation.from.0.to_le_bytes());
        write(&relation.to.0.to_le_bytes());
        write(&relation.space.0.to_le_bytes());
        if let Some(kind) = &relation.kind {
            write(&(kind.len() as u64).to_le_bytes());
            write(kind.as_bytes());
        }
        if let Some(weight) = relation.weight {
            write(&weight.to_bits().to_le_bytes());
        }
        for point in &relation.points {
            write(&point.x.to_bits().to_le_bytes());
            write(&point.y.to_bits().to_le_bytes());
        }
    }
    SceneEpoch(hash.max(1))
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

    fn item_source_count(snapshot: &StageGraphSnapshot) -> usize {
        snapshot
            .snapshot
            .tables
            .items
            .iter()
            .flatten()
            .map(|item| item.source)
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    #[test]
    fn each_staged_occurrence_is_its_own_instance() {
        let set = set_of(&[("one", chord("Major")), ("two", chord("Major"))]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        assert_eq!(
            snapshot.snapshot.active_item_count(),
            2,
            "two occurrences, two items"
        );
        assert_eq!(
            item_source_count(&snapshot),
            1,
            "one material, interned once; the Stage floor is a backdrop source"
        );
        assert_eq!(
            snapshot.snapshot.active_item(InstanceId(0)).unwrap().source,
            snapshot.snapshot.active_item(InstanceId(1)).unwrap().source,
            "both instances point at the same source"
        );
        assert_ne!(
            snapshot.cards[0], snapshot.cards[1],
            "occurrence identities stay distinct"
        );
    }

    #[test]
    fn changed_source_content_starts_a_fresh_dense_epoch() {
        let mut set = set_of(&[("one", chord("Major"))]);
        let before = stage_scene(&set, &StageSceneOptions::default());
        set.cards[0].material = chord("Minor");
        let after = stage_scene(&set, &StageSceneOptions::default());

        assert_ne!(before.epoch(), after.epoch());
        assert_eq!(before.snapshot.revision, Revision(0));
        assert_eq!(after.snapshot.revision, Revision(0));
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
            s.snapshot
                .tables
                .relations
                .iter()
                .flatten()
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
            without.snapshot.active_item_count(),
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
            .snapshot
            .tables
            .relations
            .iter()
            .flatten()
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
        assert!(catalog.iter().all(|r| r.from != r.to), "no self-relations");
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
    fn relation_inventory_keys_survive_dense_scene_epochs() {
        let set = set_of(&[("plain", chord("Major")), ("seventh", chord("Major 7"))]);
        let snake = stage_scene(&set, &StageSceneOptions::default());
        let circle = stage_scene(
            &set,
            &StageSceneOptions {
                arrangement: GraphArrangement::Circle,
                ..StageSceneOptions::default()
            },
        );
        assert_ne!(snake.epoch(), circle.epoch());

        let details = |snapshot: &StageGraphSnapshot| {
            snapshot
                .relations()
                .into_iter()
                .map(|(reference, _)| snapshot.relation_detail(reference, &set).unwrap())
                .collect::<Vec<_>>()
        };
        let snake_details = details(&snake);
        let circle_details = details(&circle);
        assert_eq!(
            snake_details
                .iter()
                .map(|detail| detail.key.clone())
                .collect::<Vec<_>>(),
            circle_details
                .iter()
                .map(|detail| detail.key.clone())
                .collect::<Vec<_>>(),
            "view-local visibility keys do not inherit dense table identity"
        );
        assert!(snake_details.iter().all(|detail| {
            !detail.label.is_empty()
                && !detail.explanation.is_empty()
                && !detail.authority.is_empty()
                && detail.weight > 0
        }));
        assert!(snake_details
            .iter()
            .any(|detail| detail.authority == "Set" && detail.key.kind == SEQUENCE_KIND));
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
                .snapshot
                .tables
                .relations
                .iter()
                .flatten()
                .all(|r| r.kind.as_deref() == Some(relation_slug(RelationKind::SharesTones))),
            "only the requested family survives"
        );
        assert_eq!(
            only_tones.snapshot.active_item_count(),
            2,
            "Set truth is untouched"
        );
    }

    #[test]
    fn a_hand_drawn_path_is_placed_but_relates_to_nothing() {
        let set = set_of(&[
            (
                "drawn",
                Material::Path {
                    positions: vec![(0, 3), (1, 5)],
                    root: PitchClass::new(0),
                },
            ),
            ("chord", chord("Major")),
        ]);
        let snapshot = stage_scene(&set, &StageSceneOptions::default());

        assert_eq!(snapshot.snapshot.active_item_count(), 2);
        assert_eq!(
            item_source_count(&snapshot),
            2,
            "a path carries its own content, so the two items use two sources"
        );
        let catalog = snapshot
            .snapshot
            .tables
            .relations
            .iter()
            .flatten()
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
            snapshot
                .snapshot
                .active_item(InstanceId(0))
                .unwrap()
                .channels,
            vec![(RECENCY_CHANNEL.to_string(), 0.75)],
            "a remote viewer shades from the scene, not from woodshed's store"
        );
        assert!(snapshot
            .snapshot
            .active_item(InstanceId(1))
            .unwrap()
            .channels
            .is_empty());
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
    fn epoch_qualifies_instance_and_relation_table_ids() {
        let set = set_of(&[("one", chord("Major")), ("two", chord("Major 7"))]);
        let first = stage_scene(&set, &StageSceneOptions::default());
        let instance = first.instance_ref_of(first.cards[0]).unwrap();
        let relation = first.relations()[0].0;
        assert_eq!(first.card_of_ref(instance), Some(first.cards[0]));
        assert!(first.relation(relation).is_some());

        let rearranged = stage_scene(
            &set,
            &StageSceneOptions {
                arrangement: GraphArrangement::Circle,
                ..StageSceneOptions::default()
            },
        );
        assert_ne!(first.epoch(), rearranged.epoch());
        assert_eq!(rearranged.card_of_ref(instance), None);
        assert!(rearranged.relation(relation).is_none());
        assert_eq!(
            StageRelationRef::from_key(&relation.key()),
            Some(relation),
            "the Cambium string seam preserves SceneEpoch and RelationId"
        );
    }

    #[test]
    fn an_empty_set_projects_an_empty_scene_not_a_broken_one() {
        let snapshot = stage_scene(&Set::default(), &StageSceneOptions::default());
        assert_eq!(snapshot.snapshot.active_item_count(), 0);
        assert!(snapshot
            .snapshot
            .tables
            .relations
            .iter()
            .flatten()
            .next()
            .is_none());
        assert_eq!(
            snapshot.snapshot.tables.spaces.iter().flatten().count(),
            1,
            "the world space still exists"
        );
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
        let bounds = snapshot.snapshot.tables.bounds;

        assert!(bounds.size.w >= options.spacing * 2.0);
        for item in snapshot.snapshot.tables.items.iter().flatten() {
            let x = item.transform.translate.x;
            assert!(
                x >= bounds.origin.x && x <= bounds.origin.x + bounds.size.w,
                "card at {x} fell outside the framed bounds"
            );
        }
    }
}
