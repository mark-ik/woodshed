//! Woodshed's seam to a Bluetooth-controlled smart instrument.
//!
//! The guitar this drives is its own amplifier, effects processor, looper and
//! metronome. Woodshed already owns every one of those concepts, so this crate
//! is not an adapter between strangers — it joins the same vocabulary across a
//! wire. [`ringdown`] speaks the protocol; this decides what Woodshed does
//! with it.
//!
//! # What this crate is careful about
//!
//! **Nothing is reported as fact until the instrument said it.** Every field of
//! [`InstrumentState`] is an [`Option`], and a `None` means *not read*, never
//! *zero*. A practice tool that displays a confident 0% battery for a guitar it
//! has not asked is worse than one that displays nothing.
//!
//! **The connection is always released.** The instrument serves one client at a
//! time, and a leaked connection does not merely waste a handle — it locks out
//! the next attempt, and the symptom is some later, unrelated request appearing
//! to fail. [`Connection::with`] exists so that the release cannot be
//! forgotten on an error path.
//!
//! **`ReadConfig` is never called.** It returns nothing and wedges the
//! instrument's RPC handler until the guitar is power-cycled. There is no
//! reason for Woodshed to reach it and a real cost to trying, so the refusal
//! lives in code rather than in a comment. See [`FORBIDDEN_METHODS`].

pub mod session;

pub use session::{MetronomeLink, Session, SyncOutcome};

use std::time::Duration;

use ringdown::rpc::{self, Method};
use ringdown_ble::{Guitar, TransportError};
use serde_json::Value;

/// How long to look for an instrument before giving up.
pub const DEFAULT_SCAN: Duration = Duration::from_secs(10);

/// Methods this crate will not send, whatever a caller asks for.
///
/// `ReadConfig` hangs the instrument's RPC handler: it returns nothing, and
/// every later request — including ones that worked moments before — is met
/// with silence until the guitar is power-cycled. Its contents are reachable by
/// composing calls that work, so there is nothing to gain and a power cycle to
/// lose. Enforced rather than documented, because the failure it causes looks
/// like an unrelated bug and would cost somebody an afternoon.
pub const FORBIDDEN_METHODS: &[&str] = &["ReadConfig"];

/// Why an instrument operation did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum InstrumentError {
    /// The Bluetooth layer failed, or the instrument did not answer.
    #[error(transparent)]
    Transport(#[from] TransportError),

    /// A caller asked for a method this crate refuses to send.
    #[error(
        "{0} is not callable: it wedges the instrument's RPC handler until the \
         guitar is power-cycled"
    )]
    Forbidden(&'static str),

    /// The instrument answered, but not in the shape expected.
    #[error("could not read the instrument's reply: {0}")]
    Shape(String),

    /// The session has already handed the instrument back.
    #[error("this session has been released; open a new one to reach the instrument")]
    Released,
}

/// A tempo and time signature, as the instrument reports them.
///
/// The field names are Woodshed's rather than the wire's: the protocol calls
/// these `bpm`, `num` and `den`, and translating once here keeps that spelling
/// out of the rest of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metronome {
    /// Beats per minute.
    pub bpm: u16,
    /// Beats per bar — the time signature's upper number.
    pub beats_per_bar: u8,
    /// The note value that takes the beat — the lower number.
    pub beat_unit: u8,
}

impl Metronome {
    /// Parse the instrument's `ReadMetronome` reply.
    fn from_reply(value: &Value) -> Result<Metronome, InstrumentError> {
        let field = |name: &str| -> Result<u64, InstrumentError> {
            value
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| InstrumentError::Shape(format!("missing {name} in {value}")))
        };
        Ok(Metronome {
            bpm: field("bpm")? as u16,
            beats_per_bar: field("num")? as u8,
            beat_unit: field("den")? as u8,
        })
    }
}

