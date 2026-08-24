//! Arpeggio lens logic (ported from woodshed-xilem's `arpeggios_view`,
//! S4 migration slice 1).
//!
//! An arpeggio's notes are a chord's tones: the quality comes from the
//! chord catalog and the shared root. Shapes are CAGED-style windows
//! anchored a fret below each low-string occurrence of the **bass tone**
//! (the inversion's starting chord tone); the transport run is the active
//! shape's notes ordered by pitch ascending from the bass, walked
//! up / down / ping-pong.

use woodshedding::chord::ChordFormula;
use woodshedding::fretboard::{Fretboard, Position};
use woodshedding::interval::Interval;
use woodshedding::pitch::Pitch;

pub use woodshedding::rehearsal::ArpeggioDirection;

/// Playable-window span in frets (a shape covers
/// `start_fret ..= start_fret + ARP_SHAPE_SPAN`).
pub const ARP_SHAPE_SPAN: u8 = 4;

/// One CAGED-style position shape: a bass-anchored fret window's chord
/// tones.
#[derive(Clone, Debug)]
pub struct ArpeggioShape {
    pub start_fret: u8,
    pub positions: Vec<Position>,
}

/// Generate the position shapes for `formula` rooted at `root`, anchored
/// on `bass` (the inversion's starting tone). Ported verbatim from
/// woodshed-xilem `generate_arpeggio_shapes`.
pub fn generate_shapes(
    fretboard: &Fretboard,
    formula: &ChordFormula,
    root: Pitch,
    bass: Interval,
) -> Vec<ArpeggioShape> {
    let all = fretboard
        .positions_for_chord(formula, root)
        .unwrap_or_default();
    // Anchor frets: a fret below each occurrence of the bass tone on the
    // lowest two strings, within a playable stretch of neck.
    let mut anchors: Vec<u8> = all
        .iter()
        .filter(|p| p.interval_from_root == Some(bass) && p.string_index <= 1 && p.fret <= 15)
        .map(|p| p.fret.saturating_sub(1))
        .collect();
    anchors.sort_unstable();
    anchors.dedup();
    if anchors.is_empty() {
        // Bass tone isn't on the low strings in range — fall back to any
        // low occurrence so the lens still shows something.
        anchors = all
            .iter()
            .filter(|p| p.interval_from_root == Some(bass) && p.fret <= 15)
            .map(|p| p.fret.saturating_sub(1))
            .collect();
        anchors.sort_unstable();
        anchors.dedup();
    }
    if anchors.is_empty() {
        anchors.push(0);
    }

    // A complete box wants at least min(chord-tones, 3) distinct pitch
    // classes so a near-empty window doesn't masquerade as a shape.
    let want_pcs = formula.intervals.len().min(3).max(1);
    let mut shapes: Vec<ArpeggioShape> = Vec::new();
    for lo in anchors {
        let hi = lo + ARP_SHAPE_SPAN;
        let positions: Vec<Position> = all
            .iter()
            .filter(|p| p.fret >= lo && p.fret <= hi)
            .cloned()
            .collect();
        let distinct_pcs = {
            let mut pcs: Vec<u8> = positions.iter().map(|p| p.pitch.pitch_class()).collect();
            pcs.sort_unstable();
            pcs.dedup();
            pcs.len()
        };
        if positions.len() >= 4 && distinct_pcs >= want_pcs {
            shapes.push(ArpeggioShape {
                start_fret: lo,
                positions,
            });
        }
    }
    if shapes.is_empty() {
        shapes.push(ArpeggioShape {
            start_fret: 0,
            positions: all,
        });
    }
    shapes
}

/// The transport run over one shape: `seq` is indices into the shape's
/// positions ordered by pitch ascending from the bass tone; `walk` is the
/// step order over `seq` per direction (UpDown ping-pongs without
/// repeating the turnaround notes).
#[derive(Clone, Debug, Default)]
pub struct ArpeggioRun {
    pub seq: Vec<usize>,
    pub walk: Vec<usize>,
}

