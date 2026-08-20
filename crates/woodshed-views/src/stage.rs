//! The Stage screen over live state (S2).
//!
//! Header dropdowns (tuning, root, using Cambium `select`), the lens strip
//! (Scale / Chord / Arpeggio / Progression / Exercise), a per-lens catalog
//! sidebar, and the fretboard rendered as DOM dots. The runner state is
//! [`UiState`]: the portable `woodshed_core::StageState` plus the
//! view-layer dropdown state; hosts call [`UiState::sync`] after any
//! dispatch so dropdown picks land in the core state.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cambium::{
    clickable, custom_leaf, el, map_state, select, text, text_field, AnyView, GenetCtx,
    GenetElement, GraphCanvasEvent, GraphCanvasNode, GraphCanvasRelation, GraphCanvasSubgraph,
    GraphCanvasSwatch, SelectState, TextInput,
};
use woodshed_core::arrangement::arrange_graph;
use woodshed_core::audio::{AudioRequest, CalibrationStatus, TransportState, TunerState};
use woodshed_core::history::{catalog_id_for_card, EngagementKind, PracticeHistory};
use woodshed_core::mere::{woodshed_mere, MereScope, WoodshedMereSnapshot};
use woodshed_core::search::{search_corpus, SearchHit};
use woodshed_core::settings::{AppSettings, RelatedGraphScope, SettingsPage};
use woodshed_core::song::SongDoc;
use woodshed_core::stage_scene::{
    stage_scene, StageGraphSnapshot, StageInstanceRef, StageRelationKey, StageRelationRef,
    StageSceneOptions, SEQUENCE_KIND,
};
use woodshed_core::storage::{AppSection, PersistedSession};
use woodshed_core::{set_from_practice, tunings, Lens, RelatedTarget, StageState, ROOT_NAMES};
use woodshedding::rehearsal::{Card, CardId, FretWindow, Hold, MarkMode, Set, Touch};

use crate::fretboard_leaf::{BoardGeom, Orientation, FRETBOARD_LEAF_KEY};

use crate::theme::ThemeMode;

mod looper;
mod related;
mod rehearsal;
mod set_tray;
mod settings;
mod templates;
mod tools;

pub const NEIGHBORHOOD_LEAF_KEY: u64 = 0x5753_4e42;
pub const SET_GRAPH_LEAF_KEY: u64 = 0x5753_5347;

/// How many related suggestions the graph swatch and the pane both show, so a
/// node and its row stay in lockstep.
pub const RELATED_LIMIT: usize = 6;

/// The Related graph as a Cambium swatch: the current material at the centre,
/// each suggestion a kind-coloured satellite, star edges. A node's id is the
/// suggestion's [`RelatedTarget`] (`None` = the centre), so a node links 1:1 to
/// its pane row and clicking navigates. Built once and shared by the view (which
/// renders it) and the host (which paints its leaf), the sanctioned pattern.
pub fn related_mere_snapshot(ui: &UiState) -> WoodshedMereSnapshot {
    match ui.app_settings.stage.related.graph_scope {
        RelatedGraphScope::Mere => woodshed_mere(&ui.practice_history, MereScope::Whole),
        RelatedGraphScope::Selection => {
            ui.stage
                .catalog_id()
                .map_or_else(WoodshedMereSnapshot::default, |center| {
                    woodshed_mere(
                        &ui.practice_history,
                        MereScope::Selection {
                            center: &center,
                            depth: ui.app_settings.stage.related.relation_depth,
                        },
                    )
                })
        }
    }
}

pub fn related_swatch_from_snapshot(
    snapshot: &WoodshedMereSnapshot,
    ui: &UiState,
    expanded: bool,
) -> GraphCanvasSwatch<String, &'static str> {
    let selected = ui.stage.catalog_id();
    let projected = swatch_projection(
        snapshot,
        selected.as_deref(),
        if expanded { 12 } else { 7 },
    );
    let edges = projected.edges();
    let focus = selected.as_deref().and_then(|id| projected.node_index(id));
    let positions = arrange_graph(
        ui.app_settings.stage.related.arrangement,
        projected.nodes.len(),
        &edges,
        focus,
    );
    let nodes = projected
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| GraphCanvasNode {
            id: node.id.clone(),
            kind: node.kind.label(),
            position: positions[index],
            label: node.title.clone(),
            key: Some(node.id.clone()),
        })
        .collect();
    let relations = if expanded {
        projected
            .relations
            .iter()
            .enumerate()
            .map(|(index, relation)| {
                let id = format!(
                    "{}:{}:{}:{index}",
                    relation.source,
                    relation.target,
                    relation.kind.label()
                );
                GraphCanvasRelation {
                    emphasized: ui.related_relation.as_deref() == Some(id.as_str()),
                    id,
                    from: relation.source.clone(),
                    to: relation.target.clone(),
                    kind: relation.kind.label().to_string(),
                    label: relation.explanation.clone(),
                    route: Vec::new(),
                    visible: true,
                }
            })
            .collect()
    } else {
        compact_relation_summaries(&projected, ui.related_relation.as_deref())
    };
    let (w, h) = if expanded { (300, 210) } else { (232, 120) };
    let mut swatch = GraphCanvasSwatch::new(
        NEIGHBORHOOD_LEAF_KEY,
        GraphCanvasSubgraph {
            nodes,
            edges: Vec::new(),
        },
    )
    .with_relations(relations)
    .with_size(w, h)
    .with_label(format!(
        "{} · {} of {} materials",
        match ui.app_settings.stage.related.graph_scope {
            RelatedGraphScope::Mere => "Woodshed mere",
            RelatedGraphScope::Selection => "Selection relations",
        },
        projected.nodes.len(),
        snapshot.nodes.len(),
    ))
    .with_node_labels(expanded);
    swatch.selected = selected;
    swatch.hovered = ui
        .related_hover
        .map(|target| ui.stage.related_target_id(target));
    swatch
}

/// Project the full joined snapshot into a bounded disclosure tree for a small
/// swatch. The mere retains every node and relation; this view chooses a
/// weighted spanning neighborhood so cross-links cannot turn a 300px preview
/// into an induced-graph hairball. Every relation between each disclosed
/// parent/child pair survives, which keeps multi-reason fanning honest.
fn swatch_projection(
    snapshot: &WoodshedMereSnapshot,
    center: Option<&str>,
    node_budget: usize,
) -> WoodshedMereSnapshot {
    let Some(center) = center
        .filter(|id| snapshot.node_index(id).is_some())
        .or_else(|| snapshot.nodes.first().map(|node| node.id.as_str()))
    else {
        return WoodshedMereSnapshot::default();
    };
    let node_budget = node_budget.max(1);
    let mut chosen = vec![center.to_string()];
    let mut discovered = BTreeSet::from([center.to_string()]);
    let mut disclosure_pairs = BTreeSet::new();
    let mut queue = VecDeque::from([center.to_string()]);

    while chosen.len() < node_budget {
        let Some(current) = queue.pop_front() else {
            break;
        };
        let mut candidates = BTreeMap::<String, u16>::new();
        for relation in &snapshot.relations {
            let other = if relation.source == current {
                Some(relation.target.as_str())
            } else if relation.target == current {
                Some(relation.source.as_str())
            } else {
                None
            };
            let Some(other) = other.filter(|id| !discovered.contains(*id)) else {
                continue;
            };
            candidates
                .entry(other.to_string())
                .and_modify(|weight| *weight = (*weight).max(relation.weight))
                .or_insert(relation.weight);
        }
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        candidates.sort_by(|(a_id, a_weight), (b_id, b_weight)| {
            b_weight.cmp(a_weight).then_with(|| a_id.cmp(b_id))
        });
        for (other, _) in candidates {
            if chosen.len() >= node_budget {
                break;
            }
            if !discovered.insert(other.clone()) {
                continue;
            }
            disclosure_pairs.insert(ordered_pair(&current, &other));
            chosen.push(other.clone());
            queue.push_back(other);
        }
    }

    let nodes = chosen
        .into_iter()
        .filter_map(|id| snapshot.nodes.iter().find(|node| node.id == id).cloned())
        .collect::<Vec<_>>();
    let relations = snapshot
        .relations
        .iter()
        .filter(|relation| {
            disclosure_pairs.contains(&ordered_pair(&relation.source, &relation.target))
        })
        .cloned()
        .collect();
    WoodshedMereSnapshot { nodes, relations }
}

fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// A compact swatch draws one route per disclosed pair. Multiplicity remains
/// in the target label and expands into independent routed cells in the taller
/// view; drawing the whole fan in 120px obscures both nodes and relationships.
fn compact_relation_summaries(
    snapshot: &WoodshedMereSnapshot,
    selected: Option<&str>,
) -> Vec<GraphCanvasRelation<String>> {
    let mut groups = BTreeMap::<(String, String), Vec<_>>::new();
    for relation in &snapshot.relations {
        groups
            .entry(ordered_pair(&relation.source, &relation.target))
            .or_default()
            .push(relation);
    }
    groups
        .into_iter()
        .map(|((from, to), relations)| {
            let strongest = relations
                .iter()
                .max_by_key(|relation| relation.weight)
                .expect("a relation group is non-empty");
            let id = format!("mere-pair:{from}:{to}");
            let mut kinds = relations
                .iter()
                .map(|relation| relation.kind.label())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            kinds.sort_unstable();
            let count = relations.len();
            GraphCanvasRelation {
                emphasized: selected == Some(id.as_str()),
                id,
                from,
                to,
                kind: if count == 1 {
                    strongest.kind.label().to_string()
                } else {
                    format!("{count} relations")
                },
                label: if count == 1 {
                    strongest.explanation.clone()
                } else {
                    format!("{}: {}", kinds.join(", "), strongest.explanation)
                },
                route: Vec::new(),
                visible: true,
            }
        })
        .collect()
}

pub fn related_swatch(ui: &UiState) -> GraphCanvasSwatch<String, &'static str> {
    let snapshot = related_mere_snapshot(ui);
    related_swatch_from_snapshot(&snapshot, ui, ui.related_expanded)
}

/// The current Set as a numbered, selectable graph. The ordered Set remains
/// the source document; this adapter supplies bounded layout and applies the
/// user's relation-visibility setting for the graph-canvas component.
///
/// Nodes are keyed by [`CardId`], so selection, hover, and the DOM key survive
/// reorder and removal. The visible number and the serpentine slot are read off
/// current order and change freely under it.
pub fn set_graph_snapshot(ui: &UiState) -> StageGraphSnapshot {
    stage_scene(
        &ui.set,
        &StageSceneOptions {
            arrangement: ui.app_settings.stage.set_arrangement,
            // Availability belongs to the scene; family and per-relation
            // visibility are view state applied below. Keeping every
            // derivable relation here also keeps the epoch stable when the
            // user edits which edges are present.
            sequence: true,
            ..StageSceneOptions::default()
        },
    )
}

