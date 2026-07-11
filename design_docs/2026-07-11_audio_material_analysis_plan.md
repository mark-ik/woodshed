# Audio-to-Material Analysis Research and Benchmark Plan

## Status

**ACTIVE RESEARCH, 2026-07-11.** The benchmark contract and scorer are landed.
No transcription or reasoning model is selected for the product.

This plan supersedes the recommendation in
[`2026-05-15_polyphonic_pitch_spike.md`](2026-05-15_polyphonic_pitch_spike.md).
That spike correctly identified guitar polyphony as difficult, but treated one
embedded detector as the decision. The current question is broader: how a
recording becomes inspectable musical evidence, catalog relations, and useful
practice material through replaceable local tools.

## Product Question

Strophe records durable performances. Woodshed should be able to use those
files immediately as loopable practice audio and, when analysis is requested,
derive tentative musical interpretations:

`recording -> observations -> catalog matches -> practice material`

The reverse path is equally important:

`Woodshed Card/Set -> rendered stems -> Strophe layers`

Woodshed-generated stems carry known notes, timing, voicing, instrument
context, and source Cards. They are ground-truth fixtures for evaluating the
return path and, later, for comparing an intended performance with a recorded
one.

## Boundary Decisions

### Audio is evidence; analysis is interpretation

Never rewrite a source recording because an analyzer changes its mind. A source
audio asset is immutable and content-addressed. Each analysis run records:

- analyzer and version;
- model/checkpoint identity when applicable;
- settings and supplied musical context;
- source digest and analyzed time span;
- observations with confidence;
- wall-clock runtime and execution device.

A corrected interpretation is a new analysis or a user-authored correction,
not a mutation of either the recording or prior output.

### Exchange files, not project internals

Woodshed should consume a decoded audio file plus a small sidecar carrying the
loop region, tempo/meter hints, source digest, and optional Strophe
session/track/phrase provenance. It must not open Strophe's Redb/Muniment
project store directly. Ordinary WAV is the first audio boundary; FLAC or other
formats can follow behind one decoder seam. Missing sidecar metadata only
reduces available context and does not make the audio unusable.

### Keep analyzers outside the realtime audio graph

This is offline work over a file or frozen capture. `woodshed-audio` retains
realtime input, transport, and playback ownership. Model loading, Python,
ONNX, GPU runtimes, and agents run behind an offline analyzer boundary and
publish normalized results when complete.

### Separate observations from catalog resolution

An analyzer may report notes, onsets, tempo candidates, chord labels, playing
techniques, or free-text descriptions. It does not directly create a Woodshed
Card. A deterministic resolver combines observations with optional context:

- instrument and tuning;
- capo and fret window;
- expected Set/Card;
- session tempo and meter;
- nearby catalog candidates;
- user corrections.

The resolver emits ranked catalog matches with explanations. Stage remains the
only action that turns a match into practice material.

### Agents are clients, not authorities

Any local or remote agent may call analyzers, compare their outputs, and explain
candidate interpretations. The normalized observation contract remains owned
by Woodshed. An agent cannot silently promote a guess to catalog truth or
replace source provenance.

## Current Tool Findings

### Baseline: existing Woodshed DSP

Woodshed already has monophonic FFT, cepstrum, and McLeod pitch detection plus
onset and tempo primitives. These form a cheap deterministic baseline and are
useful for isolated single-note exercises. They do not solve polyphonic note
tracking.

### First transcription adapter: Basic Pitch

Spotify Basic Pitch is the practical first external adapter:

- Apache-2.0;
- polyphonic and instrument-agnostic, with an explicit recommendation to use
  one instrument at a time;
- local CLI and Python API;
- note events, MIDI, pitch bends, and raw model output;
- Windows defaults to its ONNX serialization;
- arbitrary-length files are windowed and resampled to 22.05 kHz mono.

This matches Strophe's isolated mono layers well. The first spike's claim that
Woodshed should immediately embed Basic Pitch through `tract` is withdrawn:
the supported Windows ONNX path should be benchmarked out of process before a
Rust runtime is selected.

