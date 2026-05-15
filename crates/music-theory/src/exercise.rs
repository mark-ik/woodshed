//! Practice exercises: ordered fretboard positions for chromatic,
//! spider, trill, ladder, and X-pattern exercises.
//!
//! An [`Exercise`] is a named generator that, given a [`Tuning`] and
//! [`ExerciseParams`], produces an ordered list of [`ExerciseStep`]s.
//! Steps are intended to be played in sequence at a steady tempo.
//!
//! Exercises are parameterized — the same chromatic exercise can run
//! starting on fret 1, 5, 7, etc. — so the catalog stays small while
//! covering many practical variations through parameters.

use crate::tuning::Tuning;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExerciseCategory {
    Chromatic,
    Spider,
    Trill,
    Ladder,
    XPattern,
    /// Patterns inspired by Pebber Brown's curriculum (RIP).
    /// pbguitarstudio.com — extensive free PDF library on technique.
    Pentatonic,
    StringPair,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExerciseStep {
    pub string_index: usize,
    pub fret: u8,
    /// Suggested fingering (1–4); 0 = unspecified.
    pub finger: u8,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExerciseDirection {
    Ascending,
    Descending,
    Both,
}

#[derive(Copy, Clone, Debug)]
pub struct ExerciseParams {
    /// Lowest fret of the four-fret hand position.
    pub starting_fret: u8,
    pub direction: ExerciseDirection,
    /// For trill: how many alternations to produce per string.
    /// Ignored by other exercises.
    pub trill_repeats: u8,
}

impl Default for ExerciseParams {
    fn default() -> Self {
        Self {
            starting_fret: 1,
            direction: ExerciseDirection::Both,
            trill_repeats: 8,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Exercise {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ExerciseCategory,
    pub generator: fn(&Tuning, &ExerciseParams) -> Vec<ExerciseStep>,
}

impl Exercise {
    pub fn generate(&self, tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
        (self.generator)(tuning, params)
    }
}

pub fn catalog() -> &'static [Exercise] {
    CATALOG
}

pub fn catalog_in_category(
    category: ExerciseCategory,
) -> impl Iterator<Item = &'static Exercise> {
    catalog().iter().filter(move |e| e.category == category)
}

// === Generators ===

/// Standard chromatic 1-2-3-4: each string ascending fingers 1, 2, 3, 4
/// across four consecutive frets, then optionally descending.
fn chromatic_1234(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let mut steps = Vec::new();
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );

    if asc {
        for str_idx in 0..n {
            for f in 0u8..4 {
                steps.push(ExerciseStep {
                    string_index: str_idx,
                    fret: s + f,
                    finger: f + 1,
                });
            }
        }
    }
    if desc {
        for str_idx in (0..n).rev() {
            for f in (0u8..4).rev() {
                steps.push(ExerciseStep {
                    string_index: str_idx,
                    fret: s + f,
                    finger: f + 1,
                });
            }
        }
    }
    steps
}

/// Spider 1-3-2-4: same hand position as chromatic, but the finger
/// order is 1, 3, 2, 4 (then reverse on descent). Trains finger
/// independence between adjacent fingers.
fn spider_1324(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let order_asc: [u8; 4] = [0, 2, 1, 3]; // fret offsets corresponding to fingers 1, 3, 2, 4
    let order_desc: [u8; 4] = [3, 1, 2, 0];
    let mut steps = Vec::new();
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );

    if asc {
        for str_idx in 0..n {
            for &offset in &order_asc {
                steps.push(ExerciseStep {
                    string_index: str_idx,
                    fret: s + offset,
                    finger: offset + 1,
                });
            }
        }
    }
    if desc {
        for str_idx in (0..n).rev() {
            for &offset in &order_desc {
                steps.push(ExerciseStep {
                    string_index: str_idx,
                    fret: s + offset,
                    finger: offset + 1,
                });
            }
        }
    }
    steps
}

/// Trill: alternate between fingers 1 and 2 (frets `start` and `start+1`)
/// on each string for `trill_repeats` cycles.
fn trill_1_2(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let reps = params.trill_repeats.max(1);
    let mut steps = Vec::new();
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );
    let do_string = |steps: &mut Vec<ExerciseStep>, str_idx: usize| {
        for _ in 0..reps {
            steps.push(ExerciseStep {
                string_index: str_idx,
                fret: s,
                finger: 1,
            });
            steps.push(ExerciseStep {
                string_index: str_idx,
                fret: s + 1,
                finger: 2,
            });
        }
    };
    if asc {
        for str_idx in 0..n {
            do_string(&mut steps, str_idx);
        }
    }
    if desc {
        for str_idx in (0..n).rev() {
            do_string(&mut steps, str_idx);
        }
    }
    steps
}