/// One derivable Stage relation as the Set inventory presents it. `visible`
/// means present in this graph view; the relation remains in the snapshot when
/// false.
#[derive(Clone, Debug, PartialEq)]
pub struct SetGraphRelationChoice {
    pub reference: StageRelationRef,
    pub key: StageRelationKey,
    pub pair: String,
    pub relation: String,
    pub explanation: String,
    pub authority: &'static str,
    pub weight: u16,
    pub visible: bool,
}

fn set_graph_relation_visible(ui: &UiState, key: &StageRelationKey) -> bool {
    let family_visible = key.kind != SEQUENCE_KIND
        || ui
            .app_settings
            .stage
            .shows_relation(woodshedding::rehearsal::SetGraphEdgeKind::Next);
    family_visible && !ui.set_graph_hidden_relations.contains(key)
}

/// Every relation the current snapshot can derive, including relations the
/// user has withheld from this view.
pub fn set_graph_relation_choices(
    snapshot: &StageGraphSnapshot,
    ui: &UiState,
) -> Vec<SetGraphRelationChoice> {
    snapshot
        .relations()
        .into_iter()
        .filter_map(|(reference, _)| {
            let detail = snapshot.relation_detail(reference, &ui.set)?;
            let from = ui.set.cards.iter().position(|card| card.id == detail.key.from)?;
            let to = ui.set.cards.iter().position(|card| card.id == detail.key.to)?;
            let from_card = &ui.set.cards[from];
            let to_card = &ui.set.cards[to];
            let visible = set_graph_relation_visible(ui, &detail.key);
            Some(SetGraphRelationChoice {
                reference,
                key: detail.key,
                pair: format!(
                    "{} {} → {} {}",
                    from + 1,
                    from_card.label,
                    to + 1,
                    to_card.label
                ),
                relation: detail.label,
                explanation: detail.explanation,
                authority: detail.authority,
                weight: detail.weight,
                visible,
            })
        })
        .collect()
}

fn scene_position(snapshot: &StageGraphSnapshot, x: f32, y: f32) -> (f32, f32) {
    let bounds = snapshot.snapshot.tables.bounds;
    let x = if bounds.size.w > 0.0 {
        (x - bounds.origin.x) / bounds.size.w
    } else {
        0.5
    };
    let y = if bounds.size.h > 0.0 {
        (y - bounds.origin.y) / bounds.size.h
    } else {
        0.5
    };
    (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
}

pub fn set_graph_swatch_from_snapshot(
    snapshot: &StageGraphSnapshot,
    ui: &UiState,
    expanded: bool,
) -> GraphCanvasSwatch<StageInstanceRef, &'static str> {
    let set_graph = ui.set.graph();
    let nodes = snapshot
        .items()
        .into_iter()
        .filter_map(|(reference, item)| {
            let card_id = snapshot.card_of_ref(reference)?;
            let node = set_graph.node(card_id)?;
            let position = ui
                .set_graph_positions
                .get(&card_id)
                .copied()
                .unwrap_or_else(|| {
                    scene_position(
                        snapshot,
                        item.transform.translate.x,
                        item.transform.translate.y,
                    )
                });
            Some(GraphCanvasNode {
                id: reference,
                kind: node.kind,
                position,
                label: format!("{} · {}", node.number, node.label),
                key: Some(format!(
                    "stage-{}-instance-{}",
                    reference.epoch.0, reference.instance.0
                )),
            })
        })
        .collect::<Vec<_>>();
    let node_position = |reference: StageInstanceRef| {
        nodes
            .iter()
            .find(|node| node.id == reference)
            .map(|node| node.position)
    };
    let relations = snapshot
        .relations()
        .into_iter()
        .filter_map(|(reference, relation)| {
            let from = StageInstanceRef {
                epoch: snapshot.epoch(),
                instance: relation.from,
            };
            let to = StageInstanceRef {
                epoch: snapshot.epoch(),
                instance: relation.to,
            };
            let from_position = node_position(from)?;
            let to_position = node_position(to)?;
            let mut route = relation
                .points
                .iter()
                .map(|point| scene_position(snapshot, point.x, point.y))
                .collect::<Vec<_>>();
            if route.len() < 2 {
                route = vec![from_position, to_position];
            } else {
                route[0] = from_position;
                let last = route.len() - 1;
                route[last] = to_position;
            }
            let from_label = snapshot
                .card_of_ref(from)
                .and_then(|id| ui.set.cards.iter().find(|card| card.id == id))
                .map(|card| card.label.as_str())
                .unwrap_or("card");
            let to_label = snapshot
                .card_of_ref(to)
                .and_then(|id| ui.set.cards.iter().find(|card| card.id == id))
                .map(|card| card.label.as_str())
                .unwrap_or("card");
            let kind = relation.kind.as_deref().unwrap_or("relation");
            let key = snapshot.relation_key(reference)?;
            Some(GraphCanvasRelation {
                id: reference.key(),
                from,
                to,
                kind: kind.to_string(),
                label: format!("{from_label} · {kind} · {to_label}"),
                route,
                visible: set_graph_relation_visible(ui, &key),
                emphasized: ui.set_graph_relation == Some(reference),
            })
        })
        .collect();
    let rows = nodes.len().div_ceil(6).max(1);
    let (width, height) = if expanded {
        (520, (rows as u32 * 72).clamp(144, 320))
    } else {
        (300, 104)
    };
    let mut swatch = GraphCanvasSwatch::new(
        SET_GRAPH_LEAF_KEY,
        GraphCanvasSubgraph {
            nodes,
            edges: Vec::new(),
        },
    )
    .with_relations(relations)
    .with_size(width, height)
    .with_label("Staged Set graph")
    .with_expand(false)
    .with_node_labels(expanded && !ui.set_graph_drag_active)
    .with_deferred_drag_rebuild(true);
    swatch.selected = ui
        .set
        .cursor_id()
        .and_then(|card| snapshot.instance_ref_of(card));
    // Focus and hover emphasis are the canvas component's own state (it reads
    // native focus from its node buttons, so the ring is never painted where
    // the keyboard is not). This view supplies only the Set's truth.
    swatch
}

pub fn set_graph_swatch(ui: &UiState) -> GraphCanvasSwatch<StageInstanceRef, &'static str> {
    let snapshot = set_graph_snapshot(ui);
    set_graph_swatch_from_snapshot(&snapshot, ui, ui.set_tray_expanded)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StagePage {
    #[default]
    Catalog,
    Templates,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolPage {
    #[default]
    Fretboard,
    Metronome,
    Tuner,
}

/// Fretboard layout (redesign P4): how the Stage arranges catalog and
/// neck. All three render the same resolved positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BoardLayout {
    /// Catalog sidebar beside the neck (the classic).
    #[default]
    TwoPane,
    /// Full-width neck up top, catalog as a wrapping strip beneath.
    Hero,
    /// The neck alone, enlarged; the catalog stays on the other layouts.
    FullCanvas,
}

impl BoardLayout {
    pub const ALL: [BoardLayout; 3] = [
        BoardLayout::TwoPane,
        BoardLayout::Hero,
        BoardLayout::FullCanvas,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BoardLayout::TwoPane => "Two pane",
            BoardLayout::Hero => "Hero",
            BoardLayout::FullCanvas => "Full canvas",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|l| l.label() == name)
    }

    /// Root class suffix; descendant CSS keys sizing off it.
    fn class(self) -> &'static str {
        match self {
            BoardLayout::TwoPane => "layout-two-pane",
            BoardLayout::Hero => "layout-hero",
            BoardLayout::FullCanvas => "layout-canvas",
        }
    }
}

/// Coarse layout class derived by the host from available logical width.
/// It changes only presentation, never selected or persisted musical state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewportClass {
    #[default]
    Wide,
    Medium,
    Narrow,
}

impl ViewportClass {
    /// These are width bands rather than device names. They preserve a usable
    /// fretboard before the shell starts to crowd it.
    pub fn for_width(width: f32) -> Self {
        if width < 760.0 {
            Self::Narrow
        } else if width < 1_180.0 {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Wide => "viewport-wide",
            Self::Medium => "viewport-medium",
            Self::Narrow => "viewport-narrow",
        }
    }
}

/// MIDI panel state (audio-depth slice 13). Port lists + connection
/// status are host-populated; selections + toggles are view-owned and
/// the host realizes them through the `MidiBackend` seam. Transient (not
/// persisted — port availability is session-dependent).
pub struct MidiUiState {
    /// Available ports, host-populated on startup + refresh.
    pub input_ports: Vec<String>,
    pub output_ports: Vec<String>,
    /// Dropdown selection: index 0 = "None", else `ports[idx - 1]`.
    pub input_dd: SelectState,
    pub output_dd: SelectState,
    /// Slave the transport BPM to incoming MIDI clock.
    pub clock_slave: bool,
    /// Send 24-PPQN clock + Start/Stop to the connected output.
    pub clock_out: bool,
    /// Host-polled readout: BPM derived from incoming clock.
    pub clock_bpm: Option<f32>,
    /// Host-populated connection status.
    pub connected_in: Option<String>,
    pub connected_out: Option<String>,
    /// Host-polled recent-events readout (newest last).
    pub events: Vec<String>,
    /// One-shot: re-scan the port lists. Host consumes after dispatch.
    pub refresh_requested: bool,
}

impl MidiUiState {
    pub fn new() -> Self {
        Self {
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            input_dd: SelectState::new(0),
            output_dd: SelectState::new(0),
            clock_slave: false,
            clock_out: false,
            clock_bpm: None,
            connected_in: None,
            connected_out: None,
            events: Vec::new(),
            refresh_requested: false,
        }
    }
}

