//! The storage seam (genet-host plan W0.2) and the persisted session.
//!
//! The core describes what survives a restart as one serde struct; a host
//! supplies a [`Storage`] that moves the serialized form — filesystem on
//! desktop, OPFS in the browser. Every field is `#[serde(default)]`-safe
//! so old files keep loading as the struct grows.

use muniment::Backend;
use serde::{Deserialize, Serialize};

use crate::arpeggio::ArpeggioDirection;
use crate::settings::AppSettings;
pub use crate::settings::RelatedSettings;
use crate::{Lens, StageState};

/// The two things woodshed keeps between runs, as named slots on a
/// [`muniment::Backend`].
///
/// This used to be two traits, `Storage` and `SettingsStorage`, each a single
/// hardcoded slot moving a `String`. That was a restatement of what muniment
/// already provides, and its lack of a key space is why a second thing to store
/// needed a second trait rather than a second name. A third needs neither now.
///
/// The host supplies the backend, which is where the platform split lives:
/// filesystem on desktop, OPFS or IndexedDB in the browser, redb or zip where a
/// real store is wanted. Sealing is [`crate::sealed_backend::SealedBackend`],
/// composed in by the host rather than known about here.
pub struct SessionStore<B> {
    backend: B,
}

/// The session slot: artifact state, what the practitioner was doing.
const SESSION_SLOT: &str = "session";
/// The settings slot: install and persona-facing preferences. Separate from the
/// session because they answer different questions and are written at different
/// times, which is the distinction the two old traits were really drawing.
const SETTINGS_SLOT: &str = "settings";

impl<B: Backend> SessionStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// The backend, for a host that also drives it directly.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn load(&self) -> Option<String> {
        self.read(SESSION_SLOT)
    }

    pub fn save(&self, contents: &str) {
        self.write(SESSION_SLOT, contents);
    }

    pub fn load_settings(&self) -> Option<String> {
        self.read(SETTINGS_SLOT)
    }

    pub fn save_settings(&self, contents: &str) {
        self.write(SETTINGS_SLOT, contents);
    }

    /// Read one slot, treating every failure as absence.
    ///
    /// A backend error, non-UTF-8 bytes, and a seal that will not open all mean
    /// the same thing to the app: there is no previous session, start fresh. The
    /// alternative is stranding practice behind an error the user cannot act on.
    fn read(&self, slot: &str) -> Option<String> {
        let bytes = pollster::block_on(self.backend.get(slot)).ok()??;
        String::from_utf8(bytes).ok()
    }

    /// Write one slot, logging rather than surfacing failure, because a broken
    /// disk must not strand practice.
    fn write(&self, slot: &str, contents: &str) {
        if let Err(error) = pollster::block_on(self.backend.put(slot, contents.as_bytes())) {
            eprintln!("[woodshed] could not persist {slot}: {error}");
        }
    }
}

/// The top-level product section. Legacy Practice and Song values migrate at
/// deserialization into Stage and Looper respectively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppSection {
    #[default]
    #[serde(alias = "Practice")]
    Stage,
    Rehearsal,
    #[serde(alias = "Song")]
    Looper,
    Tools,
    Settings,
}

impl AppSection {
    pub const ALL: [AppSection; 5] = [
        AppSection::Stage,
        AppSection::Rehearsal,
        AppSection::Looper,
        AppSection::Tools,
        AppSection::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AppSection::Stage => "Stage",
            AppSection::Rehearsal => "Rehearsal",
            AppSection::Looper => "Looper",
            AppSection::Tools => "Tools",
            AppSection::Settings => "Settings",
        }
    }
}

/// The artifact/session state that survives a restart. Application preferences
/// are stored through [`SettingsStorage`] instead of being flattened here.
/// `decode_session` still exposes legacy flattened settings for migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedSession {
    #[serde(alias = "tab")]
    pub section: AppSection,
    pub lens: Lens,
    pub root_idx: usize,
    pub scale_idx: usize,
    pub chord_idx: usize,
    pub arpeggio_idx: usize,
    pub arpeggio_position_idx: usize,
    pub arpeggio_direction: ArpeggioDirection,
    pub arpeggio_inversion: u8,
    pub progression_idx: Option<usize>,
    pub progression_expanded: usize,
    pub exercise_idx: usize,
    pub exercise_starting_fret: u8,
    /// The rehearsal set (cards + cursor + loop mode).
    pub set: woodshedding::rehearsal::Set,
    /// The song lane: bars + song-level flags.
    pub song: crate::song::SongDoc,
    /// Typed catalog engagement used by Related ranking and future history
    /// views. Defaults empty for sessions written before the field existed.
    pub practice_history: crate::history::PracticeHistory,
    /// Desktop-host workspace presentation policy, encoded by Woodshed's view
    /// layer. The portable core retains these opaque bytes in the existing
    /// session slot without learning the shared Workbench schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_json: Option<String>,
}

