# Song Mode — Integration Design

Captures the integration surface for the [`Song`] data model and the
[`chord_audio`] renderer now that both have shipped as pure types
with full tests. This doc is the runway for the engine wiring and UI
gestures.

---

## What's done

- **`woodshed_audio::Song`** — multi-bar arrangement with cursor,
  pending-change queue (SR-16 pattern), and add/remove/copy/paste/
  move operations. 13 tests.
- **`woodshed_audio::Bar`** — per-bar BPM, time signature, optional
  `audio_buffer` (the looper layer), optional `chord_ref`.
- **`woodshed_audio::ChordRender` + `render_chord()`** — pure
  additive-synthesis chord renderer. Sqrt-N loudness compensation,
  ADSR envelope, optional strum offset. Produces a `SampleBuffer`. 7
  tests.
- Bar-boundary advance: `Song::advance(frames, sr)` walks the cursor
  forward, applying queued changes when boundaries cross. Loops at
  end (or stops, if `one_shot`).
- Serde: `Song`, `Bar`, `ChordRef`, `SongCursor` all serializable.
  Audio buffers are `#[serde(skip)]` — see "Save format" below.

## What's pending

Three remaining layers, in dependency order:

### 1. Engine integration

The audio engine (`SequencerEngine` / `process_buffer`) is currently
single-pattern. Song integration wraps it:

```rust
pub struct SongEngine {
    state: Arc<Mutex<SongState>>,
    _stream: Stream,
}

struct SongState {
    song: Song,
    /// Per-bar prerendered audio (chord + loop layers mixed).
    /// Refreshed lazily on first play of each bar.
    bar_caches: Vec<Option<SampleBuffer>>,
    /// Looper input — set by InputEngine fanout, consumed when
    /// `song.recording` is true to fill the current bar's buffer.
    input_ring: VecDeque<f32>,
}
```

Per-frame loop in `process_buffer`-style:
1. Read `song.cursor` to know which bar + sample.
2. If `song.recording && song.playing`, read input from the input
   fanout's shared ring and write into the cursor bar's audio_buffer.
3. Pull the bar's mixed audio (cached: chord + existing loop) at the
   cursor sample. Write into output.
4. Call `song.advance(frames, sample_rate)` — handles boundaries.

**Why a cache per bar?** Chord audio is rendered once per bar and
reused on every loop pass. Re-rendering each pass would mean a
synthesis pass per bar per loop, which is wasteful (chords don't
change samples between iterations).

### 2. Input fanout integration

The `Looper` data type stays — single-bar use. For Song's per-bar
recording, we don't need a separate type; the audio capture path is:

```
InputEngine (cpal callback)
  └─ pushes samples to a shared ring
     └─ SongEngine (output callback) drains ring into
        cursor bar's audio_buffer when recording is armed
```

This means **the input fanout grows one more analyzer** —
`LooperCaptureAnalyzer` — that just buffers samples for the
SongEngine to consume. Cheap, no DSP.

### 3. UI

The big one. Three new screen elements:

**Bar list strip.** Horizontal scrollable strip showing each bar as
a card. Each card shows:
- Bar number, label (if any), tempo (if non-default), time sig.
- Compact chord name (if `chord_ref.is_some()`).
- Tiny indicator dots for "has audio buffer", "currently playing
  (cursor here)", "armed for next record".

Click a card to select it. Selected card highlights; selection drives
the per-bar editor.

**Per-bar editor.** Below the strip. Shows:
- Tempo input (number + slider).
- Time signature picker.
- Chord picker (root note + formula dropdown — uses the existing
  chord catalog, calls `chord_audio::render_chord` for preview).
- "Record into this bar" button (queues `PendingChange::StartRecording`).
- "Clear audio" button.
- Bar label text input.

**Transport row.** Standalone Play / Stop / Record buttons that
queue song-level pending changes. Per Mark's design call: **record
is separate from play**. Play just transports through the song;
record arms capture into the cursor bar at the next boundary.

Add / remove / copy / paste live as small buttons on the bar list
itself — `+` to add at end, `X` per card to remove, right-click menu
for copy/paste, drag-to-reorder for `move_bar`.

## Save format

Songs save as JSON via serde. The JSON is *self-contained metadata*
— bars, tempos, chord refs, labels — but **audio buffers are not
embedded**.

Two reasonable patterns for the audio:

- **Sidecar `.wav` per bar.** A `.song` directory containing
  `song.json` plus `bar_0.wav`, `bar_3.wav`, etc. (only bars with
  audio). On load, reconstruct buffers from the sidecar files.
  Friendly to inspect / share / edit one bar's audio externally.
- **Single bundle**. Zip the JSON + WAVs into a `.wsong` archive.
  Tidier on disk; opaque to inspection.

Sidecar wins for V1 — easier to debug, easier to share, no archive
dependency.

## Chord audio reuse

`render_chord(ChordRender, sample_rate) -> SampleBuffer` is generic.
It already serves Song's per-bar chord playback. The other natural
consumer is **chord cards** in the existing Chords / Progressions
tabs — click a card to hear the chord.

App-side translation (woodshedding theory → audio crate's pitch
list):

```rust
fn chord_pitches_hz(
    formula: &ChordFormula,
    root: Pitch,
) -> Vec<f32> {
    let mut out = vec![root.frequency_hz()];
    for interval in &formula.intervals {
        let p = root.transpose_up(*interval);
        out.push(p.frequency_hz());
    }
    out
}
```

(This requires `frequency_hz()` on Pitch — already exists in
woodshedding.) The app builds the Vec<f32>, calls
`render_chord(ChordRender::strum(pitches, 1.0), 48_000)`, then either
plays the resulting `SampleBuffer` directly (via a one-shot Sound
trigger) or stashes it in a Song bar's `audio_buffer`.

## Decisions still open

1. **Tempo transition at bar boundaries.** Hard step or smooth ramp
   over the previous bar's last beat? Hard step is honest (the song
   says "bar 5 is at 90 BPM") and matches the SR-16; ramp is more
   musical. *Recommendation: hard step for V1, surface a ramp toggle
   later if anyone asks.*
2. **Per-bar subdivision.** Currently `Bar` stores its own
   `Subdivision`. Should the click also change per-bar
   (eighth-notes on bar 1, sixteenths on bar 2)? Yes — the field
   exists, the engine just needs to honor it. Mostly a UI question
   ("subdivision picker per bar" or "song-wide").
3. **Selection vs. cursor.** The "selected bar" (UI focus for the
   editor) and the "playback cursor" (where audio is) are different
   things. Make them visibly distinct (e.g. cursor = filled
   highlight, selection = outline). Both should be clickable.
4. **Multi-bar copy.** Copy one bar (`copy_bar`) is shipped. Copy
   a range is a follow-up — the data model permits it trivially,
   but the UI gesture is more work (shift-click to multi-select).

## Out of scope reaffirmed

Confirmed by Mark, restating for record:

- **No per-track recording.** One audio buffer per bar, period.
- **No real-time effects.** Recorded audio is the dry signal as
  captured.
- **No MIDI export of songs.** Audio export only (already supported
  via `export_wav`).

If any of these come up later, they're separate features.