impl Default for MidiUiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Runner state: the portable core slice plus view-layer widget state.
pub struct UiState {
    pub stage: StageState,
    pub set: Set,
    pub practice_history: PracticeHistory,
    pub app_settings: AppSettings,
    /// Rehearsal dwell transport running (transient).
    pub rehearsal_running: bool,
    pub song: SongDoc,
    pub song_playing: bool,
    /// Host-polled live bar cursor during playback (timeline follow).
    pub song_bar_live: usize,
    /// Which bar the editor targets (click a chip to select).
    pub song_edit_cursor: usize,
    /// One-shot commands for the audio host, in the order they were asked
    /// for. Push with [`Self::request`]; the host drains the queue each sync.
    /// Continuous audio state (transport, tuner, record-replace) is not here:
    /// it is realized idempotently from this struct every frame.
    pub audio_requests: Vec<AudioRequest>,
    pub section: AppSection,
    pub stage_page: StagePage,
    pub tool_page: ToolPage,
    /// Host-observed width band. This is transient rather than a user setting.
    pub viewport: ViewportClass,
    /// Host-observed window height in logical px, transient. Bounds a vertical
    /// fretboard so a tall neck scrolls inside its viewport instead of running off
    /// the page; 0 until the host reports it (treated as "unbounded").
    pub viewport_h: f32,
    /// Window-chrome requests the host consumes after dispatch (CSD).
    pub transport: TransportState,
    pub tuner: TunerState,
    /// A device/stream failure reported by the host's audio backend.
    pub audio_error: Option<String>,
    pub tuning_dd: SelectState,
    pub root_dd: SelectState,
    /// Corpus search field (a small always-available field in the nav row).
    pub search: TextInput,
    /// Rename buffer for the selected card's label. A text field owns a
    /// `TextInput`, but a card stores a plain `String`, so the buffer loads on
    /// selection change and writes back as you type — see
    /// [`Self::sync_card_rename`]. `card_rename_for` is the card index the
    /// buffer currently holds, which is what makes "did the selection move?"
    /// answerable. Transient.
    pub card_rename: TextInput,
    pub card_rename_for: Option<usize>,
    /// MIDI panel state (Settings tab).
    pub midi: MidiUiState,
    /// Latency-calibration state (Settings tab). `calib_active` drives host
    /// polling; the calibration commands ride `audio_requests`.
    pub calib_status: CalibrationStatus,
    pub calib_active: bool,
    /// Accepted round-trip latency (ms), host-reflected.
    pub latency_ms: Option<f32>,
    /// Looper (song-mode record) state. Recording status and per-bar loop
    /// flags are host-reflected; `song_record_replace` is a view-owned
    /// toggle; the toggle/clear commands ride `audio_requests`.
    pub song_recording: bool,
    pub song_loop_bars: Vec<bool>,
    pub song_record_replace: bool,
    /// Whether the document-bottom Set tray shows its Cards and editor.
    pub set_tray_expanded: bool,
    /// Wall-clock now, Unix epoch milliseconds, refreshed by the host each
    /// frame. The view layer reads no clock of its own (a browser host has a
    /// different one, and the portable core has none), so every practice event
    /// is dated from here. `None` on a host that supplies no clock, which dates
    /// its events as unknown rather than as 1970.
    pub now_ms: Option<u64>,
    /// When the rehearsal's active card became active. The span between this and
    /// the completion is the measured practice the evidence layer rests on, so
    /// it is a real elapsed measurement, not a per-card guess.
    pub card_started_ms: Option<u64>,
    // Set-graph hover and focus emphasis are owned by `cambium::graph_canvas`.
    // They were transient paint state this struct held only to route back into
    // the view on the next rebuild; the component keeps them now.
    /// Whether the selected graph node is expanded into the shared Card editor.
    pub set_graph_card_expanded: bool,
    /// Node positions moved in this view. They override the arrangement in the
    /// Cambium adapter and never alter Set order or persisted material truth.
    pub set_graph_positions: BTreeMap<CardId, (f32, f32)>,
    /// A captured Set-graph gesture is in progress. The desktop host reads
    /// this to keep pointer motion on the view-only fast path; Up clears it and
    /// permits the ordinary backend/persistence tail once for the gesture.
    pub set_graph_drag_active: bool,
    /// Derivable relations withheld from this graph view. Semantic keys survive
    /// dense scene epochs; the underlying relation remains in the snapshot.
    /// Transient until a named graph-view blueprint owns it.
    pub set_graph_hidden_relations: BTreeSet<StageRelationKey>,
    /// The activated scene relation, qualified by the dense scene epoch.
    pub set_graph_relation: Option<StageRelationRef>,
    /// Note markers the user has pinned as `(string_index, fret)` to keep their
    /// detail card open. Multi-pin, to compare. Transient (not persisted).
    pub pinned_markers: Vec<(usize, u8)>,
    /// The marker under the pointer right now, as `(string_index, fret)`, whose
    /// detail card peeks (hover shows, click marks). Set on hover Enter, cleared
    /// on Leave. Transient.
    pub hover_peek: Option<(usize, u8)>,
    /// The Related suggestion under the pointer, shared by the graph swatch and
    /// the suggestions pane so hovering either highlights the other. Transient.
    pub related_hover: Option<RelatedTarget>,
    /// Whether the Related graph swatch is expanded to its taller size.
    pub related_expanded: bool,
    /// Activated relation cell in the joined-mere swatch.
    pub related_relation: Option<String>,
    /// Whether practice is written anywhere at all.
    ///
    /// False for a session the user chose to run with no persona: there is no
    /// key to seal it with and no persona to seal it to, so nothing is stored
    /// and the window closing is the end of it. The app plays exactly the same
    /// either way, which is why the nav row says so rather than leaving it to
    /// be discovered on the next launch.
    pub practice_saved: bool,
    /// The startup persona pick, while one is open. `Some` only on a machine
    /// whose vault holds several personas with none chosen; the host seeds it
    /// before the first frame and clears it when the choice is acted on. While
    /// it is set, [`stage_root`] renders the gate instead of the product.
    pub persona: Option<crate::persona::PersonaPick>,
    /// Set by the Settings row that asks to practise as somebody else. The
    /// host takes it, reads the vault, and puts the gate up; a view cannot,
    /// because reading the roster is vault work.
    pub persona_switch_requested: bool,
    /// What is protecting this session, reported by the store when it opened.
    /// `None` before a store opens at all, which includes a declined gate.
    pub seal: Option<crate::persona::PracticeSeal>,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    pub fn new() -> Self {
        let stage = StageState::new();
        Self {
            set: Set::default(),
            practice_history: PracticeHistory::default(),
            app_settings: AppSettings::default(),
            rehearsal_running: false,
            song: SongDoc::default(),
            song_playing: false,
            song_bar_live: 0,
            song_edit_cursor: 0,
            audio_requests: Vec::new(),
            section: AppSection::Stage,
            stage_page: StagePage::default(),
            tool_page: ToolPage::default(),
            viewport: ViewportClass::default(),
            viewport_h: 0.0,
            transport: TransportState::default(),
            tuner: TunerState::default(),
            audio_error: None,
            tuning_dd: SelectState::new(stage.tuning_idx),
            root_dd: SelectState::new(stage.root_idx),
            search: TextInput::new(""),
            card_rename: TextInput::new(""),
            card_rename_for: None,
            midi: MidiUiState::new(),
            calib_status: CalibrationStatus::Idle,
            calib_active: false,
            latency_ms: None,
            song_recording: false,
            song_loop_bars: Vec::new(),
            song_record_replace: false,
            set_tray_expanded: true,
            now_ms: None,
            card_started_ms: None,
            set_graph_card_expanded: true,
            set_graph_positions: BTreeMap::new(),
            set_graph_drag_active: false,
            set_graph_hidden_relations: BTreeSet::new(),
            set_graph_relation: None,
            pinned_markers: Vec::new(),
            hover_peek: None,
            related_hover: None,
            related_expanded: false,
            related_relation: None,
            practice_saved: true,
            persona: None,
            persona_switch_requested: false,
            seal: None,
            stage,
        }
    }

    /// Apply one event from the epoch-qualified Set scene. Drag positions are
    /// view-local, relation activation retains `RelationId`, and stale events
    /// fail closed when their epoch no longer matches the rendered snapshot.
    pub fn handle_set_graph_event(
        &mut self,
        snapshot: &StageGraphSnapshot,
        event: GraphCanvasEvent<StageInstanceRef>,
    ) {
        match event {
            GraphCanvasEvent::Activate(reference) => {
                let Some(id) = snapshot.card_of_ref(reference) else {
                    return;
                };
                let was_selected = self.set.cursor_id() == Some(id);
                if !self.set.select_id(id) {
                    return;
                }
                self.set_graph_relation = None;
                self.set_graph_card_expanded = if was_selected {
                    !self.set_graph_card_expanded
                } else {
                    true
                };
            }
            GraphCanvasEvent::Drag(drag) => {
                // A stale release must still close the host's cheap drag tail.
                // The position remains epoch-qualified and is ignored below.
                if matches!(drag.phase, cambium::PointerPhase::Up) {
                    self.set_graph_drag_active = false;
                }
                if let Some(card) = snapshot.card_of_ref(drag.id) {
                    self.set_graph_positions.insert(card, drag.position);
                    if !matches!(drag.phase, cambium::PointerPhase::Up) {
                        self.set_graph_drag_active = true;
                    }
                }
            }
            GraphCanvasEvent::RelationActivate(key) => {
                let Some(reference) = StageRelationRef::from_key(&key) else {
                    return;
                };
                if snapshot.relation(reference).is_some() {
                    self.set_graph_relation =
                        (self.set_graph_relation != Some(reference)).then_some(reference);
                }
            }
            GraphCanvasEvent::Expand => self.set_tray_expanded = true,
        }
    }

    /// Toggle one relation's presence in this graph view. The scene relation
    /// and Set truth remain untouched. Withholding the selected relation also
    /// clears the ephemeral activation because its hit target disappears.
    pub fn toggle_set_graph_relation(
        &mut self,
        snapshot: &StageGraphSnapshot,
        key: StageRelationKey,
    ) {
        let now_hidden = if self.set_graph_hidden_relations.remove(&key) {
            false
        } else {
            self.set_graph_hidden_relations.insert(key.clone());
            true
        };
        if now_hidden
            && self
                .set_graph_relation
                .and_then(|reference| snapshot.relation_key(reference))
                .is_some_and(|selected| selected == key)
        {
            self.set_graph_relation = None;
        }
    }

    /// Present every derivable relation. Family visibility is restored too,
    /// so "Show all" means all rather than only all members of an already
    /// filtered family.
    pub fn show_all_set_graph_relations(&mut self) {
        self.set_graph_hidden_relations.clear();
        for kind in woodshedding::rehearsal::SetGraphEdgeKind::ALL {
            if !self.app_settings.stage.shows_relation(kind) {
                self.app_settings.stage.toggle_relation(kind);
            }
        }
    }

    /// Withhold every currently derivable relation from this graph view.
    pub fn hide_all_set_graph_relations(&mut self, snapshot: &StageGraphSnapshot) {
        self.set_graph_hidden_relations
            .extend(snapshot.relations().into_iter().filter_map(|(reference, _)| {
                snapshot.relation_key(reference)
            }));
        self.set_graph_relation = None;
    }

