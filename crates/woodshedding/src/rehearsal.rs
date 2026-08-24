//! The rehearsal card/set vocabulary — Woodshed's portable practice core.
//!
//! A [`Set`] is an ordered run of [`Card`]s you practice in sequence; a
//! card is one piece of [`Material`] with enough context to put it on the
//! neck and play it: where it sits (a [`Setting`]), how it's played (a
//! [`Touch`]), and how long to stay on it (a [`Timing`]). Said as a teacher
//! would: "take this card, the material is C minor pentatonic, in this
//! setting, with this touch, hold it eight bars."
//!
//! Everything here is pure, serializable data with no UI / audio / I/O
//! dependency, so the desktop app and any future CLI / web shell share one
//! persistable core. Selections resolve by *name* at the edge, so a catalog
//! edit between sessions can't scramble a saved set. Roots are stored as a
//! bare [`PitchClass`]; an app's spelled pitch-class type converts in and
//! out at the boundary.

use serde::{Deserialize, Serialize};

use crate::pitch::PitchClass;

/// Direction the arpeggio transport walks a shape's notes (by pitch).
/// `UpDown` ascends then descends without repeating the turnaround notes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArpeggioDirection {
    #[default]
    UpDown,
    Up,
    Down,
}

impl ArpeggioDirection {
    pub fn next(self) -> Self {
        match self {
            Self::UpDown => Self::Up,
            Self::Up => Self::Down,
            Self::Down => Self::UpDown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::UpDown => "Up/Down",
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }
}

/// The atomic, practiceable "what" of a card. Progressions / exercises /
/// songs are *recipes* that fill a set with these, not variants here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Material {
    Scale {
        name: String,
        root: PitchClass,
    },
    Chord {
        name: String,
        root: PitchClass,
    },
    /// A fixed playable sequence; a user/catalog exercise's steps live
    /// here, referenced by name.
    Riff {
        name: String,
    },
    /// A hand-drawn path: arranged notes carried *inline* as an ordered visit
    /// list of `(string_index, fret)`, plus the root they were drawn over so
    /// their degrees still name themselves. Every other material names a
    /// catalog formula; this one carries its own content — "content is arranged
    /// notes and relationships", literally. What Draw mode saves.
    Path {
        positions: Vec<(usize, u8)>,
        root: PitchClass,
    },
}

impl Material {
    /// Short kind tag for badges in the set view.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Scale { .. } => "Scale",
            Self::Chord { .. } => "Chord",
            Self::Riff { .. } => "Riff",
            Self::Path { .. } => "Path",
        }
    }
}

/// Where a card came from: the recipe that stamped it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Recipe {
    Progression {
        name: String,
        key: PitchClass,
    },
    Exercise {
        name: String,
    },
    PracticeSet {
        name: String,
    },
    /// `bar` is the source bar index in the song, so a playing song engine
    /// can map its bar cursor back to the exact card (used by the
    /// song-follow clock).
    Song {
        name: String,
        bar: usize,
    },
}

/// What keeps time for the card under the cursor. A derived, runtime value
/// (not stored): the song engine when a song card is playing, the
/// metronome when it's running, else manual stepping.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Clock {
    Manual,
    Metronome,
    Song,
}

/// A window onto the neck: the first visible fret and how many frets wide.
/// A card can pin one (e.g. a practice item's hand position); when `None`
/// the stage uses the live fret window.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct FretWindow {
    pub start: u8,
    pub span: u8,
}

/// What the card's marked notes mean for playback. Marking a note is a neutral
/// selection; the mode is what you *do* with the selection. A general
/// select-then-act primitive (the "node" is a fretboard marker here, but the
/// same shape applies to graph nodes). Realizes the touch model's Selection
/// axis: which notes take part.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkMode {
    /// Marks are just a saved selection; every note still sounds.
    #[default]
    Off,
    /// Only the marked notes play; the rest go quiet (and dim).
    Solo,
    /// The marked notes go quiet (and dim); the rest play.
    Mute,
}

impl MarkMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Solo => "Solo",
            Self::Mute => "Mute",
        }
    }
}

