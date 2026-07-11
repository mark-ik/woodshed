# Polyphonic Pitch Detection — Research Spike

> **Superseded 2026-07-11** by
> [`2026-07-11_audio_material_analysis_plan.md`](2026-07-11_audio_material_analysis_plan.md).
> This document remains useful background on guitar acoustics. Its recommendation
> to embed Basic Pitch through `tract` is no longer current; the new plan starts
> with a model-neutral, out-of-process benchmark.

The current Woodshed tuner uses `pitch-detector` (FFT + cepstrum
backends) for **monophonic** detection: one fundamental at a time.
This spike captures the technology landscape for polyphonic detection
(chord recognition, multi-string analysis), the trade-offs, and a
recommendation. **No code lands from this doc.**

---

## What would polyphonic detection enable?

- **Chord recognition**: "you're playing G major right now" displayed
  on screen.
- **Multi-note exercise scoring**: scale runs scored against the
  expected note sequence regardless of timing variation.
- **Practice transcription**: capture what you played and display it
  as tab.

None of these are critical. The current monophonic tuner is enough
for everything Woodshed ships today. Polyphonic is a *future*
feature that opens new UI surface but doesn't unblock anything.

## Why it's hard

Guitar polyphonic transcription is one of the harder MIR problems.
The fundamental issues:

1. **Overtones overlap**. The 2nd harmonic of low E (164 Hz) is
   octave-E, which is also a real note someone might play.
2. **Sympathetic resonance**. Open strings ring when other strings
   are struck, smearing the spectrum.
3. **Inharmonicity**. Real strings are slightly inharmonic — partials
   aren't exact integer multiples — and the deviation depends on
   string gauge, tension, and pluck strength.
4. **Onset coincidence**. Strums hit all 6 strings within ~5ms; the
   onset envelope doesn't separate them.

This is why even Apple's "Auto-Tab" features in Logic and similar
DAW tools are still considered cutting-edge and imperfect.

## Approaches surveyed

### 1. Classical DSP — multi-F0 spectral analysis

Algorithms like **NMF** (non-negative matrix factorization),
**probabilistic latent component analysis**, or **Salience-function
peak picking** (e.g. Klapuri's method).

- **Pros**: deterministic, runs on a CPU, no model files, no GPU.
- **Cons**: research-grade complexity (papers, weeks of integration),
  mediocre on guitar specifically because of the overtone issues
  above. Best results are still ~70-80% accuracy on chord ID.
- **State of practice**: still beats nothing, but no commercial
  product seriously uses these standalone in 2026.

### 2. aubio

`aubio` is the de facto open-source MIR library. Excellent quality.

- **Pros**: production-ready, C library with battle-tested algorithms.
- **Cons**: **GPL-3 licensed**. Linking it would force Woodshed to
  be GPL-3, conflicting with our current MIT/Apache-2.0 dual license.
  This rules it out for our use case unless we re-license Woodshed
  entirely. **Not viable.**

### 3. Neural networks — basic-pitch, CREPE, PESTO

The current state of the art. Trained on large transcription
datasets; vastly better than classical DSP.

- **basic-pitch** (Spotify, 2022): open-source, Apache-2.0,
  polyphonic, originally designed for guitar specifically. Ships as
  a small TensorFlow model (~1MB). Inference is fast on CPU.
- **CREPE** (NYU MARL, 2018): monophonic but extremely accurate.
  Larger model (~80MB). Real-time on CPU.
- **PESTO** (Inria, 2023): faster CREPE variant.

Inference paths in Rust:
- **`tract`** — pure Rust ONNX inference. Tested on basic-pitch's
  ONNX export. Slower than native (~5x), but no Python.
- **`ort`** — onnxruntime bindings. Faster, requires ORT shared lib.
- **`burn`** — pure Rust ML framework. Could host models natively.

- **Pros**: huge accuracy lift. Real-time-capable on modern CPUs.
- **Cons**:
  - Model files add ~10-100MB to the binary or distribution. For
    a guitar-practice app aiming at lightweight desktop install,
    that's not catastrophic but is meaningful.
  - GPU acceleration is optional but desirable; requires CUDA/Metal
    integration we don't have.
  - Latency: typical models want ~50-100ms of audio context for
    high accuracy. Fine for displayed chord recognition; too laggy
    for "live pitch indicator" use cases (where the monophonic
    tuner stays appropriate).
  - Mobile: model size and inference cost both push back on the
    eventual mobile build.

### 4. Hybrid — onset-triggered chord ID

Run monophonic per-string detection on the input, but at each onset,
also classify the spectrum into one of N chord templates (major,
minor, dom7, etc.) using a simple ML head or even a hand-tuned
template match. This sidesteps full transcription and just answers
"is this a chord I know?"

- **Pros**: simpler than full polyphonic, useful for the most common
  practice case ("am I playing the right chord?"). No giant model.
- **Cons**: doesn't handle voicings well — different inversions of
  the same chord look spectrally different. Limited to recognized
  chord types.

## Recommendation

**Don't ship polyphonic detection in the next 2-3 milestones.**
Reasoning:

1. Monophonic is sufficient for the tuner, exercises, scale practice,
   and onset-based timing feedback (the features the user has flagged
   so far).
2. The high-quality path (neural) demands a meaningful integration
   investment (model files, inference runtime, latency budget,
   mobile story).
3. The low-quality paths (classical DSP, hybrid) give mediocre UX
   that we'd want to replace anyway.

**When to revisit:** if a concrete feature *requires* polyphonic
recognition. Examples that would justify it:
- "Chord recognition" tab where you play a chord and the app names
  it. Then we'd want basic-pitch via tract.
- "Score my playing" feature where the app compares your performance
  to a target progression in real-time.

**If we do build it,** start with **basic-pitch via tract**:
- Apache-2.0 compatible licensing on both pieces.
- Pure Rust path (no ORT shared lib).
- Good guitar-specific training data behind the model.
- Inference cost acceptable on desktop CPUs.

Expected scope: ~1-2 weeks of integration work, plus model bundling
into the binary.

## What this doc does not say

- It does not commit to ever shipping polyphonic detection.
- It does not commit to basic-pitch specifically — if a better
  Rust-native polyphonic detector lands between now and the day we
  start, we should re-evaluate.
- It does not address the broader "transcription" feature (audio →
  tab notation), which would compose polyphonic detection with
  rhythm quantization and tab layout. That's its own project.