    /// Jump to a search result: focus the target lens/tab (or fill the
    /// set, or swap the tuning), then clear the field so the dropdown
    /// closes.
    fn apply_search_hit(&mut self, hit: SearchHit) {
        match hit {
            SearchHit::Scale(i) => {
                self.stage.set_lens(Lens::Scales);
                self.stage.select_scale(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Chord(i) => {
                self.stage.set_lens(Lens::Chords);
                self.stage.select_chord(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Arpeggio(i) => {
                self.stage.set_lens(Lens::Arpeggios);
                self.stage.select_arpeggio(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Progression(i) => {
                self.stage.set_lens(Lens::Progressions);
                self.stage.select_progression(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Exercise(i) => {
                self.stage.set_lens(Lens::Exercises);
                self.stage.select_exercise(i);
                self.stage_page = StagePage::Catalog;
                self.section = AppSection::Stage;
            }
            SearchHit::Recipe(i) => {
                if let Some(ps) = woodshedding::practice::catalog().get(i) {
                    self.set = set_from_practice(ps);
                    self.section = AppSection::Rehearsal;
                }
            }
            SearchHit::Tuning(i) => {
                self.stage.set_tuning(i);
                self.app_settings.tuning.tuning_idx = self.stage.tuning_idx;
                self.tuning_dd = SelectState::new(self.stage.tuning_idx);
                self.section = AppSection::Stage;
            }
        }
        self.search = TextInput::new("");
    }

    /// Mirror dropdown picks into the core state. Hosts call this after
    /// every dispatch (the `select` widget mutates only its own
    /// `SelectState`).
    pub fn sync(&mut self) {
        self.stage.set_tuning(self.tuning_dd.selected);
        self.app_settings.tuning.tuning_idx = self.stage.tuning_idx;
        self.stage.set_root(self.root_dd.selected);
    }

    pub fn theme(&self) -> ThemeMode {
        ThemeMode::from_name(&self.app_settings.appearance.theme).unwrap_or_default()
    }

    pub fn set_theme(&mut self, theme: ThemeMode) {
        self.app_settings.appearance.theme = theme.label().to_string();
    }

    pub fn board_layout(&self) -> BoardLayout {
        BoardLayout::from_name(&self.app_settings.fretboard.board_layout).unwrap_or_default()
    }

    pub fn set_board_layout(&mut self, layout: BoardLayout) {
        self.app_settings.fretboard.board_layout = layout.label().to_string();
    }

    /// The neck window shown now, `(from, to)`, both resolved: `to` reflects the
    /// instrument's full neck when no explicit end is set. `sync_neck` keeps
    /// `self.stage`'s frets in step with the settings, so these read from it.
    pub fn neck_from(&self) -> u8 {
        self.stage.fret_start
    }
    pub fn neck_to(&self) -> u8 {
        self.stage.fret_count
    }
    /// Whether the end auto-tracks the instrument's full neck (no explicit end).
    pub fn neck_is_full(&self) -> bool {
        self.app_settings.fretboard.neck_end.is_none()
    }

    /// The largest fret the range may reach — matches `apply_neck`'s clamp.
    const NECK_MAX: i32 = 30;

    /// Nudge the window's first fret, kept in `0..=to`.
    pub fn nudge_neck_start(&mut self, delta: i32) {
        let to = self.neck_to() as i32;
        let next = (self.app_settings.fretboard.neck_start as i32 + delta).clamp(0, to);
        self.app_settings.fretboard.neck_start = next as u8;
    }

    /// Nudge the window's last fret, kept in `from..=NECK_MAX`. Setting it makes
    /// the end explicit (it stops auto-tracking the instrument).
    pub fn nudge_neck_end(&mut self, delta: i32) {
        let from = self.app_settings.fretboard.neck_start as i32;
        let next = (self.neck_to() as i32 + delta).clamp(from, Self::NECK_MAX);
        self.app_settings.fretboard.neck_end = Some(next as u8);
    }

    /// Reset the end to the instrument's full neck (auto-tracking).
    pub fn set_neck_full(&mut self) {
        self.app_settings.fretboard.neck_end = None;
    }

    /// Pin or unpin a marker's detail card (multi-pin, so clicking toggles).
    pub fn toggle_pin(&mut self, string_index: usize, fret: u8) {
        let key = (string_index, fret);
        if let Some(i) = self.pinned_markers.iter().position(|&p| p == key) {
            self.pinned_markers.remove(i);
        } else {
            self.pinned_markers.push(key);
        }
    }

    pub fn is_pinned(&self, string_index: usize, fret: u8) -> bool {
        self.pinned_markers.contains(&(string_index, fret))
    }

    pub fn clear_pins(&mut self) {
        self.pinned_markers.clear();
    }

    /// Mark or unmark a board position on the card under the Rehearsal cursor.
    /// Marking is a neutral selection; [`Self::card_mark_mode`] decides what it
    /// does to playback. The board is the material editor ("click edits, hover
    /// shows").
    pub fn toggle_card_mark(&mut self, string_index: usize, fret: u8) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        let marked = &mut self.set.cards[cursor].setting.marked;
        let key = (string_index, fret);
        if let Some(i) = marked.iter().position(|&p| p == key) {
            marked.remove(i);
        } else {
            marked.push(key);
        }
    }

    /// The card under the Rehearsal cursor, if any.
    fn current_card(&self) -> Option<&Card> {
        self.set
            .cards
            .get(self.set.cursor.min(self.set.cards.len().saturating_sub(1)))
    }

    /// Whether a board position is marked on the card under the cursor.
    pub fn card_marked(&self, string_index: usize, fret: u8) -> bool {
        self.current_card()
            .is_some_and(|c| c.setting.marked.contains(&(string_index, fret)))
    }

    /// The mark mode of the card under the cursor (Off if there is no card).
    pub fn card_mark_mode(&self) -> MarkMode {
        self.current_card()
            .map(|c| c.setting.mark_mode)
            .unwrap_or_default()
    }

    /// Set the mark mode of the card under the cursor.
    pub fn set_card_mark_mode(&mut self, mode: MarkMode) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        self.set.cards[cursor].setting.mark_mode = mode;
    }

    /// Clear every mark on the card under the cursor.
    pub fn clear_card_marks(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        self.set.cards[cursor].setting.marked.clear();
    }

    /// Whether a position is silenced (and dimmed) by the current mark mode:
    /// Solo excludes the unmarked, Mute excludes the marked. With nothing
    /// marked the mode is inert.
    pub fn card_excluded(&self, string_index: usize, fret: u8) -> bool {
        let Some(card) = self.current_card() else {
            return false;
        };
        if card.setting.marked.is_empty() {
            return false;
        }
        let marked = card.setting.marked.contains(&(string_index, fret));
        match card.setting.mark_mode {
            MarkMode::Off => false,
            MarkMode::Solo => !marked,
            MarkMode::Mute => marked,
        }
    }

    /// What a Stage-board marker click does: in draw mode it appends the marker
    /// to the hand-drawn path; otherwise it pins the detail card (the prior
    /// behaviour). Resolved at click time so toggling Draw needs no rebuild.
    pub fn board_marker_click(&mut self, string_index: usize, fret: u8) {
        if self.stage.draw_mode {
            self.stage.append_to_path(string_index, fret);
        } else {
            self.toggle_pin(string_index, fret);
        }
    }

    pub fn nudge_bpm(&mut self, delta: f32) {
        self.transport.nudge_bpm(delta);
        self.app_settings.metronome.bpm = self.transport.bpm;
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.transport.bpm = bpm.clamp(30.0, 300.0);
        self.app_settings.metronome.bpm = self.transport.bpm;
    }

    fn selected_card_index(&self) -> Option<usize> {
        (!self.set.cards.is_empty()).then(|| self.set.cursor.min(self.set.cards.len() - 1))
    }

    /// Push the neck-window settings (start + end, or the instrument default)
    /// into the stage each frame, so the board's extent tracks the settings and
    /// the current instrument with no special-casing of tuning/settings changes.
    pub fn sync_neck(&mut self) {
        self.stage.apply_neck(
            self.app_settings.fretboard.neck_start,
            self.app_settings.fretboard.neck_end,
        );
    }

    /// Keep the rename buffer and the selected card's label in step. The buffer
    /// is authoritative while the selection holds still (so typing renames the
    /// card); moving to another card reloads it from that card. The host calls
    /// this each frame.
    pub fn sync_card_rename(&mut self) {
        let Some(cursor) = self.selected_card_index() else {
            self.card_rename_for = None;
            return;
        };
        if self.card_rename_for != Some(cursor) {
            // Selection moved: adopt this card's label.
            self.card_rename = TextInput::new(self.set.cards[cursor].label.clone());
            self.card_rename_for = Some(cursor);
        } else if self.card_rename.text() != self.set.cards[cursor].label {
            // Typed: the buffer is the rename.
            self.set.cards[cursor].label = self.card_rename.text().to_string();
        }
    }

    pub fn cycle_card_touch(&mut self) {
        let Some(cursor) = self.selected_card_index() else { return };
        let card = &mut self.set.cards[cursor];
        card.touch = match &card.touch {
            Touch::Block => Touch::Arpeggiate {
                direction: Default::default(),
                inversion: 0,
            },
            Touch::Arpeggiate { direction, inversion } => {
                use woodshed_core::arpeggio::ArpeggioDirection as D;
                match direction {
                    D::UpDown => Touch::Arpeggiate {
                        direction: D::Up,
                        inversion: *inversion,
                    },
                    D::Up => Touch::Arpeggiate {
                        direction: D::Down,
                        inversion: *inversion,
                    },
                    D::Down => Touch::Walk,
                }
            }
            Touch::Walk => Touch::Block,
        };
    }

    pub fn cycle_card_hold(&mut self) {
        let Some(cursor) = self.selected_card_index() else { return };
        let card = &mut self.set.cards[cursor];
        card.timing.hold = match card.timing.hold {
            Hold::Manual => Hold::Bars(2),
            Hold::Bars(2) => Hold::Bars(4),
            Hold::Bars(4) => Hold::Bars(8),
            Hold::Bars(_) => Hold::Seconds(30.0),
            Hold::Seconds(_) | Hold::Reps(_) => Hold::Manual,
        };
    }

    pub fn nudge_card_bpm(&mut self, delta: f32) {
        let Some(cursor) = self.selected_card_index() else { return };
        let card = &mut self.set.cards[cursor];
        let base = card.timing.bpm.unwrap_or(self.transport.bpm);
        card.timing.bpm = Some((base + delta).clamp(30.0, 300.0));
    }

    pub fn shift_card_window(&mut self, delta: i8) {
        let Some(cursor) = self.selected_card_index() else { return };
        let max_fret = self.stage.fret_count;
        let card = &mut self.set.cards[cursor];
        let window = card
            .setting
            .fret_window
            .unwrap_or(FretWindow { start: 0, span: 4 });
        let max_start = max_fret.saturating_sub(window.span);
        let start = if delta < 0 {
            window.start.saturating_sub(delta.unsigned_abs())
        } else {
            window.start.saturating_add(delta as u8).min(max_start)
        };
        card.setting.fret_window = Some(FretWindow {
            start,
            span: window.span,
        });
    }

    pub fn clear_card_window(&mut self) {
        let Some(cursor) = self.selected_card_index() else { return };
        self.set.cards[cursor].setting.fret_window = None;
    }

    /// Update the transient layout band after a host resize. Returns whether a
    /// view rebuild is required.
    pub fn set_viewport_width(&mut self, width: f32) -> bool {
        let next = ViewportClass::for_width(width);
        if self.viewport == next {
            false
        } else {
            self.viewport = next;
            true
        }
    }

    /// Record the host's window height (logical px), which bounds a vertical
    /// board's scroll viewport. Rebuild only on a meaningful change, so a resize
    /// drag does not rebuild every pixel.
    pub fn set_viewport_height(&mut self, height: f32) -> bool {
        if (self.viewport_h - height).abs() >= 16.0 {
            self.viewport_h = height;
            true
        } else {
            false
        }
    }

    /// Pitches + shape for the on-demand "♪ Hear" preview, resolved from
    /// context: the current rehearsal card on the Rehearsal tab, else the
    /// active Stage lens. Empty pitches = nothing to voice. The host calls
    /// this when it drains an [`AudioRequest::PreviewVoicing`].
    pub fn preview_voicing(&self) -> (Vec<f32>, f32, f32) {
        if self.section == AppSection::Rehearsal && !self.set.cards.is_empty() {
            let cursor = self.set.cursor.min(self.set.cards.len() - 1);
            return self.stage.card_sounding_pitches(&self.set.cards[cursor]);
        }
        self.stage.voicing_preview()
    }

    pub fn request_preview(&mut self) {
        let subject_id = if self.section == AppSection::Rehearsal && !self.set.cards.is_empty() {
            let cursor = self.set.cursor.min(self.set.cards.len() - 1);
            catalog_id_for_card(&self.set.cards[cursor])
        } else {
            self.stage.catalog_id()
        };
        if let Some(subject_id) = subject_id {
            self.practice_history
                .record(self.now_ms, subject_id, EngagementKind::Previewed, None, None);
        }
        self.request(AudioRequest::PreviewVoicing);
    }

    /// Ask the audio host to do one thing. Requests are kept in order and
    /// drained by the host each sync; asking twice does it twice.
    pub fn request(&mut self, request: AudioRequest) {
        self.audio_requests.push(request);
    }

    pub fn stage_current(&mut self, from_id: Option<String>) {
        let subject_id = self.stage.catalog_id();
        if let Some(card) = self.stage.card_from_lens() {
            self.set.push(card);
            if let Some(subject_id) = subject_id {
                self.practice_history
                    .record(self.now_ms, subject_id, EngagementKind::Staged, from_id, None);
            }
        }
    }

    /// Save the hand-drawn path as a card in the set — draw → save → practice.
    /// The drawing is kept, so you can refine and save a variant. No history
    /// record: a drawn path is not a catalog subject.
    pub fn save_drawn_path(&mut self) {
        if let Some(card) = self.stage.card_from_drawn_path() {
            self.set.push(card);
        }
    }

    pub fn record_rehearsal_cursor(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        // The card is now the one being played, so this is where its practice
        // span opens; `complete_rehearsal_cursor` closes it.
        self.card_started_ms = self.now_ms;
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        if let Some(id) = catalog_id_for_card(&self.set.cards[cursor]) {
            self.practice_history
                .record(self.now_ms, id, EngagementKind::Rehearsed, None, None);
        }
    }

    pub fn complete_rehearsal_cursor(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        // Close the span opened when this card became active. Both ends must be
        // dated for the measurement to mean anything, and a clock that went
        // backwards (a system time change mid-session) yields no measurement
        // rather than a wrapped one.
        let practiced_ms = match (self.now_ms, self.card_started_ms) {
            (Some(now), Some(started)) => now.checked_sub(started),
            _ => None,
        };
        self.card_started_ms = self.now_ms;
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        if let Some(id) = catalog_id_for_card(&self.set.cards[cursor]) {
            self.practice_history.record(
                self.now_ms,
                id,
                EngagementKind::Completed,
                None,
                practiced_ms,
            );
        }
    }

    /// Snapshot the persistable subset (the W0.2 seam's payload).
    pub fn to_persisted(&self) -> PersistedSession {
        PersistedSession::capture(
            &self.stage,
            self.section,
            &self.set,
            &self.song,
            &self.practice_history,
        )
    }

    /// Restore a persisted session (indices clamp; unknown theme names
    /// fall back to the default).
    pub fn apply_persisted(
        &mut self,
        session: &PersistedSession,
        app_settings: AppSettings,
    ) {
        session.restore(&mut self.stage, &app_settings);
        self.set = session.set.clone();
        self.song = session.song.clone();
        self.practice_history = session.practice_history.clone();
        self.app_settings = app_settings;
        self.set_graph_positions.clear();
        self.set_graph_drag_active = false;
        self.set_graph_hidden_relations.clear();
        self.set_graph_relation = None;
        self.related_relation = None;
        // The one bounded migration for sessions written before occurrence
        // identity: Cards gain ids, the legacy single-boolean edge toggle
        // becomes a relation set. Both persist on the next save, and both
        // are idempotent for sessions written after.
        self.set.ensure_card_ids();
        self.app_settings.stage.adopt_legacy_relation_visibility();
        self.section = session.section;
        self.transport.bpm = self.app_settings.metronome.bpm.clamp(30.0, 300.0);
        self.tuning_dd = SelectState::new(self.stage.tuning_idx);
        self.root_dd = SelectState::new(self.stage.root_idx);
    }
}

/// Boxed heterogeneous child view over [`UiState`].
pub type UiChild = Box<dyn AnyView<UiState, (), GenetCtx, GenetElement>>;

fn pill(section: AppSection, active: bool) -> UiChild {
    Box::new(clickable(
        el("span", text(section.label()))
            .attr("class", if active { "pill pill-active" } else { "pill" }),
        move |ui: &mut UiState, _| {
            ui.section = section;
        },
    ))
}

fn header(ui: &UiState) -> UiChild {
    let tuning_names: Vec<&str> = tunings().iter().map(|t| t.name).collect();
    let tuning_dd = map_state(select(&ui.tuning_dd, &tuning_names), |ui: &mut UiState| {
        &mut ui.tuning_dd
    });
    let root_dd = map_state(select(&ui.root_dd, &ROOT_NAMES), |ui: &mut UiState| {
        &mut ui.root_dd
    });
    Box::new(
        el(
            "div",
            (
                el("span", text("Tuning")).attr("class", "header-label"),
                tuning_dd,
                el("span", ()).attr("class", "header-gap"),
                el("span", text("Root")).attr("class", "header-label"),
                root_dd,
            ),
        )
        .attr("class", "header-row"),
    )
}

fn transport(ui: &UiState) -> UiChild {
    let play_label = if ui.transport.playing { "Stop" } else { "Play" };
    let tuner_label = if ui.tuner.enabled {
        "Tuner: on"
    } else {
        "Tuner: off"
    };
    let readout = if let Some(err) = &ui.audio_error {
        format!("audio: {err}")
    } else if ui.tuner.enabled {
        match &ui.tuner.reading {
            Some(r) => format!(
                "{}{} {}{:.0}¢{}",
                r.note,
                r.octave,
                if r.cents >= 0.0 { "+" } else { "" },
                r.cents,
                if r.in_tune { "  in tune" } else { "" },
            ),
            None => "listening...".to_string(),
        }
    } else {
        String::new()
    };
    Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.transport.playing = !ui.transport.playing;
                    },
                ),
                clickable(
                    el("div", text("-")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.nudge_bpm(-5.0),
                ),
                el("div", text(format!("{:.0} bpm", ui.transport.bpm))).attr("class", "t-readout"),
                clickable(
                    el("div", text("+")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.nudge_bpm(5.0),
                ),
                clickable(
                    el("div", text("♪ Hear")).attr("class", "t-btn t-hear"),
                    |ui: &mut UiState, _| ui.request_preview(),
                ),
                el("span", ()).attr("class", "header-gap"),
                clickable(
                    el("div", text(tuner_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.tuner.enabled = !ui.tuner.enabled;
                        if !ui.tuner.enabled {
                            ui.tuner.reading = None;
                        }
                    },
                ),
                el("div", text(readout)).attr("class", "t-readout"),
                el("span", ()).attr("class", "header-gap"),
                clickable(
                    el("div", text("Stage")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage_current(None),
                ),
                el("div", text(format!("{} cards", ui.set.cards.len()))).attr("class", "t-readout"),
            ),
        )
        .attr("class", "transport"),
    )
}

fn lens_strip(ui: &UiState) -> UiChild {
    let mut items: Vec<UiChild> = Lens::ALL
        .iter()
        .map(|&lens| {
            let class = if ui.stage_page == StagePage::Catalog && lens == ui.stage.lens {
                "lens lens-active"
            } else {
                "lens"
            };
            Box::new(clickable(
                el("div", text(lens.label())).attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.stage_page = StagePage::Catalog;
                    ui.stage.set_lens(lens);
                },
            )) as UiChild
        })
        .collect();
    let templates_class = if ui.stage_page == StagePage::Templates {
        "lens lens-active"
    } else {
        "lens"
    };
    items.push(Box::new(clickable(
        el("div", text("Sets")).attr("class", templates_class),
        |ui: &mut UiState, _| ui.stage_page = StagePage::Templates,
    )));
    Box::new(el("div", items).attr("class", "lens-strip"))
}

fn sidebar(ui: &UiState) -> UiChild {
    let items: Vec<UiChild> = match ui.stage.lens {
        Lens::Scales => ui
            .stage
            .scales()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let class = if i == ui.stage.scale_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(s.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_scale(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Chords => ui
            .stage
            .chords()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let class = if i == ui.stage.chord_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                let label = if c.symbol.is_empty() {
                    c.name.to_string()
                } else {
                    format!("{} ({})", c.name, c.symbol)
                };
                Box::new(clickable(
                    el("div", text(label)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_chord(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Arpeggios => ui
            .stage
            .chords()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let class = if i == ui.stage.arpeggio_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                let label = if c.symbol.is_empty() {
                    c.name.to_string()
                } else {
                    format!("{} ({})", c.name, c.symbol)
                };
                Box::new(clickable(
                    el("div", text(label)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_arpeggio(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Progressions => ui
            .stage
            .progressions()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let class = if ui.stage.progression_idx == Some(i) {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(p.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_progression(i);
                    },
                )) as UiChild
            })
            .collect(),
        Lens::Exercises => ui
            .stage
            .exercises()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let class = if i == ui.stage.exercise_idx {
                    "side-item side-active"
                } else {
                    "side-item"
                };
                Box::new(clickable(
                    el("div", text(e.name)).attr("class", class),
                    move |ui: &mut UiState, _| {
                        ui.stage.select_exercise(i);
                    },
                )) as UiChild
            })
            .collect(),
    };
    Box::new(el("div", items).attr("class", "side"))
}


/// Render a transport lens's board (Arpeggio / Exercise / Progression) on the
/// painted leaf, so it shares the crisp neck, orientation, marker style, and
/// scroll viewport that Scale / Chord use. `controls` sits above the neck (a
/// transport deck or the progression's chord cards); `caption` reads below. The
/// leaf itself is fed by the host from `lens_markers`; here we place it and draw
/// the label overlay from the same markers, so paint and labels agree. The
/// transport's current step is drawn bright by the leaf (its `active`) and its
/// label is brightened to match.
fn leaf_section_board(
    ui: &UiState,
    markers: &[woodshed_core::LensMarker],
    controls: UiChild,
    caption: String,
    aria: String,
) -> UiChild {
    let string_count = ui.stage.string_count();
    let geom = BoardGeom {
        string_count,
        fret_start: ui.stage.fret_start,
        fret_count: ui.stage.fret_count,
        orientation: Orientation::from_name(&ui.app_settings.fretboard.orientation),
    };
    let (w, h) = geom.size_u32();
    let (mw, mh) = geom.marker_size();
    let labels: Vec<UiChild> = markers
        .iter()
        .filter(|m| geom.in_window(m.fret) && m.string_index < string_count)
        .map(|m| {
            let (px, py) = geom.note_pos(m.string_index, m.fret);
            let (lx, ly) = (px - mw / 2.0, py - mh / 2.0);
            let class = if m.is_current {
                "fret-label step"
            } else if m.is_trail {
                // The Exercise's fading trail: dimmed to match its faint marker.
                "fret-label excluded"
            } else {
                "fret-label"
            };
            // A spoken marker: its note, then where it sits (guitar-numbered
            // strings, 1 = highest). The painted neck is invisible to the DOM, so
            // this is how the notes reach assistive tech and a semantic driver.
            let a11y = format!(
                "{}, string {}, fret {}",
                m.label,
                string_count - m.string_index,
                m.fret
            );
            Box::new(
                el("div", text(m.label.clone()))
                    .attr("class", class)
                    .attr("aria-label", a11y)
                    .attr(
                        "style",
                        format!("left:{lx:.1}px; top:{ly:.1}px; width:{mw:.1}px; height:{mh:.1}px"),
                    ),
            ) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                controls,
                el(
                    "div",
                    (
                        custom_leaf::<UiState, ()>(FRETBOARD_LEAF_KEY, w, h),
                        el("div", labels)
                            .attr("class", "label-layer")
                            .attr("style", format!("width:{w}px; height:{h}px")),
                    ),
                )
                .attr("class", board_viewport_class(geom.orientation))
                .attr("aria-label", aria)
                .attr("style", board_viewport_style(geom.orientation, w, h, ui.viewport_h)),
                el("div", text(caption)).attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    ) as UiChild
}

fn exercise_board_view(ui: &UiState) -> UiChild {
    let state = &ui.stage;
    let board = state.exercise_board();
    let play_label = if state.exercise_playing {
        "Pause"
    } else {
        "Run"
    };
    let deck: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.stage.exercise_playing = !ui.stage.exercise_playing;
                    },
                ),
                clickable(
                    el("div", text("Step")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.exercise_advance(),
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.stage.exercise_nudge_fret(-1),
                ),
                el("div", text(format!("fret {}", board.starting_fret))).attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| ui.stage.exercise_nudge_fret(1),
                ),
            ),
        )
        .attr("class", "transport"),
    );
    let caption = format!(
        "{} — step {}/{} · {}",
        board.name,
        board.step + 1,
        board.total,
        board.description,
    );
    let aria = format!("{} exercise, {} notes", board.name, board.dots.len());
    leaf_section_board(ui, &ui.stage.lens_markers().0, deck, caption, aria)
}

fn progression_board_view(ui: &UiState) -> UiChild {
    let Some(board) = ui.stage.progression_board() else {
        return Box::new(
            el(
                "div",
                el("div", text("Pick a progression from the list.")).attr("class", "placeholder"),
            )
            .attr("class", "board"),
        );
    };
    let cards: Vec<UiChild> = board
        .cards
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let class = if c.is_expanded {
                "prog-card prog-card-active"
            } else {
                "prog-card"
            };
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(c.numeral.clone())).attr("class", "prog-numeral"),
                        el("div", text(c.chord_label.clone())).attr("class", "prog-chord"),
                    ),
                )
                .attr("class", class),
                move |ui: &mut UiState, _| {
                    ui.stage.progression_expand(i);
                },
            )) as UiChild
        })
        .collect();
    let controls: UiChild = Box::new(el("div", cards).attr("class", "prog-cards"));
    let caption = format!(
        "{} — {} · showing {}",
        ui.stage.material_name(),
        board.description,
        board.expanded_label,
    );
    let aria = format!(
        "{} progression, {} chord tones",
        ui.stage.material_name(),
        board.dots.len()
    );
    leaf_section_board(ui, &ui.stage.lens_markers().0, controls, caption, aria)
}

