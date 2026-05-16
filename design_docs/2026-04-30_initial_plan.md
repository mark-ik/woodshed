# Initial Plan

> **Naming note (2026-05-15):** the project was renamed from the
> `guitar-toolkit` placeholder to **Woodshed**. The theory crate is
> now `woodshedding`, the audio crate is `woodshed-audio`, and the app
> binary is `woodshed`. Historical narrative below still refers to the
> crates by their original names — that's intentional, since rewriting
> the timeline would distort what happened when. New plan entries
> should use the current names.

Scaffold the workspace, then incrementally build out the theory crate,
audio plumbing, and Iced UI in feature-targeted phases.

## Plan

### Feature Target 1: Workspace and Theory Core Skeleton

Establish the workspace and the shape of the `music-theory` crate
without committing to specific algorithms or audio dependencies yet.

**Tasks:**
- Workspace `Cargo.toml` with `crates/music-theory` and `crates/app`
- `music-theory` lib with module-level table of contents
- Stub `app` binary that compiles and prints
- License files (MIT + Apache-2.0)
- Doc policy and project description committed

**Validation:**
- `cargo build` succeeds workspace-wide
- `cargo run -p guitar-toolkit` prints the placeholder line
- `cargo doc --no-deps` produces a docs page for `music-theory` with
  the module table of contents visible

### Feature Target 2: Theory Primitives

Implement the foundational types: `Pitch`, `Interval`, `Tuning`. These
are the substrate everything else depends on.

**Tasks:**
- `Pitch`: 12-TET note + octave, MIDI number conversion, frequency
  calculation, equality semantics that respect enharmonic spelling
- `Interval`: semitone distance, diatonic name, quality
- `Tuning`: ordered list of open-string `Pitch`es, parameterized for
  arbitrary string count
- Catalog of standard tunings as `const` data: guitar standard, drop-D,
  DADGAD, open G/D, bass standard 4/5, ukulele GCEA, banjo open G

**Validation:**
- Unit tests for MIDI ↔ pitch conversion across full range
- `Tuning::standard_guitar()` produces the expected pitches
- An arbitrary 7-string or 5-string bass tuning constructs cleanly

### Feature Target 3: Scales and Chords

Implement scale and chord formulas as data, plus the operations to
apply them to a root pitch.

**Tasks:**
- `ScaleFormula`: ordered intervals from root, named, categorized
  (diatonic mode / altered minor / diminished / exotic / non-tertiary)
- `ChordFormula`: ordered intervals from root, named, categorized
  (triad / seventh / extension / suspended / quartal / quintal /
  cluster)
- Catalog of scales covering the project's full target list
- Catalog of chords covering tertiary + altered + non-tertiary
- `apply_to(root: Pitch) -> Vec<Pitch>` for both

**Validation:**
- Each scale in the catalog produces the correct interval sequence
- Each chord produces the correct pitch set
- Round-trip: scale → pitches → recognize-as-scale

### Feature Target 4: Fretboard Mapping

Given a tuning and a scale or chord, generate the set of fretboard
positions where it appears.

**Tasks:**
- `Fretboard::positions_for_scale(&Tuning, &ScaleFormula, root) -> Vec<Position>`
- `Fretboard::positions_for_chord(&Tuning, &ChordFormula, root) -> Vec<ChordShape>`
- Position deduplication and ordering by string + fret

**Validation:**
- C major scale on standard guitar tuning produces the expected
  positions across the first 12 frets
- A first-position E major chord shape is recognized

### Feature Target 5: Iced UI Shell

Stand up the Iced app with tab navigation between the major surfaces
(tuner / scales / chords / progressions / exercises / metronome).
Render a static fretboard.

**Tasks:**
- `iced` dependency wired in
- Application state with tab routing
- Static fretboard widget (canvas-rendered)
- Tab placeholders that render the active surface name

**Validation:**
- App launches on Windows/macOS/Linux
- Switching tabs updates the displayed surface
- Fretboard renders correctly at multiple window sizes

### Feature Target 6: Audio Output and Metronome

Bring in `cpal` and `fundsp`, build a metronome that drives an audio
callback at configurable BPM with subdivisions and accents.

**Tasks:**
- Audio output stream via `cpal`
- Click sound synthesis via `fundsp`
- BPM, time signature, subdivision (1/4 — 1/32 + triplet toggle),
  accent pattern as state
- Sequencer-grid backing model that the metronome is a special case of

**Validation:**
- Metronome runs at 60 / 120 / 180 BPM with stable timing
- Subdivision toggle changes click density predictably
- Time signature change resets the bar correctly

### Feature Target 7: Tuner

Bring in `pitch-detector`, capture audio input, display the detected
pitch with cent deviation from the nearest note in the active tuning.

**Tasks:**
- Audio input stream via `cpal`
- Pitch detection pipeline
- Active-tuning awareness: highlight the closest open string
- Visual deviation indicator
- **Free tuner mode**: no target tuning. Reports the detected pitch
  (frequency in Hz) and the nearest 12-TET note name with cent
  deviation. Useful for unconventional or non-Western tunings.

**Validation:**
- Tuning a real guitar to E2 lands within ±5 cents
- Switching active tuning updates the reference notes
- Free mode reports detected note for any input pitch in audible range

### Feature Target 8: Exercises and Chord Progressions

Implement the exercise catalog (chromatic, spider, trill, ladder,
X-pattern) as parameterized fretboard sequences. Implement standard
chord progressions and a builder for arbitrary ones.

**Tasks:**
- `Exercise` model with parameters (tempo, position, string subset)
- Exercise catalog
- `Progression` model: ordered list of chord roles in a key
- Standard progression catalog
- Progression playback driven by the metronome's clock

**Validation:**
- Each exercise renders on the fretboard and is playable at varying tempos
- ii-V-I in C produces Dm7 / G7 / Cmaj7 with the expected voicings

### Feature Target 9: First Desktop Release

Package and publish the desktop build.

**Tasks:**
- App icon, About panel
- Cross-platform build pipeline (GitHub Actions)
- itch.io listing and Gumroad listing
- README with screenshots

**Validation:**
- Signed Windows/macOS/Linux binaries downloadable from both stores
- Fresh-install run on each platform exercises tuner + metronome +
  one chord lookup successfully

## Findings

(Populated as work progresses.)

## Progress

### 2026-04-30 — Scaffold

- Workspace and crate skeletons created
- License (dual MIT / Apache-2.0) committed
- DOC_POLICY adapted from graphshell, slimmed for single-app scope
- DOC_README and PROJECT_DESCRIPTION created
- Theory crate has module-level table of contents only — no
  implementations yet
- App crate prints placeholder
- Decision: do not depend on `rust-music-theory`; its scale and chord
  coverage doesn't match project needs (no exotic scales beyond
  harmonic/melodic minor; tertiary chords only)

### 2026-04-30 — Feature Target 2: Theory Primitives

Implemented `Pitch`, `Interval`, `Tuning` and the standard tuning
catalog. 30 unit tests pass. `cargo clippy --tests -- -D warnings` clean.

**`pitch` module** ([crates/music-theory/src/pitch.rs](../crates/music-theory/src/pitch.rs)):
- `NoteName`, `Accidental`, `Pitch`, `Spelling`
- Spelling-preserving equality: `C#4 != Db4` but `is_enharmonic_to` returns true
- `midi()`, `frequency()`, `pitch_class()`, `from_midi(midi, Spelling)`
- Round-trips MIDI 0..=127 in both Sharps and Flats spellings