Source: <https://github.com/spotify/basic-pitch>

### Multi-instrument comparator: MT3

MT3 produces multi-instrument transcription and is Apache-2.0, but its public
path is a research-oriented T5X/Colab stack, has no releases, and is explicitly
not a supported Google product. It is a useful quality comparator, not the
first integration target.

Source: <https://github.com/magenta/mt3>

### Rhythm and tonal reference: Essentia

Essentia's extractor can emit beats, BPM, onsets, tuning, pitch-class profiles,
keys, and chord summaries as JSON. It is valuable for benchmark comparison.
The library is AGPL-3.0 and its published pretrained models are non-commercial
unless separately licensed, so it must not become a Woodshed product
dependency without a deliberate licensing decision.

Sources: <https://essentia.upf.edu/streaming_extractor_music.html> and
<https://essentia.upf.edu/licensing_information.html>

### Optional source separation

Strophe layers normally isolate one contributor and should be analyzed before
source separation. For imported mixes, separation is an optional preprocessing
stage. Meta's SAM Audio supports text-, span-, and visual-prompted separation
and publishes checkpoints and evaluation tooling under its own SAM license.
Its resource cost and license require evaluation. The older Demucs repository
is unmaintained and should be a legacy baseline only.

Sources: <https://github.com/facebookresearch/sam-audio> and
<https://github.com/facebookresearch/demucs>

### Optional semantic reasoners

Gemma 4 is one plausible local client, not a privileged dependency. Its native
audio path accepts 30 seconds of mono 16 kHz audio. That is suitable for coarse
description and short-region reasoning but is not evidence of note-accurate
transcription. Other local multimodal models and text-only agents consuming
specialist outputs belong in the same benchmark lane.

Source: <https://ai.google.dev/gemma/docs/capabilities/audio>

## Benchmark Corpus

The corpus is layered so a failure identifies the broken capability.

### A. Synthetic catalog truth

Render scales, chord voicings, arpeggios, and short progressions directly from
Woodshed. Vary register, tempo, note duration, strum spread, and dynamics.
These cases have exact note and catalog truth but limited timbral realism.

### B. Human isolated performances

Record known Woodshed Cards through Strophe-style mono capture. Cover guitar,
bass, ukulele, and at least one custom tuning. Preserve the intended Card and
ask the player to mark mistakes rather than correcting the recording.

### C. Effects and capture conditions

Re-record or process a subset through clean amplification, distortion,
compression, delay, and reverb; include microphone and direct-input paths.
This tests robustness without changing the intended material.

### D. Mixtures

Combine known stems into two- and four-part mixes. Evaluate direct
transcription and optional separation plus transcription separately. Mixture
results must not obscure isolated-layer performance.

### E. Open performances

Add short improvisations with human annotations and competing plausible
interpretations. These test usefulness and confidence calibration rather than
only exact-note recovery.

Do not use private Strophe recordings as a published corpus by default. Corpus
entries need explicit consent and redistribution terms.

## Normalized Result Contract

The initial scaffold uses JSON with:

- `schema_version`;
- `run` metadata: analyzer, version, source identifier, settings, elapsed time;
- `notes`: onset, offset, MIDI pitch, confidence;
- `chords`: time span, root pitch class, quality, confidence;
- `tempo`: BPM and confidence;
- `catalog_candidates`: stable Woodshed catalog ID, score, and reasons.

This is a benchmark interchange format, not yet the durable application model.
`scripts/audio_analysis_bench.py` currently scores notes and catalog retrieval.
Adapters translate native analyzer output into this format; Woodshed never asks
every model to implement an in-process Rust trait.

## Metrics

### Machine metrics

- note onset precision/recall/F1 at 50 ms and 50 cents;
- note onset+offset F1, with offset tolerance of 20% or 50 ms;
- average overlap for matched notes;
- tempo absolute and octave-aware error;
- chord root, quality, and duration-weighted segment accuracy;
- catalog top-1, top-3, and top-5 retrieval;
- confidence calibration;
- runtime, peak memory, model size, and cold-start cost.