fn arpeggio_board_view(ui: &UiState) -> UiChild {
    use woodshed_core::arpeggio::ArpeggioDirection;
    let state = &ui.stage;
    let board = state.arpeggio_board();
    let dir_label = match board.direction {
        ArpeggioDirection::UpDown => "Up-Down",
        ArpeggioDirection::Up => "Up",
        ArpeggioDirection::Down => "Down",
    };
    let play_label = if state.arpeggio_playing {
        "Pause"
    } else {
        "Run"
    };
    let deck: UiChild = Box::new(
        el(
            "div",
            (
                clickable(
                    el("div", text(play_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| {
                        ui.stage.arpeggio_playing = !ui.stage.arpeggio_playing;
                    },
                ),
                clickable(
                    el("div", text("Step")).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_advance(),
                ),
                clickable(
                    el("div", text(dir_label)).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_cycle_direction(),
                ),
                clickable(
                    el("div", text(board.inversion_label.clone())).attr("class", "t-btn"),
                    |ui: &mut UiState, _| ui.stage.arpeggio_cycle_inversion(),
                ),
                clickable(
                    el("div", text("<")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let b = ui.stage.arpeggio_board();
                        let prev = (b.position_idx + b.shape_count - 1) % b.shape_count;
                        ui.stage.arpeggio_select_position(prev);
                    },
                ),
                el(
                    "div",
                    text(format!(
                        "shape {}/{}",
                        board.position_idx + 1,
                        board.shape_count
                    )),
                )
                .attr("class", "t-readout"),
                clickable(
                    el("div", text(">")).attr("class", "t-btn t-narrow"),
                    |ui: &mut UiState, _| {
                        let b = ui.stage.arpeggio_board();
                        let next = (b.position_idx + 1) % b.shape_count;
                        ui.stage.arpeggio_select_position(next);
                    },
                ),
            ),
        )
        .attr("class", "transport"),
    );
    let caption = format!(
        "{} — frets {}-{}, step {}/{}",
        state.material_name(),
        board.start_fret,
        board.start_fret + woodshed_core::arpeggio::ARP_SHAPE_SPAN,
        board.step + 1,
        board.walk_len,
    );
    let aria = format!("{} arpeggio, {} notes", state.material_name(), board.dots.len());
    leaf_section_board(ui, &ui.stage.lens_markers().0, deck, caption, aria)
}

/// Every fretboard sits in a `.board-viewport`: an overflow container that also
/// serves as its overlay's containing block, so the leaf and its absolute
/// label/card layers clip and scroll as one. Shared by all four board lenses.
fn board_viewport_class(_orientation: Orientation) -> &'static str {
    "board-viewport"
}

/// The viewport's inline size + which axis scrolls. A horizontal neck runs along
/// x: the flex column bounds its width, so `overflow-x:auto` scrolls the excess
/// frets while its short height is pinned exactly — a wide range fits the pane and
/// scrolls instead of pushing the layout. A vertical neck runs along y, where the
/// row gives no natural height bound, so the host-reported window height bounds
/// it: a tall neck is capped and `overflow-y:auto` scrolls it internally, while a
/// neck shorter than the cap keeps its own height. `avail_h` is the window's
/// logical height (0 = not yet reported → the board keeps its natural height).
fn board_viewport_style(orientation: Orientation, w: u32, h: u32, avail_h: f32) -> String {
    match orientation {
        Orientation::Horizontal => {
            format!("height:{h}px; overflow-x:auto; overflow-y:hidden;")
        }
        Orientation::Vertical => {
            // The board's share of the window, leaving room for the header, lens
            // strip, deck, and caption. Below the cap the neck uses its own height,
            // so a short neck shows no needless scrollbar.
            let cap = (avail_h * 0.60).round() as u32;
            if avail_h > 0.0 && cap > 0 && h > cap {
                format!("width:{w}px; height:{cap}px; overflow-y:auto; overflow-x:hidden;")
            } else {
                format!("width:{w}px;")
            }
        }
    }
}

/// A pinned marker's detail card. Cards flow in a wrapping strip under the
/// board (never overlapping each other or the neck — the old marker-anchored
/// popovers stacked illegibly when adjacent markers were pinned). Shows note +
/// octave, scale degree + interval, the string/fret, Play, and an unpin ×.
fn note_card(d: &woodshed_core::FretDot, string_count: usize) -> UiChild {
    let title = format!("{}{}", d.label, d.octave);
    let sub = if d.degree.is_empty() {
        d.interval_name.clone()
    } else {
        format!("{} · {}", d.degree, d.interval_name)
    };
    // Guitar-style numbering: string 1 is the highest (largest index).
    let pos = format!("string {} · fret {}", string_count - d.string_index, d.fret);
    let freq = d.frequency;
    let (si, fret) = (d.string_index, d.fret);
    Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        el("span", text(title)).attr("class", "note-card-title"),
                        clickable(
                            el("span", text("×")).attr("class", "note-card-close"),
                            move |ui: &mut UiState, _| ui.toggle_pin(si, fret),
                        ),
                    ),
                )
                .attr("class", "note-card-head"),
                el("div", text(sub)).attr("class", "note-card-row"),
                el("div", text(pos)).attr("class", "note-card-row"),
                clickable(
                    el("div", text("♪ Play")).attr("class", "note-card-play"),
                    move |ui: &mut UiState, _| ui.request(AudioRequest::PreviewNote(freq)),
                ),
            ),
        )
        .attr("class", "note-card"),
    ) as UiChild
}