impl ArpeggioRun {
    pub fn new(positions: &[Position], bass: Interval, direction: ArpeggioDirection) -> Self {
        let mut seq: Vec<usize> = (0..positions.len()).collect();
        seq.sort_by_key(|&i| (positions[i].pitch.midi(), positions[i].string_index));
        let inv_start = seq
            .iter()
            .position(|&i| positions[i].interval_from_root == Some(bass))
            .unwrap_or(0);
        if inv_start > 0 && inv_start < seq.len() {
            seq.drain(0..inv_start);
        }
        let n = seq.len();
        let walk: Vec<usize> = match direction {
            _ if n == 0 => Vec::new(),
            ArpeggioDirection::Up => (0..n).collect(),
            ArpeggioDirection::Down => (0..n).rev().collect(),
            ArpeggioDirection::UpDown => {
                let mut v: Vec<usize> = (0..n).collect();
                if n > 1 {
                    v.extend((1..n - 1).rev());
                }
                v
            }
        };
        Self { seq, walk }
    }

    pub fn walk_len(&self) -> usize {
        self.walk.len().max(1)
    }

    /// The position index (into the shape's positions) under the cursor.
    pub fn position_at(&self, step: usize) -> Option<usize> {
        if self.walk.is_empty() {
            return None;
        }
        Some(self.seq[self.walk[step % self.walk.len()]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use woodshedding::chord::catalog as chord_catalog;
    use woodshedding::pitch::Spelling;
    use woodshedding::tuning::{catalog as tuning_catalog, Tuning};

    fn board() -> Fretboard {
        Fretboard::new(Tuning::from_spec(&tuning_catalog()[0]), 12)
    }

    fn major() -> &'static ChordFormula {
        chord_catalog()
            .iter()
            .find(|c| c.name == "Major")
            .expect("Major chord")
    }

    #[test]
    fn shapes_generate_and_are_windowed() {
        let root = Pitch::from_midi(57, Spelling::Sharps); // A
        let shapes = generate_shapes(&board(), major(), root, Interval::PERFECT_UNISON);
        assert!(!shapes.is_empty());
        for s in &shapes {
            assert!(s
                .positions
                .iter()
                .all(|p| p.fret >= s.start_fret && p.fret <= s.start_fret + ARP_SHAPE_SPAN));
        }
    }

    #[test]
    fn run_ascends_from_bass_and_ping_pongs() {
        let root = Pitch::from_midi(57, Spelling::Sharps);
        let shapes = generate_shapes(&board(), major(), root, Interval::PERFECT_UNISON);
        let positions = &shapes[0].positions;
        let run = ArpeggioRun::new(
            positions,
            Interval::PERFECT_UNISON,
            ArpeggioDirection::UpDown,
        );
        assert!(!run.seq.is_empty());
        // First note of the run is the bass tone (unison from root).
        let first = run.position_at(0).unwrap();
        assert_eq!(
            positions[first].interval_from_root,
            Some(Interval::PERFECT_UNISON)
        );
        // seq is pitch-ascending.
        let midis: Vec<i32> = run.seq.iter().map(|&i| positions[i].pitch.midi()).collect();
        assert!(midis.windows(2).all(|w| w[0] <= w[1]));
        // UpDown walk length = 2n - 2 for n > 1.
        let n = run.seq.len();
        if n > 1 {
            assert_eq!(run.walk.len(), 2 * n - 2);
        }
    }

    #[test]
    fn down_walk_starts_at_top() {
        let root = Pitch::from_midi(57, Spelling::Sharps);
        let shapes = generate_shapes(&board(), major(), root, Interval::PERFECT_UNISON);
        let positions = &shapes[0].positions;
        let run = ArpeggioRun::new(positions, Interval::PERFECT_UNISON, ArpeggioDirection::Down);
        let first = run.position_at(0).unwrap();
        let top_midi = run
            .seq
            .iter()
            .map(|&i| positions[i].pitch.midi())
            .max()
            .unwrap();
        assert_eq!(positions[first].pitch.midi(), top_midi);
    }
}