impl Default for PersistedSession {
    fn default() -> Self {
        Self::capture(
            &StageState::new(),
            AppSection::Stage,
            &woodshedding::rehearsal::Set::default(),
            &crate::song::SongDoc::default(),
            &crate::history::PracticeHistory::default(),
        )
    }
}

impl PersistedSession {
    /// Snapshot the persistable subset of the app state.
    pub fn capture(
        stage: &StageState,
        section: AppSection,
        set: &woodshedding::rehearsal::Set,
        song: &crate::song::SongDoc,
        practice_history: &crate::history::PracticeHistory,
    ) -> Self {
        Self {
            set: set.clone(),
            song: song.clone(),
            practice_history: practice_history.clone(),
            workspace_json: None,
            section,
            lens: stage.lens,
            root_idx: stage.root_idx,
            scale_idx: stage.scale_idx,
            chord_idx: stage.chord_idx,
            arpeggio_idx: stage.arpeggio_idx,
            arpeggio_position_idx: stage.arpeggio_position_idx,
            arpeggio_direction: stage.arpeggio_direction,
            arpeggio_inversion: stage.arpeggio_inversion,
            progression_idx: stage.progression_idx,
            progression_expanded: stage.progression_expanded,
            exercise_idx: stage.exercise_idx,
            exercise_starting_fret: stage.exercise_starting_fret,
        }
    }

    /// Restore the persisted subset onto a fresh state. Indices route
    /// through the clamping setters so a session written against a larger
    /// future catalog degrades instead of panicking.
    pub fn restore(&self, stage: &mut StageState, settings: &AppSettings) {
        stage.set_lens(self.lens);
        stage.set_tuning(settings.tuning.tuning_idx);
        stage.set_root(self.root_idx);
        stage.select_scale(self.scale_idx);
        stage.select_chord(self.chord_idx);
        stage.select_arpeggio(self.arpeggio_idx);
        stage.arpeggio_position_idx = self.arpeggio_position_idx;
        stage.arpeggio_direction = self.arpeggio_direction;
        stage.arpeggio_inversion = self.arpeggio_inversion;
        if let Some(i) = self.progression_idx {
            stage.select_progression(i);
            stage.progression_expand(self.progression_expanded);
        }
        stage.select_exercise(self.exercise_idx);
        stage.exercise_starting_fret = self.exercise_starting_fret;
    }
}

/// The result of reading a session file. New files contain only
/// [`PersistedSession`]; old flat files may also return application settings so
/// a host can migrate them to its separate settings file on the next save.
#[derive(Debug)]
pub struct SessionLoad {
    pub session: PersistedSession,
    pub legacy_settings: Option<AppSettings>,
}

/// Decode the current session format and detect the pre-C5 flattened settings
/// payload without making those fields part of the new session type.
pub fn decode_session(contents: &str) -> Result<SessionLoad, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(contents)?;
    let session = serde_json::from_value(value.clone())?;
    let has_legacy_settings = [
        "theme",
        "tuning_idx",
        "bpm",
        "settings_page",
        "reduce_motion",
        "board_layout",
        "show_set_sequence_edges",
    ]
    .iter()
    .any(|key| value.get(key).is_some());
    let legacy_settings = has_legacy_settings
        .then(|| serde_json::from_value(value))
        .transpose()?;
    Ok(SessionLoad {
        session,
        legacy_settings,
    })
}