/// Ladder: diagonal climb across strings — each note moves up one
/// string and one fret (or down both). Visualizes as a staircase.
fn ladder(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let mut steps = Vec::new();
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );

    if asc {
        // Diagonal ascent: string 0 fret s, string 1 fret s+1, ..., last string fret s+n-1.
        for str_idx in 0..n {
            steps.push(ExerciseStep {
                string_index: str_idx,
                fret: s + str_idx as u8,
                finger: 0,
            });
        }
    }
    if desc {
        for str_idx in (0..n).rev() {
            steps.push(ExerciseStep {
                string_index: str_idx,
                fret: s + str_idx as u8,
                finger: 0,
            });
        }
    }
    steps
}

/// Pentatonic Box Shift (Pebber Brown): minor pentatonic shape rooted
/// at `starting_fret`, then shifted up by 3 frets (a minor third).
/// Trains shape recognition + position shifting. Pattern per box:
/// each adjacent string-pair plays two notes (root-shape: fingers 1
/// & 4 on the lower string, fingers 1 & 3 on the upper).
fn pentatonic_box_shift(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let mut steps = Vec::new();
    // Pentatonic box-1 fret offsets per string (E-shape minor pentatonic):
    // string-0 (lowest pitch) frets s, s+3 with fingers 1, 4
    // strings 1..n-1 frets s, s+2 with fingers 1, 3 (approx for v1)
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );

    let box_offsets: [(u8, u8); 2] = [(0, 1), (3, 4)]; // (offset, finger)
    let upper_offsets: [(u8, u8); 2] = [(0, 1), (2, 3)];

    let render_box = |steps: &mut Vec<ExerciseStep>, base: u8| {
        for str_idx in 0..n {
            let pattern = if str_idx == 0 { box_offsets } else { upper_offsets };
            for &(off, finger) in &pattern {
                steps.push(ExerciseStep {
                    string_index: str_idx,
                    fret: base + off,
                    finger,
                });
            }
        }
    };

    if asc {
        render_box(&mut steps, s);
        render_box(&mut steps, s + 3);
    }
    if desc {
        // Box 2 first, then box 1, descending strings
        for &base in &[s + 3, s] {
            for str_idx in (0..n).rev() {
                let pattern = if str_idx == 0 { box_offsets } else { upper_offsets };
                for &(off, finger) in pattern.iter().rev() {
                    steps.push(ExerciseStep {
                        string_index: str_idx,
                        fret: base + off,
                        finger,
                    });
                }
            }
        }
    }
    steps
}

/// Two-String Climb (Pebber Brown): four-note pattern confined to a
/// pair of adjacent strings, climbing up the neck by one fret per
/// repetition. Trains alternate picking and string-crossing without
/// shifting hand position much.
fn two_string_climb(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    if n < 2 {
        return Vec::new();
    }
    let s = params.starting_fret;
    let mut steps = Vec::new();
    let asc = matches!(
        params.direction,
        ExerciseDirection::Ascending | ExerciseDirection::Both
    );
    let desc = matches!(
        params.direction,
        ExerciseDirection::Descending | ExerciseDirection::Both
    );
    // Pair: strings 0 and 1 (lowest two by physical order). Pattern at
    // each rep: lower-string fret(0,1), upper-string fret(0,1), then
    // shift up one fret. 4 notes per rep × 4 reps default.
    let reps = 4u8;
    let do_pair = |steps: &mut Vec<ExerciseStep>, lower: usize, upper: usize, base: u8| {
        for i in 0..reps {
            let f = base + i;
            steps.push(ExerciseStep { string_index: lower, fret: f, finger: 1 });
            steps.push(ExerciseStep { string_index: lower, fret: f + 1, finger: 2 });
            steps.push(ExerciseStep { string_index: upper, fret: f, finger: 1 });
            steps.push(ExerciseStep { string_index: upper, fret: f + 1, finger: 2 });
        }
    };
    if asc {
        do_pair(&mut steps, 0, 1, s);
    }
    if desc {
        // Reverse: same pair, reverse fret order
        for i in (0..reps).rev() {
            let f = s + i;
            steps.push(ExerciseStep { string_index: 1, fret: f + 1, finger: 2 });
            steps.push(ExerciseStep { string_index: 1, fret: f, finger: 1 });
            steps.push(ExerciseStep { string_index: 0, fret: f + 1, finger: 2 });
            steps.push(ExerciseStep { string_index: 0, fret: f, finger: 1 });
        }
    }
    steps
}