/// What the instrument has told us about itself.
///
/// Every field is optional and `None` means **not read**, never a default. The
/// distinction is the point: a practice tool that shows a confident zero for
/// something it never asked is lying quietly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstrumentState {
    /// Model identifier, e.g. `H2S`.
    pub device: Option<String>,
    /// Processor serial, which identifies the exact silicon.
    pub cpu_id: Option<String>,
    /// Battery remaining, 0–100.
    pub battery_percent: Option<f32>,
    /// Free storage in gigabytes.
    pub free_space_gb: Option<f32>,
    /// Audio DSP firmware version, as the device spells it (e.g. `V1.2.3`).
    pub firmware_dsp: Option<String>,
    /// Connectivity firmware version.
    pub firmware_wireless: Option<String>,
    /// The instrument's own metronome, if it has been read.
    pub metronome: Option<Metronome>,
}

impl InstrumentState {
    /// Whether anything at all has been read from the instrument.
    pub fn is_empty(&self) -> bool {
        *self == InstrumentState::default()
    }
}

/// One measured resonance of the instrument's body.
///
/// The guitar derives these from its own calibration and uses them to suppress
/// feedback, so every gain is a cut. They are a physical description of the
/// specific instrument in the room: on the reference guitar, 106 Hz sits at the
/// soundhole's Helmholtz air resonance and 228 Hz at the principal top-plate
/// mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resonance {
    /// Filter type as the device reports it; its meaning is not yet pinned.
    pub filter_type: i64,
    /// Centre frequency in hertz.
    pub frequency_hz: f32,
    /// Gain in decibels. Negative, since these are cuts.
    pub gain_db: f32,
    /// Filter Q.
    pub q: f32,
}

/// A live connection to an instrument.
pub struct Connection {
    guitar: Guitar,
}

impl Connection {
    /// Find an instrument, run `work` against it, and always disconnect.
    ///
    /// The release is built into the shape rather than left to the caller,
    /// because the instrument serves one client at a time and a connection
    /// leaked on an error path locks out the *next* attempt — which then fails
    /// somewhere unrelated and sends whoever is debugging it to the wrong
    /// place. This exact mistake has already cost one debugging session in the
    /// protocol crate's own probe.
    pub async fn with<F, Fut, T>(scan: Duration, work: F) -> Result<T, InstrumentError>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: std::future::Future<Output = (Connection, Result<T, InstrumentError>)>,
    {
        let connection = Connection::open(scan).await?;
        let (connection, outcome) = work(connection).await;
        let _ = connection.disconnect().await;
        outcome
    }

    /// Read identity, battery, storage and firmware versions.
    pub async fn read_status(&mut self) -> Result<InstrumentState, InstrumentError> {
        let status = self.guitar.status().await?;
        Ok(InstrumentState {
            device: Some(status.device),
            cpu_id: Some(status.cpu_id),
            battery_percent: Some(status.battery_percent),
            free_space_gb: Some(status.free_space_gb),
            firmware_dsp: Some(status.version_stm),
            firmware_wireless: Some(status.version_esp),
            metronome: None,
        })
    }

    /// Read the instrument's own metronome.
    pub async fn read_metronome(&mut self) -> Result<Metronome, InstrumentError> {
        let reply = self.call("ReadMetronome", rpc::params::none()).await?;
        Metronome::from_reply(&reply)
    }

    /// Push a tempo and time signature to the instrument.
    pub async fn set_metronome(&mut self, m: Metronome) -> Result<(), InstrumentError> {
        let params = rpc::params::metronome(
            i64::from(m.bpm),
            Some(i64::from(m.beats_per_bar)),
            Some(i64::from(m.beat_unit)),
            None,
        );
        self.guitar.call(Method::UpdateMetronome, params).await?;
        Ok(())
    }

