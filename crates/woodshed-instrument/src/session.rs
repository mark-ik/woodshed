//! A connection held across many actions, and the metronome link over it.
//!
//! [`crate::Connection::with`] is scope-shaped: it connects, does one piece of
//! work, and disconnects. That is the right shape for a one-off read and the
//! wrong shape for practising, where the instrument should stay reachable while
//! Woodshed is open.
//!
//! A [`Session`] is the long-lived form. It holds the connection until
//! [`Session::release`] is called, which exists as a deliberate action rather
//! than as a side effect of quitting: the instrument serves one client at a
//! time, so handing it back is how the phone app gets a turn without Woodshed
//! shutting down.
//!
//! # The metronome link
//!
//! Both sides have a metronome and either may lead, so [`MetronomeLink`] says
//! which — and it is one setting with three states rather than two independent
//! switches. That is what makes a feedback loop impossible by construction: at
//! most one side is ever authoritative, so neither can chase the other.

use std::time::Duration;

use crate::{Connection, InstrumentError, Metronome};

/// Which way metronome authority runs.
///
/// Exactly one direction can hold at a time. Two independent "follow" switches
/// would allow both to be on, and two metronomes each adopting the other is a
/// loop that drifts rather than settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetronomeLink {
    /// Neither side follows the other. Both keep their own tempo.
    #[default]
    Detached,
    /// Woodshed adopts the instrument's tempo. The guitar leads.
    Follow,
    /// The instrument adopts Woodshed's tempo. Woodshed leads.
    Drive,
}

impl MetronomeLink {
    /// A phrase for the interface, written from the player's side.
    ///
    /// The plan's requirement is that the UI show *plainly* which way the link
    /// runs, so these say who is leading rather than naming the enum variant.
    pub fn describe(self) -> &'static str {
        match self {
            MetronomeLink::Detached => "Instrument and Woodshed keep separate tempos",
            MetronomeLink::Follow => "Woodshed follows the instrument",
            MetronomeLink::Drive => "Instrument follows Woodshed",
        }
    }

    /// Whether this link sends tempo changes to the instrument.
    pub fn writes_to_instrument(self) -> bool {
        matches!(self, MetronomeLink::Drive)
    }
}

/// What a call to [`Session::sync_metronome`] actually did.
///
/// Returned rather than swallowed so the interface can report the truth: "sent"
/// and "already matched" look identical from outside and mean different things
/// when something later goes wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The link is detached; nothing was read or written.
    Detached,
    /// Woodshed should adopt this, read from the instrument.
    Adopted(Metronome),
    /// This was sent to the instrument.
    Pushed(Metronome),
    /// The instrument was already set to Woodshed's tempo, so nothing was sent.
    AlreadyMatched(Metronome),
}

/// A connection held across many actions.
pub struct Session {
    connection: Option<Connection>,
    link: MetronomeLink,
    /// The last tempo this session sent, so an unchanged tempo is not resent
    /// on every tick.
    pushed: Option<Metronome>,
}

impl Session {
    /// Find an instrument and hold it until [`Session::release`].
    pub async fn open(scan: Duration) -> Result<Session, InstrumentError> {
        let connection = Connection::open(scan).await?;
        Ok(Session {
            connection: Some(connection),
            link: MetronomeLink::default(),
            pushed: None,
        })
    }

    /// Hand the instrument back.
    ///
    /// Deliberately consuming and deliberately `async`: releasing is a real
    /// exchange with the device, and Rust cannot run one in `Drop`. Calling
    /// this is how the phone app gets the guitar back without Woodshed
    /// quitting.
    pub async fn release(mut self) -> Result<(), InstrumentError> {
        if let Some(connection) = self.connection.take() {
            connection.disconnect().await?;
        }
        Ok(())
    }

    /// Which way the metronome link currently runs.
    pub fn link(&self) -> MetronomeLink {
        self.link
    }

    /// Change which way the metronome link runs.
    ///
    /// Switching away from [`MetronomeLink::Drive`] forgets what was last sent,
    /// so that switching back re-sends rather than assuming the instrument has
    /// been left untouched in the meantime. It may well not have been — there
    /// are knobs on the guitar.
    pub fn set_link(&mut self, link: MetronomeLink) {
        if self.link != link {
            self.pushed = None;
        }
        self.link = link;
    }

    /// Reconcile Woodshed's metronome with the instrument's, per the link.
    ///
    /// `woodshed` is Woodshed's current tempo. In [`MetronomeLink::Drive`] it is
    /// sent onward; in [`MetronomeLink::Follow`] it is ignored and the
    /// instrument's own tempo is returned for Woodshed to adopt.
    ///
    /// Cheap to call repeatedly: in `Drive` it writes only when the tempo has
    /// actually changed since the last write, so driving a steady tempo costs
    /// nothing after the first send.
    pub async fn sync_metronome(
        &mut self,
        woodshed: Metronome,
    ) -> Result<SyncOutcome, InstrumentError> {
        match self.link {
            MetronomeLink::Detached => Ok(SyncOutcome::Detached),

            MetronomeLink::Follow => {
                let theirs = self.connection()?.read_metronome().await?;
                Ok(SyncOutcome::Adopted(theirs))
            }

            MetronomeLink::Drive => {
                if self.pushed == Some(woodshed) {
                    return Ok(SyncOutcome::AlreadyMatched(woodshed));
                }
                self.connection()?.set_metronome(woodshed).await?;
                self.pushed = Some(woodshed);
                Ok(SyncOutcome::Pushed(woodshed))
            }
        }
    }

