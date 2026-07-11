//! Waveform overview extraction from plain mono sample slices.

/// One waveform column's signed amplitude extent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveformPeak {
    /// Lowest finite sample in the column.
    pub min: f32,
    /// Highest finite sample in the column.
    pub max: f32,
}

impl WaveformPeak {
    /// Scale this column by a linear gain, preserving min/max ordering.
    pub fn scaled(self, gain: f32) -> Self {
        let a = self.min * gain;
        let b = self.max * gain;
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }
}

/// Reduce `samples` into at most `columns` signed min/max columns.
///
/// Every input sample belongs to exactly one column. Empty input or a zero
/// column request returns an empty vector. Non-finite samples are treated as
/// silence so corrupt analysis data cannot poison a retained UI path.
pub fn min_max_peaks(samples: &[f32], columns: usize) -> Vec<WaveformPeak> {
    if samples.is_empty() || columns == 0 {
        return Vec::new();
    }
    let bins = columns.min(samples.len());
    (0..bins)
        .map(|bin| {
            let start = bin * samples.len() / bins;
            let end = ((bin + 1) * samples.len() / bins).max(start + 1);
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for sample in &samples[start..end] {
                let sample = if sample.is_finite() { *sample } else { 0.0 };
                min = min.min(sample);
                max = max.max(sample);
            }
            WaveformPeak { min, max }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_every_sample_into_signed_columns() {
        let peaks = min_max_peaks(&[-1.0, 0.5, -0.25, 0.75, 0.2, -0.8], 3);
        assert_eq!(
            peaks,
            vec![
                WaveformPeak {
                    min: -1.0,
                    max: 0.5
                },
                WaveformPeak {
                    min: -0.25,
                    max: 0.75
                },
                WaveformPeak {
                    min: -0.8,
                    max: 0.2
                },
            ]
        );
    }

    #[test]
    fn an_impulse_survives_downsampling() {
        let mut samples = vec![0.0; 128];
        samples[63] = 1.0;
        samples[64] = -0.75;
        let peaks = min_max_peaks(&samples, 8);
        assert!(peaks.iter().any(|peak| peak.max == 1.0));
        assert!(peaks.iter().any(|peak| peak.min == -0.75));
    }

    #[test]
    fn silence_and_short_buffers_are_stable() {
        assert_eq!(
            min_max_peaks(&[0.0, 0.0], 8),
            vec![WaveformPeak::default(), WaveformPeak::default()]
        );
        assert!(min_max_peaks(&[], 8).is_empty());
        assert!(min_max_peaks(&[1.0], 0).is_empty());
    }

    #[test]
    fn scaling_preserves_order_for_negative_gain() {
        assert_eq!(
            WaveformPeak {
                min: -0.5,
                max: 1.0
            }
            .scaled(-2.0),
            WaveformPeak {
                min: -2.0,
                max: 1.0
            }
        );
    }
}
