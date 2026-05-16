//! Intervals between pitches.
//!
//! An [`Interval`] is a `(number, quality)` pair where `number` is the
//! diatonic distance (1 = unison, 2 = second, 8 = octave, 9 = ninth, ...)
//! and `quality` is `Diminished`, `Minor`, `Perfect`, `Major`, or
//! `Augmented`. Some combinations are invalid (e.g. "major fourth"); use
//! [`Interval::try_new`] to construct intervals safely.

use core::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Quality {
    Diminished,
    Minor,
    Perfect,
    Major,
    Augmented,
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Quality::Diminished => "dim",
            Quality::Minor => "min",
            Quality::Perfect => "perf",
            Quality::Major => "maj",
            Quality::Augmented => "aug",
        };
        f.write_str(s)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IntervalError {
    /// Diatonic number must be >= 1.
    ZeroNumber,
    /// `Major` / `Minor` are only valid for non-perfect degrees (2, 3, 6, 7);
    /// `Perfect` is only valid for perfect degrees (1, 4, 5, 8, ...).
    InvalidQuality,
}

impl fmt::Display for IntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntervalError::ZeroNumber => f.write_str("interval number must be >= 1"),
            IntervalError::InvalidQuality => {
                f.write_str("quality not valid for this diatonic degree")
            }
        }
    }
}

impl std::error::Error for IntervalError {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Interval {
    number: u8,
    quality: Quality,
}

impl Interval {
    /// Construct an interval, validating the quality/number combination.
    pub const fn try_new(number: u8, quality: Quality) -> Result<Self, IntervalError> {
        if number == 0 {
            return Err(IntervalError::ZeroNumber);
        }
        let degree = ((number - 1) % 7) + 1;
        let perfect_eligible = matches!(degree, 1 | 4 | 5);
        match (perfect_eligible, quality) {
            (true, Quality::Major | Quality::Minor) => Err(IntervalError::InvalidQuality),
            (false, Quality::Perfect) => Err(IntervalError::InvalidQuality),
            _ => Ok(Self { number, quality }),
        }
    }

    /// Internal unchecked constructor for compile-time constants.
    const fn unchecked(number: u8, quality: Quality) -> Self {
        Self { number, quality }
    }

    pub const fn number(self) -> u8 {
        self.number
    }

    pub const fn quality(self) -> Quality {
        self.quality
    }

    /// True if the diatonic degree (1, 4, 5 mod 7) admits perfect quality.
    pub const fn is_perfect_eligible(number: u8) -> bool {
        if number == 0 {
            return false;
        }
        let degree = ((number - 1) % 7) + 1;
        matches!(degree, 1 | 4 | 5)
    }

    /// Semitone count of this interval. Negative-going intervals are not
    /// represented here; use a signed count externally if direction matters.
    pub const fn semitones(self) -> i32 {
        let n = self.number as i32;
        let octaves = (n - 1) / 7;
        let degree = ((n - 1) % 7) + 1;
        let (base, perfect_eligible) = match degree {
            1 => (0, true),
            2 => (2, false),
            3 => (4, false),
            4 => (5, true),
            5 => (7, true),
            6 => (9, false),
            7 => (11, false),
            _ => (0, true),
        };
        let offset = match self.quality {
            Quality::Perfect => 0,
            Quality::Major => 0,
            Quality::Minor => -1,
            Quality::Augmented => 1,
            Quality::Diminished => {
                if perfect_eligible {
                    -1
                } else {
                    -2
                }
            }
        };
        octaves * 12 + base + offset
    }

