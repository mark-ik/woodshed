//! The Stage screen over live state (S2).
//!
//! Header dropdowns (tuning, root, using Cambium `select`), the lens strip
//! (Scale / Chord / Arpeggio / Progression / Exercise), a per-lens catalog
//! sidebar, and the fretboard rendered as DOM dots. The runner state is
//! [`UiState`]: the portable `woodshed_core::StageState` plus the
//! view-layer dropdown state; hosts call [`UiState::sync`] after any
//! dispatch so dropdown picks land in the core state.

use std::collections::HashMap;

use woodshed_core::audio::{CalibrationStatus, TransportState, TunerState};
use woodshed_core::history::{catalog_id_for_card, EngagementKind, PracticeHistory};
use woodshed_core::search::{search_corpus, SearchHit};
use woodshed_core::settings::{AppSettings, SettingsPage};
use woodshed_core::song::SongDoc;
use woodshed_core::storage::{AppSection, PersistedSession};
use woodshed_core::{set_from_practice, tunings, Lens, RelatedTarget, StageState, ROOT_NAMES};
use woodshedding::rehearsal::{Card, FretWindow, Hold, MarkMode, Set, Touch};
use cambium::{
    clickable, custom_leaf, el, map_state, select, text, text_field, AnyView, GenetCtx,
    GenetElement, GraphCanvasEdge, GraphCanvasNode, GraphCanvasSubgraph, GraphCanvasSwatch,
    SelectState, TextInput,
};

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

/// How many related suggestions the graph swatch and the pane both show, so a
/// node and its row stay in lockstep.
pub const RELATED_LIMIT: usize = 6;

