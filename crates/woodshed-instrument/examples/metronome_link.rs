//! W2's hardware proof: the metronome link, both directions, against a real
//! instrument.
//!
//! Run with a HyVibe guitar powered on, the phone app disconnected, and USB
//! mode off:
//!
//! ```text
//! cargo run -p woodshed-instrument --example metronome_link
//! ```
//!
//! **This writes to the instrument**, which is the point — the done-condition
//! is that a tempo set in Woodshed reaches the guitar. It reads the existing
//! setting first and puts it back at the end, so the instrument is left as it
//! was found even if that setting was deliberate.

use std::time::Duration;

use woodshed_instrument::{InstrumentError, MetronomeLink, Metronome, Session, SyncOutcome};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match run().await {
        Ok(()) => println!("\nmetronome link confirmed against hardware."),
        Err(e) => {
            eprintln!("\nFAILED: {e}");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<(), InstrumentError> {
    println!("opening a session (scanning 10s)...");
    let mut session = Session::open(Duration::from_secs(10)).await?;
    println!("  connected; link starts {:?}", session.link());
    println!("  \"{}\"", session.link().describe());

    // Everything from here is fallible, and the instrument must be handed back
    // whatever happens — so the work is done in a closure and the release runs
    // after it either way.
    let outcome = exercise(&mut session).await;
    println!("\nreleasing the instrument...");
    session.release().await?;
    println!("  released; the phone app can have it back");
    outcome
}

async fn exercise(session: &mut Session) -> Result<(), InstrumentError> {
    // 1. Detached does nothing at all.
    println!("\n[1/5] detached: sync should touch nothing");
    let idle = session.sync_metronome(tempo(120)).await?;
    println!("      {idle:?}");
    assert_eq!(idle, SyncOutcome::Detached);

    // 2. Follow: adopt whatever the instrument is set to, and remember it so it
    //    can be restored.
    println!("\n[2/5] follow: read the instrument's own tempo");
    session.set_link(MetronomeLink::Follow);
    println!("      \"{}\"", session.link().describe());
    let original = match session.sync_metronome(tempo(120)).await? {
        SyncOutcome::Adopted(m) => m,
        other => {
            return Err(InstrumentError::Shape(format!(
                "expected Follow to adopt, got {other:?}"
            )));
        }
    };
    println!(
        "      instrument is at {} bpm, {}/{}",
        original.bpm, original.beats_per_bar, original.beat_unit
    );

    // 3. Drive: send something demonstrably different from what was there.
    let target = Metronome {
        bpm: if original.bpm == 96 { 88 } else { 96 },
        beats_per_bar: 4,
        beat_unit: 4,
    };
    println!(
        "\n[3/5] drive: send {} bpm, {}/{}",
        target.bpm, target.beats_per_bar, target.beat_unit
    );
    session.set_link(MetronomeLink::Drive);
    println!("      \"{}\"", session.link().describe());
    let pushed = session.sync_metronome(target).await?;
    println!("      {pushed:?}");
    assert_eq!(pushed, SyncOutcome::Pushed(target));

    // 4. A second sync of the same tempo must not write again.
    println!("\n[4/5] drive again, unchanged: should not write");
    let repeat = session.sync_metronome(target).await?;
    println!("      {repeat:?}");
    assert_eq!(repeat, SyncOutcome::AlreadyMatched(target));

    // 5. Read it back through the instrument itself. This is the actual
    //    done-condition: the tempo set here arrived there.
    println!("\n[5/5] read back from the instrument");
    let readback = session.connection()?.read_metronome().await?;
    println!(
        "      instrument reports {} bpm, {}/{}",
        readback.bpm, readback.beats_per_bar, readback.beat_unit
    );
    if readback.bpm == target.bpm {
        println!("      *** THE TEMPO SET IN WOODSHED REACHED THE INSTRUMENT ***");
    } else {
        println!(
            "      MISMATCH: sent {} bpm, instrument reports {}",
            target.bpm, readback.bpm
        );
    }

    // Put back what was found. The setting may have been deliberate, and a
    // tool that quietly retunes someone's metronome is a tool they stop
    // trusting.
    println!(
        "\nrestoring {} bpm, {}/{}",
        original.bpm, original.beats_per_bar, original.beat_unit
    );
    session.set_link(MetronomeLink::Drive);
    session.sync_metronome(original).await?;
    let restored = session.connection()?.read_metronome().await?;
    println!(
        "  instrument now reports {} bpm, {}/{}",
        restored.bpm, restored.beats_per_bar, restored.beat_unit
    );

    Ok(())
}

fn tempo(bpm: u16) -> Metronome {
    Metronome {
        bpm,
        beats_per_bar: 4,
        beat_unit: 4,
    }
}
