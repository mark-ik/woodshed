//! What woodshed owns that the host does not.
//!
//! The host owns the window, the surface, the retained layout, the paint pass,
//! hit testing, input routing, and the accessibility tree. Everything left is
//! woodshed's, and it lives here: the audio and MIDI seams, the session store,
//! the theme the sheet was generated from, the leaf-rebuild signatures, and the
//! self-drive lane.
//!
//! It is held in an `Rc<RefCell<Shared>>` captured by the host hooks, because
//! the hooks are plain closures and the application state proper (`UiState`)
//! lives inside the host's runner, reachable only through a hook's context.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use personae::roster::Roster;
use woodshed_core::storage::SessionStore;
use woodshed_views::theme::ThemeMode;

use crate::audio::CpalBackend;
use crate::midi::MidiHost;
use crate::scenario::{Observed, ScenarioLane};
use crate::storage::{open_store_as, HostBackend};

/// Presented-frame timings gathered only while a Set-graph drag is active.
/// The scenario receipt reads these, but the counters are also useful when a
/// headed debug run needs to say which phase consumed its frame budget.
#[derive(Default)]
pub struct DragFrameMetrics {
    pending: Option<(std::time::Instant, bool)>,
    pub samples: u64,
    pub total_us: u64,
    pub max_us: u64,
    pub viewport_us: u64,
    pub drive_us: u64,
    pub leaves_us: u64,
    pub root_rebuilds: u64,
    pub host_samples: u64,
    pub host_total_us: u64,
    pub host_max_us: u64,
    pub host_relayout_us: u64,
    pub host_layout_update_us: u64,
    pub host_layout_tick_us: u64,
    pub host_layout_apply_us: u64,
    pub host_layout_rebuild_us: u64,
    pub host_layout_mutations: u64,
    pub host_layout_rebuilds: u64,
    pub host_leaf_boxes_us: u64,
    pub host_leaf_render_us: u64,
    pub host_leaf_repaints: u64,
    pub host_fragments_us: u64,
    pub host_emit_us: u64,
    pub host_raster_us: u64,
    pub host_acquire_us: u64,
    pub host_clear_us: u64,
    pub host_compose_us: u64,
    pub host_present_us: u64,
    pub host_a11y_us: u64,
    pub raster_inner_us: u64,
    pub tile_invalidate_us: u64,
    pub dirty_tile_rebuild_us: u64,
    pub master_compose_us: u64,
    pub vello_render_us: u64,
    pub dirty_tiles: u64,
    pub max_dirty_tiles: u64,
}

impl DragFrameMetrics {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn begin(&mut self, drag_active: bool) {
        self.pending = Some((std::time::Instant::now(), drag_active));
    }

    pub fn note_viewport(&mut self, elapsed: std::time::Duration, rebuilt: bool) {
        if self.pending.is_some_and(|(_, drag)| drag) {
            self.viewport_us = self.viewport_us.saturating_add(micros(elapsed));
            self.root_rebuilds = self.root_rebuilds.saturating_add(u64::from(rebuilt));
        }
    }

    pub fn note_drive(&mut self, elapsed: std::time::Duration, rebuilt: bool) {
        if self.pending.is_some_and(|(_, drag)| drag) {
            self.drive_us = self.drive_us.saturating_add(micros(elapsed));
            self.root_rebuilds = self.root_rebuilds.saturating_add(u64::from(rebuilt));
        }
    }

    pub fn note_leaves(&mut self, elapsed: std::time::Duration) {
        if self.pending.is_some_and(|(_, drag)| drag) {
            self.leaves_us = self.leaves_us.saturating_add(micros(elapsed));
        }
    }