Use `mir_eval` as the comparison reference where its transcription metrics
apply. The local scorer intentionally has a small auditable implementation for
smoke tests, not a claim of benchmark standardization.

### Product metrics

- Can the user see why a catalog match was proposed?
- Can a wrong note or chord be corrected without re-running everything?
- Does the result produce a useful rehearsal fragment or comparison?
- Are uncertainty and competing interpretations visible?
- Does analysis remain optional and local by default?

## Graph Projection

`woodshed-graph` should eventually project, rather than own, the analysis log.
Likely nodes are source asset, analysis run, temporal region, observation,
catalog material, staged Card, and practice event. Likely relations include:

- `derived from`;
- `detected in`;
- `supports` / `contradicts`;
- `realizes`;
- `variation of`;
- `corrected by`;
- `rehearsed as`.

Confidence, time span, analyzer identity, and evidence stay attached to the
analysis observation. Catalog formulas remain stable truth nodes.

## Plan

### R1. Freeze the benchmark contract

Land representative normalized fixtures, validation, and smoke scoring. Do not
add model dependencies to the Cargo workspace.

Done when malformed observations fail clearly, matching is one-to-one, note
and catalog metrics are deterministic, and the fixtures run on Windows with
the standard Python installation.

### R2. Generate catalog fixtures

Add a non-realtime generator that renders a small balanced catalog subset and
writes source audio plus exact normalized annotations. Start with Major,
Minor, Dominant 7, one altered chord, two scales, two arpeggiation directions,
and one progression.

Done when every generated file can be reproduced from a committed manifest and
its annotation points back to stable Woodshed catalog IDs.

### R3. Adapt Basic Pitch

Pin a tested Basic Pitch version in an isolated environment, invoke its CLI,
and normalize note events. Record runtime/model metadata. Do not bundle it with
the application.

Done when one command analyzes all A/B cases, failures are per-case rather than
fatal to the run, and results are reproducible on the reference Windows host.

### R4. Test context-constrained resolution

Resolve raw note events twice: blind, then with instrument/tuning/capo/tempo and
expected-neighborhood context. Keep analyzer output unchanged so the resolver's
contribution is measurable.

Done when the report distinguishes transcription quality from catalog-retrieval
quality and identifies cases where context causes a wrong confident match.

### R5. Compare optional lanes

Add at most one multi-instrument transcription comparator, one separation
comparator for D cases, and one local reasoner/agent. A candidate enters only if
it has runnable weights or binaries, compatible research use, and a normalizer.

Done when each lane has measured quality, resource cost, license, install
friction, and failure behavior. Popularity alone is not a selection criterion.

### R6. Decide the product slice

Choose the smallest capability that clears a written quality floor. Likely
first slices are isolated-layer note extraction plus catalog suggestions, or
performance-against-known-Card comparison. Full mix-to-tab is not the default.

Done when the selected feature has a local execution path, visible uncertainty,
user correction, provenance, cancellation, and a setting controlling whether
analysis runs at all.

## Stop Rules

- Do not embed an ML runtime before the out-of-process baseline is useful.
- Do not make full-mix separation a prerequisite for isolated Strophe layers.
- Do not infer string/fret positions as facts when several fingerings fit.
- Do not treat an agent's prose as structured analysis.
- Do not publish or train on user recordings without explicit consent.
- Do not merge analysis observations into catalog truth.

## Progress

### 2026-07-11

- Audited Woodshed's Set/Card, input analyzer, and catalog-graph seams plus
  Strophe's immutable phrase/media boundary.
- Re-evaluated the 2026-05-15 polyphonic spike against current tools.
- Selected Basic Pitch as the first adapter to benchmark, not a committed
  product dependency.
- Kept MT3, separation, and local reasoners as optional comparator lanes.
- Landed the normalized JSON smoke scorer and fixtures.
- Confirmed `basic-pitch` 0.4.0 is published but not installed in the reference
  Python environment. R3 requires a disposable pinned environment and must
  measure setup friction separately from inference.