/// How the card sits on the neck (the space axis): instrument setup +
/// where / which shape on the neck.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Setting {
    /// Instrument family (string adapter); drives tuning lookup.
    pub instrument: String,
    /// Specific tuning name, or `None` for the instrument default.
    #[serde(default)]
    pub tuning: Option<String>,
    /// Capo position in frets (`None`/0 = open). The resolver applies the
    /// pitch shift; the shape you finger is unchanged.
    #[serde(default)]
    pub capo: Option<u8>,
    /// Selected voicing for chord material; `None` = all chord tones.
    #[serde(default)]
    pub voicing_idx: Option<usize>,
    /// Pinned neck window (e.g. a practice item's hand position); `None`
    /// = use the live fret window.
    #[serde(default)]
    pub fret_window: Option<FretWindow>,
    /// Notes the player has marked on the board, as guitar-model
    /// `(string_index, fret)`. Marking is a neutral selection; `mark_mode`
    /// decides what it does to playback (solo / mute). Neck-space, so it lives
    /// with the setting.
    #[serde(default)]
    pub marked: Vec<(usize, u8)>,
    /// What the marked set does to playback and display.
    #[serde(default)]
    pub mark_mode: MarkMode,
}

/// How you play the card.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Touch {
    #[default]
    Block,
    Arpeggiate {
        direction: ArpeggioDirection,
        inversion: u8,
    },
    /// Visit the material's notes one at a time, in the order the *material*
    /// carries — a drawn [`Material::Path`] walks as drawn, a scale climbs.
    /// Block sounds the notes together; Walk follows the arrangement. Where
    /// Arpeggiate imposes a direction on a chord's tones, Walk defers to the
    /// material's own order, which is the point of material that has one.
    Walk,
}

/// How long to stay on a card before moving on. (The app steps the set
/// manually or auto-advances by this dwell.)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Hold {
    #[default]
    Manual,
    Bars(u8),
    Seconds(f32),
    Reps(u16),
}

/// Tempo and how long to stay on a card.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Timing {
    #[serde(default)]
    pub bpm: Option<f32>,
    #[serde(default)]
    pub hold: Hold,
}

/// Stable identity for one staged Card occurrence.
///
/// It survives reorder, save/load, selection, and lowering into Rehearsal or
/// Looper. Staging the same material twice yields two ids, because the
/// occurrence is the thing being identified, not the material. Ids are minted
/// by the owning [`Set`] and never reused within it, so a removed card's id
/// cannot come back attached to different material (which is how a captured
/// loop layer ends up on the wrong chord).
///
/// [`CardId::UNASSIGNED`] is what a Set saved before this existed
/// deserializes with; [`Set::ensure_card_ids`] replaces it at load.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct CardId(pub u64);

impl CardId {
    /// The pre-identity value. Never handed out by [`Set::push`].
    pub const UNASSIGNED: Self = Self(0);

    pub fn is_assigned(self) -> bool {
        self != Self::UNASSIGNED
    }
}

/// One card: a piece of material with the context to put it on the neck
/// and play it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    /// Stable occurrence identity, assigned when the card enters a [`Set`].
    /// A card held outside a Set may carry [`CardId::UNASSIGNED`].
    #[serde(default)]
    pub id: CardId,
    /// Human-readable label, e.g. "C minor — scale", "Am7 · voicing 2".
    pub label: String,
    pub material: Material,
    pub setting: Setting,
    pub touch: Touch,
    pub timing: Timing,
    /// Which recipe stamped this card (a one-time stamp, not a live
    /// binding). `None` = hand-added.
    pub from: Option<Recipe>,
}

/// Whether stepping past the end of the set wraps around.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopMode {
    #[default]
    Off,
    /// Wrap: stepping past the last card returns to the first (and vice
    /// versa), so a set loops like a practice set.
    All,
}

/// A set: an ordered run of cards plus a cursor (the card on the stage).
/// Every view (the stage, the timeline) reads from here, so the wiring
/// concentrates in one owned model. Persisted so a session survives a
/// restart.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Set {
    pub cards: Vec<Card>,
    /// Index of the card on the stage. Meaningless when `cards` is empty;
    /// clamped on mutation. Order is authoritative while the product is
    /// linear, so the cursor stays an index; identity-addressed callers use
    /// [`Set::cursor_id`] and [`Set::select_id`].
    pub cursor: usize,
    #[serde(default)]
    pub loop_mode: LoopMode,
    /// Highest occurrence id handed out so far. Monotonic, so removing a card
    /// never frees its id for reuse.
    #[serde(default)]
    last_card_id: u64,
}

