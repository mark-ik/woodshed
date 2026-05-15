Recommendation: rename woodshed-audio, rewrite the description on publication, leave the actual feature additions for after the rename + publish. You want item 1 (serde) in the polish pass, the rest as numbered future-work issues on GitHub.



Serde on the pattern types (the workspace Cargo.toml literally says "Future: serde — tuning/preset persistence" and nothing in sequencer.rs has it yet). Cheap, opens up preset saving and sharing.



More Sound variants beyond Click. The enum is set up to extend. Drum kit samples (load from .wav), basic FM synth voice. Lets the metronome graduate into the "simple drum machine" your README promises.



Tap tempo. BPM detection from key/touch input, then BPM detection from audio (onset-detect a few hits, compute tempo). 30 lines for the input case, more for audio.



Onset / timing feedback. Already have input capture and a known click schedule. Compute "you're playing this beat 30ms late on average" — instantly useful for practice, no other tuner app does this well.



Loop record / overdub. Capture user input over a click pattern, play it back next bar. Combine with onset-detection and you've got a "practice with yourself" feature that's rare in this space.



Audio file export. Render a pattern to .wav. Trivial once you have offline rendering working.



MIDI in/out (midir is the idiomatic Rust crate). Drives external drum machines, accepts MIDI clock sync. Possibly webMIDI?



WAV/FLAC sample loader for the drum kit case (hound + symphonia). Possibly other filetypes?



Polyphonic detection mode — your current pitch detector is monophonic. Polyphonic is hard but the YIN-based libraries (aubio, or rolling your own; pros and cons?) exist.
