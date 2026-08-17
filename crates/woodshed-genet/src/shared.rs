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