/// One staged Card as a graph node. `id` is the occurrence's stable identity;
/// `index` and `number` are read off current Set order, so they change under
/// reorder while `id` does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetGraphNode {
    pub id: CardId,
    pub index: usize,
    pub number: usize,
    pub label: String,
    pub kind: &'static str,
}

/// A typed relation between two staged Card occurrences, addressed by stable
/// identity so a projected edge cannot drift onto a different card when the
/// Set is reordered.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SetGraphEdge {
    pub from: CardId,
    pub to: CardId,
    pub kind: SetGraphEdgeKind,
}

/// Relations which are facts of the Set itself. Harmonic relations belong to
/// a richer projection layered over this snapshot, rather than being written
/// back as Set truth.
///
/// Serializable and ordered because relation *visibility* persists as a set of
/// these kinds: a family added later joins the set instead of adding a second
/// boolean.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SetGraphEdgeKind {
    Next,
}

impl SetGraphEdgeKind {
    /// Every relation family this projection can currently derive.
    pub const ALL: [Self; 1] = [Self::Next];

    pub fn label(self) -> &'static str {
        match self {
            Self::Next => "Sequence",
        }
    }
}

/// The graph projection of one Set. It is derived, so Cards, Rehearsal, and
/// Looper continue to share one persisted material document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetGraph {
    pub nodes: Vec<SetGraphNode>,
    pub edges: Vec<SetGraphEdge>,
}

impl SetGraph {
    pub fn node(&self, id: CardId) -> Option<&SetGraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The projection with only `kinds` of relation drawn. Filtering is a view
    /// operation: it drops projected edges and never touches Set truth.
    pub fn with_relations(mut self, kinds: &[SetGraphEdgeKind]) -> Self {
        self.edges.retain(|edge| kinds.contains(&edge.kind));
        self
    }
}

impl Set {
    /// Build a Set from ordered cards, minting an occurrence id for each.
    pub fn from_cards(cards: impl IntoIterator<Item = Card>, loop_mode: LoopMode) -> Self {
        let mut set = Self {
            loop_mode,
            ..Self::default()
        };
        for card in cards {
            set.push(card);
        }
        set
    }

    /// Hand out the next occurrence id. Monotonic within this Set.
    fn mint_card_id(&mut self) -> CardId {
        self.last_card_id += 1;
        CardId(self.last_card_id)
    }

    /// Give every card an occurrence id, for a Set loaded from a session
    /// written before ids existed. Cards that already have one keep it, and
    /// the mint continues past the highest id in the file, so a partially
    /// migrated Set cannot mint a collision. Idempotent.
    pub fn ensure_card_ids(&mut self) {
        let highest = self
            .cards
            .iter()
            .map(|card| card.id.0)
            .max()
            .unwrap_or_default();
        self.last_card_id = self.last_card_id.max(highest);
        for index in 0..self.cards.len() {
            if !self.cards[index].id.is_assigned() {
                self.cards[index].id = self.mint_card_id();
            }
        }
    }

    /// Position of the occurrence `id` in current Set order.
    pub fn index_of(&self, id: CardId) -> Option<usize> {
        if !id.is_assigned() {
            return None;
        }
        self.cards.iter().position(|card| card.id == id)
    }

    pub fn id_at(&self, index: usize) -> Option<CardId> {
        self.cards.get(index).map(|card| card.id)
    }

    /// The occurrence on the stage, by identity.
    pub fn cursor_id(&self) -> Option<CardId> {
        self.id_at(self.cursor.min(self.cards.len().saturating_sub(1)))
    }

    /// Put the cursor on `id`. Returns false if it is no longer in the Set,
    /// leaving the cursor where it was.
    pub fn select_id(&mut self, id: CardId) -> bool {
        match self.index_of(id) {
            Some(index) => {
                self.cursor = index;
                true
            }
            None => false,
        }
    }