    pub fn finish(&mut self, profile: Option<cambium_genet_winit_host::FrameProfile>) {
        let Some((started, drag_active)) = self.pending.take() else {
            return;
        };
        if !drag_active {
            return;
        }
        let elapsed = micros(started.elapsed());
        self.samples = self.samples.saturating_add(1);
        self.total_us = self.total_us.saturating_add(elapsed);
        self.max_us = self.max_us.max(elapsed);
        let Some(profile) = profile else {
            return;
        };
        self.host_samples = self.host_samples.saturating_add(1);
        self.host_total_us = self.host_total_us.saturating_add(profile.total_us);
        self.host_max_us = self.host_max_us.max(profile.total_us);
        self.host_relayout_us = self.host_relayout_us.saturating_add(profile.relayout_us);
        self.host_layout_update_us = self
            .host_layout_update_us
            .saturating_add(profile.layout_update_us);
        self.host_layout_tick_us = self
            .host_layout_tick_us
            .saturating_add(profile.layout_tick_us);
        self.host_layout_apply_us = self
            .host_layout_apply_us
            .saturating_add(profile.layout_apply_us);
        self.host_layout_rebuild_us = self
            .host_layout_rebuild_us
            .saturating_add(profile.layout_rebuild_us);
        self.host_layout_mutations = self
            .host_layout_mutations
            .saturating_add(profile.layout_mutations);
        self.host_layout_rebuilds = self
            .host_layout_rebuilds
            .saturating_add(u64::from(profile.layout_rebuilt));
        self.host_leaf_boxes_us = self
            .host_leaf_boxes_us
            .saturating_add(profile.leaf_boxes_us);
        self.host_leaf_render_us = self
            .host_leaf_render_us
            .saturating_add(profile.leaf_render_us);
        self.host_leaf_repaints = self
            .host_leaf_repaints
            .saturating_add(profile.leaf_repaints);
        self.host_fragments_us = self
            .host_fragments_us
            .saturating_add(profile.leaf_fragments_us);
        self.host_emit_us = self.host_emit_us.saturating_add(profile.emit_scene_us);
        self.host_raster_us = self.host_raster_us.saturating_add(profile.raster_us);
        self.host_acquire_us = self.host_acquire_us.saturating_add(profile.acquire_us);
        self.host_clear_us = self.host_clear_us.saturating_add(profile.clear_us);
        self.host_compose_us = self.host_compose_us.saturating_add(profile.compose_us);
        self.host_present_us = self.host_present_us.saturating_add(profile.present_us);
        self.host_a11y_us = self.host_a11y_us.saturating_add(profile.a11y_us);
        self.raster_inner_us = self.raster_inner_us.saturating_add(profile.raster_total_us);
        self.tile_invalidate_us = self
            .tile_invalidate_us
            .saturating_add(profile.tile_invalidate_us);
        self.dirty_tile_rebuild_us = self
            .dirty_tile_rebuild_us
            .saturating_add(profile.dirty_tile_rebuild_us);
        self.master_compose_us = self
            .master_compose_us
            .saturating_add(profile.master_compose_us);
        self.vello_render_us = self.vello_render_us.saturating_add(profile.vello_render_us);
        self.dirty_tiles = self.dirty_tiles.saturating_add(profile.dirty_tiles);
        self.max_dirty_tiles = self.max_dirty_tiles.max(profile.dirty_tiles);
    }

    pub fn average_us(&self) -> u64 {
        if self.samples == 0 {
            0
        } else {
            self.total_us / self.samples
        }
    }

    pub fn host_average_us(&self) -> u64 {
        if self.host_samples == 0 {
            0
        } else {
            self.host_total_us / self.host_samples
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "drag frames={} avg={}us max={}us viewport={}us drive={}us leaves={}us root-rebuilds={} host-frames={} host-avg={}us host-max={}us relayout={}us layout-update={}us tick={}us apply={}us layout-rebuild={}us layout-mutations={} layout-rebuilds={} leaf-boxes={}us leaf-render={}us leaf-repaints={} fragments={}us emit={}us raster={}us acquire={}us clear={}us compose={}us present={}us a11y={}us raster-inner={}us invalidate={}us rebuild={}us master={}us vello={}us dirty-tiles={} max-dirty-tiles={}",
            self.samples,
            self.average_us(),
            self.max_us,
            self.viewport_us,
            self.drive_us,
            self.leaves_us,
            self.root_rebuilds,
            self.host_samples,
            self.host_average_us(),
            self.host_max_us,
            self.host_relayout_us,
            self.host_layout_update_us,
            self.host_layout_tick_us,
            self.host_layout_apply_us,
            self.host_layout_rebuild_us,
            self.host_layout_mutations,
            self.host_layout_rebuilds,
            self.host_leaf_boxes_us,
            self.host_leaf_render_us,
            self.host_leaf_repaints,
            self.host_fragments_us,
            self.host_emit_us,
            self.host_raster_us,
            self.host_acquire_us,
            self.host_clear_us,
            self.host_compose_us,
            self.host_present_us,
            self.host_a11y_us,
            self.raster_inner_us,
            self.tile_invalidate_us,
            self.dirty_tile_rebuild_us,
            self.master_compose_us,
            self.vello_render_us,
            self.dirty_tiles,
            self.max_dirty_tiles,
        )
    }
}

fn micros(elapsed: std::time::Duration) -> u64 {
    elapsed.as_micros().min(u64::MAX as u128) as u64
}

/// The application's own state, beside the host's.
pub struct Shared {
    /// The W0.1 audio seam: cpal on desktop, Web Audio on the web host.
    pub backend: Option<CpalBackend>,
    /// The MIDI seam: midir on desktop, Web MIDI on the web host.
    pub midi: MidiHost,
    /// Named slots over a host backend: files on desktop, OPFS on the web host.
    /// Which backend is decided at startup by [`open_store`] — sealed to the
    /// chosen persona when the family vault opens, plain files otherwise.
    ///
    /// `None` only while the persona gate is up. Opening the store is what
    /// derives the sealing key, so it cannot happen before the persona is
    /// settled: doing it early would mint a `default` persona beside the ones
    /// the user has and seal this session to a stranger. Nothing reads or
    /// writes practice through this while it is `None`, which is the point.
    pub storage: Option<SessionStore<HostBackend>>,
    /// What is protecting the store, for Settings to report. Seeded onto the
    /// first `UiState` beside the gate, then kept current by a switch.
    pub seal: Option<woodshed_views::persona::PracticeSeal>,
    /// The roster the startup gate asks about, handed to the first `UiState`
    /// and taken from here. `None` on every machine the convention decides for.
    pub pending_roster: Option<Roster>,