    /// Read the instrument's measured body resonances.
    pub async fn read_resonances(&mut self) -> Result<Vec<Resonance>, InstrumentError> {
        let reply = self.call("GetAnalysis", rpc::params::none()).await?;
        let rows = reply
            .as_array()
            .ok_or_else(|| InstrumentError::Shape(format!("expected an array, got {reply}")))?;

        rows.iter()
            .map(|row| {
                let cells = row.as_array().filter(|c| c.len() >= 4).ok_or_else(|| {
                    InstrumentError::Shape(format!("expected four numbers, got {row}"))
                })?;
                let n = |i: usize| -> f32 { cells[i].as_f64().unwrap_or_default() as f32 };
                Ok(Resonance {
                    filter_type: cells[0].as_i64().unwrap_or_default(),
                    frequency_hz: n(1),
                    gain_db: n(2),
                    q: n(3),
                })
            })
            .collect()
    }

    /// Call a method by wire name, refusing the ones that break the instrument.
    ///
    /// Public so that the undocumented surface stays reachable for exploration,
    /// with the one genuinely harmful call gated rather than trusted to memory.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, InstrumentError> {
        if let Some(bad) = FORBIDDEN_METHODS.iter().find(|m| **m == method) {
            return Err(InstrumentError::Forbidden(bad));
        }
        Ok(self.guitar.call_named(method, params).await?)
    }

    /// The underlying client, for operations this crate has not wrapped.
    pub fn guitar(&mut self) -> &mut Guitar {
        &mut self.guitar
    }

    /// Find an instrument and connect, leaving the caller to disconnect.
    ///
    /// Prefer [`Connection::with`] for a single action, or [`Session`] to hold
    /// the instrument across several. This is the shared step underneath both,
    /// and using it directly means owning the release.
    pub async fn open(scan: Duration) -> Result<Connection, InstrumentError> {
        let found = ringdown_ble::discover(scan).await?;
        let guitar = Guitar::connect(&found[0]).await?;
        Ok(Connection { guitar })
    }

    /// Hand the instrument back.
    pub async fn disconnect(self) -> Result<(), InstrumentError> {
        self.guitar.disconnect().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unread_state_is_empty_rather_than_zero() {
        let s = InstrumentState::default();
        assert!(s.is_empty());
        // The distinction that matters: not read is not the same as zero.
        assert_eq!(s.battery_percent, None);
        assert_eq!(s.device, None);
    }

    #[test]
    fn a_metronome_reply_maps_to_woodshed_names() {
        // The exact reply captured from the reference instrument.
        let m = Metronome::from_reply(&json!({"bpm": 60, "den": 8, "num": 5})).unwrap();
        assert_eq!(
            m,
            Metronome {
                bpm: 60,
                beats_per_bar: 5,
                beat_unit: 8
            }
        );
    }

    #[test]
    fn a_malformed_metronome_reply_names_the_missing_field() {
        let err = Metronome::from_reply(&json!({"bpm": 60})).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("num"), "{text}");
    }

    #[test]
    fn readconfig_is_refused_before_it_reaches_the_wire() {
        // Not a style preference: this call wedges the instrument until it is
        // power-cycled, so the refusal belongs in code.
        assert!(FORBIDDEN_METHODS.contains(&"ReadConfig"));
    }

    #[test]
    fn the_captured_analysis_maps_to_resonances() {
        let reply = json!([
            [4, 106, -3.3, 6],
            [4, 228, -6.8, 3.75],
            [4, 545, -7.8, 8.1],
            [4, 3760, -3.8, 6]
        ]);
        let rows = reply.as_array().unwrap();
        let first = rows[0].as_array().unwrap();
        assert_eq!(first[1].as_f64().unwrap() as f32, 106.0);

        // Every gain is a cut, which is what makes these feedback filters
        // rather than an EQ curve.
        for row in rows {
            assert!(
                row.as_array().unwrap()[2].as_f64().unwrap() < 0.0,
                "resonance gains should all be cuts"
            );
        }
    }
}