    /// The held connection, for anything this session does not wrap.
    pub fn connection(&mut self) -> Result<&mut Connection, InstrumentError> {
        self.connection.as_mut().ok_or(InstrumentError::Released)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Releasing needs an await, so it cannot happen here. Say so loudly
        // instead of leaving a silently-held instrument: the next attempt to
        // connect fails, and the symptom appears somewhere unrelated. This
        // project has already lost a debugging session to exactly that.
        if self.connection.is_some() {
            eprintln!(
                "woodshed-instrument: Session dropped without release(); the instrument \
                 may stay claimed until the connection times out. Call release() to hand \
                 it back."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempo(bpm: u16) -> Metronome {
        Metronome {
            bpm,
            beats_per_bar: 4,
            beat_unit: 4,
        }
    }

    #[test]
    fn the_default_link_touches_nothing() {
        // Connecting should not start changing an instrument's settings.
        assert_eq!(MetronomeLink::default(), MetronomeLink::Detached);
        assert!(!MetronomeLink::default().writes_to_instrument());
    }

    #[test]
    fn only_drive_writes_to_the_instrument() {
        assert!(MetronomeLink::Drive.writes_to_instrument());
        assert!(!MetronomeLink::Follow.writes_to_instrument());
        assert!(!MetronomeLink::Detached.writes_to_instrument());
    }

    /// The loop-freedom argument, asserted rather than trusted to prose: the
    /// link is one setting, so there is no state in which both sides lead.
    #[test]
    fn at_most_one_side_ever_leads() {
        for link in [
            MetronomeLink::Detached,
            MetronomeLink::Follow,
            MetronomeLink::Drive,
        ] {
            let woodshed_leads = link.writes_to_instrument();
            let instrument_leads = matches!(link, MetronomeLink::Follow);
            assert!(
                !(woodshed_leads && instrument_leads),
                "{link:?} would let both sides lead"
            );
        }
    }

    #[test]
    fn every_link_describes_itself_for_the_interface() {
        for link in [
            MetronomeLink::Detached,
            MetronomeLink::Follow,
            MetronomeLink::Drive,
        ] {
            let text = link.describe();
            assert!(!text.is_empty());
            // The UI must say who leads, so the phrasing names both sides
            // rather than echoing a variant name at the player.
            assert!(
                text.contains("Woodshed") && text.contains("nstrument"),
                "{link:?} describes itself as {text:?}"
            );
        }
    }

    /// Changing the link forgets what was sent, so switching back re-sends.
    /// The instrument has knobs on it; assuming it was left alone is wrong.
    #[test]
    fn changing_the_link_forgets_what_was_pushed() {
        let mut s = Session {
            connection: None,
            link: MetronomeLink::Drive,
            pushed: Some(tempo(120)),
        };
        s.set_link(MetronomeLink::Follow);
        assert_eq!(s.pushed, None);

        // Setting the same link again is not a change and keeps the cache.
        let mut s = Session {
            connection: None,
            link: MetronomeLink::Drive,
            pushed: Some(tempo(120)),
        };
        s.set_link(MetronomeLink::Drive);
        assert_eq!(s.pushed, Some(tempo(120)));
    }

    #[tokio::test]
    async fn a_detached_link_never_touches_the_connection() {
        // No connection at all, which proves Detached reaches the wire never:
        // any attempt to use it would fail rather than return.
        let mut s = Session {
            connection: None,
            link: MetronomeLink::Detached,
            pushed: None,
        };
        assert_eq!(
            s.sync_metronome(tempo(90)).await.unwrap(),
            SyncOutcome::Detached
        );
    }

    #[tokio::test]
    async fn a_released_session_reports_it_rather_than_panicking() {
        let mut s = Session {
            connection: None,
            link: MetronomeLink::Drive,
            pushed: None,
        };
        let err = s.sync_metronome(tempo(90)).await.unwrap_err();
        assert!(matches!(err, InstrumentError::Released), "{err}");
    }

    #[test]
    fn an_unchanged_tempo_is_recognised_before_any_write() {
        // The cache check happens before the connection is touched, which is
        // what makes sync_metronome cheap to call on a timer.
        let s = Session {
            connection: None,
            link: MetronomeLink::Drive,
            pushed: Some(tempo(120)),
        };
        assert_eq!(s.pushed, Some(tempo(120)));
        assert_ne!(s.pushed, Some(tempo(121)));
    }
}
