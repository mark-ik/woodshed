//! In-place sample-buffer shaping ops.
//!
//! Pure, length-preserving transforms over a mono `f32` slice — the
//! looper/sampler "shape the take" kernels (gain, normalize, reverse).
//! Length-preserving so a bar-locked loop stays the right length for
//! modulo playback. A consumer holding its samples behind an `Arc`
//! (Woodshed's `SampleBuffer`) calls these on `Arc::make_mut(..)`.

/// Multiply every sample by `gain`, soft-clamping to `[-1.0, 1.0]`.
/// A negative gain also inverts phase.
pub fn apply_gain(samples: &mut [f32], gain: f32) {
    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

/// Scale so the loudest sample sits at `peak` (e.g. `1.0`). No-op for a
/// silent buffer (avoids divide-by-zero / inf blow-up).
pub fn normalize(samples: &mut [f32], peak: f32) {
    let max = samples.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
    if max <= f32::EPSILON {
        return;
    }
    let scale = peak / max;
    for s in samples.iter_mut() {
        *s = (*s * scale).clamp(-1.0, 1.0);
    }
}

/// Reverse the buffer in place.
pub fn reverse(samples: &mut [f32]) {
    samples.reverse();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_scales_and_clamps() {
        let mut b = vec![0.25, -0.5, 0.8];
        apply_gain(&mut b, 2.0);
        assert!((b[0] - 0.5).abs() < 1e-6);
        assert!((b[1] - -1.0).abs() < 1e-6);
        assert!((b[2] - 1.0).abs() < 1e-6); // clamped from 1.6
    }

    #[test]
    fn normalize_brings_peak_to_target() {
        let mut b = vec![0.1, -0.4, 0.2];
        normalize(&mut b, 1.0);
        let max = b.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
        assert!((max - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_silent_is_noop() {
        let mut b = vec![0.0, 0.0];
        normalize(&mut b, 1.0);
        assert!(b.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn reverse_flips_order() {
        let mut b = vec![1.0, 2.0, 3.0];
        reverse(&mut b);
        assert_eq!(b, vec![3.0, 2.0, 1.0]);
    }
}