    /// Theme the current sheet was generated from; a change re-skins.
    pub theme: ThemeMode,
    /// The accessibility skin the current sheet was generated with; a change
    /// re-skins alongside the theme.
    pub reduce_motion: bool,
    pub text_scale: String,

    /// Last arpeggio auto-advance instant (the step clock while the arpeggio
    /// transport runs).
    pub last_arp_step: Option<std::time::Instant>,
    /// Last rehearsal dwell-advance instant.
    pub last_rehearsal_step: Option<std::time::Instant>,
    /// The song last pushed through the backend seam (push on change).
    pub last_song: woodshed_core::song::SongDoc,

    /// Rebuild signatures for the custom-paint leaves: a leaf is re-rendered
    /// only when the model behind it actually moved.
    pub neighborhood_sig: u64,
    pub set_graph_sig: u64,
    pub fretboard_sig: u64,
    pub rehearsal_fretboard_sig: u64,

    /// The self-drive lane (`WOODSHED_SCENARIO`); `None` for an ordinary run.
    pub scenario: Option<ScenarioLane>,
    /// Where a scenario's captures and sentinel go (`WOODSHED_CAPTURE_DIR`).
    pub capture_dir: Option<PathBuf>,
    /// Semantic transitions since the driver last drained them.
    pub events: Vec<String>,
    /// The last sampled observation, for diffing into `events`.
    pub observed: Observed,
    /// Dispatch-tail diagnostics used by the headed interaction receipts.
    /// View-only graph motion must outnumber the one full sync at release.
    pub view_only_dispatches: u64,
    pub full_dispatch_syncs: u64,
    /// Per-presented-frame drag timings and the live origin used by the
    /// frame-spaced scenario pointer verbs.
    pub drag_frame_metrics: DragFrameMetrics,
    pub scenario_drag_origin: Option<(f32, f32)>,
}

impl Shared {
    /// Everything that can be built before the window exists.
    pub fn boot() -> Rc<RefCell<Self>> {
        // Asked before the store opens, not after: see `crate::persona`.
        let pending_roster = crate::persona::pending_roster();
        // Opened here only when nobody needs asking; behind a gate the store
        // (and so the seal) arrives later, from `persona::settle`.
        let opened = pending_roster.is_none().then(|| open_store_as(None));
        let (storage, seal) = match opened {
            Some((storage, seal)) => (Some(storage), Some(seal)),
            None => (None, None),
        };
        Rc::new(RefCell::new(Self {
            backend: None,
            midi: MidiHost::new(),
            storage,
            seal,
            pending_roster,
            theme: ThemeMode::default(),
            reduce_motion: false,
            text_scale: "Normal".into(),
            last_arp_step: None,
            last_rehearsal_step: None,
            last_song: woodshed_core::song::SongDoc::default(),
            neighborhood_sig: 0,
            set_graph_sig: 0,
            fretboard_sig: 0,
            rehearsal_fretboard_sig: 0,
            scenario: ScenarioLane::from_env(),
            capture_dir: ScenarioLane::capture_dir_from_env(),
            events: Vec::new(),
            observed: Observed::default(),
            view_only_dispatches: 0,
            full_dispatch_syncs: 0,
            drag_frame_metrics: DragFrameMetrics::default(),
            scenario_drag_origin: None,
        }))
    }

    /// The current theme's sheet with the accessibility preferences applied
    /// (reduced motion, text scale). Regenerated on a theme or preference
    /// change; the host relayouts under whatever this returns.
    pub fn accessible_sheet(&self) -> String {
        woodshed_views::theme::apply_accessibility(
            self.theme.css(),
            self.reduce_motion,
            woodshed_views::theme::text_scale_factor(&self.text_scale),
        )
    }

    /// Adopt the skin settings a frame observed. Returns the new sheet when one
    /// of them changed, for the caller to hand the host.
    pub fn reskin_if_changed(
        &mut self,
        theme: ThemeMode,
        reduce_motion: bool,
        text_scale: String,
    ) -> Option<String> {
        if theme == self.theme
            && reduce_motion == self.reduce_motion
            && text_scale == self.text_scale
        {
            return None;
        }
        self.theme = theme;
        self.reduce_motion = reduce_motion;
        self.text_scale = text_scale;
        Some(self.accessible_sheet())
    }
}