/// X-pattern: alternates between low and high strings, working
/// inward — string 0 → string n-1 → string 1 → string n-2 → ...
/// All on a single fret to start; subsequent passes shift fret.
fn x_pattern(tuning: &Tuning, params: &ExerciseParams) -> Vec<ExerciseStep> {
    let n = tuning.string_count();
    let s = params.starting_fret;
    let mut steps = Vec::new();
    let mut low = 0usize;
    let mut high = n - 1;
    let mut fret_offset: u8 = 0;
    while low <= high {
        steps.push(ExerciseStep {
            string_index: low,
            fret: s + fret_offset,
            finger: (fret_offset + 1).min(4),
        });
        if low != high {
            steps.push(ExerciseStep {
                string_index: high,
                fret: s + fret_offset,
                finger: (fret_offset + 1).min(4),
            });
        }
        if high == 0 {
            break;
        }
        low += 1;
        high -= 1;
        fret_offset = (fret_offset + 1) % 4;
    }
    steps
}

static CATALOG: &[Exercise] = &[
    Exercise {
        name: "Chromatic 1-2-3-4",
        description: "Each string ascending fingers 1, 2, 3, 4 across four \
                      consecutive frets, then descending. The canonical \
                      warmup.",
        category: ExerciseCategory::Chromatic,
        generator: chromatic_1234,
    },
    Exercise {
        name: "Spider 1-3-2-4",
        description: "Same four-fret position as chromatic, but the finger \
                      order is 1, 3, 2, 4 ascending and the reverse \
                      descending. Builds independence between adjacent \
                      fingers.",
        category: ExerciseCategory::Spider,
        generator: spider_1324,
    },
    Exercise {
        name: "Trill 1-2",
        description: "Alternate fingers 1 and 2 (adjacent frets) on each \
                      string. Builds finger speed and even tone.",
        category: ExerciseCategory::Trill,
        generator: trill_1_2,
    },
    Exercise {
        name: "Ladder",
        description: "Diagonal staircase across strings — one fret up per \
                      string. Trains string crossing and position shifts.",
        category: ExerciseCategory::Ladder,
        generator: ladder,
    },
    Exercise {
        name: "X-Pattern",
        description: "Alternates between outer and inner strings, working \
                      inward. Trains wide string skips.",
        category: ExerciseCategory::XPattern,
        generator: x_pattern,
    },
    // ── Pebber Brown's curriculum (RIP) ──
    Exercise {
        name: "Pentatonic Box Shift",
        description: "Pebber Brown: play box-1 minor pentatonic shape, \
                      then shift up a minor third (3 frets) and repeat. \
                      Builds shape recognition and position shifting.",
        category: ExerciseCategory::Pentatonic,
        generator: pentatonic_box_shift,
    },
    Exercise {
        name: "Two-String Climb",
        description: "Pebber Brown: four-note pattern confined to two \
                      adjacent strings, climbing up the neck one fret \
                      per repetition. Picking + string-crossing drill.",
        category: ExerciseCategory::StringPair,
        generator: two_string_climb,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::{Instrument, Tuning};

    fn standard_guitar() -> Tuning {
        Tuning::find_for("Standard", Instrument::Guitar).unwrap()
    }

    fn find(name: &str) -> &'static Exercise {
        catalog().iter().find(|e| e.name == name).unwrap()
    }

    #[test]
    fn chromatic_ascending_only_produces_24_notes_on_six_strings() {
        let ex = find("Chromatic 1-2-3-4");
        let params = ExerciseParams {
            starting_fret: 1,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        // 6 strings × 4 frets = 24 notes
        assert_eq!(steps.len(), 24);
    }

    #[test]
    fn chromatic_both_directions_produces_48_notes() {
        let ex = find("Chromatic 1-2-3-4");
        let steps = ex.generate(&standard_guitar(), &ExerciseParams::default());
        assert_eq!(steps.len(), 48);
    }

    #[test]
    fn chromatic_starts_at_specified_fret() {
        let ex = find("Chromatic 1-2-3-4");
        let params = ExerciseParams {
            starting_fret: 5,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        assert_eq!(steps[0].fret, 5);
        assert_eq!(steps[0].finger, 1);
        assert_eq!(steps[3].fret, 8);
        assert_eq!(steps[3].finger, 4);
    }

    #[test]
    fn spider_1324_uses_finger_order_1_3_2_4() {
        let ex = find("Spider 1-3-2-4");
        let params = ExerciseParams {
            starting_fret: 1,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        // First four steps are first string with fingers 1, 3, 2, 4
        let fingers: Vec<u8> = steps.iter().take(4).map(|s| s.finger).collect();
        assert_eq!(fingers, vec![1, 3, 2, 4]);
    }

    #[test]
    fn trill_alternates_two_frets() {
        let ex = find("Trill 1-2");
        let params = ExerciseParams {
            starting_fret: 5,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 4,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        // First string: 4 reps × 2 notes = 8 steps alternating 5, 6, 5, 6, ...
        let first_string: Vec<u8> = steps.iter().take(8).map(|s| s.fret).collect();
        assert_eq!(first_string, vec![5, 6, 5, 6, 5, 6, 5, 6]);
    }

    #[test]
    fn ladder_produces_diagonal_pattern() {
        let ex = find("Ladder");
        let params = ExerciseParams {
            starting_fret: 1,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        assert_eq!(steps.len(), 6);
        // Each step is on consecutive string + consecutive fret
        for (i, step) in steps.iter().enumerate() {
            assert_eq!(step.string_index, i);
            assert_eq!(step.fret, 1 + i as u8);
        }
    }

    #[test]
    fn x_pattern_alternates_outer_strings() {
        let ex = find("X-Pattern");
        let steps = ex.generate(&standard_guitar(), &ExerciseParams::default());
        // First two steps should be string 0 then string 5 (outer pair)
        assert_eq!(steps[0].string_index, 0);
        assert_eq!(steps[1].string_index, 5);
    }

    #[test]
    fn all_exercises_generate_at_least_one_step() {
        let g = standard_guitar();
        for ex in catalog() {
            let steps = ex.generate(&g, &ExerciseParams::default());
            assert!(!steps.is_empty(), "exercise {} produced no steps", ex.name);
        }
    }

    #[test]
    fn exercises_work_with_arbitrary_string_counts() {
        // 4-string bass should work with chromatic.
        let bass = Tuning::find_for("Standard 4", Instrument::Bass).unwrap();
        let ex = find("Chromatic 1-2-3-4");
        let steps = ex.generate(&bass, &ExerciseParams::default());
        // 4 strings × 4 frets × 2 directions = 32 steps
        assert_eq!(steps.len(), 32);
    }

    #[test]
    fn catalog_in_category_filters() {
        let chromatic_count = catalog_in_category(ExerciseCategory::Chromatic).count();
        assert_eq!(chromatic_count, 1);
        let trill_count = catalog_in_category(ExerciseCategory::Trill).count();
        assert_eq!(trill_count, 1);
    }

    #[test]
    fn pentatonic_box_shift_produces_two_boxes() {
        let ex = find("Pentatonic Box Shift");
        let params = ExerciseParams {
            starting_fret: 5,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        // 6 strings × 2 notes per box × 2 boxes = 24 steps ascending
        assert_eq!(steps.len(), 24);
        // First box should contain frets at and around starting_fret.
        assert!(steps.iter().take(12).any(|s| s.fret == 5));
        // Second box shifted by 3 frets.
        assert!(steps.iter().skip(12).any(|s| s.fret == 8));
    }

    #[test]
    fn two_string_climb_uses_only_two_strings() {
        let ex = find("Two-String Climb");
        let params = ExerciseParams {
            starting_fret: 1,
            direction: ExerciseDirection::Ascending,
            trill_repeats: 8,
        };
        let steps = ex.generate(&standard_guitar(), &params);
        // All steps should be on string 0 or 1.
        for s in &steps {
            assert!(s.string_index < 2, "step on string {}", s.string_index);
        }
        // 4 reps × 4 notes per rep = 16
        assert_eq!(steps.len(), 16);
    }
}
