//! The storage seam (serval-host plan W0.2) and the persisted session.
//!
//! The core describes what survives a restart as one serde struct; a host
//! supplies a [`Storage`] that moves the serialized form — filesystem on
//! desktop, OPFS in the browser. Every field is `#[serde(default)]`-safe
//! so old files keep loading as the struct grows.

use personae::{IdentityProvider, seal_bytes, unseal_bytes};
use serde::{Deserialize, Serialize};

use crate::arpeggio::ArpeggioDirection;
use crate::{Lens, StageState};

/// The host-supplied persistence realization.
pub trait Storage {
    /// The serialized session from the previous run, if any.
    fn load(&self) -> Option<String>;
    /// Persist the serialized session. Failures should be logged by the
    /// implementation, not surfaced as app errors — a broken disk must
    /// not strand practice.
    fn save(&self, contents: &str);
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

/// User-owned controls for the Related projection. Hidden identities are
/// stable catalog IDs, so a dismissal survives catalog reordering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelatedSettings {
    pub use_history: bool,
    pub show_neighborhood: bool,
    pub dismissed_ids: Vec<String>,
}

impl Default for RelatedSettings {
    fn default() -> Self {
        Self {
            use_history: true,
            show_neighborhood: true,
            dismissed_ids: Vec::new(),
        }
    }
}

/// Everything that survives a restart. Mirrors woodshed-xilem's persisted
/// subset: selections and dials persist, transport cursors and playing
/// flags do not.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedSession {
    #[serde(alias = "tab")]
    pub section: AppSection,
    pub lens: Lens,
    pub tuning_idx: usize,
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
    pub bpm: f32,
    /// Theme name, opaque to the core (the view layer owns the theme
    /// vocabulary).
    pub theme: String,
    /// Fretboard layout name (redesign P4), opaque like `theme`.
    pub board_layout: String,
    /// The rehearsal set (cards + cursor + loop mode).
    pub set: woodshedding::rehearsal::Set,
    /// The song lane: bars + song-level flags.
    pub song: crate::song::SongDoc,
    /// Typed catalog engagement used by Related ranking and future history
    /// views. Defaults empty for sessions written before the field existed.
    pub practice_history: crate::history::PracticeHistory,
    pub related: RelatedSettings,
}

impl Default for PersistedSession {
    fn default() -> Self {
        Self::capture(
            &StageState::new(),
            AppSection::Stage,
            120.0,
            "Slate",
            "Two pane",
            &woodshedding::rehearsal::Set::default(),
            &crate::song::SongDoc::default(),
            &crate::history::PracticeHistory::default(),
            &RelatedSettings::default(),
        )
    }
}

impl PersistedSession {
    /// Snapshot the persistable subset of the app state.
    pub fn capture(
        stage: &StageState,
        section: AppSection,
        bpm: f32,
        theme: &str,
        board_layout: &str,
        set: &woodshedding::rehearsal::Set,
        song: &crate::song::SongDoc,
        practice_history: &crate::history::PracticeHistory,
        related: &RelatedSettings,
    ) -> Self {
        Self {
            board_layout: board_layout.to_string(),
            set: set.clone(),
            song: song.clone(),
            practice_history: practice_history.clone(),
            related: related.clone(),
            section,
            lens: stage.lens,
            tuning_idx: stage.tuning_idx,
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
            bpm,
            theme: theme.to_string(),
        }
    }