    pub fn card(&self, id: CardId) -> Option<&Card> {
        self.index_of(id).map(|index| &self.cards[index])
    }

    pub fn card_mut(&mut self, id: CardId) -> Option<&mut Card> {
        self.index_of(id).map(|index| &mut self.cards[index])
    }

    /// Project the ordered Set as numbered Card nodes joined by typed `Next`
    /// edges. Nodes are addressed by [`CardId`]; the visible number is derived
    /// from current order. Consumers may filter edge kinds
    /// ([`SetGraph::with_relations`]) without changing Set truth.
    pub fn graph(&self) -> SetGraph {
        let nodes: Vec<SetGraphNode> = self
            .cards
            .iter()
            .enumerate()
            .map(|(index, card)| SetGraphNode {
                id: card.id,
                index,
                number: index + 1,
                label: card.label.clone(),
                kind: card.material.tag(),
            })
            .collect();
        let edges = nodes
            .windows(2)
            .map(|pair| SetGraphEdge {
                from: pair[0].id,
                to: pair[1].id,
                kind: SetGraphEdgeKind::Next,
            })
            .collect();
        SetGraph { nodes, edges }
    }

    /// Stage a card as a new occurrence. The id is minted here, so staging the
    /// same material twice yields two distinct occurrences.
    pub fn push(&mut self, card: Card) {
        let id = self.mint_card_id();
        self.cards.push(Card { id, ..card });
    }

    /// Insert a copy of the card at `idx` right after it. The copy is a new
    /// occurrence with its own id; the original keeps its own.
    pub fn duplicate(&mut self, idx: usize) {
        if let Some(c) = self.cards.get(idx).cloned() {
            let id = self.mint_card_id();
            self.cards.insert(idx + 1, Card { id, ..c });
        }
    }

    /// Remove the card at `idx`, keeping the cursor on a valid slot.
    pub fn remove(&mut self, idx: usize) {
        if idx >= self.cards.len() {
            return;
        }
        self.cards.remove(idx);
        if self.cursor >= self.cards.len() {
            self.cursor = self.cards.len().saturating_sub(1);
        }
    }

    /// Swap the card at `idx` with its neighbor in `dir` (-1 up, +1 down),
    /// keeping the cursor following the moved card if it was the cursor.
    pub fn move_card(&mut self, idx: usize, dir: i32) {
        let target = idx as i32 + dir;
        if idx >= self.cards.len() || target < 0 || target as usize >= self.cards.len() {
            return;
        }
        let target = target as usize;
        self.cards.swap(idx, target);
        if self.cursor == idx {
            self.cursor = target;
        } else if self.cursor == target {
            self.cursor = idx;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(label: &str) -> Card {
        Card {
            id: CardId::UNASSIGNED,
            label: label.into(),
            material: Material::Riff { name: label.into() },
            setting: Setting::default(),
            touch: Touch::default(),
            timing: Timing::default(),
            from: None,
        }
    }

    fn set_of(labels: &[&str]) -> Set {
        Set::from_cards(labels.iter().map(|l| card(l)), LoopMode::Off)
    }

    #[test]
    fn set_graph_numbers_occurrences_and_exposes_sequence_edges() {
        let set = set_of(&["Shape", "Shape", "Turnaround"]);

        let graph = set.graph();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].number, 1);
        assert_eq!(graph.nodes[1].number, 2);
        assert_eq!(graph.nodes[0].label, graph.nodes[1].label);
        assert_ne!(
            graph.nodes[0].id, graph.nodes[1].id,
            "the same material staged twice is two occurrences"
        );
        assert_eq!(
            graph.edges,
            vec![
                SetGraphEdge {
                    from: graph.nodes[0].id,
                    to: graph.nodes[1].id,
                    kind: SetGraphEdgeKind::Next,
                },
                SetGraphEdge {
                    from: graph.nodes[1].id,
                    to: graph.nodes[2].id,
                    kind: SetGraphEdgeKind::Next,
                },
            ]
        );
    }