/// Associated-data + persona-derivation context binding woodshed's sealed session
/// to its purpose. Also the salt the sealing key derives under, so the key is
/// specific to this use.
pub(crate) const WOODSHED_SEAL_CONTEXT: &[u8] = b"woodshed.practice-session.seal.v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trips_through_json() {
        let mut stage = StageState::new();
        stage.set_lens(Lens::Arpeggios);
        stage.set_tuning(3);
        stage.set_root(3);
        stage.select_arpeggio(5);
        stage.arpeggio_inversion = 2;
        stage.select_progression(1);
        let mut set = woodshedding::rehearsal::Set::default();
        set.push(stage.card_from_lens().expect("arpeggio card"));
        let mut song = crate::song::SongDoc::default();
        song.name = "My Song".into();
        song.bars.push(crate::song::SongBar::default());
        song.one_shot = true;
        let mut history = crate::history::PracticeHistory::default();
        history.record(
            Some(1_000),
            woodshed_graph::chord_id("Minor 7"),
            crate::history::EngagementKind::Staged,
            Some(woodshed_graph::scale_id("Dorian")),
            None,
        );
        let related = RelatedSettings {
            use_history: false,
            show_neighborhood: false,
            dismissed_ids: vec![woodshed_graph::chord_id("Minor 7")],
            ..Default::default()
        };
        let settings = AppSettings {
            page: crate::settings::SettingsPage::Tuning,
            appearance: crate::settings::AppearanceSettings {
                theme: "Ember".into(),
            },
            tuning: crate::settings::TuningSettings { tuning_idx: 3 },
            stage: crate::settings::StageSettings {
                related: related.clone(),
                ..Default::default()
            },
            fretboard: crate::settings::FretboardSettings {
                board_layout: "Hero".into(),
                marker_style: "Sharp".into(),
                ..Default::default()
            },
            metronome: crate::settings::MetronomeSettings { bpm: 96.0 },
            ..AppSettings::default()
        };
        let snap = PersistedSession::capture(&stage, AppSection::Settings, &set, &song, &history);
        let json = serde_json::to_string(&snap).unwrap();
        let wire: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            wire.get("theme").is_none(),
            "application settings leave the session wire"
        );
        assert!(wire.get("tuning_idx").is_none());
        let settings_json = serde_json::to_string(&settings).unwrap();
        let settings_wire: serde_json::Value = serde_json::from_str(&settings_json).unwrap();
        assert_eq!(settings_wire["theme"], "Ember");
        assert_eq!(settings_wire["tuning_idx"], 3);
        assert_eq!(settings_wire["bpm"], 96.0);
        let back: PersistedSession = serde_json::from_str(&json).unwrap();
        let mut restored = StageState::new();
        back.restore(&mut restored, &settings);
        assert_eq!(restored.lens, Lens::Arpeggios);
        assert_eq!(restored.tuning_idx, 3);
        assert_eq!(restored.root_idx, 3);
        assert_eq!(restored.arpeggio_idx, 5);
        assert_eq!(restored.arpeggio_inversion, 2);
        assert_eq!(restored.progression_idx, Some(1));
        assert_eq!(back.section, AppSection::Settings);
        assert_eq!(back.set.cards.len(), 1, "the rehearsal set round-trips");
        assert_eq!(back.set.cards[0].label, "Csus4 arpeggio");
        assert_eq!(back.song.name, "My Song", "the song doc round-trips");
        assert_eq!(back.song.bars.len(), 1);
        assert!(back.song.one_shot);
        // The practice lineage round-trips through the session wire: the
        // engagement, its subject, and the traversal that produced it.
        assert_eq!(back.practice_history.len(), history.len());
        assert_eq!(
            back.practice_history.recent(1)[0].subject_id,
            history.recent(1)[0].subject_id
        );
        assert_eq!(settings.stage.related, related);
    }

    #[test]
    fn unknown_fields_and_missing_fields_tolerated() {
        let loaded = decode_session(r#"{"lens":"Chords","bpm":88.0}"#).unwrap();
        assert_eq!(loaded.session.lens, Lens::Chords);
        assert_eq!(loaded.legacy_settings.as_ref().unwrap().metronome.bpm, 88.0);
        assert_eq!(
            loaded.session.section,
            AppSection::Stage,
            "missing fields default"
        );
        // Out-of-range indices clamp through the setters.
        let huge: PersistedSession = serde_json::from_str(r#"{"scale_idx":99999}"#).unwrap();
        let mut s = StageState::new();
        huge.restore(&mut s, &AppSettings::default());
        assert!(s.scale_idx < s.scales().len());
    }

    #[test]
    fn the_legacy_sequence_edge_toggle_becomes_a_relation_set() {
        use woodshedding::rehearsal::SetGraphEdgeKind;

        // A session written while Set-graph edge visibility was one boolean.
        let mut off: AppSettings =
            serde_json::from_str(r#"{"show_set_sequence_edges":false}"#).unwrap();
        assert!(
            off.stage.shows_relation(SetGraphEdgeKind::Next),
            "the default set is unchanged until the legacy value is adopted"
        );

        off.stage.adopt_legacy_relation_visibility();
        assert!(!off.stage.shows_relation(SetGraphEdgeKind::Next));

        // The legacy key is consumed, so the next save writes only the set.
        let json = serde_json::to_string(&off).unwrap();
        assert!(!json.contains("show_set_sequence_edges"));
        assert!(json.contains("visible_set_relations"));

        // A session written after the migration is untouched by it.
        let mut on: AppSettings = serde_json::from_str(&json).unwrap();
        on.stage.adopt_legacy_relation_visibility();
        assert!(!on.stage.shows_relation(SetGraphEdgeKind::Next));
    }

    #[test]
    fn legacy_tabs_migrate_to_current_sections() {
        let practice: PersistedSession = serde_json::from_str(r#"{"tab":"Practice"}"#).unwrap();
        assert_eq!(practice.section, AppSection::Stage);
        let song: PersistedSession = serde_json::from_str(r#"{"tab":"Song"}"#).unwrap();
        assert_eq!(song.section, AppSection::Looper);
    }
}