    /// Restore the persisted subset onto a fresh state. Indices route
    /// through the clamping setters so a session written against a larger
    /// future catalog degrades instead of panicking.
    pub fn restore(&self, stage: &mut StageState) {
        stage.set_lens(self.lens);
        stage.set_tuning(self.tuning_idx);
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

/// Associated-data + persona-derivation context binding woodshed's sealed session
/// to its purpose. Also the salt the sealing key derives under, so the key is
/// specific to this use.
const WOODSHED_SEAL_CONTEXT: &[u8] = b"woodshed.practice-session.seal.v1";

/// A [`Storage`] that seals the practice session at rest under a persona-derived
/// key — the woodshed pole of the personae suite adoption (one persona, encrypt
/// at rest, carry-ready).
///
/// Wraps an inner host [`Storage`] that moves a `String`. The session is sealed
/// to XChaCha20-Poly1305 bytes and hex-encoded to fit that String seam, so the
/// host's filesystem / OPFS realization is unchanged. Build it with
/// [`SealedStorage::for_provider`] so the key derives from the user's personae
/// identity: any device holding the persona seed re-derives it and reads the
/// state, while anyone without it gets `None` (a fresh session) rather than the
/// plaintext.
pub struct SealedStorage<S: Storage> {
    inner: S,
    key: [u8; 32],
}

impl<S: Storage> SealedStorage<S> {
    /// Seal under an explicit 32-byte key (the primitive; a host that already
    /// holds the key, and the tests, use this).
    pub fn new(inner: S, key: [u8; 32]) -> Self {
        Self { inner, key }
    }

    /// Derive the sealing key from the user's personae identity, so the sealed
    /// session is readable on any device that carries the persona seed.
    pub fn for_provider(
        inner: S,
        provider: &dyn IdentityProvider,
    ) -> Result<Self, personae::IdentityError> {
        let key = provider.derive_keypair(WOODSHED_SEAL_CONTEXT)?.to_seed();
        Ok(Self::new(inner, key))
    }
}

impl<S: Storage> Storage for SealedStorage<S> {
    fn load(&self) -> Option<String> {
        let hexed = self.inner.load()?;
        let sealed = hex::decode(hexed.trim()).ok()?;
        let plain = unseal_bytes(&self.key, WOODSHED_SEAL_CONTEXT, &sealed).ok()?;
        String::from_utf8(plain).ok()
    }

    fn save(&self, contents: &str) {
        // A broken seal must not strand practice: on failure, skip the write so
        // the prior sealed session stays, matching the Storage contract.
        if let Ok(sealed) = seal_bytes(&self.key, WOODSHED_SEAL_CONTEXT, contents.as_bytes()) {
            self.inner.save(&hex::encode(sealed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trips_through_json() {
        let mut stage = StageState::new();
        stage.set_lens(Lens::Arpeggios);
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
            woodshed_graph::chord_id("Minor 7"),
            crate::history::EngagementKind::Staged,
            Some(woodshed_graph::scale_id("Dorian")),
        );
        let related = RelatedSettings {
            use_history: false,
            show_neighborhood: false,
            dismissed_ids: vec![woodshed_graph::chord_id("Minor 7")],
        };
        let snap =
            PersistedSession::capture(
                &stage,
                AppSection::Settings,
                96.0,
                "Ember",
                "Hero",
                &set,
                &song,
                &history,
                &related,
            );
        let json = serde_json::to_string(&snap).unwrap();
        let back: PersistedSession = serde_json::from_str(&json).unwrap();
        let mut restored = StageState::new();
        back.restore(&mut restored);
        assert_eq!(restored.lens, Lens::Arpeggios);
        assert_eq!(restored.root_idx, 3);
        assert_eq!(restored.arpeggio_idx, 5);
        assert_eq!(restored.arpeggio_inversion, 2);
        assert_eq!(restored.progression_idx, Some(1));
        assert_eq!(back.section, AppSection::Settings);
        assert_eq!(back.bpm, 96.0);
        assert_eq!(back.theme, "Ember");
        assert_eq!(back.set.cards.len(), 1, "the rehearsal set round-trips");
        assert_eq!(back.set.cards[0].label, "Csus4 arpeggio");
        assert_eq!(back.song.name, "My Song", "the song doc round-trips");
        assert_eq!(back.song.bars.len(), 1);
        assert!(back.song.one_shot);
        assert_eq!(back.practice_history.events, history.events);
        assert_eq!(back.related, related);
    }

    #[test]
    fn unknown_fields_and_missing_fields_tolerated() {
        let back: PersistedSession =
            serde_json::from_str(r#"{"lens":"Chords","bpm":88.0}"#).unwrap();
        assert_eq!(back.lens, Lens::Chords);
        assert_eq!(back.bpm, 88.0);
        assert_eq!(back.section, AppSection::Stage, "missing fields default");
        // Out-of-range indices clamp through the setters.
        let huge: PersistedSession =
            serde_json::from_str(r#"{"scale_idx":99999}"#).unwrap();
        let mut s = StageState::new();
        huge.restore(&mut s);
        assert!(s.scale_idx < s.scales().len());
    }

    #[test]
    fn legacy_tabs_migrate_to_current_sections() {
        let practice: PersistedSession = serde_json::from_str(r#"{"tab":"Practice"}"#).unwrap();
        assert_eq!(practice.section, AppSection::Stage);
        let song: PersistedSession = serde_json::from_str(r#"{"tab":"Song"}"#).unwrap();
        assert_eq!(song.section, AppSection::Looper);
    }

    // A mock host Storage holding one String (what SealedStorage wraps; a real
    // host writes it to disk / OPFS). Shared cell so a test can peek at rest.
    #[derive(Clone, Default)]
    struct MemStorage(std::rc::Rc<std::cell::RefCell<Option<String>>>);
    impl Storage for MemStorage {
        fn load(&self) -> Option<String> {
            self.0.borrow().clone()
        }
        fn save(&self, contents: &str) {
            *self.0.borrow_mut() = Some(contents.to_string());
        }
    }

    #[test]
    fn sealed_session_is_encrypted_at_rest_and_round_trips() {
        let backing = MemStorage::default();
        let sealed = SealedStorage::new(backing.clone(), [9u8; 32]);

        let json = serde_json::to_string(&PersistedSession::default()).unwrap();
        sealed.save(&json);

        // At rest it is not the plaintext session.
        let at_rest = backing.load().unwrap();
        assert_ne!(at_rest, json);
        assert!(
            !at_rest.contains("\"bpm\""),
            "no plaintext session fields at rest"
        );
        // Reading back through the sealed storage recovers the session.
        assert_eq!(sealed.load().unwrap(), json);
    }

    #[test]
    fn a_wrong_key_reads_no_session_rather_than_crashing() {
        let backing = MemStorage::default();
        SealedStorage::new(backing.clone(), [1u8; 32]).save("a session");
        assert!(SealedStorage::new(backing, [2u8; 32]).load().is_none());
    }

    #[test]
    fn a_second_device_with_the_same_persona_reads_the_sealed_session() {
        use personae::InMemoryProvider;
        let backing = MemStorage::default();
        let seed = [3u8; 32];

        // Device A seals under its persona.
        SealedStorage::for_provider(backing.clone(), &InMemoryProvider::from_seed(seed))
            .unwrap()
            .save("my practice state");

        // Device B, same persona seed (paired via the wallet), re-derives the key
        // and reads it — the carry property.
        let device_b =
            SealedStorage::for_provider(backing.clone(), &InMemoryProvider::from_seed(seed)).unwrap();
        assert_eq!(device_b.load().unwrap(), "my practice state");

        // A different persona cannot read it.
        let other =
            SealedStorage::for_provider(backing, &InMemoryProvider::from_seed([4u8; 32])).unwrap();
        assert!(other.load().is_none());
    }
}