pub(super) fn board(ui: &UiState) -> UiChild {
    let state = &ui.stage;
    if state.lens == Lens::Arpeggios {
        return arpeggio_board_view(ui);
    }
    if state.lens == Lens::Progressions {
        return progression_board_view(ui);
    }
    if state.lens == Lens::Exercises {
        return exercise_board_view(ui);
    }
    let dot_list = state.dots();
    let string_count = state.string_count();
    // The board is painted by a Sprigging leaf (crisp strings/wires/nut/inlays
    // and coloured markers). Over it sit two CSS layers: the note labels (each a
    // click target that pins its detail card — multi-pin, so clicking toggles)
    // and the pinned cards. Positions come from the leaf's own marker geometry so
    // overlay and paint align.
    let geom = BoardGeom {
        string_count,
        fret_start: state.fret_start,
        fret_count: state.fret_count,
        orientation: Orientation::from_name(&ui.app_settings.fretboard.orientation),
    };
    let (w, h) = geom.size_u32();
    let (mw, mh) = geom.marker_size();
    let labels: Vec<UiChild> = dot_list
        .iter()
        .map(|d| {
            let (si, fret) = (d.string_index, d.fret);
            let (px, py) = geom.note_pos(si, fret);
            let lx = px - mw / 2.0;
            let ly = py - mh / 2.0;
            // While drawing, a marker on the path shows its step number instead
            // of its note name, so the order reads without playing it. A path may
            // revisit a note, so its indices join ("2,5").
            let steps: Vec<String> = state
                .authored_path
                .iter()
                .enumerate()
                .filter(|(_, &p)| p == (si, fret))
                .map(|(i, _)| (i + 1).to_string())
                .collect();
            let numbered = state.draw_mode && !steps.is_empty();
            let label_text = if numbered {
                steps.join(",")
            } else {
                d.label.clone()
            };
            let class = if numbered {
                "fret-label step"
            } else if ui.is_pinned(si, fret) {
                "fret-label pinned"
            } else {
                "fret-label"
            };
            // A marker is a real, named button in the accessibility tree: the
            // paint gives no semantics, so the spoken note/role/place rides here
            // as an aria-label (also what a semantic driver resolves it by).
            let a11y = woodshed_core::marker_a11y_label(d, string_count);
            Box::new(clickable(
                el("div", text(label_text))
                    .attr("class", class)
                    .attr("role", "button")
                    .attr("aria-label", a11y)
                    .attr(
                        "style",
                        format!("left:{lx:.1}px; top:{ly:.1}px; width:{mw:.1}px; height:{mh:.1}px"),
                    ),
                move |ui: &mut UiState, _| ui.board_marker_click(si, fret),
            )) as UiChild
        })
        .collect();
    // Pinned cards flow in a wrapping strip below the neck — never overlapping
    // each other (adjacent pins used to stack their popovers) and never hiding
    // the board.
    let cards: Vec<UiChild> = ui
        .pinned_markers
        .iter()
        .filter_map(|&(si, fret)| {
            dot_list
                .iter()
                .find(|d| d.string_index == si && d.fret == fret)
                .map(|d| note_card(d, string_count))
        })
        .collect();
    let card_strip =
        (!cards.is_empty()).then(|| el("div", cards).attr("class", "note-card-strip"));
    Box::new(
        el(
            "div",
            (
                // The board sits in a scroll viewport: the leaf keeps its natural
                // size (a wide neck is wider than the pane), and the viewport —
                // an overflow container that is also the overlay's containing
                // block — clips and scrolls the leaf and its absolute label
                // layer together. So an arbitrary fret range fits the pane and
                // the neck scrolls, instead of the board pushing the layout wide.
                el(
                    "div",
                    (
                        custom_leaf::<UiState, ()>(FRETBOARD_LEAF_KEY, w, h),
                        // The overlay layer spans the whole board, not just its
                        // auto content box: a lifted layer fully inside the
                        // viewport clip has that clip dropped (so its markers would
                        // escape), but a full-board box spills the clip and keeps
                        // it, so the labels clip and scroll with the leaf.
                        el("div", labels)
                            .attr("class", "label-layer")
                            .attr("style", format!("width:{w}px; height:{h}px")),
                    ),
                )
                .attr("class", board_viewport_class(geom.orientation))
                // Names the neck as a region so assistive tech announces which
                // board this is before the player steps through its notes.
                .attr(
                    "aria-label",
                    format!(
                        "{} fretboard, {} notes, frets {}\u{2013}{}",
                        state.material_name(),
                        dot_list.len(),
                        state.fret_start,
                        state.fret_count
                    ),
                )
                .attr("style", board_viewport_style(geom.orientation, w, h, ui.viewport_h)),
                card_strip,
                el(
                    "div",
                    (
                        clickable(
                            el(
                                "div",
                                text(if state.scale_run_playing {
                                    "■ Stop"
                                } else {
                                    "♪ Run"
                                }),
                            )
                            .attr("class", "run-btn"),
                            |ui: &mut UiState, _| ui.stage.toggle_scale_run(),
                        ),
                        clickable(
                            el("div", text("Path")).attr(
                                "class",
                                if state.path_shown {
                                    "run-btn path-on"
                                } else {
                                    "run-btn"
                                },
                            ),
                            |ui: &mut UiState, _| ui.stage.toggle_path(),
                        ),
                        clickable(
                            el(
                                "div",
                                text(if state.draw_mode {
                                    format!("Draw ({})", state.authored_path.len())
                                } else {
                                    "Draw".to_string()
                                }),
                            )
                            .attr(
                                "class",
                                if state.draw_mode {
                                    "run-btn draw-on"
                                } else {
                                    "run-btn"
                                },
                            ),
                            |ui: &mut UiState, _| ui.stage.toggle_draw_mode(),
                        ),
                        // Path-editing tools: appear once you're drawing or have
                        // a drawn path. Retrograde, rotate the start, undo, clear.
                        (state.draw_mode || !state.authored_path.is_empty()).then(|| {
                            el(
                                "div",
                                (
                                    clickable(
                                        el("div", text("Undo")).attr("class", "draw-tool"),
                                        |ui: &mut UiState, _| ui.stage.undo_path(),
                                    ),
                                    clickable(
                                        el("div", text("Reverse")).attr("class", "draw-tool"),
                                        |ui: &mut UiState, _| ui.stage.reverse_path(),
                                    ),
                                    clickable(
                                        el("div", text("Rotate")).attr("class", "draw-tool"),
                                        |ui: &mut UiState, _| ui.stage.rotate_path(),
                                    ),
                                    // Move the shape a whole octave, which keeps
                                    // every note's pitch class — so it stays on
                                    // the material's tones. The spider's move.
                                    // Shown only when it fits: on the default
                                    // 12-fret neck an octave rarely does, and a
                                    // button that silently no-ops is worse than
                                    // no button.
                                    state.can_shift_path(-12).then(|| {
                                        clickable(
                                            el("div", text("8ve -")).attr("class", "draw-tool"),
                                            |ui: &mut UiState, _| ui.stage.shift_path(-12),
                                        )
                                    }),
                                    state.can_shift_path(12).then(|| {
                                        clickable(
                                            el("div", text("8ve +")).attr("class", "draw-tool"),
                                            |ui: &mut UiState, _| ui.stage.shift_path(12),
                                        )
                                    }),
                                    clickable(
                                        el("div", text("Clear")).attr("class", "draw-tool"),
                                        |ui: &mut UiState, _| ui.stage.clear_path(),
                                    ),
                                    // Save closes the loop: the drawn path
                                    // becomes a card you can rehearse.
                                    clickable(
                                        el("div", text("Save")).attr("class", "draw-tool save"),
                                        |ui: &mut UiState, _| ui.save_drawn_path(),
                                    ),
                                ),
                            )
                            .attr("class", "draw-tools")
                        }),
                        el(
                            "div",
                            text(format!(
                                "{} — {} positions",
                                state.material_name(),
                                dot_list.len()
                            )),
                        )
                        .attr("class", "scale-name"),
                        (!ui.pinned_markers.is_empty()).then(|| {
                            clickable(
                                el(
                                    "div",
                                    text(format!("Close {} cards", ui.pinned_markers.len())),
                                )
                                .attr("class", "clear-pins"),
                                |ui: &mut UiState, _| ui.clear_pins(),
                            )
                        }),
                    ),
                )
                .attr("class", "board-caption"),
            ),
        )
        .attr("class", "board"),
    )
}