/// The Related graph as a Cambium swatch: the current material at the centre,
/// each suggestion a kind-coloured satellite, star edges. A node's id is the
/// suggestion's [`RelatedTarget`] (`None` = the centre), so a node links 1:1 to
/// its pane row and clicking navigates. Built once and shared by the view (which
/// renders it) and the host (which paints its leaf), the sanctioned pattern.
pub fn related_swatch(ui: &UiState) -> GraphCanvasSwatch<Option<RelatedTarget>, &'static str> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let mut nodes: Vec<GraphCanvasNode<Option<RelatedTarget>, &'static str>> = Vec::new();
    let mut edges: Vec<GraphCanvasEdge<Option<RelatedTarget>>> = Vec::new();
    let has_center = ui.stage.catalog_id().is_some();
    if let Some(id) = ui.stage.catalog_id() {
        let title = id.split_once(':').map(|(_, t)| t).unwrap_or(id.as_str()).to_string();
        nodes.push(GraphCanvasNode {
            id: None,
            kind: ui.stage.lens.label(),
            position: (0.5, 0.5),
            label: title,
            // Stable selector key (catalog id) for a driver/test, distinct from
            // the displayed label.
            key: Some(id),
        });
    }
    let suggestions =
        ui.stage
            .related_material_configured(&ui.practice_history, &ui.app_settings.stage.related, RELATED_LIMIT);
    let n = suggestions.len();
    for (i, s) in suggestions.into_iter().enumerate() {
        let angle = i as f32 / n.max(1) as f32 * TAU - FRAC_PI_2;
        let key = s.title.clone();
        nodes.push(GraphCanvasNode {
            id: Some(s.target),
            kind: s.kind,
            position: (0.5 + angle.cos() * 0.40, 0.5 + angle.sin() * 0.40),
            label: s.title,
            key: Some(key),
        });
        if has_center {
            edges.push(GraphCanvasEdge {
                from: None,
                to: Some(s.target),
            });
        }
    }
    let (w, h) = if ui.related_expanded {
        (300, 210)
    } else {
        (232, 120)
    };
    let mut swatch = GraphCanvasSwatch::new(NEIGHBORHOOD_LEAF_KEY, GraphCanvasSubgraph { nodes, edges })
        .with_size(w, h)
        .with_label("What might I stage next");
    swatch.hovered = ui.related_hover.map(Some);
    swatch
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
    /// One-shot rewind request the host consumes after dispatch.
    pub song_rewind_requested: bool,
    pub section: AppSection,
    pub stage_page: StagePage,
    pub tool_page: ToolPage,
    /// Host-observed width band. This is transient rather than a user setting.
    pub viewport: ViewportClass,
    /// Window-chrome requests the host consumes after dispatch (CSD).
    pub chrome_minimize: bool,
    pub chrome_maximize: bool,
    pub chrome_close: bool,
    pub chrome_drag: bool,
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
    /// One-shot "♪ Hear" request the host consumes after dispatch: voice
    /// the current lens (or rehearsal card) through the audio backend.
    pub preview_requested: bool,
    /// One-shot single-note play request (frequency, Hz) from a marker card's
    /// play button; the host consumes it via the backend's `preview_note`.
    pub preview_note_requested: Option<f32>,
    /// MIDI panel state (Settings tab).
    pub midi: MidiUiState,
    /// Latency-calibration state (Settings tab). `calib_active` drives
    /// host polling; the request flags are consumed after dispatch.
    pub calib_status: CalibrationStatus,
    pub calib_active: bool,
    pub calib_start_requested: bool,
    pub calib_cancel_requested: bool,
    pub calib_accept_requested: bool,
    /// Accepted round-trip latency (ms), host-reflected.
    pub latency_ms: Option<f32>,
    /// Looper (song-mode record) state. Recording status + per-bar loop
    /// flags are host-reflected; the toggle/clear requests are consumed
    /// after dispatch; `song_record_replace` is a view-owned toggle.
    pub song_recording: bool,
    pub song_loop_bars: Vec<bool>,
    pub song_record_replace: bool,
    pub song_record_toggle_requested: bool,
    pub song_clear_loop_requested: bool,
    /// Whether the document-bottom Set tray shows its Cards and editor.
    pub set_tray_expanded: bool,
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
            song_rewind_requested: false,
            section: AppSection::Stage,
            stage_page: StagePage::default(),
            tool_page: ToolPage::default(),
            viewport: ViewportClass::default(),
            chrome_minimize: false,
            chrome_maximize: false,
            chrome_close: false,
            chrome_drag: false,
            transport: TransportState::default(),
            tuner: TunerState::default(),
            audio_error: None,
            tuning_dd: SelectState::new(stage.tuning_idx),
            root_dd: SelectState::new(stage.root_idx),
            search: TextInput::new(""),
            card_rename: TextInput::new(""),
            card_rename_for: None,
            preview_requested: false,
            preview_note_requested: None,
            midi: MidiUiState::new(),
            calib_status: CalibrationStatus::Idle,
            calib_active: false,
            calib_start_requested: false,
            calib_cancel_requested: false,
            calib_accept_requested: false,
            latency_ms: None,
            song_recording: false,
            song_loop_bars: Vec::new(),
            song_record_replace: false,
            song_record_toggle_requested: false,
            song_clear_loop_requested: false,
            set_tray_expanded: true,
            pinned_markers: Vec::new(),
            hover_peek: None,
            related_hover: None,
            related_expanded: false,
            stage,
        }
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

    /// Pitches + shape for the on-demand "♪ Hear" preview, resolved from
    /// context: the current rehearsal card on the Rehearsal tab, else the
    /// active Stage lens. Empty pitches = nothing to voice. The host
    /// consumes [`Self::preview_requested`] and calls this.
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
                .record(subject_id, EngagementKind::Previewed, None);
        }
        self.preview_requested = true;
    }

    pub fn stage_current(&mut self, from_id: Option<String>) {
        let subject_id = self.stage.catalog_id();
        if let Some(card) = self.stage.card_from_lens() {
            self.set.push(card);
            if let Some(subject_id) = subject_id {
                self.practice_history
                    .record(subject_id, EngagementKind::Staged, from_id);
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
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        if let Some(id) = catalog_id_for_card(&self.set.cards[cursor]) {
            self.practice_history
                .record(id, EngagementKind::Rehearsed, None);
        }
    }

    pub fn complete_rehearsal_cursor(&mut self) {
        if self.set.cards.is_empty() {
            return;
        }
        let cursor = self.set.cursor.min(self.set.cards.len() - 1);
        if let Some(id) = catalog_id_for_card(&self.set.cards[cursor]) {
            self.practice_history
                .record(id, EngagementKind::Completed, None);
        }
    }

    /// Snapshot the persistable subset (the W0.2 seam's payload).
    pub fn to_persisted(&self) -> PersistedSession {
        PersistedSession::capture(
            &self.stage,
            self.section,
            &self.app_settings,
            &self.set,
            &self.song,
            &self.practice_history,
        )
    }

    /// Restore a persisted session (indices clamp; unknown theme names
    /// fall back to the default).
    pub fn apply_persisted(&mut self, session: &PersistedSession) {
        session.restore(&mut self.stage);
        self.set = session.set.clone();
        self.song = session.song.clone();
        self.practice_history = session.practice_history.clone();
        self.app_settings = session.settings.clone();
        self.section = session.section;
        self.transport.bpm = session.settings.metronome.bpm.clamp(30.0, 300.0);
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

/// The class for one fret cell. Fret 0 is the open-string column and carries the
/// nut. Inlay position markers are shown by the fret-number ruler (bolded at the
/// marker frets); proper on-board inlays wait for the board-layer rendering pass.
fn fret_cell_class(fret: u8) -> &'static str {
    if fret == 0 {
        "fret nut-gap"
    } else {
        "fret"
    }
}

/// The fret-number ruler under a board. One cell per fret column, sharing the
/// `.fret` grid metrics so each number sits under its own fret.
fn fret_number_row(fret_count: u8) -> UiChild {
    let cells: Vec<UiChild> = (0..=fret_count)
        .map(|fret| {
            let class = if matches!(fret, 3 | 5 | 7 | 9 | 12 | 15 | 17 | 19 | 21 | 24) {
                "fret-num fret-num-marker"
            } else {
                "fret-num"
            };
            Box::new(el("div", text(fret.to_string())).attr("class", class)) as UiChild
        })
        .collect();
    Box::new(el("div", cells).attr("class", "fret-nums")) as UiChild
}

/// A board's string rows with the fret-number ruler appended beneath them. Every
/// board view builds its rows the same way, so they all get a numbered board.
fn fretboard(mut rows: Vec<UiChild>, fret_count: u8) -> Vec<UiChild> {
    rows.push(fret_number_row(fret_count));
    rows
}

/// Wrap a string's fret cells in its row, tagging the row with a thickness tier
/// (`string-1` thick .. `string-6` thin). Lower strings (smaller index, since
/// tunings read low to high) render thicker. The thickness itself is CSS, so the
/// tier just selects it.
fn string_row(cells: Vec<UiChild>, string_index: usize, string_count: usize) -> UiChild {
    let tier = if string_count > 1 {
        // 0.0 at the lowest string, 1.0 at the highest, mapped onto tiers 1..=6.
        let t = string_index as f32 / (string_count - 1) as f32;
        1 + (t * 5.0).round() as u32
    } else {
        3
    };
    Box::new(el("div", cells).attr("class", format!("string string-{tier}"))) as UiChild
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
    let dots: HashMap<(usize, u8), (usize, String)> = board
        .dots
        .iter()
        .map(|d| ((d.string_index, d.fret), (d.recency, d.label.clone())))
        .collect();
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = fret_cell_class(fret);
                    match dots.get(&(string_index, fret)) {
                        Some((recency, label)) => {
                            let dot_class = if *recency == 0 {
                                "dot step-dot"
                            } else {
                                "dot trail-dot"
                            };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            string_row(cells, string_index, state.string_count())
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                deck,
                el("div", fretboard(rows, ui.stage.fret_count)),
                el(
                    "div",
                    text(format!(
                        "{} — step {}/{} · {}",
                        board.name,
                        board.step + 1,
                        board.total,
                        board.description,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
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
    let dots: HashMap<(usize, u8), (bool, String)> = board
        .dots
        .iter()
        .map(|d| ((d.string_index, d.fret), (d.is_root, d.label.clone())))
        .collect();
    let state = &ui.stage;
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = fret_cell_class(fret);
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, label)) => {
                            let dot_class = if *is_root { "dot root-dot" } else { "dot" };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            string_row(cells, string_index, state.string_count())
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", cards).attr("class", "prog-cards"),
                el("div", fretboard(rows, ui.stage.fret_count)),
                el(
                    "div",
                    text(format!(
                        "{} — {} · showing {}",
                        state.material_name(),
                        board.description,
                        board.expanded_label,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
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
    let dots: HashMap<(usize, u8), (bool, bool, String)> = board
        .dots
        .iter()
        .map(|d| {
            (
                (d.string_index, d.fret),
                (d.is_root, d.is_current, d.label.clone()),
            )
        })
        .collect();
    let rows: Vec<UiChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<UiChild> = (0..=state.fret_count)
                .map(|fret| {
                    let cell_class = fret_cell_class(fret);
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, is_current, label)) => {
                            let dot_class = if *is_current {
                                "dot step-dot"
                            } else if *is_root {
                                "dot root-dot"
                            } else {
                                "dot"
                            };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as UiChild
                        }
                        None => Box::new(el("div", ()).attr("class", cell_class)) as UiChild,
                    }
                })
                .collect();
            string_row(cells, string_index, state.string_count())
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                deck,
                el("div", fretboard(rows, ui.stage.fret_count)),
                el(
                    "div",
                    text(format!(
                        "{} — frets {}-{}, step {}/{}",
                        state.material_name(),
                        board.start_fret,
                        board.start_fret + woodshed_core::arpeggio::ARP_SHAPE_SPAN,
                        board.step + 1,
                        board.walk_len,
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

const CARD_W: f32 = 132.0;
/// Approximate card height, for edge-flip placement (title, two rows, play).
const CARD_H: f32 = 112.0;

/// A pinned marker's detail card, placed just above or below its marker (top-half
/// markers get the card below, bottom-half above, to keep it inside the board).
/// Shows note + octave, scale degree + interval, and the string/fret.
fn note_card(d: &woodshed_core::FretDot, geom: &BoardGeom, string_count: usize) -> UiChild {
    let (cx, cy) = geom.note_pos(d.string_index, d.fret);
    let (_mw, mh) = geom.marker_size();
    let (_bw, bh) = geom.size();
    // Place the card below markers in the upper half of the board, above those
    // in the lower half, so it stays on-screen — by screen position, so it holds
    // for either orientation.
    let below = cy < bh / 2.0;
    let top = if below {
        cy + mh / 2.0 + 6.0
    } else {
        cy - mh / 2.0 - 6.0 - CARD_H
    };
    let left = (cx - CARD_W / 2.0).max(2.0);
    let title = format!("{}{}", d.label, d.octave);
    let sub = if d.degree.is_empty() {
        d.interval_name.clone()
    } else {
        format!("{} · {}", d.degree, d.interval_name)
    };
    // Guitar-style numbering: string 1 is the highest (largest index).
    let pos = format!("string {} · fret {}", string_count - d.string_index, d.fret);
    let freq = d.frequency;
    Box::new(
        el(
            "div",
            (
                el("div", text(title)).attr("class", "note-card-title"),
                el("div", text(sub)).attr("class", "note-card-row"),
                el("div", text(pos)).attr("class", "note-card-row"),
                clickable(
                    el("div", text("♪ Play")).attr("class", "note-card-play"),
                    move |ui: &mut UiState, _| ui.preview_note_requested = Some(freq),
                ),
            ),
        )
        .attr("class", "note-card")
        .attr("style", format!("left:{left:.1}px; top:{top:.1}px; width:{CARD_W:.1}px")),
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
    let cards: Vec<UiChild> = ui
        .pinned_markers
        .iter()
        .filter_map(|&(si, fret)| {
            dot_list
                .iter()
                .find(|d| d.string_index == si && d.fret == fret)
                .map(|d| note_card(d, &geom, string_count))
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        custom_leaf::<UiState, ()>(FRETBOARD_LEAF_KEY, w, h),
                        el("div", labels).attr("class", "label-layer"),
                        el("div", cards).attr("class", "card-layer"),
                    ),
                )
                .attr("class", "fretboard-stack")
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
                .attr("style", format!("width:{w}px; height:{h}px")),
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
    let body: UiChild = match (ui.board_layout(), ui.viewport) {
        (BoardLayout::TwoPane, ViewportClass::Wide) => {
            Box::new(el("div", (sidebar(ui), board(ui), related::panel(ui))).attr("class", "body"))
        }
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
    Box::new(el(
        "div",
        (header(ui), transport(ui), lens_strip(ui), body, set_tray::view(ui)),
    ))
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
    let mut nav: Vec<UiChild> = AppSection::ALL
        .iter()
        .map(|&section| pill(section, section == ui.section))
        .collect();
    nav.push(Box::new(el("div", ()).attr("class", "nav-spacer")));
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
