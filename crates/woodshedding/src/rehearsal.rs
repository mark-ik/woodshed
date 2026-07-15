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
    Scale { name: String, root: PitchClass },
    Chord { name: String, root: PitchClass },
    /// A fixed playable sequence; a user/catalog exercise's steps live
    /// here, referenced by name.
    Riff { name: String },
}

impl Material {
    /// Short kind tag for badges in the set view.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Scale { .. } => "Scale",
            Self::Chord { .. } => "Chord",
            Self::Riff { .. } => "Riff",
        }
    }
}

/// Where a card came from: the recipe that stamped it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Recipe {
    Progression { name: String, key: PitchClass },
    Exercise { name: String },
    PracticeSet { name: String },
    /// `bar` is the source bar index in the song, so a playing song engine
    /// can map its bar cursor back to the exact card (used by the
    /// song-follow clock).
    Song { name: String, bar: usize },
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

/// One card: a piece of material with the context to put it on the neck
/// and play it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
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
    /// clamped on mutation.
    pub cursor: usize,
    #[serde(default)]
    pub loop_mode: LoopMode,
}

impl Set {
    pub fn push(&mut self, card: Card) {
        self.cards.push(card);
    }

    /// Insert a copy of the card at `idx` right after it.
    pub fn duplicate(&mut self, idx: usize) {
        if let Some(c) = self.cards.get(idx).cloned() {
            self.cards.insert(idx + 1, c);
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