fn stage_screen(ui: &UiState) -> UiChild {
    if ui.stage_page == StagePage::Templates {
        return Box::new(el(
            "div",
            (header(ui), lens_strip(ui), templates::screen(ui), set_tray::view(ui)),
        ));
    }
    // The wide three-column arm is viewport-bounded: the screen is a column
    // filling the window, the body row takes the remaining height, and each
    // column (catalog / board / related) scrolls independently inside it — so a
    // long scale list or a wide neck scrolls in place instead of stretching the
    // page or shoving the Related panel off the window edge. The stacked
    // Medium/Narrow layouts keep their flowing page scroll.
    let wide3 = matches!(
        (ui.board_layout(), ui.viewport),
        (BoardLayout::TwoPane, ViewportClass::Wide)
    );
    let body: UiChild = match (ui.board_layout(), ui.viewport) {
        (BoardLayout::TwoPane, ViewportClass::Wide) => Box::new(
            el("div", (sidebar(ui), board(ui), related::panel(ui)))
                .attr("class", "body stage-body"),
        ),
        (BoardLayout::FullCanvas, _) => board(ui),
        // Preserve the catalog on smaller surfaces without narrowing the neck.
        _ => Box::new(el(
            "div",
            (
                board(ui),
                related::panel(ui),
                el("div", (sidebar(ui),)).attr("class", "side-strip"),
            ),
        )),
    };
    let screen = el(
        "div",
        (header(ui), transport(ui), lens_strip(ui), body, set_tray::view(ui)),
    );
    Box::new(if wide3 {
        screen.attr("class", "stage-screen")
    } else {
        screen
    })
}

fn tab_content(ui: &UiState) -> UiChild {
    match ui.section {
        AppSection::Stage => stage_screen(ui),
        AppSection::Rehearsal => rehearsal::screen(ui),
        AppSection::Looper => looper::screen(ui),
        AppSection::Tools => tools::screen(ui),
        AppSection::Settings => settings::screen(ui),
    }
}

/// The corpus search field + its results dropdown — a small always-on
/// field in the nav row (no toolbar reorganization). Typing recomputes
/// the results; clicking one jumps to its lens/tab.
fn search_view(ui: &UiState) -> UiChild {
    let field = map_state(text_field(&ui.search), |ui: &mut UiState| &mut ui.search);
    let query = ui.search.text().to_string();
    let results = search_corpus(&query, 8);
    let list = (!results.is_empty()).then(|| {
        let items: Vec<UiChild> = results
            .into_iter()
            .map(|r| {
                let hit = r.hit;
                Box::new(clickable(
                    el(
                        "div",
                        (
                            el("span", text(r.label)).attr("class", "search-label"),
                            el("span", text(r.kind)).attr("class", "search-kind"),
                        ),
                    )
                    .attr("class", "search-item"),
                    move |ui: &mut UiState, _| ui.apply_search_hit(hit),
                )) as UiChild
            })
            .collect();
        el("div", items)
            .attr("class", "search-list")
            .attr("style", "position: absolute; top: 100%; right: 0;")
    });
    // The field renders its buffer as element content, so it has no native
    // placeholder; overlay a hint while the query is empty. It is pointer-transparent
    // (see `.search-hint`), so clicking the hint still focuses the field.
    let hint = query
        .is_empty()
        .then(|| el("div", text("Search catalog")).attr("class", "search-hint"));
    Box::new(
        el("div", (field, hint, list))
            .attr("class", "search-wrap")
            .attr("style", "position: relative;"),
    )
}