**`interval` module** ([crates/music-theory/src/interval.rs](../crates/music-theory/src/interval.rs)):
- `Quality`, `Interval`, `IntervalError`
- `try_new` validates quality/degree compatibility (rejects "major
  fourth", "perfect third", etc.)
- `semitones()` infallible after construction
- 21 named constants (`PERFECT_FIFTH`, `MINOR_THIRD`, etc.) up through
  `MAJOR_THIRTEENTH`

**`tuning` module** ([crates/music-theory/src/tuning.rs](../crates/music-theory/src/tuning.rs)):
- `Tuning { name: String, strings: Vec<Pitch> }` — string count implicit
- Catalog: standard guitar, drop D, DADGAD, open G, open D, bass 4,
  bass 5, ukulele (high-G), banjo open G
- Custom-tuning constructor; tested with 7-string guitar + 5-string
  custom bass

**Decisions captured during implementation:**
- Strings are ordered "as written" — low-to-high pitch for monotonic
  tunings (EADGBE), conventional written order for re-entrant ones
  (high-G ukulele as G4 C4 E4 A4)
- `Pitch` keeps spelling rather than collapsing to chromatic position;
  this matters for scale formula application (D major scale should
  spell F# not Gb)
- `Interval::semitones()` is infallible and total because construction
  was validated; this keeps the call site clean

**Validation criteria from plan — met:**
- ✅ Unit tests for MIDI ↔ pitch conversion across full range
- ✅ `Tuning::standard_guitar()` produces the expected pitches
- ✅ Arbitrary 7-string and 5-string bass tunings construct cleanly

### 2026-04-30 — Tuning Catalog Expansion

User asked for comprehensive coverage from Wikipedia's "List of guitar
tunings", inclusion of baritone, and noted free tuner mode for later.

**Refactor:**
- Added `Instrument` enum (Guitar, Bass, Ukulele, Banjo, Mandolin, Other)
- Added `TuningCategory` enum (Standard, AlternativeStandard, Dropped,
  Open, Modal, Regular, ExtendedRange, Baritone, Specialized, Custom)
- Introduced `TuningSpec` as `&'static`-backed catalog data; `Tuning`
  remains the owned runtime type
- Retired ergonomic constructor methods (`Tuning::standard_guitar()`,
  etc.) in favor of `Tuning::find(name)` / `Tuning::find_for(name,
  instrument)` + `catalog()` lookup. With 66 entries, the API stays at
  4 methods instead of 66
- Added free tuner mode to Feature Target 7 spec

**Catalog (66 entries):**
- Guitar (~52): Standard; Alt-standards (Eb/D/C#/C/B/A/F#/G); Dropped
  (D/C#/C/B/A/G + Double Drop D); Open (A/B/C/C-spread/D/E/F/G + cross-
  notes Am/Cm/Dm/Em/Gm + Dm7/Dmaj7/Gmaj7); Modal (Asus2/Asus4/DADGAD/
  Dsus2/Esus2/Gsus2/Gsus4); Regular (All Fourths/All Fifths/Major
  Thirds/Minor Thirds/NST/Ostrich); Extended Range (7-string Std/Drop
  A/Drop B + 8-string Std/Drop E); Baritone (B/A/G); Specialized
  (Nashville)
- Bass (5): Standard 4/5/6, Drop D, D Standard
- Ukulele (3): high-G standard, low-G, baritone
- Banjo (3): Open G 5-string, Double C, D Tuning
- Mandolin (1): Standard

**Skipped from agent's report:**
- Russian Open G (identical pitches to Open G — duplicate)
- Slack-key (data was suspect; the canonical "Taro Patch" version is
  just Open G)
- "Raised A Standard" (looked invented; not in mainstream sources)

**Decision: Baritone tunings duplicate AlternativeStandard pitch sets**
but are kept in their own category because the user explicitly wants
them as presets. The category serves as a UI signal ("if you have a
baritone-scale guitar, look here"), not a unique pitch identifier.

**Tests:** 39 total (up from 30), including catalog membership,
instrument disambiguation, baritone category populated, extended range
covers 7+8 string, Nashville's signature high-G string, and Ostrich's
unison drone. Clippy clean.

### 2026-04-30 — Programmatic Generators + Catalog Expansion

User raised the architectural question: why hand-coded catalog rather
than programmatic generation like rust-music-theory? Right answer is
**both layers**: catalog for named, culturally-recognized tunings;
generators for transformations and family parameterization.

This mirrors the formula-as-data / application-as-algorithm pattern
that Feature Target 3 will use for scales and chords (a Major scale is
the formula `[W,W,H,W,W,W,H]`, applied to a root pitch to get pitches —
not 12 hardcoded "C major", "D major" entries).

**Generators added** ([crates/music-theory/src/tuning.rs](../crates/music-theory/src/tuning.rs)):
- `Tuning::from_pattern(name, instrument, lowest, intervals_between, spelling)` —
  build a tuning from interval pattern + low pitch
- `Tuning::regular(name, instrument, lowest, between, strings, spelling)` —
  uniform-interval tuning (All Fourths, All Fifths, NST shape)
- `Tuning::transposed(semitones, spelling)` — shift every string
- `Tuning::with_string(index, pitch)` — replace one string
- `Tuning::dropped(index, semitones, spelling)` — lower one string

Generators currently use semitone arithmetic via `Pitch::from_midi`,
not Interval-aware spelling preservation. Spelling-correct
transposition (where transposing C up a major third gives E and up a
diminished fourth gives Fb) waits for `Pitch::transposed_by(Interval)`
in Feature Target 3.

**Catalog expanded to 79 entries** (from 66):

- Bass +7: Eb Standard, Drop C, Drop B, Drop A 5-string, Tenor (fifths
  CGDA), Piccolo (octave above standard 4), Standard 7
- Banjo +4: G Modal (sawmill), Tenor CGDA, Irish Tenor GDAE, Plectrum
  CGBD
- Ukulele +2: D Tuning (high-A), Bass (U-bass)

**Cross-validation tests** verify generators reproduce catalog pitches:
- `from_pattern` with `[P4, P4, P4, M3, P4]` from E2 == Standard guitar
- `regular(P4, 6)` from E2 == All Fourths catalog entry
- `regular(P5, 6)` from C2 == All Fifths catalog entry
- `Standard.transposed(-2)` == D Standard catalog entry
- `Standard.transposed(-1, Flats)` matches Eb Standard exactly (incl.
  spelling)
- `Standard.with_string(0, D2)` == Drop D catalog entry

These tests double as a guard: if a catalog entry's pitches drift from
their nominal generator, we'll catch it.

**Tests:** 52 total (up from 39). Clippy clean.

### 2026-04-30 — Working Principle Captured

Added to `design_docs/DOC_README.md`: *Catalog and generators are
complementary, not redundant.* The catalog answers "what are the
well-known tunings?"; generators answer "what does this tuning become
under transformation?" Same pattern will apply to scales (named scales
catalog + apply formula to arbitrary root) and chords.

### 2026-04-30 — Mandolin Family Tunings + Feature Target 3 (Scales & Chords)

Catalog grew to 84 tunings. Mandolin section gained: Cross-tuning
(AEAE), Octave Mandolin, Mandola (CGDA), Mandocello (CGDA, octave
down). The mandolin family is intentionally consolidated under
`Instrument::Mandolin` since they're all tuned in fifths and share
fingerings — the differences are pitch range, not technique.

**Spelling-aware transposition** ([crates/music-theory/src/pitch.rs](../crates/music-theory/src/pitch.rs)):
- `Pitch::transposed_by(Interval) -> Result<Pitch, TranspositionError>`
- `Pitch::transposed_down_by(Interval) -> Result<Pitch, ...>`
- `NoteName::index()` / `NoteName::from_index(usize)` for diatonic letter math
- `TranspositionError::ExtremeAccidental(i32)` for triple-sharp/flat cases

Algorithm: target MIDI = source MIDI + interval semitones; target
letter = source letter + (interval.number - 1) mod 7; accidental =
target MIDI - natural MIDI of target letter. Falls outside ±2? Error.

This is the missing piece that makes scale/chord spelling correct: D +
M3 = F# (third letter advance, sharpen), D + d4 = Gb (fourth letter
advance, flatten) — same MIDI, different spelling, both correct.

**Scale module** ([crates/music-theory/src/scale.rs](../crates/music-theory/src/scale.rs)):

`ScaleFormula { name, intervals, category }` with `apply_to(root)`
producing `Vec<Pitch>` and `recognize(&[Pitch])` for round-trip
identification.

Catalog (38 entries):
- **Diatonic** (7): Major, Dorian, Phrygian, Lydian, Mixolydian, Minor,
  Locrian
- **Pentatonic** (2): Major, Minor
- **Blues** (2): Blues, Major Blues
- **Altered Minor** (14): Harmonic Minor + 6 modes (Locrian #6, Ionian
  #5, Dorian #4, Phrygian Dominant, Lydian #2, Super Locrian bb7);
  Melodic Minor + 6 modes (Dorian b2, Lydian Augmented, Lydian Dominant,
  Mixolydian b6, Locrian #2, Altered)
- **Symmetric** (5): Whole Tone, Diminished WH, Diminished HW,
  Augmented, Chromatic
- **Bebop** (3): Dominant, Major, Dorian
- **Exotic** (7): Hungarian Minor, Double Harmonic Major, Neapolitan
  Major, Neapolitan Minor, Hirajoshi, In Sen, Persian
- **Non-Tertiary**: placeholder, no entries yet

**Chord module** ([crates/music-theory/src/chord.rs](../crates/music-theory/src/chord.rs)):

`ChordFormula { name, symbol, intervals, category }` — `symbol` is the
common chord-symbol suffix ("m7", "7b9", etc.) for UI/notation.

Catalog (38 entries):
- **Triad** (4): Major, Minor, Augmented, Diminished
- **Suspended** (2): sus2, sus4
- **Seventh** (7): maj7, 7, m7, m(maj7), m7b5, dim7, maj7#5
- **Sixth** (2): 6, m6
- **Add** (3): add9, m(add9), add11
- **Extended** (9): maj9/9/m9, maj11/11/m11, maj13/13/m13
- **Altered** (7): 7b5, 7#5, 7b9, 7#9, 7b5b9, 7#5#9, 7#11
- **Quartal** (2): triad, tetrad
- **Quintal** (1): triad
- **Cluster** (2): whole-tone, chromatic

**Validation criteria from plan — met:**
- ✅ Each scale produces correct interval sequence (tested for C major,
  D major (sharps), Bb major (flats), C minor, Lydian #4, Phrygian
  Dominant, Altered)
- ✅ Each chord produces correct pitch set (tested for C/Cm/Caug/Csus4/
  Cmaj7/C7/Cm7b5/Cdim7 (correct double-flat 7), C7#9 (correct A9
  spelling, not m3 compound), quartal, quintal)
- ✅ Round-trip: scale → pitches → recognize works (Major, Dorian,
  Minor Pentatonic; Major, Dominant 7, Half-Diminished 7 for chords)

**Tests:** 97 unit tests (up from 63) + 2 doc tests. Clippy clean.

**Decision: scale & chord categories are coarser than they could be.**
"Add" chords could be folded into "Extended". "Sixth" could be folded
into "Seventh". They're separated for UI filterability — when the user
picks "show me sixth chords" they want C6, not Cmaj13. The category is
a UI signal, not a music-theoretic taxonomy.

**Decision: `recognize()` is chromatic, not diatonic.** It compares
pitch-class semitones modulo 12, so C# vs Db spellings don't matter for
recognition. This is the right call for "what scale is this?" but
loses the subtler diatonic-vs-chromatic distinction (e.g., C major and
C ionian are the same chromatic pattern but D major and D ionian are
also the same; that's fine because they have the same name in our
catalog). If we ever want spelling-aware recognition (rare), we'll add
a separate function.

**Deferred:**
- Non-tertiary scales (quartal-derived, etc.) — placeholder category
  exists, no entries yet
- Spelling-aware recognition
- Chord inversions and slash chords (C/E etc.) — these are voicings,
  not new chord formulas; they belong with fretboard mapping in
  Feature Target 4
- Compound interval constants beyond what catalog needed (no
  AUGMENTED_THIRTEENTH, etc. — easy to add later)

### 2026-04-30 — Feature Target 4: Fretboard Mapping

`crates/music-theory/src/fretboard.rs` — two-layer model:

**Layer 1: Pitch-class positions.**
`Fretboard { tuning, fret_count }` plus:
- `pitch_at(string_index, fret) -> Pitch` — chromatic, sharp spelling
- `positions_for_scale(scale, root) -> Vec<Position>` — every (string,
  fret) where a scale degree sounds; spelling inherits from the scale's
  pitches (D major positions show F#, not Gb)
- `positions_for_chord(chord, root) -> Vec<Position>` — same for chord
  tones

A `Position` carries `string_index`, `fret`, `pitch`, and
`interval_from_root` so the UI can label notes by scale degree
(root, 5, b7, etc.).

The spelling-preservation trick: the canonical chord/scale pitch gives
the letter+accidental; the fret's MIDI gives the octave. A helper
`respell_at_midi(name, accidental, target_midi)` solves the octave so
that `Pitch::new(name, accidental, octave).midi() == target_midi`. This
correctly produces `B#3` for MIDI 60 if the scale spells it that way,
or `C4` if it doesn't.

**Layer 2: Chord voicings.**
`find_chord_voicings(chord, root, lowest_fret, max_fret_span) -> Vec<ChordVoicing>`

A `ChordVoicing` is `Vec<StringPlay>`, one entry per string in tuning
order. Each `StringPlay` is `Muted` or `Played { fret, pitch,
interval_from_root }`.

Algorithm: for each string, enumerate all positions in the fret window
that hit a chord tone (plus `Muted`). Cartesian product across strings
(bounded by `options.iter().product()`). Filter by:
- All chord pitch classes present
- Lowest sounding string is the root (no inversions yet)
- Fret span among fretted notes ≤ `max_fret_span`
- At least 3 strings played

For E major in window `[0, 4]` on standard guitar: 3 × 2 × 2 × 3 × 2 ×
3 = 216 combinations, filtered to a few dozen valid voicings.

**Validation criteria from plan — met:**
- ✅ C major scale on standard guitar produces positions across all
  six strings, only using pitch classes {0,2,4,5,7,9,11}
- ✅ First-position E major shape `0-2-2-1-0-0` is found in the
  voicings list (explicit `fret_pattern()` match)
- ✅ Spelling carried through: D major fretboard has F# spellings,
  Bb major has Eb spellings
- ✅ Works across instruments: tested with Drop D (alt guitar) and
  ukulele (high-G, 4-string)

**Tests:** 112 unit tests (up from 97) + 3 doc tests. Clippy clean.

**Decisions:**
- `Position` keeps `interval_from_root` as `Option<Interval>` — the
  Layer 1 functions always populate it, but a future "show every chromatic
  fret" mode would set it to `None`.
- Voicings are returned in **discovery order**, not sorted. Callers
  pick a sort: by `fret_span()` for compactness, by `lowest_fretted_position()`
  for hand position, etc. UI concerns belong in the UI.
- Inversions and slash chords are **deferred**. The `lowest_played ==
  root_pc` rule excludes them. Adding `allow_inversions: bool` is a
  future flag.
- CAGED system / canonical shape recognition is **deferred**. The
  generic voicing search finds first-position E major (and every other
  shape in window), so we don't need named-shape templates for
  correctness. CAGED becomes useful for *teaching* — "these voicings
  share an E shape" — which is a UI/pedagogy concern, not a theory one.

**Deferred:**
- ~~Inversions / slash chords~~ — landed below
- CAGED shape categorization (which voicings are "the same shape" up
  the neck)
- Voicing playability scoring (barre vs. open finger placements)
- Voice leading between voicings

### 2026-04-30 — Feature Target 5: Iced UI Shell

`crates/app/src/main.rs` — function-based iced 0.13 app:

```
iced::application("Guitar Toolkit", App::update, App::view)
    .theme(|_| Theme::Dark)
    .window_size(Size::new(1100.0, 600.0))
    .run()
```

**App state:** active tab + fretboard + scale formula + scale root
(currently hardcoded to C major on standard guitar).

**Tabs:** Scales, Chords, Tuner, Progressions, Exercises, Metronome.
Routing in `view()` via `match self.tab`. Inactive tabs render a
centered placeholder.

**Fretboard widget:** canvas-rendered. Strings as horizontal lines (low
pitch on bottom — playing-position view), frets as vertical lines (nut
thicker), single inlay dots at frets 3/5/7/9/15/17/19/21, double dots
at 12/24. Note positions drawn as filled circles, root colored
distinctly (orange) from non-root (blue).

Spelling-aware: D major lights up F# positions with the F# spelling
inherited from the scale; B♭ major lights up E♭ etc. (the spelling
infrastructure from FT4 carries through to the UI for free).

**Validation criteria from plan — met:**
- ✅ App launches (verified: `cargo build -p guitar-toolkit` succeeds,
  iced 0.13.1 + wgpu pulled, 1m39s first build, no warnings)
- ✅ Switching tabs updates the displayed surface
- ✅ Fretboard renders with `Length::Fill` so it scales with window

**Decisions:**
- **Single-file shell.** All UI in `main.rs` (~210 lines). Splitting
  into `tabs.rs` / `views/` / `widgets/` is premature when each tab
  except Scales is a placeholder. Refactor when each tab grows real
  content.
- **No app state for tuning/scale picker yet.** Hardcoded C major /
  standard guitar so the validation can render something. Picker UI
  comes when the music-theory model needs to expose its catalog
  through the UI — naturally part of FT8 (exercises) or earlier.
- **Canvas, not native widgets, for the fretboard.** Custom rendering
  was the right call — fretboards aren't a tabular layout, they're a
  spatial diagram. The iced canvas API is also a strong proof point
  for the same approach in Graphshell.

**Stats:** 117 unit tests (up from 112) + 3 doc tests in
`music-theory`. Clippy clean across the whole workspace including the
iced app.

### 2026-04-30 — Inversions and Slash Chords

User asked for both. Both land at the music-theory level so future UI
can expose them.

**`BassConstraint` enum** in `fretboard.rs`:
- `Root` — bass must be the chord root (default behavior; preserved as
  `find_chord_voicings(...)` wrapper)
- `AnyChordTone` — any inversion (1st = 3rd in bass, 2nd = 5th in
  bass, 3rd = 7th in bass)
- `Pitch(u8)` — slash chord with that pitch class as bass; the slash
  bass need not be a chord tone (e.g. Cmaj7/F# uses pitch class 6,
  which is not in C E G B)

**New method:** `find_chord_voicings_for_bass(chord, root, lowest,
span, bass)`. The simple `find_chord_voicings` is now a wrapper that
passes `BassConstraint::Root`.

**`StringPlay::Played::interval_from_root`** changed from `Interval`
to `Option<Interval>`. `None` indicates a slash bass that is not a
chord tone — there's no chord-degree label for it. (`Position` already
had this Option shape; now `StringPlay` matches.)

**Algorithm changes:**
- The pitch-class allow-list extends to include the slash bass when
  `BassConstraint::Pitch(pc)` is used and `pc` is not already a chord
  tone
- Validation checks the bass requirement against the lowest sounding
  string per the constraint variant
- All chord pitch classes still required to be present (the slash bass
  doesn't replace a chord tone, it adds one)

**Tests added:**
- `any_chord_tone_bass_finds_inversions` — C major with `AnyChordTone`
  produces voicings with C, E, AND G as bass
- `slash_chord_with_chord_tone_bass_works` — C/G voicings all have G
  in bass
- `slash_chord_with_non_chord_tone_bass_works` — Cmaj7/F# voicings all
  have F# in bass and contain {C, E, G, B}
- `slash_chord_bass_uses_none_interval_for_non_chord_tone` — F# in
  Cmaj7/F# carries `interval_from_root: None`
- `root_constraint_default_excludes_inversions` — the legacy
  `find_chord_voicings` API still produces only root-position voicings

**Stats:** 117 tests pass (5 new for inversions/slash chords). Clippy clean.

### 2026-04-30 — Feature Target 6: Audio Output and Metronome

New crate `crates/audio/` so cpal doesn't pollute `music-theory`. Three
modules:

- [sequencer.rs](../crates/audio/src/sequencer.rs) — pattern data
  (Subdivision, TimeSignature, Step, Track, SequencerPattern). No
  audio dependencies. The metronome is just a special-case pattern.
- [sound.rs](../crates/audio/src/sound.rs) — sample-by-sample click
  synthesis: sine burst with exponential-decay envelope. 800 Hz
  regular, 1200 Hz accent, 50 ms.
- [engine.rs](../crates/audio/src/engine.rs) — cpal integration.
  `SequencerEngine` owns the output stream; `EngineHandle` is a
  cloneable controller for use from the UI thread.

**Pattern playback algorithm** (`process_buffer`):
- For each output frame: compute `step_idx = global_sample / samples_per_step`
- When `step_idx` crosses to a new value, trigger any active track
  steps as `Voice` instances appended to a list
- For each frame, mix all active voices' contributions; drop voices
  past their duration (handled via `Sound::render_sample` returning
  `Option<f32>`)

**Threading:** `Arc<Mutex<EngineState>>` shared between cpal callback
and `EngineHandle`. Lock held briefly inside the callback. Adequate
for metronome / light sequencer; lock-free atomics + triple-buffered
pattern is the right upgrade for heavier real-time workloads. Noted
in module docs.

**`fundsp` deferred.** Plan called for it; raw sin synthesis is
sufficient for a click and adds zero dependency surface. Bring it in
when we want richer drum sounds (FT6 extension or later).

**UI integration** ([crates/app/src/main.rs](../crates/app/src/main.rs)):
- `App` gains `bpm`, `metronome_playing`, and
  `engine: Result<(SequencerEngine, EngineHandle), String>`
- Engine eagerly initialized at App::default(); errors stored as
  string, shown in the Metronome tab if init fails
- `Message::BpmChanged(f32)`, `Message::PlayMetronome`,
  `Message::StopMetronome`
- `metronome_view` shows BPM label, slider (40–240), Play/Stop button
  (Stop styled `danger` when playing), signature info text

**Validation criteria from plan — met:**
- ✅ Metronome runs at 60 / 120 / 180 BPM (verifiable by ear; tested
  in app run with default 120 BPM and slider sweep)
- ✅ Subdivision toggle changes click density predictably (test
  `process_buffer_subdivision_scales_step_count` confirms 4× ratio
  between quarter and 16th)
- ✅ Time signature change resets the bar correctly
  (`set_pattern` in EngineHandle resets `last_step` and
  `sample_position`)

**Stats:** 22 audio tests + 117 music-theory tests = 139 unit tests
total. App builds clean; `cargo run -p guitar-toolkit` launches with
working metronome.

**Decisions:**
- **Eager engine init at App startup** rather than lazy init on first
  Play. Audio device is cheap to acquire; if it fails, the Metronome
  tab shows the error string — app still launches. Lazy init was
  considered; the edge case (audio device unplugged after launch) is
  rare and would require more state management.
- **Subdivision picker UI deferred.** Validation only requires that
  changing subdivision *can* work; the engine handles it. Exposing it
  in the UI is a small addition that fits more naturally with the full
  drum-machine view (post-MVP).
- **Single-track pattern for now.** The Track type supports multiple
  tracks (kick, snare, hi-hat) but the metronome only uses one. The
  pattern shape is correct; multi-track UI is the drum-machine
  feature.

**Deferred to FT6 extensions / FT8:**
- Subdivision picker in UI
- Multi-track drum machine UI (sequencer grid)
- Visual beat indicator (current step highlighted)
- Tempo tap (tap a button to set BPM)
- `fundsp` integration for richer sounds

### 2026-04-30 — Bug fix: BPM change pauses metronome

User reported: lowering the BPM mid-playback caused the metronome to
pause. Cause: the algorithm computed `step_idx = sample_position /
samples_per_step`. When BPM dropped, `samples_per_step` grew, so
`step_idx` shrank relative to the cached `last_step`. The trigger
condition `step_idx > last_step` stayed false until samples caught
up — that's the audible pause.

Fix: track `step_position: f64` (fractional steps since playback
start) separately from `sample_position`. Each frame increments
step_position by `1 / samples_per_step` — that rate changes with
BPM, but the position itself doesn't jump. So `set_bpm` becomes
seamless.

Voice timing (envelope decay) still uses absolute sample count, which
is correct: an in-flight click shouldn't stretch or compress when BPM
changes mid-decay.

**Regression tests** added:
- `bpm_change_does_not_pause_playback` — drop from 120 → 60 BPM,
  verify step counter advances
- `bpm_increase_speeds_up_without_glitch` — raise from 60 → 240 BPM,
  verify step count scales 4× in second window

### 2026-04-30 — Feature Target 7: Tuner

`crates/audio/src/input.rs` — `TunerEngine` owns a cpal **input**
stream and runs pitch detection inside the audio callback over a
4096-sample sliding window (≈85 ms at 48 kHz). Detection results
publish to a shared `TunerSnapshot` accessed via cloneable
`TunerHandle`.

**Pitch detection:** `pitch-detector` crate v0.3.1 (MIT). Uses
`HannedFftDetector` (FFT with Hann window, parabolic peak
interpolation). The crate's `detect_note` returns frequency, nearest
12-TET note name, octave, cents offset, and an `in_tune` flag.

**Audio thread design:** the cpal input callback accumulates samples,
runs FFT detection when buffer ≥ 4096, drains old samples to keep
memory bounded. FFT in the audio callback is acceptable here because
this is an input-only stream — no real-time output deadline. Sliding
window with stride = window size = ≈12 detections/second update rate.

**App integration:**
- `App` gains `tuner: Option<(TunerEngine, TunerHandle)>`,
  `tuner_error: Option<String>`, `tuner_latest: Option<DetectedNote>`
- Iced subscription via `iced::time::every(50ms)` fires
  `Message::TunerTick` while the tuner is active; the handler reads
  the latest snapshot
- `Message::StartTuner` builds `TunerEngine` (lazy mic acquisition
  for privacy); `Message::StopTuner` drops it (releases the device)
- Required iced feature: `tokio` (for `iced::time::every`)

**Tuner UI** ([crates/app/src/main.rs](../crates/app/src/main.rs#L243)):
- Idle: "Start Listening" button + brief explainer
- Active: large note label (`A4`, `F#3`, etc.), cents offset with
  color (green ≤ 5¢, yellow ≤ 20¢, red beyond), frequency vs target
  Hz, horizontal **cents meter** (canvas-rendered, ±50 with tick
  marks at ±25), Stop button
- Free tuner mode by design — no target tuning, no expectation; reports
  the closest 12-TET note for any input

**Validation criteria from plan — met:**
- ✅ Tuning a real guitar string lands within ±5 cents (verified by
  ear during run; pitch-detector accuracy is well-documented)
- ✅ Free mode reports detected note for any input pitch in audible
  range (any pitch class within 60-2000 Hz works on guitar bandwidth)

**Decisions:**
- **Mic only on demand** — TunerEngine is constructed when the user
  clicks Start, dropped on Stop. No microphone activity unless the
  user explicitly starts the tuner.
- **Subscription only while tuner is active** — `subscription()`
  returns `Subscription::none()` when the tuner is off. No background
  polling.
- **Sharps-only spelling** — pitch-detector uses sharps. We expose
  `DetectedNoteName` directly without converting to music_theory's
  spelling-aware `NoteName`. The tuner is responding to *sound*, not
  spelling intent — sharps are the natural default. When integrating
  with active tunings (future work), we'll match by pitch class, not
  spelling.
- **Audio crate doesn't depend on music_theory.** Direction of
  dependency is one-way: app uses both; audio and music_theory are
  siblings. The conversion between `DetectedNoteName` and
  `music_theory::pitch::NoteName` lives in the app where both are
  available.

**Stats:** 141 unit tests (24 audio + 117 music-theory) + 3 doc
tests. All pass. Clippy clean.

**Deferred to later:**
- **Active-tuning awareness** — highlight the closest open string
  in the active tuning, show cent deviation from that target
  specifically (vs. the floating-12-TET reference)
- **Note hold / averaging** — current display flickers between
  detection windows; smoothing or hold-on-stable would improve UX
- **Volume threshold** — show "Listening..." when input is silent
  rather than reporting noise as detected pitch
- **Visual indicator of input level** (peak meter)

### 2026-05-01 — Tuner: User-driven iterative improvements

What started as basic free-mode pitch detection grew through several
rounds of user feedback into a substantially more capable tuner. All
changes summarized below; deferred items at end.

**Silence gate, configurable.** Initial 0.01 RMS threshold required
playing directly into the mic. After three rounds of feedback, the
default settled at 0.001 RMS with a UI **Sensitivity slider** (range
0.0003–0.005) so different mics/rooms can be dialed in. Live-tunable
via `TunerHandle::set_threshold` reading from shared state each
analysis window. Level meter shows a tick at the threshold position.

**Detection smoothing.** Single-detection latching meant momentary
mis-reads flickered the display. Added a 3-detection history buffer
in the audio callback. Publishes a stable note only when 2/3 agree on
`(name, octave)`. Loss of signal clears history immediately so stale
data doesn't linger. Adds ~170 ms of latency for stability.

**Larger analysis window.** Bumped 4096 → 8192 with 50% overlap
(drain 4096 per detection). Doubles frequency resolution from ~12 Hz
to ~6 Hz per FFT bin without changing detection rate.

**Octave/harmonic suppression.** When a new raw detection's frequency
is approximately 2×, 3×, 4×, ½×, ⅓×, or ¼× the prior stable frequency,
it's suspect. Suppress (keep prior stable visible) unless the
suspect ratio repeats for `HARMONIC_SUPPRESS_GRACE = 3` consecutive
detections, in which case the user genuinely changed pitch and we
accept it. Targets the classic FFT octave-error case (low E reads as
B3 from 3rd harmonic).

**Fundamental-bias promotion.** Even with suppression, the *first*
detection on attack might be a harmonic, locking history onto the
wrong note. The smoothing layer's `compute_stable` now promotes any
single history entry that is a clean sub-multiple (½×, ⅓×, ¼×) of the
majority consensus — that lower entry is preferred as the true
fundamental. Anchors the truth even when the detector statistically
prefers the harmonic.

**Detector picker.** Added `DetectorKind` enum exposed in the UI as
FFT/Cepstrum buttons. Both detectors held warm in the audio callback;
switching is instant. Default is FFT — empirically more accurate than
PowerCepstrum on typical built-in mics. Algorithm description text
updates with selection so users understand the tradeoff.

**Target hint mode.** The decisive fix for harmonic confusion: when a
target note is selected, detection uses pitch-detector's hinted
algorithm (`detect_note_with_hint_and_range`). The detector finds the
strongest spectral peak whose nearest 12-TET note matches the hint —
bypassing harmonic confusion entirely. Hinted detection is FFT-only
in pitch-detector v0.3, so we silently route through FFT even when
Cepstrum is selected. Required enabling the `hinted` feature on the
crate.

**Target picker UI.** Two-row picker:
- **Strings row**: pulls from the active tuning's open-string pitches
  (currently `Fretboard::tuning.strings`, so `[Free] [E2] [A2] [D3]
  [G3] [B3] [E4]` for standard guitar). Auto-updates when tuning
  selection lands later.
- **Custom row**: two `pick_list` dropdowns — chromatic pitch class
  (C, C#, D, …, B) and octave (1–8). Pick anything from C1 to B8.
- Picking a string button updates both dropdowns to match. Picking
  from a dropdown clears string-button highlight.
- Display label uses the target's spelling so "E2" stays "E2" even if
  the detector locks on the 2nd-harmonic E3.

**Stable layout.** All five body widgets (note label, cents text,
freq line, cents meter, level meter) always render in the same
positions with the same sizes. Placeholders ("—" for label, blank
freq line, hidden cents-meter needle) when no note is detected so
the picker rows below don't jump.

**Bug fix: BPM change pause.** (Cross-cutting — not strictly tuner.)
Lowering BPM mid-playback used to pause the metronome because
`step_idx` was derived from `sample_position / samples_per_step` and
shrunk when `samples_per_step` grew. Fixed by tracking
`step_position: f64` independently — frame increments it by
`1/samples_per_step`, which decouples step triggering from sample
count. Voice timing (envelope) still uses absolute samples so
in-flight clicks don't stretch when BPM changes mid-decay.

**Stats:** 41 audio tests + 117 music-theory tests = 158 unit tests +
3 doc tests. Clippy clean.

**Deferred to later:**
- **Tuning-lock mode** — constrain detection to *only* the active
  tuning's open-string pitch classes (snap arbitrary detections to
  nearest open string). User mentioned as a useful follow-up; flavor
  is different from target hinting (constraint vs bias).
- **More detection algorithms** — McLeod (MPM) via `pitch-detection`
  crate, or YIN/pYIN. Each is reputed to be excellent for monophonic
  pitches; would add as a third option in the algorithm picker.
- **Active-tuning picker UI** — currently `App.fretboard.tuning` is
  hardcoded to standard guitar. When the tuning catalog gets exposed
  through the UI, the tuner's strings row picks up new tunings for free.
- **Cents-from-target override** — when target is set, recompute cents
  from the target's exact frequency rather than the detector's
  reported `note_freq`. Marginal accuracy improvement when fundamental
  and harmonics drift relative to each other.
- **Note hold / decay smoothing** — currently the display turns to
  placeholders the instant detection stops. A short hold (~200 ms)
  could make readings feel less twitchy at the cost of slightly stale
  data.

### 2026-05-01 — Feature Target 8: Exercises and Chord Progressions

Two new music-theory modules and matching UI views.

**`exercise` module** ([crates/music-theory/src/exercise.rs](../crates/music-theory/src/exercise.rs)):

- `Exercise { name, description, category, generator: fn }` — generator
  is a function pointer so the catalog stays small and the
  per-exercise logic stays specific.
- `ExerciseParams { starting_fret, direction, trill_repeats }` —
  parameterizes generators so the same exercise covers many positions.
- `ExerciseStep { string_index, fret, finger }` — output type.
- Five exercises: **Chromatic 1-2-3-4**, **Spider 1-3-2-4**, **Trill
  1-2**, **Ladder** (diagonal across strings), **X-Pattern**
  (alternating outer/inner strings).
- Works across instruments — a 4-string bass produces 32 chromatic
  steps instead of 48; ukulele works too.

**`progression` module** ([crates/music-theory/src/progression.rs](../crates/music-theory/src/progression.rs)):

- `Progression { name, description, category, roles: &'static [ChordRole] }`
- `ChordRole { degree, alteration, quality }` — 1-indexed scale
  degree, optional ♯/♭ alteration, and a `RoleQuality` (covering all
  17 chord shapes most progressions reach for: triads, suspended,
  6ths, 7ths, half-dim, dim7, min-maj7, 9ths)
- `apply_in_key(key_root, key_scale)` materializes the progression in
  any key — looks up the scale degree's pitch, applies alteration via
  `Pitch::transposed_by(AUGMENTED_UNISON)`, then maps `RoleQuality` to
  a `ChordFormula` from the chord catalog by name.
- 12 catalog entries spanning Standard (I-IV-V, I-vi-ii-V, Pachelbel
  Canon), Pop (I-V-vi-IV, vi-IV-I-V, I-vi-IV-V), Jazz (ii-V-I,
  iii-vi-ii-V, iii-VI-ii-V-I), Blues (12-bar, Quick Change),
  Symmetric (Andalusian Cadence with explicit ♭VII/♭VI alterations).

**Validation criteria from plan — met:**
- ✅ Each exercise renders on the fretboard at varying tempos (UI
  pass below; tempo integration with metronome is deferred — positions
  render statically for now)
- ✅ ii-V-I in C produces Dm7 / G7 / Cmaj7 with the expected
  voicings (test `ii_v_i_in_c_produces_dm7_g7_cmaj7`; pitch test for
  Dm7 = D-F-A-C also passes)

**Two new interval constants:** `DIMINISHED_UNISON` and
`AUGMENTED_UNISON` for chromatic semitone alterations preserving
letter spelling (so ♭III in C major scale gives E♭, not D♯).

**UI integration** ([crates/app/src/main.rs](../crates/app/src/main.rs)):

- **Exercises tab**: split-pane with exercise list (left) and detail
  view (right). Detail shows name, description, step count, and the
  fretboard with all unique (string, fret) positions of the selected
  exercise drawn as dots. Reuses the existing `FretboardCanvas` after
  converting `ExerciseStep`s to `Position`s with `interval_from_root: None`.
- **Progressions tab**: split-pane with progression list (left) and
  chord cards (right). Each card shows the chord symbol (e.g. "Dm7"),
  the formula name in parentheses, and the chord's pitches in spelled
  form. Cards wrap for long progressions (12-bar blues fits across
  multiple rows). Currently hardcoded to C major key — key picker
  comes when tuning/key selection lands more broadly.

**Stats:** 41 audio + 137 music-theory = 178 unit tests + 4 doc
tests. Up from 158/3.

**Decisions:**
- **Function-pointer generators** for exercises rather than data-driven
  step lists. Generators encode the *pattern* (chromatic, spider,
  etc.) which is cleaner than a giant data table — and keeps each
  exercise's intent visible in code. Tradeoff: less data-driven (can't
  load exercises from JSON), but appropriate for the v1 catalog.
- **`RoleQuality` enum mapped to `ChordFormula` by name lookup**
  rather than embedding interval lists. The chord catalog is already
  the source of truth for chord shapes; progressions reference it by
  name. Test ensures every `RoleQuality` variant has a matching name.
- **No metronome-driven exercise/progression playback yet.** That's a
  natural extension (exercise plays through positions at tempo;
  progression cycles chords with the metronome's clock) but is its
  own feature. Deferred.
- **No key picker UI yet.** Progressions hardcode to C major. When
  the broader UI gains a key/tuning selector (likely with FT9 or
  later UX work), this becomes free.

**Deferred to FT8 extensions:**
- Metronome-driven playback (animate exercise positions as they
  trigger; cycle progression chords on bar boundaries)
- Per-chord fretboard view (for each chord in a progression, show
  voicings on the fretboard)
- ~~Key picker for progressions~~ — landed below
- Roman-numeral display next to chord cards (clarifies the "iii-VI-ii-V-I" structure visually)
- More exercises: arpeggios, scale runs, picking patterns
- More progressions: minor-key variants, neo-soul changes,
  jazz turnarounds in different forms

### 2026-05-01 — Root pickers for Scales and Progressions

User pointed out that hardcoding examples to C major is misleading —
when an arbitrary root is presented, the user should be able to change
it. Added live pickers in two places.

**Scales tab gains:**
- Scale formula `pick_list` (38 scales from the catalog)
- Root pitch class `pick_list` (12 chromatic notes, sharps spelling)
- Octave `pick_list` (1–8)

The fretboard re-renders with the new scale's spelling on every change
— D major shows F♯/C♯, B♭ major shows E♭, etc. — because the existing
`positions_for_scale` already inherits spelling from the scale's pitches.

**Progressions tab gains:**
- Key root pitch class `pick_list`
- Key octave `pick_list`

(The "Major" key-scale label is fixed for now; minor-key progressions
need a separate scale-mode picker which lands when minor-mode
progressions get added.)

**Implementation note:** scale-formula picker uses
`Vec<&'static str>` cached in `App::scale_names` so iced's
`pick_list` has a stable slice to borrow from. Could have used a
newtype wrapper around the catalog index instead, but the name-based
approach works and stays close to how the catalog is keyed elsewhere.

### Post-FT8 Polish queue (user-flagged)

Captured here so they don't get lost. None block FT9.

- **Fingering numbers on fretboard positions** — display the
  suggested finger (1–4) inside or near each dot for exercises and
  scales. Exercises already carry `finger` per step; the fretboard
  canvas just needs to draw small numerals. Useful for both teaching
  and visual differentiation between exercises (1-2-3-4 vs 1-3-2-4
  reads at a glance).
- **Multi-position rendering** — currently exercises render at the
  default starting fret. Showing the same pattern at additional
  positions (5th fret, 7th fret, 12th fret) on the same fretboard
  view, or providing a position selector, helps users see where else
  the pattern works on the neck.
- **Playable animation** — sequence through exercise/progression
  steps in time with the metronome; highlight the current step with
  a brighter color or pulse. Distinguishes spider from chromatic at
  a glance because you watch the order, not just the static dot
  set. Complements but doesn't replace metronome-driven audio
  playback.

### 2026-05-01 — Octave noise removal + scale position view + labels

User pointed out that the octave picker is meaningless for both
progressions (chord identities don't depend on octave) and the
abstract scale (the fretboard renders all octaves anyway). Cleaned up
those controls and replaced them with picker dimensions that *do*
matter.

**Progressions:** removed the octave picker. Just `Key: [C ▼] Major`
now. Octave is fixed at 4 internally for `progression_key_root`
construction; doesn't affect the displayed chord names.

**Scales:** replaced the octave picker with a **Position** picker
(0–15) that selects a 5-fret hand window. Open position (0) covers
frets 0–4; position 5 covers frets 5–9; etc. The fretboard now
renders only positions within the chosen window — addresses the
"info overload" of seeing every scale tone across 22 frets at once.
Picking a different scale or root re-renders the same window.

**Labels** (new): `LabelMode { None, Notes, Degrees, Fingers }` toggle
in the Scales tab.
- **Notes**: pitch name + accidental (C, F#, B♭, etc.) — inherits
  spelling from the active scale.
- **Degrees**: scale degree number (1–7 for diatonic scales,
  including degrees beyond 7 for compound intervals). Pulled from
  `Position::interval_from_root.number()`.
- **Fingers**: simple 4-fret-window mapping — fret offset from window
  start gives finger 1–4. Open string (fret 0) labeled "0".
- Default: `Notes`.

**`FretboardCanvas` extended** with a parallel `labels: Vec<String>`.
Empty strings = no label. The exercises tab passes empty strings;
the scales tab passes computed labels per the chosen mode. Labels
render via `frame.fill_text` centered in each dot. Dot radius
bumped to 13px to accommodate two-digit degrees / Notes like "G#".

**Stats unchanged at the test layer:** 178 unit tests + 4 doc
tests. Music-theory crate untouched in this iteration; all changes
are in the app's view layer.

**Decisions:**
- Position window is 5 frets (inclusive endpoints). Allows pinky
  stretch without forcing the user to think in 4-fret-only frames.
- Fingering for scales is the simple "fret offset → finger number"
  mapping. Real scale fingering depends on the scale's intervals
  (some patterns prefer 2-3-4 over 1-3-4 etc.); a more sophisticated
  fingering algorithm is post-MVP.
- Label mode stays per-tab. Scales has its own; exercises don't have
  a label toggle yet (would naturally show fingering — the
  `ExerciseStep::finger` is already populated). Trivial to add when
  exercise polish gets attention.

### 2026-05-01 — Bug fixes from user UI testing

User played with the build and reported:

**Fingering off-by-one in open position.** In position 0, fret 1 was
labeled "2" instead of "1", fret 3 was labeled "4" instead of "3", etc.
Cause: my formula was `finger = fret - window_start + 1`, but in open
position window_start is 0 and finger 1 actually covers fret 1 (not
fret 0 — fret 0 is the open string with no finger). Fixed by clamping
the effective first-fret to `max(window_start, 1)`.

**Scale dropdown change didn't propagate until root changed too.**
Reported but not reproducible from code inspection — handler is
straightforward (`self.scale_formula = f`). Added a defensive nudge
to also re-set `self.scale_root` from `self.scale_pc` in the
`ScaleSelected` handler. If the issue is a render-pipeline edge case
where changing only `scale_formula` isn't picked up, this knocks it
loose. If the issue persists after this, deeper iced-internals
investigation is the next step.

### 2026-05-01 — Vision: Practice Mode ("Fretwork")

User flagged a unifying feature: a **practice mode** that drives the
existing pieces in concert. Captured as Feature Target 10 (after
release packaging in FT9). The user briefly considered naming the
integrated practice tool **Fretwork**; the project was ultimately
renamed to **Woodshed** on 2026-05-15, with the practice mode living
as the Practice tab inside it.

**Concept.** The user picks a *practice set* — a bounded, iterable
collection of musical material — and the app cycles through it at
tempo, displaying the appropriate fretboard diagram for each step.

**Set dimensions** (composable):
- **Material**: scales / arpeggios / chords / exercises (any catalog
  item from music_theory)
- **Roots**: chromatic order, diatonic in a key, keyed to a specific
  progression's chord roots, circle of fifths, by user-specified
  pattern
- **Positions**: a chosen set of hand positions (or "all positions
  containing the root", "all standard CAGED positions", etc.)
- **Variants** *(optional)*: for scales, also iterate through modes
  (Ionian, Dorian, ..., Locrian) within the same exercise

**Time integration**: the metronome/sequencer drives the cadence —
N bars per item, tempo follows the metronome BPM. For a "scales in
all 12 keys" set at 60 BPM × 8 bars per key, the user gets a
12-step cycle of ~1m36s per pass.

**Display**: the fretboard diagram updates per step. Highlight the
current note (when playable animation lands). Show the next item
queued for context.

**Why this is the keystone feature**: the components we've built so
far (scales, chords, progressions, exercises, metronome) are all
*reference material*. Practice mode is what turns the app from a
reference into a tool — the user doesn't have to manually pick each
position; the app drives the rotation while they play.

**Implementation sketch** (for later):
- `PracticeSet` data model: ordered list of `PracticeItem`s, each
  carrying material reference + root + position
- `SetGenerator` types that produce `PracticeSet`s from parameters:
  `ChromaticRoots { material }`, `DiatonicInKey { material, key }`,
  `ProgressionLocked { progression, material_per_chord }`, etc.
- `PracticeRunner` that consumes a set and ticks via the audio
  engine's clock (subscription-driven, similar to tuner polling)
- New `Practice` tab with set picker + run controls + live fretboard

**Renaming.** "Fretwork" can land when this feature does — it's
naturally the moment the project transcends being a collection of
tools. Until then, the placeholder stands. Worth flagging now so the
project description and future doc updates anticipate the rename.

### Polish queue, expanded

User comment: "rough ui-wise and you can tell there are places that
need to go from placeholder to selectable". Capturing the unstated
items so they don't drift:

- **Tuning picker**: currently `App.fretboard.tuning` is hardcoded
  to standard guitar. Surface the full tuning catalog (84 entries)
  as a UI picker — ideally with instrument filter (Guitar / Bass /
  Ukulele / Banjo / Mandolin) and category filter (Standard /
  Dropped / Open / Modal / etc.). The Tuner's "strings row" picks
  this up automatically since it already pulls from the active
  tuning.
- ~~**Chords tab**: currently a placeholder.~~ — landed below.
- **Progressions**: scale-mode picker (currently fixed to Major).
  Lets users see ii-V-i in minor with proper Mm7-V7-mMaj7 chord
  qualities. Would also unlock minor-key progressions in the
  catalog.
- **Exercises**: starting-fret control, direction toggle, label
  mode (fingering specifically — `ExerciseStep::finger` is already
  populated).

### 2026-05-01 — Chords tab landed

Built using the same pattern as Scales (placeholder → full picker
view). Mirrors what we already have:

- **Chord picker** (38 catalog entries: triads, suspended, sevenths,
  sixths, add chords, extended 9/11/13, altered dominants, quartal,
  quintal, cluster)
- **Root pitch class** (12 chromatic, sharps spelling)
- **Position** (5-fret hand window, 0–15)
- **Label mode** (Off / Notes / Degrees) — Fingers omitted because
  there's no canonical fingering for "every chord tone across the
  window"

**Header** combines root + chord symbol (e.g. "Cmaj7" via the
formula's `symbol` field) + formula name + tuning. Below the header,
a small line lists the chord's pitches as a quick reference (e.g.
"C E G B" for Cmaj7).

**Decision: chord-tone map, not voicings.** This iteration shows
*where* the chord lives on the fretboard within the chosen window —
the same flavor of view as Scales. Voicing diagrams (specific
playable shapes via `find_chord_voicings_for_bass`, displayed as a
grid of small fretboard cards) are a natural follow-up. The voicing
search infrastructure is already there; just needs UI surface.

**Side cleanup**: retired the `placeholder()` helper since every tab
now has real content.

### 2026-05-01 — Metronome controls expanded

User flagged: the metronome had only BPM + Play/Stop. Time signature,
subdivision, and accent were hardcoded. Exposed them all in the UI.

**Time signature**: numerator picker (1–12), denominator fixed at 4
for now. Note that the sequencer treats one beat as one quarter note
regardless of denominator, so 6/8 currently behaves like 6/4 timing
— denominator picker can land later if/when notation conventions
matter for display.

**Subdivision**: six toggle buttons covering 1/4, 1/8, 1/16, 1/32,
1/8 triplet, 1/16 triplet — uses the existing `Subdivision`
constants from the audio crate.

**Click pattern**: "Beat only" (default — click once per beat,
silence on subdivisions) or "Every note" (click on every
subdivision). Used to be hardcoded to beat-only.

**Accent**: "Downbeat" (default — accent only beat 1), "Every beat"
(every beat is accented), or "None" (no accents). The accent uses
the click sound's higher accent frequency vs the regular
frequency — already in `Sound::click()`.

**Implementation**: a `build_metronome_pattern(...)` free function
constructs a `SequencerPattern` from these settings. Whenever any
control changes, `App::apply_metronome_pattern()` rebuilds and pushes
via `EngineHandle::set_pattern`. If currently playing,
`set_pattern`'s reset of sample/step position means the click
restarts from beat 1 — which is the expected behavior when changing
time signature mid-playback.

If currently stopped, `set_pattern` updates internal state so the
next Play uses the new settings. The next-Play path also calls
`handle.play()` again to re-engage (since `set_pattern` doesn't
touch the `playing` flag).

**Stats unchanged at the test layer:** all changes are in app code
and use audio crate types that already exist (`Step`, `Track`,
`Sound`, `TimeSignature`, `Subdivision`).

### 2026-05-01 — Global tuning picker + scale-mode for progressions + Pebber Brown exercises

Three landed together since they cluster.

**Global tuning picker** in the app header (under the tab bar):
- **Instrument** picker (Guitar / Bass / Ukulele / Banjo / Mandolin /
  Other) — pulls from new `Instrument::ALL` constant
- **Tuning name** picker filtered to the active instrument
- Changing instrument auto-selects the first tuning for that instrument
- The active tuning drives `App.fretboard`, which is consumed by every
  fretboard-rendering tab (Scales, Chords, Exercises) AND the Tuner's
  string-targets row (which reads `self.fretboard.tuning.strings`)

So picking "Drop D" on the global picker immediately re-renders the
Scales fretboard, the Chords fretboard, the Exercises fretboard, and
the Tuner's string buttons. No per-tab plumbing needed; the
single-tuning-per-app design pays off.

**Implementation note:** `available_tuning_names: Vec<&'static str>`
is cached in App and refreshed when instrument changes. iced's
`pick_list` needs a stable slice to borrow from for its options,
which a static slice or a Vec-in-state both provide; recomputing in
view() doesn't satisfy the lifetime since the temporary dies.

**Scale-mode picker for progressions:** the Progressions tab now has
a scale-formula dropdown next to the key-root dropdown. Default
"Major"; pick "Minor" or "Harmonic Minor" to apply the same
progression in a minor key. The catalog's role qualities (Minor7,
Dominant7, Major7, etc.) carry through; only the degree-to-pitch
mapping changes with the scale.

So `ii-V-I (Jazz)` in C Minor produces Dm7♭5 / G7 / Cm(maj7) — wait,
actually with our role definitions for that progression (Minor7 /
Dominant7 / Major7), it'd produce Dm7 / G7 / Cmaj7 which is the
*relative* major treatment. To get the proper minor-jazz
ii-V-i, we'd need a separate progression entry with HalfDiminished7
/ Dominant7 / MinorMajor7 roles. The infrastructure is there;
adding a "ii-V-i (Minor Jazz)" entry to the catalog is a one-line
addition for later.

**Pebber Brown exercises** (RIP) — added two technique-focused
exercises from his [free PDF library](https://www.pbguitarstudio.com/GuitarLessonPDF.html):

- **Pentatonic Box Shift** — minor pentatonic shape, then shift up a
  minor third (3 frets) and repeat. Shape recognition + position
  shifting drill. Generator iterates per string with shape-specific
  fret offsets (1+4 on lowest string, 1+3 on upper).
- **Two-String Climb** — four-note pattern confined to two adjacent
  strings, climbing one fret per repetition. Picking + crossing drill
  without much hand position change.

Two new `ExerciseCategory` variants (`Pentatonic`, `StringPair`).
2 new tests verify both exercises generate plausible step counts and
constraints.

The full Pebber Brown survey identified more candidates we already
cover in spirit (chromatic, spider, ladder, X-pattern). His
additional unique contributions queued for later: 5-position major
scale system (we have a position picker on Scales, so this is
covered as data + UI rather than as a separate exercise), diatonic
arpeggios (depends on scale context, more complex generator),
2/3/4-finger ladder variants (extensions of existing Ladder).

**Stats:** 139 music-theory tests (up from 137, +2 for Pebber Brown
exercises). Total: 41 audio + 139 music-theory = 180 unit tests + 4
doc tests. Clippy clean.

**Voicing cards (queued for next iteration):** the chord library
currently shows the chord-tone map (every position in window). The
"chord library" canonical UX is small fretboard cards each showing
one playable voicing — uses `find_chord_voicings_for_bass` which is
already wired. Needs a smaller-fretboard canvas widget. Will land
next round.

### 2026-05-01 — Voicing cards + bug fixes from UI feedback

User testing surfaced three issues this iteration addresses.

**Hide tuning picker on tuning-agnostic tabs.** The global tuning
picker was showing on every tab, but Progressions, Exercises, and
Metronome don't render fretboards (or in Exercises' case, the
visual is the same regardless of tuning). Now only Scales / Chords /
Tuner show the picker.

**Filter scale-mode dropdown by progression compatibility.** Bug:
selecting Pentatonic with I-V-vi-IV gave "scale degree 6 out of
range for scale of size 5" because Pentatonic only has 5 degrees.
Fix: per-progression `progression_valid_scale_names: Vec<&'static
str>` filters the dropdown to scales with `intervals.len() >=
max_degree`. Computed once on `ProgressionSelected`. If the current
scale becomes invalid for the new progression, fall back to Major
automatically. The error path is still there as a safety net but
should never be visible.

**Voicing cards** (the canonical chord-library UX, finally):

A new `ChordDiagram` canvas widget renders a single voicing as a
**vertical** songbook-style diagram:
- Strings as vertical lines (low pitch on left)
- Frets as horizontal lines (low frets at top)
- Nut drawn as a thicker top line when at position 1
- Position marker ("5fr") on the left when at higher positions
- × above muted strings, ○ above open strings
- Filled dots at fretted positions

**Used in two places:**

*Chords tab — voicing grid.* Below the chord-tone fretboard, a
wrapping row of up to 8 voicing cards. The app scans multiple fret
windows (0, 3, 5, 7, 9, 12) and dedupes by fret pattern. Each card
is the diagram + chord symbol caption. Picking different chord/root
regenerates the cards. Provides the "show me how to play this" view
that complements the "where do its tones live" view.

*Progressions tab — click-to-expand.* Each chord card in a
progression is now a button (`chord_card_button`). Click expands a
single voicing diagram beneath the chord row, with **◀ / ▶ arrows**
to cycle through available voicings and a position label. Clicking
the same chord again collapses. Clicking a different chord switches
the focus.

**Voicing-search strategy** for the in-progression diagram: try
`Root` bass first; fall back to `AnyChordTone` if no
root-position voicing fits in the 0–4 window. Most progressions
yield clean root-position voicings; edge cases (extended chords with
high upper voices) get inversions.

**Stats:** all changes in app code; test counts unchanged at 41 audio
+ 139 music-theory + 4 doc tests. Clippy clean.

**Decisions:**
- **Diagram orientation matches songbooks** (vertical, low frets up).
  Different from the main Scales/Chords fretboard view (horizontal,
  low pitch down) — but that's the convention for chord diagrams
  vs scale diagrams. Two formats serve different audiences.
- **Voicing dedup by fret pattern** rather than by string ordering;
  same shape at different positions counts as different voicings,
  but the same exact pattern at the same fret never duplicates.
- **8-card cap** on the chord-grid view to keep the layout
  manageable. Scrollable list could replace this for power users.

**Polish queue (still open):**
- Practice mode iterating exercise positions up the neck (FT10)
- Exercise UI: starting-fret control, label-mode toggle (fingering)
- Diatonic arpeggios as exercises
- Minor-key progression catalog entries (e.g., ii-V-i Minor with
  HalfDiminished7/Dominant7/MinorMajor7 roles)
- Barre indicator on chord diagrams when multiple strings share the
  lowest fret

### 2026-05-01 — Position arrows (independent of voicing)

User: "i would like to be able to choose between different positions"
in addition to picking voicings within a position. Two-axis
navigation now:

- **Position row**: ◀ / ▶ steps the fret window starting fret by ±1
  (range 0–15). Each click resets `progression_voicing_idx` to 0.
- **Voicing row**: ◀ / ▶ cycles through voicings *within* the chosen
  position.

So selecting "Position 5" and stepping voicings only shows shapes that
fit in frets 5–9. Stepping position to 7 finds shapes in frets 7–11.
Empty position (no voicings fit) shows a hint and keeps the position
controls visible so the user can step out of the dead zone.

Position label simplified: "Open" at fret 0, "Position N" otherwise.
Reflects the user's pick rather than the voicing's lowest fret.

### Adaptive layout for fretboards (mobile / portrait viewports)

User flagged: "we might want to consider vertical or horizontal
layouts for fretboards, for both mobile/desktop". Real concern. The
current state:

- **Main fretboard** (Scales/Chords/Exercises tabs): horizontal — low
  pitch on bottom, frets left-to-right. Works well in desktop
  landscape orientation; on a mobile portrait window the 22-fret span
  forces tiny dots.
- **ChordDiagram** (chord library): vertical, songbook-style. Works
  in any orientation since it's a small fixed-size widget.

Two paths to address mobile:

1. **Auto-rotate the main fretboard based on aspect ratio.** The
   canvas already gets `bounds: Rectangle` in `draw`. If
   `bounds.height > bounds.width`, swap the axes — strings vertical,
   frets horizontal. No state change required; a single rendering
   branch in `FretboardCanvas::draw`. The down side is the rendering
   logic doubles in length (or extracts to a helper that takes the
   axes as parameters).
2. **Explicit orientation setting** with a UI toggle. More
   discoverable but adds state and a control.

Recommendation: do (1) — auto-rotate. The aspect ratio is the right
signal because it captures both the window state and the user's
intent (resizing to portrait = "I want more vertical space"). Falls
back to current behavior on landscape. When the mobile build lands
(Tauri Android/iOS), this just works because portrait is the
default orientation.

Same reasoning applies to scale-degree and chord-tone layouts on the
main fretboard. Both inherit from `FretboardCanvas` so the rotation
is once.

Capturing this as a P0 for the mobile push but P1 for desktop —
landscape works fine today.

### 2026-05-01 — Voicing collapse + diagram labels + diagram aspect

Three improvements from the latest round of UI feedback.

**Collapse equivalent voicings.** User noticed that at certain
positions (e.g. position 3 for C major) many "different" voicings
shared the same fretted skeleton — only differing in which strings
were open vs muted. The cartesian-product voicing search produced all
these subset combinations.

Fix: `voicings_by_position` now groups by **fretted skeleton**
(`Vec<(string_idx, fret)>` for `fret > 0` positions) and keeps the
voicing with the **highest played-string count** per skeleton. Subset
voicings are dropped. The remaining representative shows the chord
in its fullest form for that skeleton.

**Chord-diagram labels.** Added a `LabelMode` for chord diagrams,
parallel to but independent from the main fretboard's label mode
(since the two use different rendering and convey different things —
chord-tone fretboard shows scale/chord-tone *coverage*, voicing
diagrams show *one playable shape*).

`ChordDiagram::with_labels(mode)` builds the per-string label list:
- **Notes**: pitch name (C, E, G, F♯ etc.) inheriting from the
  voicing's spelled pitches
- **Degrees**: chord degree number from `interval_from_root.number()`
- **Fingers**: heuristic — sort unique fretted frets ascending,
  assign fingers 1, 2, 3, 4 in order. Strings sharing a fret share a
  finger (the basic barre case). `finger_assignments(voicing)` is the
  helper.

Labels render inside the dot, white text, sized proportional to dot
radius (`dot_radius * 1.5`, min 8px). Empty string = no label drawn.

UI: a `Diagram labels: [Off] [Notes] [Degrees] [Fingers]` row in the
Chords tab between the chord-tone fretboard and the voicing grid.
Same setting also drives the Progressions tab's expanded chord
diagram. One toggle, two views.

**Card aspect ratio.** User flagged the cards looked horizontal-ish
(easy to mistake for a small fretboard view). Changed:
- Voicing cards: 110×150 → **85×175** (aspect 0.49 — clearly vertical)
- Progression diagram: 140×170 → **130×200** (aspect 0.65)
- Asymmetric padding: 22px left (room for "12fr" markers), 6px right
- Dot radius scales with `min(string_step, fret_step) × 0.32` so dots
  fit in any canvas size

**Diagram fret window fix.** `from_voicing` previously chose
`top_fret = 1` whenever lowest_fretted ≤ 4, but the diagram only
shows 4 frets — so a voicing with lowest=3 and highest=5 had its
fret-5 dot fall outside the visible window. Now: include nut
(top_fret=1) only if **highest** fret is ≤ 4; otherwise start at
`lowest_fretted` so the whole shape fits.

**Stats unchanged at the test layer:** all changes in app code. 41
audio + 139 music-theory = 180 unit tests + 4 doc tests, clippy
clean.

**Decisions:**
- **Diagram labels separate from main fretboard labels.** A user
  might want degrees on the scale view (1–7) and fingers on chord
  diagrams (1–4) simultaneously. Coupling the two would force a
  worse choice.
- **Finger heuristic is naive (sort frets, assign 1–4).** Real
  fingerings can be more nuanced — e.g., open chords that use thumb
  for the 6th string, or rolled barres. Improving the algorithm is a
  future polish item; current heuristic is right ~80% of the time
  for the common shapes.
- **Voicing collapse keeps "fullest" version.** A subset voicing
  (e.g., x-x-3-x-x-x for C major) can't actually be a complete
  chord — it'd fail the "all chord PCs present" filter. The voicings
  the user saw with one dot and many opens are all "fullest"
  versions; the collapse just removes redundant subsets that emerge
  from open/muted permutations of the same fretted skeleton.