    #[test]
    fn reorder_changes_numbering_without_changing_identity() {
        let mut set = set_of(&["One", "Two", "Three"]);
        let before: Vec<CardId> = set.cards.iter().map(|c| c.id).collect();
        set.cursor = 0;
        let staged = set.cursor_id().unwrap();

        set.move_card(0, 1);

        let graph = set.graph();
        assert_eq!(
            graph.node(staged).unwrap().number,
            2,
            "number follows order"
        );
        assert_eq!(set.cursor_id(), Some(staged), "cursor follows the card");
        let after: Vec<CardId> = set.cards.iter().map(|c| c.id).collect();
        assert_eq!(after, vec![before[1], before[0], before[2]]);
    }

    #[test]
    fn duplicating_mints_a_new_occurrence_and_editing_keeps_one() {
        let mut set = set_of(&["Riff"]);
        let original = set.cards[0].id;

        set.duplicate(0);
        assert_eq!(set.cards.len(), 2);
        assert_eq!(set.cards[0].id, original, "the original is untouched");
        assert_ne!(set.cards[1].id, original, "the copy is its own occurrence");

        set.card_mut(original).unwrap().label = "Riff, slower".into();
        assert_eq!(set.cards[0].id, original, "editing does not re-identify");
    }

    #[test]
    fn removal_drops_only_incident_edges_and_never_reuses_the_id() {
        let mut set = set_of(&["One", "Two", "Three"]);
        let [first, second, third] = [set.cards[0].id, set.cards[1].id, set.cards[2].id];

        set.remove(1);

        let graph = set.graph();
        assert!(graph.node(second).is_none());
        assert_eq!(
            graph.edges,
            vec![SetGraphEdge {
                from: first,
                to: third,
                kind: SetGraphEdgeKind::Next,
            }],
            "the survivors re-derive one Next edge; nothing dangles"
        );

        set.push(card("Four"));
        let minted = set.cards.last().unwrap().id;
        assert_ne!(minted, second, "a removed id is never handed out again");
    }

    #[test]
    fn hiding_a_relation_family_changes_only_the_view() {
        let set = set_of(&["One", "Two"]);
        let hidden = set.graph().with_relations(&[]);
        assert_eq!(hidden.nodes.len(), 2, "nodes remain");
        assert!(hidden.edges.is_empty());
        assert_eq!(set.graph().edges.len(), 1, "Set truth is unchanged");
    }

    #[test]
    fn a_round_trip_preserves_occurrence_identity_and_the_selected_card() {
        let mut set = set_of(&["One", "Two", "Three"]);
        set.cursor = 2;
        let selected = set.cursor_id().unwrap();

        let json = serde_json::to_string(&set).unwrap();
        let back: Set = serde_json::from_str(&json).unwrap();

        assert_eq!(
            back.cards.iter().map(|c| c.id).collect::<Vec<_>>(),
            set.cards.iter().map(|c| c.id).collect::<Vec<_>>()
        );
        assert_eq!(back.cursor_id(), Some(selected));
    }

    #[test]
    fn a_legacy_set_gains_ids_at_load_without_colliding() {
        // A session written before occurrence identity existed: no ids, no
        // mint state. Serde fills both with their defaults.
        let legacy = r#"{"cards":[
            {"label":"One","material":{"Riff":{"name":"One"}},"setting":{"instrument":"guitar"},"touch":"Block","timing":{},"from":null},
            {"label":"Two","material":{"Riff":{"name":"Two"}},"setting":{"instrument":"guitar"},"touch":"Block","timing":{},"from":null}
        ],"cursor":1}"#;
        let mut set: Set = serde_json::from_str(legacy).unwrap();
        assert!(set.cards.iter().all(|c| !c.id.is_assigned()));

        set.ensure_card_ids();

        let ids: Vec<CardId> = set.cards.iter().map(|c| c.id).collect();
        assert!(ids.iter().all(|id| id.is_assigned()));
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            set.cursor_id(),
            Some(ids[1]),
            "the cursor still points at Two"
        );

        set.push(card("Three"));
        assert!(
            !ids.contains(&set.cards[2].id),
            "the mint continues past the migrated ids"
        );

        let before = set.cards.iter().map(|c| c.id).collect::<Vec<_>>();
        set.ensure_card_ids();
        assert_eq!(
            set.cards.iter().map(|c| c.id).collect::<Vec<_>>(),
            before,
            "migration is idempotent"
        );
    }
}