/// Shared product root. Desktop hosts add CSD chrome and resize affordances;
/// browser hosts supply neither. Boxed so hosts can name the runner view type.
pub fn stage_root(ui: &UiState) -> UiChild {
    // The persona gate stands in front of everything, because everything behind
    // it is unread: the practice session is sealed to a persona nobody has
    // named yet. See `crate::persona`.
    if let Some(pick) = &ui.persona {
        return crate::persona::persona_gate(pick);
    }
    let mut nav: Vec<UiChild> = AppSection::ALL
        .iter()
        .map(|&section| pill(section, section == ui.section))
        .collect();
    nav.push(Box::new(el("div", ()).attr("class", "nav-spacer")));
    nav.extend(crate::persona::unsaved_notice(ui));
    nav.push(search_view(ui));
    Box::new(
        el(
            "div",
            (el("div", nav).attr("class", "pills"), tab_content(ui)),
        )
        .attr(
            "class",
            format!("root {} {}", ui.board_layout().class(), ui.viewport.class()),
        ),
    )
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    fn staged_set() -> UiState {
        let mut ui = UiState::new();
        ui.stage_current(None);
        ui
    }

    #[test]
    fn one_stage_snapshot_drives_compact_and_expanded_views() {
        let mut ui = staged_set();
        ui.stage_current(None);
        let snapshot = set_graph_snapshot(&ui);
        let compact = set_graph_swatch_from_snapshot(&snapshot, &ui, false);
        let expanded = set_graph_swatch_from_snapshot(&snapshot, &ui, true);

        assert_eq!(
            compact
                .graph
                .nodes
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            expanded
                .graph
                .nodes
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            compact
                .relations
                .iter()
                .map(|relation| relation.id.as_str())
                .collect::<Vec<_>>(),
            expanded
                .relations
                .iter()
                .map(|relation| relation.id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(compact
            .graph
            .nodes
            .iter()
            .all(|node| node.id.epoch == snapshot.epoch()));
        assert_ne!(
            (compact.width, compact.height),
            (expanded.width, expanded.height)
        );
    }

    #[test]
    fn stage_relation_activation_and_drag_stay_epoch_and_view_local() {
        let mut ui = staged_set();
        ui.stage_current(None);
        let snapshot = set_graph_snapshot(&ui);
        let instance = snapshot.items()[0].0;
        let card = snapshot.card_of_ref(instance).unwrap();
        let relation = snapshot.relations()[0].0;
        let order = ui.set.cards.iter().map(|card| card.id).collect::<Vec<_>>();

        ui.handle_set_graph_event(
            &snapshot,
            GraphCanvasEvent::RelationActivate(relation.key()),
        );
        assert_eq!(ui.set_graph_relation, Some(relation));

        ui.handle_set_graph_event(
            &snapshot,
            GraphCanvasEvent::Drag(cambium::GraphCanvasNodeDrag {
                id: instance,
                phase: cambium::PointerPhase::Move,
                position: (0.22, 0.78),
            }),
        );
        assert!(
            ui.set_graph_drag_active,
            "captured motion stays on the view-only dispatch path"
        );
        assert_eq!(ui.set_graph_positions.get(&card), Some(&(0.22, 0.78)));
        assert_eq!(
            ui.set.cards.iter().map(|card| card.id).collect::<Vec<_>>(),
            order,
            "view-local motion cannot reorder Set truth"
        );
        let swatch = set_graph_swatch_from_snapshot(&snapshot, &ui, true);
        assert_eq!(
            swatch
                .graph
                .nodes
                .iter()
                .find(|node| node.id == instance)
                .unwrap()
                .position,
            (0.22, 0.78)
        );

        ui.handle_set_graph_event(
            &snapshot,
            GraphCanvasEvent::Drag(cambium::GraphCanvasNodeDrag {
                id: instance,
                phase: cambium::PointerPhase::Up,
                position: (0.22, 0.78),
            }),
        );
        assert!(
            !ui.set_graph_drag_active,
            "release restores the ordinary backend and persistence tail"
        );
    }

    #[test]
    fn relation_inventory_withholds_edges_without_editing_scene_truth() {
        let mut ui = UiState::new();
        ui.stage.set_lens(Lens::Chords);
        let major = ui
            .stage
            .chords()
            .iter()
            .position(|chord| chord.name == "Major")
            .unwrap();
        ui.stage.select_chord(major);
        ui.stage_current(None);
        let major_seven = ui
            .stage
            .chords()
            .iter()
            .position(|chord| chord.name == "Major 7")
            .unwrap();
        ui.stage.select_chord(major_seven);
        ui.stage_current(None);

        let snapshot = set_graph_snapshot(&ui);
        let choices = set_graph_relation_choices(&snapshot, &ui);
        assert!(choices.len() > 2, "sequence plus catalog relations");
        assert!(choices.iter().all(|choice| choice.visible));
        assert!(choices.iter().all(|choice| {
            !choice.pair.is_empty()
                && !choice.relation.is_empty()
                && !choice.explanation.is_empty()
                && !choice.authority.is_empty()
        }));

        let hidden = choices[1].clone();
        ui.set_graph_relation = Some(hidden.reference);
        ui.toggle_set_graph_relation(&snapshot, hidden.key.clone());
        assert_eq!(
            snapshot.relations().len(),
            choices.len(),
            "visibility never removes source relations"
        );
        assert_eq!(ui.set_graph_relation, None, "a hidden hit target is deselected");
        assert_eq!(
            set_graph_relation_choices(&snapshot, &ui)
                .iter()
                .filter(|choice| choice.visible)
                .count(),
            choices.len() - 1
        );
        let swatch = set_graph_swatch_from_snapshot(&snapshot, &ui, true);
        assert_eq!(
            swatch
                .relations
                .iter()
                .filter(|relation| relation.visible)
                .count(),
            choices.len() - 1
        );

        ui.show_all_set_graph_relations();
        assert!(
            set_graph_relation_choices(&snapshot, &ui)
                .iter()
                .all(|choice| choice.visible)
        );
        ui.hide_all_set_graph_relations(&snapshot);
        assert!(
            set_graph_relation_choices(&snapshot, &ui)
                .iter()
                .all(|choice| !choice.visible)
        );
    }

    #[test]
    fn stage_scene_parallel_relations_reach_cambium_as_distinct_routes() {
        let mut ui = UiState::new();
        ui.stage.set_lens(Lens::Chords);
        let major = ui
            .stage
            .chords()
            .iter()
            .position(|chord| chord.name == "Major")
            .unwrap();
        ui.stage.select_chord(major);
        ui.stage_current(None);
        let major_seven = ui
            .stage
            .chords()
            .iter()
            .position(|chord| chord.name == "Major 7")
            .unwrap();
        ui.stage.select_chord(major_seven);
        ui.stage_current(None);

        let snapshot = set_graph_snapshot(&ui);
        let swatch = set_graph_swatch_from_snapshot(&snapshot, &ui, true);
        let routes = swatch.projected_relations();
        assert!(routes.len() > 2, "sequence plus several catalog reasons");
        let middles = routes
            .iter()
            .filter_map(|(_, route)| route.get(1).copied())
            .map(|(x, y)| (x.to_bits(), y.to_bits()))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            middles.len(),
            routes.len(),
            "Cambium fans every relation cell onto its own visible route"
        );
    }

    #[test]
    fn related_swatch_discloses_a_bounded_spanning_neighborhood() {
        let mut ui = UiState::new();
        ui.stage.set_lens(Lens::Chords);
        let snapshot = related_mere_snapshot(&ui);
        let compact = related_swatch_from_snapshot(&snapshot, &ui, false);
        let expanded = related_swatch_from_snapshot(&snapshot, &ui, true);

        assert!(snapshot.nodes.len() > compact.graph.nodes.len());
        assert!(compact.graph.nodes.len() <= 7);
        assert!(compact.relations.len() <= compact.graph.nodes.len().saturating_sub(1));
        assert!(expanded.graph.nodes.len() <= 12);
        assert!(expanded.graph.nodes.len() >= compact.graph.nodes.len());
        for swatch in [&compact, &expanded] {
            let ids = swatch
                .graph
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<BTreeSet<_>>();
            assert!(swatch
                .relations
                .iter()
                .all(|relation| ids.contains(relation.from.as_str())
                    && ids.contains(relation.to.as_str())));
        }
    }

    #[test]
    fn a_rehearsed_card_records_the_span_it_was_actually_played_for() {
        let mut ui = staged_set();
        ui.now_ms = Some(1_000);
        ui.record_rehearsal_cursor();
        // Nine seconds of playing, per the host's clock.
        ui.now_ms = Some(10_000);
        ui.complete_rehearsal_cursor();

        let subject = catalog_id_for_card(&ui.set.cards[0]).expect("catalog subject");
        assert_eq!(
            ui.practice_history.total_practiced_ms(&subject),
            9_000,
            "the measurement is the span between becoming active and completing"
        );
        assert_eq!(ui.practice_history.last_seen_ms(&subject), Some(10_000));
    }

    #[test]
    fn a_host_with_no_clock_measures_nothing_rather_than_guessing() {
        let mut ui = staged_set();
        ui.now_ms = None;
        ui.record_rehearsal_cursor();
        ui.complete_rehearsal_cursor();

        let subject = catalog_id_for_card(&ui.set.cards[0]).expect("catalog subject");
        assert_eq!(ui.practice_history.total_practiced_ms(&subject), 0);
        assert!(
            !ui.practice_history.has_times(),
            "undated events must not read as practised at epoch zero"
        );
    }

    #[test]
    fn a_backwards_clock_yields_no_measurement() {
        // A system time change mid-session must not produce a wrapped span.
        let mut ui = staged_set();
        ui.now_ms = Some(10_000);
        ui.record_rehearsal_cursor();
        ui.now_ms = Some(1_000);
        ui.complete_rehearsal_cursor();

        let subject = catalog_id_for_card(&ui.set.cards[0]).expect("catalog subject");
        assert_eq!(ui.practice_history.total_practiced_ms(&subject), 0);
    }

    #[test]
    fn completing_one_card_opens_the_next_cards_span() {
        // The rehearsal advance completes a card and makes the next one active
        // in the same beat, so the second span must start at the completion,
        // not at whenever the run began.
        let mut ui = staged_set();
        ui.now_ms = Some(1_000);
        ui.record_rehearsal_cursor();
        ui.now_ms = Some(5_000);
        ui.complete_rehearsal_cursor();
        assert_eq!(ui.card_started_ms, Some(5_000));
        ui.now_ms = Some(6_500);
        ui.complete_rehearsal_cursor();

        let subject = catalog_id_for_card(&ui.set.cards[0]).expect("catalog subject");
        assert_eq!(
            ui.practice_history.total_practiced_ms(&subject),
            4_000 + 1_500,
            "each span measures its own card, and they do not overlap"
        );
    }
}

/// The audio-request queue's semantics, which a boolean flag per request
/// could not provide.
#[cfg(test)]
mod audio_request_tests {
    use super::*;

    #[test]
    fn repeated_requests_all_survive_in_the_order_they_were_asked() {
        let mut ui = UiState::new();
        ui.request(AudioRequest::PreviewNote(440.0));
        ui.request(AudioRequest::SongRewind);
        ui.request(AudioRequest::PreviewNote(880.0));

        // A flag would have kept one note and lost the other, and would have
        // had no way to say the rewind happened between them.
        assert_eq!(
            ui.audio_requests,
            [
                AudioRequest::PreviewNote(440.0),
                AudioRequest::SongRewind,
                AudioRequest::PreviewNote(880.0),
            ]
        );
    }

    #[test]
    fn draining_leaves_nothing_behind_to_repeat() {
        let mut ui = UiState::new();
        ui.request(AudioRequest::PreviewVoicing);
        let drained: Vec<_> = std::mem::take(&mut ui.audio_requests);
        assert_eq!(drained, [AudioRequest::PreviewVoicing]);
        assert!(
            ui.audio_requests.is_empty(),
            "one drain site owns consumption; a missed clear cannot repeat a command"
        );
    }
}