    // Common named intervals.
    pub const DIMINISHED_UNISON: Self = Self::unchecked(1, Quality::Diminished);
    pub const PERFECT_UNISON: Self = Self::unchecked(1, Quality::Perfect);
    pub const AUGMENTED_UNISON: Self = Self::unchecked(1, Quality::Augmented);
    pub const MINOR_SECOND: Self = Self::unchecked(2, Quality::Minor);
    pub const MAJOR_SECOND: Self = Self::unchecked(2, Quality::Major);
    pub const AUGMENTED_SECOND: Self = Self::unchecked(2, Quality::Augmented);
    pub const MINOR_THIRD: Self = Self::unchecked(3, Quality::Minor);
    pub const MAJOR_THIRD: Self = Self::unchecked(3, Quality::Major);
    pub const DIMINISHED_FOURTH: Self = Self::unchecked(4, Quality::Diminished);
    pub const PERFECT_FOURTH: Self = Self::unchecked(4, Quality::Perfect);
    pub const AUGMENTED_FOURTH: Self = Self::unchecked(4, Quality::Augmented);
    pub const DIMINISHED_FIFTH: Self = Self::unchecked(5, Quality::Diminished);
    pub const PERFECT_FIFTH: Self = Self::unchecked(5, Quality::Perfect);
    pub const AUGMENTED_FIFTH: Self = Self::unchecked(5, Quality::Augmented);
    pub const MINOR_SIXTH: Self = Self::unchecked(6, Quality::Minor);
    pub const MAJOR_SIXTH: Self = Self::unchecked(6, Quality::Major);
    pub const AUGMENTED_SIXTH: Self = Self::unchecked(6, Quality::Augmented);
    pub const DIMINISHED_SEVENTH: Self = Self::unchecked(7, Quality::Diminished);
    pub const MINOR_SEVENTH: Self = Self::unchecked(7, Quality::Minor);
    pub const MAJOR_SEVENTH: Self = Self::unchecked(7, Quality::Major);
    pub const PERFECT_OCTAVE: Self = Self::unchecked(8, Quality::Perfect);
    pub const MINOR_NINTH: Self = Self::unchecked(9, Quality::Minor);
    pub const MAJOR_NINTH: Self = Self::unchecked(9, Quality::Major);
    pub const AUGMENTED_NINTH: Self = Self::unchecked(9, Quality::Augmented);
    pub const PERFECT_ELEVENTH: Self = Self::unchecked(11, Quality::Perfect);
    pub const AUGMENTED_ELEVENTH: Self = Self::unchecked(11, Quality::Augmented);
    pub const MINOR_THIRTEENTH: Self = Self::unchecked(13, Quality::Minor);
    pub const MAJOR_THIRTEENTH: Self = Self::unchecked(13, Quality::Major);
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.quality, self.number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_intervals_have_correct_semitones() {
        assert_eq!(Interval::PERFECT_UNISON.semitones(), 0);
        assert_eq!(Interval::PERFECT_FOURTH.semitones(), 5);
        assert_eq!(Interval::PERFECT_FIFTH.semitones(), 7);
        assert_eq!(Interval::PERFECT_OCTAVE.semitones(), 12);
    }

    #[test]
    fn major_minor_intervals_have_correct_semitones() {
        assert_eq!(Interval::MINOR_SECOND.semitones(), 1);
        assert_eq!(Interval::MAJOR_SECOND.semitones(), 2);
        assert_eq!(Interval::MINOR_THIRD.semitones(), 3);
        assert_eq!(Interval::MAJOR_THIRD.semitones(), 4);
        assert_eq!(Interval::MINOR_SIXTH.semitones(), 8);
        assert_eq!(Interval::MAJOR_SIXTH.semitones(), 9);
        assert_eq!(Interval::MINOR_SEVENTH.semitones(), 10);
        assert_eq!(Interval::MAJOR_SEVENTH.semitones(), 11);
    }

    #[test]
    fn augmented_and_diminished_match_enharmonic_neighbors() {
        assert_eq!(Interval::AUGMENTED_FOURTH.semitones(), 6);
        assert_eq!(Interval::DIMINISHED_FIFTH.semitones(), 6);
    }

    #[test]
    fn compound_intervals_extend_correctly() {
        assert_eq!(Interval::MAJOR_NINTH.semitones(), 14);
        assert_eq!(Interval::MINOR_NINTH.semitones(), 13);
        assert_eq!(Interval::PERFECT_ELEVENTH.semitones(), 17);
        assert_eq!(Interval::MAJOR_THIRTEENTH.semitones(), 21);
    }

    #[test]
    fn try_new_rejects_invalid_quality_for_perfect_degree() {
        assert_eq!(
            Interval::try_new(4, Quality::Major),
            Err(IntervalError::InvalidQuality)
        );
        assert_eq!(
            Interval::try_new(5, Quality::Minor),
            Err(IntervalError::InvalidQuality)
        );
    }

    #[test]
    fn try_new_rejects_perfect_for_imperfect_degree() {
        assert_eq!(
            Interval::try_new(3, Quality::Perfect),
            Err(IntervalError::InvalidQuality)
        );
        assert_eq!(
            Interval::try_new(7, Quality::Perfect),
            Err(IntervalError::InvalidQuality)
        );
    }

    #[test]
    fn try_new_rejects_zero_number() {
        assert_eq!(
            Interval::try_new(0, Quality::Perfect),
            Err(IntervalError::ZeroNumber)
        );
    }

    #[test]
    fn try_new_accepts_valid_combinations() {
        assert!(Interval::try_new(1, Quality::Perfect).is_ok());
        assert!(Interval::try_new(3, Quality::Major).is_ok());
        assert!(Interval::try_new(5, Quality::Diminished).is_ok());
        assert!(Interval::try_new(11, Quality::Augmented).is_ok());
    }

    #[test]
    fn perfect_eligibility_repeats_at_octave() {
        assert!(Interval::is_perfect_eligible(1));
        assert!(Interval::is_perfect_eligible(4));
        assert!(Interval::is_perfect_eligible(5));
        assert!(Interval::is_perfect_eligible(8));
        assert!(Interval::is_perfect_eligible(11));
        assert!(Interval::is_perfect_eligible(12));
        assert!(!Interval::is_perfect_eligible(2));
        assert!(!Interval::is_perfect_eligible(7));
        assert!(!Interval::is_perfect_eligible(9));
    }
}
