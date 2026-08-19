//! Portable practice-engagement history, over chartulary's lineage model.
//!
//! **Adopted stemma 2026-07-26.** This was a hand-rolled event log until the
//! strength model came up and the investigation found woodshed already depended
//! on a better one: `chartulary::stemma` maintains per-subject
//! `first_seen_at_ms` / `last_seen_at_ms` / `visit_count`, and aggregates
//! per-pair traversals with their own recency — exactly the inputs a decayed,
//! recency-weighted strength model needs, and exactly what the local log did not
//! keep. Building the model on the local log first would have been the throwaway
//! path.
//!
//! The split follows the same line as the typed relations: **woodshed owns the
//! musical judgment, the substrate owns the structure.** Stemma holds the dated,
//! branching lineage; woodshed names the engagement kinds and decides what they
//! weigh. So the vocabulary here is unchanged and every consumer still speaks
//! [`PracticeHistory`] — only the store beneath it moved.
//!
//! Two deliberate mappings:
//!
//! - **The kind lives in the visit's `context`, not in stemma's
//!   `TransitionKind`.** Those variants answer "how did you get here"
//!   (`LinkClick`, `Back`, `Reload`) and map one-to-one onto a browsing trace;
//!   previewed / staged / completed answer "what did you do with it". Forcing
//!   one into the other would be a costume, so the app's own semantics ride the
//!   payload slot built for them.
//! - **Datedness rides the context too.** `visit_entry` takes a `u64`, not an
//!   `Option`, so an undated engagement (a host with no clock, or a session
//!   written before woodshed kept time) records `at_ms = 0` with
//!   `dated: false`. A reader must never read that as 1970.

use chartulary::stemma::{EntryPrivacy, Stemma, TransitionKind};
use serde::de::{Deserialize as _, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use woodshedding::rehearsal::{Card, Material, Touch};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngagementKind {
    Previewed,
    Staged,
    Rehearsed,
    Completed,
    Looped,
    Recorded,
}

impl EngagementKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Previewed => "Previewed",
            Self::Staged => "Staged",
            Self::Rehearsed => "Rehearsed",
            Self::Completed => "Completed",
            Self::Looped => "Looped",
            Self::Recorded => "Recorded",
        }
    }

    /// Whether this kind is evidence of *practice* rather than of interest.
    ///
    /// The plan's own line: preview and staging are evidence of interest;
    /// completed rehearsal time is evidence of practice. Ranking leans on the
    /// distinction, so it lives on the kind rather than in each query's filter.
    pub fn is_practice(self) -> bool {
        !matches!(self, Self::Previewed)
    }
}

/// What woodshed hangs on one stemma visit: the engagement's kind, the practice
/// span where there is one, whether its timestamp is real, and the subject the
/// player chose it *from*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engagement {
    pub kind: EngagementKind,
    /// The subject this engagement was chosen from, as the player's stated
    /// reason — staging a suggestion names where it came from.
    ///
    /// Deliberately not left to the lineage's own edge. Stemma parents a visit
    /// to the path actually walked, which is a different fact: "what I looked at
    /// before" is not "what I chose this from", and a first engagement has no
    /// parent at all yet can still name its source. Both facts are worth
    /// keeping, so the walked path stays stemma's and the stated reason rides
    /// here — the same division that puts the kind here rather than in
    /// `TransitionKind`.
    #[serde(default)]
    pub from_id: Option<String>,
    /// Elapsed practice, milliseconds, where the event has a span to measure —
    /// a completed rehearsal card. `None` for the instantaneous kinds.
    #[serde(default)]
    pub practiced_ms: Option<u64>,
    /// Whether the visit's `created_at_ms` is an observation or a placeholder.
    #[serde(default)]
    pub dated: bool,
}

/// The practice lineage: catalog ids as entry keys, no entry payload (the
/// catalog owns names; history must not copy them), one owner, and woodshed's
/// [`Engagement`] as the per-visit context.
type Lineage = Stemma<String, (), String, Engagement>;

/// The single owner of the practice lineage. Woodshed has one practitioner, so
/// one cursor: the visit path is the practice path. Per-session owners would
/// branch it, and nothing asks for that yet.
const PRACTITIONER: &str = "practitioner";

/// One engagement, read back out. A view over the lineage rather than the
/// stored shape, so consumers keep the flat vocabulary they had.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PracticeEvent {
    pub subject_id: String,
    pub kind: EngagementKind,
    /// The subject engaged immediately before this one, along the practice path.
    #[serde(default)]
    pub from_id: Option<String>,
    /// Unix epoch milliseconds; `None` when the engagement is undated.
    #[serde(default)]
    pub at_ms: Option<u64>,
    #[serde(default)]
    pub practiced_ms: Option<u64>,
}

/// One aggregated movement in the practice path. Catalog and history share
/// the same ids, so this can join the material graph without copying nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PracticeTransition {
    pub from_id: String,
    pub to_id: String,
    pub traversal_count: u64,
    pub latest_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct PracticeHistory {
    lineage: Lineage,
}

impl PracticeHistory {
    /// Record an engagement at `at_ms` (Unix epoch milliseconds from the host;
    /// `None` where no clock is available), with `practiced_ms` where the
    /// engagement has a measured span.
    ///
    /// `from_id` is the player's stated reason and is stored as such; the
    /// lineage separately records the path actually walked. Keeping both is why
    /// a single "staged Minor 7 from Dorian" still carries its provenance even
    /// though it is the first engagement and has no parent visit.
    pub fn record(
        &mut self,
        at_ms: Option<u64>,
        subject_id: impl Into<String>,
        kind: EngagementKind,
        from_id: Option<String>,
        practiced_ms: Option<u64>,
    ) {
        let stamp = at_ms.unwrap_or_default();
        let owner = self.lineage.ensure_owner(PRACTITIONER.to_string(), None);
        let entry = self.lineage.resolve_or_create_entry(
            subject_id.into(),
            (),
            stamp,
            EntryPrivacy::LocalOnly,
        );
        // The lineage is local practice, and the transition vocabulary is
        // navigation's; `Unknown` is the honest value for a movement woodshed
        // does not describe in those terms. The kind rides the context.
        let _ = self.lineage.visit_entry(
            owner,
            entry,
            Engagement {
                kind,
                from_id,
                practiced_ms,
                dated: at_ms.is_some(),
            },
            TransitionKind::Unknown,
            stamp,
        );
    }

    /// How many times the player chose `subject_id` *from* `from_id`, counting
    /// only the kinds that are evidence of practice.
    ///
    /// The stated reason, not the walked path — this is what ranks a suggestion
    /// the player has taken before. For the path, see [`Self::traversals`].
    pub fn related_transition_count(&self, from_id: &str, subject_id: &str) -> usize {
        self.visits_of(subject_id)
            .filter(|context| context.kind.is_practice())
            .filter(|context| context.from_id.as_deref() == Some(from_id))
            .count()
    }

    /// How many times practice actually *moved* from one subject to the next,
    /// with the most recent such move — the lineage's own aggregate rather than
    /// the player's stated reason. The recency a decayed strength model wants.
    pub fn traversals(&self, from_id: &str, subject_id: &str) -> (u64, Option<u64>) {
        let (Some(from), Some(to)) = (
            self.lineage.entry_id_by_key(&from_id.to_string()),
            self.lineage.entry_id_by_key(&subject_id.to_string()),
        ) else {
            return (0, None);
        };
        self.lineage
            .aggregated_entry_edges()
            .into_iter()
            .find(|edge| edge.from_entry == from && edge.to_entry == to)
            .map(|edge| {
                let at = (edge.latest_transition_at_ms > 0).then_some(edge.latest_transition_at_ms);
                (edge.traversal_count, at)
            })
            .unwrap_or((0, None))
    }

    /// The most recent engagements, newest first, along the practice path.
    pub fn recent(&self, limit: usize) -> Vec<PracticeEvent> {
        let Some(owner) = self.lineage.owner_id_by_identity(&PRACTITIONER.to_string()) else {
            return Vec::new();
        };
        let path = self
            .lineage
            .linear_history_visits_of_owner(owner)
            .unwrap_or_default();
        let mut out = Vec::new();
        for visit_id in path.iter().rev().take(limit) {
            let Some(visit) = self.lineage.visit(*visit_id) else {
                continue;
            };
            let Some(entry) = self.lineage.entry(visit.entry) else {
                continue;
            };
            out.push(PracticeEvent {
                subject_id: entry.key.clone(),
                kind: visit.context.kind,
                from_id: visit.context.from_id.clone(),
                at_ms: visit.context.dated.then_some(visit.created_at_ms),
                practiced_ms: visit.context.practiced_ms,
            });
        }
        out
    }

    /// Aggregated directed movements through practice history. Stemma remains
    /// the authority; this is the product-shaped projection a mere consumes.
    pub fn transitions(&self) -> Vec<PracticeTransition> {
        self.lineage
            .aggregated_entry_edges()
            .into_iter()
            .filter_map(|edge| {
                let from = self.lineage.entry(edge.from_entry)?;
                let to = self.lineage.entry(edge.to_entry)?;
                Some(PracticeTransition {
                    from_id: from.key.clone(),
                    to_id: to.key.clone(),
                    traversal_count: edge.traversal_count,
                    latest_at_ms: (edge.latest_transition_at_ms > 0)
                        .then_some(edge.latest_transition_at_ms),
                })
            })
            .collect()
    }

    /// Total measured practice on `subject_id`, milliseconds. An hour of
    /// rehearsal outweighs fifty previews rather than tying with them on count.
    pub fn total_practiced_ms(&self, subject_id: &str) -> u64 {
        self.visits_of(subject_id)
            .filter_map(|context| context.practiced_ms)
            .sum()
    }

    /// When `subject_id` was last engaged, if any of its engagements are dated.
    /// The basis for recency: strength has to decay, and decay needs an age.
    pub fn last_seen_ms(&self, subject_id: &str) -> Option<u64> {
        let entry = self.lineage.entry_id_by_key(&subject_id.to_string())?;
        self.lineage
            .visits()
            .filter(|(_, visit)| visit.entry == entry && visit.context.dated)
            .map(|(_, visit)| visit.created_at_ms)
            .max()
    }

    /// How many times `subject_id` has been engaged at all, of any kind.
    /// Maintained by the lineage rather than counted here.
    pub fn engagement_count(&self, subject_id: &str) -> u64 {
        self.lineage
            .entry_id_by_key(&subject_id.to_string())
            .and_then(|id| self.lineage.entry(id))
            .map(|entry| entry.visit_count)
            .unwrap_or_default()
    }

    /// Whether any engagement is dated. A history restored from a session
    /// written before woodshed kept time answers false, which a
    /// recency-weighted reader must know before it ranks by age.
    pub fn has_times(&self) -> bool {
        self.lineage
            .visits()
            .any(|(_, visit)| visit.context.dated)
    }

    /// Total engagements recorded, all subjects.
    pub fn len(&self) -> usize {
        self.lineage.visit_count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn visits_of<'a>(&'a self, subject_id: &str) -> impl Iterator<Item = &'a Engagement> + 'a {
        let entry = self.lineage.entry_id_by_key(&subject_id.to_string());
        self.lineage
            .visits()
            .filter(move |(_, visit)| Some(visit.entry) == entry)
            .map(|(_, visit)| &visit.context)
    }
}

/// The flat log woodshed persisted before adopting the lineage. Read-only: it
/// exists so a saved session migrates, and it is never written again.
#[derive(Deserialize)]
struct FlatLog {
    #[serde(default)]
    events: Vec<PracticeEvent>,
}

/// Either wire form, newest first. A session written by the flat log replays
/// into the lineage on load and persists as a lineage on the next save — the
/// one bounded migration, with no parallel model kept alive.
#[derive(Deserialize)]
#[serde(untagged)]
enum HistoryWire {
    Lineage(chartulary::stemma::StemmaSnapshot<String, (), String, Engagement>),
    Flat(FlatLog),
}

impl Serialize for PracticeHistory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.lineage.to_snapshot().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PracticeHistory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match HistoryWire::deserialize(deserializer)? {
            HistoryWire::Lineage(snapshot) => Self {
                lineage: Lineage::from_snapshot(snapshot),
            },
            HistoryWire::Flat(flat) => {
                // Replay in recorded order. Each engagement keeps its kind,
                // time, and measured span exactly; the edges are rebuilt from
                // the order actually walked, which is what the flat log's
                // `from_id` recorded in every flow that produced one.
                let mut history = Self::default();
                for event in flat.events {
                    history.record(
                        event.at_ms,
                        event.subject_id,
                        event.kind,
                        event.from_id,
                        event.practiced_ms,
                    );
                }
                history
            }
        })
    }
}

/// The catalog subject a card practises, for practice history and the Related
/// graph. `None` when the card has no catalog identity — a hand-drawn path is
/// its own content, not a catalog entry, so it stays out of both.
pub fn catalog_id_for_card(card: &Card) -> Option<String> {
    Some(match &card.material {
        Material::Scale { name, .. } => woodshed_graph::scale_id(name),
        Material::Chord { name, .. } if matches!(card.touch, Touch::Arpeggiate { .. }) => {
            woodshed_graph::arpeggio_id(name)
        }
        Material::Chord { name, .. } => woodshed_graph::chord_id(name),
        Material::Riff { name } => woodshed_graph::exercise_id(name),
        Material::Path { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_transitions_ignore_preview_only_events() {
        let mut history = PracticeHistory::default();
        history.record(
            Some(1_000),
            "scale:Dorian",
            EngagementKind::Rehearsed,
            None,
            None,
        );
        history.record(
            Some(2_000),
            "chord:Minor",
            EngagementKind::Previewed,
            Some("scale:Dorian".into()),
            None,
        );
        assert_eq!(
            history.related_transition_count("scale:Dorian", "chord:Minor"),
            0,
            "an interested glance is not a practised transition"
        );
        history.record(
            Some(3_000),
            "scale:Dorian",
            EngagementKind::Rehearsed,
            None,
            None,
        );
        history.record(
            Some(4_000),
            "chord:Minor",
            EngagementKind::Staged,
            Some("scale:Dorian".into()),
            None,
        );
        assert_eq!(
            history.related_transition_count("scale:Dorian", "chord:Minor"),
            1
        );
    }

    #[test]
    fn an_engagement_keeps_the_time_the_host_supplied() {
        let mut history = PracticeHistory::default();
        history.record(
            Some(1_700_000_000_000),
            "scale:Major",
            EngagementKind::Rehearsed,
            None,
            None,
        );
        assert_eq!(history.last_seen_ms("scale:Major"), Some(1_700_000_000_000));
        assert!(history.has_times());
        assert_eq!(history.engagement_count("scale:Major"), 1);
    }

    #[test]
    fn measured_practice_outweighs_a_pile_of_previews() {
        let mut history = PracticeHistory::default();
        history.record(
            Some(10_000),
            "scale:Dorian",
            EngagementKind::Completed,
            None,
            Some(540_000),
        );
        for at in [1_u64, 2, 3, 4, 5] {
            history.record(Some(at), "scale:Lydian", EngagementKind::Previewed, None, None);
        }
        assert_eq!(history.total_practiced_ms("scale:Dorian"), 540_000);
        assert_eq!(
            history.total_practiced_ms("scale:Lydian"),
            0,
            "an interested glance is not practice time"
        );
        assert_eq!(
            history.engagement_count("scale:Lydian"),
            5,
            "frequency is still counted, and the lineage keeps it"
        );
    }

    #[test]
    fn practice_spans_accumulate_across_sessions() {
        let mut history = PracticeHistory::default();
        history.record(
            Some(10),
            "chord:Major 7",
            EngagementKind::Completed,
            None,
            Some(60_000),
        );
        history.record(
            Some(20),
            "chord:Major 7",
            EngagementKind::Completed,
            None,
            Some(90_000),
        );
        assert_eq!(history.total_practiced_ms("chord:Major 7"), 150_000);
        assert_eq!(history.last_seen_ms("chord:Major 7"), Some(20));
        assert_eq!(history.engagement_count("chord:Major 7"), 2);
    }

    #[test]
    fn recent_reads_newest_first_and_reports_the_stated_reason() {
        let mut history = PracticeHistory::default();
        history.record(Some(1), "scale:Major", EngagementKind::Rehearsed, None, None);
        history.record(
            Some(2),
            "chord:Major 7",
            EngagementKind::Staged,
            Some("scale:Major".into()),
            None,
        );
        let recent = history.recent(2);
        assert_eq!(recent[0].subject_id, "chord:Major 7");
        assert_eq!(recent[0].from_id.as_deref(), Some("scale:Major"));
        assert_eq!(recent[1].subject_id, "scale:Major");
        assert_eq!(recent[1].from_id, None, "nothing was named for the first");
    }

    #[test]
    fn the_stated_reason_and_the_walked_path_are_separate_facts() {
        let mut history = PracticeHistory::default();
        // Practice moved Major -> Minor 7 along the path, but the player named
        // Dorian as the reason for the second one (a suggestion taken from a
        // frontier while the focus sat elsewhere).
        history.record(Some(1), "scale:Major", EngagementKind::Rehearsed, None, None);
        history.record(
            Some(2),
            "chord:Minor 7",
            EngagementKind::Staged,
            Some("scale:Dorian".into()),
            None,
        );

        // The stated reason ranks the suggestion...
        assert_eq!(
            history.related_transition_count("scale:Dorian", "chord:Minor 7"),
            1
        );
        // ...and is not confused with a traversal that never happened.
        assert_eq!(history.traversals("scale:Dorian", "chord:Minor 7"), (0, None));
        // The walked path is its own fact, with its own recency.
        assert_eq!(
            history.traversals("scale:Major", "chord:Minor 7"),
            (1, Some(2))
        );
        assert_eq!(
            history.related_transition_count("scale:Major", "chord:Minor 7"),
            0,
            "the player did not name Major as the reason"
        );
    }

    #[test]
    fn a_lineage_round_trips_through_the_session_wire() {
        let mut history = PracticeHistory::default();
        history.record(Some(1), "scale:Major", EngagementKind::Rehearsed, None, None);
        history.record(
            Some(2),
            "chord:Major 7",
            EngagementKind::Completed,
            Some("scale:Major".into()),
            Some(30_000),
        );

        let json = serde_json::to_string(&history).unwrap();
        let back: PracticeHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.total_practiced_ms("chord:Major 7"), 30_000);
        assert_eq!(back.last_seen_ms("chord:Major 7"), Some(2));
        assert_eq!(
            back.related_transition_count("scale:Major", "chord:Major 7"),
            1,
            "the traversal survives the round trip"
        );
    }

    #[test]
    fn the_flat_log_woodshed_used_to_persist_replays_into_the_lineage() {
        // A session written by the previous store, including the pre-clock
        // shape: no `at_ms`, no `practiced_ms`, and a `next_sequence` field the
        // lineage has no use for.
        let flat = r#"{"events":[
            {"subject_id":"scale:Major","kind":"Rehearsed","from_id":null,"sequence":0},
            {"subject_id":"chord:Minor 7","kind":"Staged","from_id":"scale:Major","sequence":1,
             "at_ms":9000,"practiced_ms":null}
        ],"next_sequence":2}"#;
        let history: PracticeHistory = serde_json::from_str(flat).unwrap();

        assert_eq!(history.len(), 2, "both engagements replayed");
        assert_eq!(
            history.related_transition_count("scale:Major", "chord:Minor 7"),
            1,
            "the traversal the flat log recorded survives the migration"
        );
        // The undated one stays unknown rather than becoming 1970.
        assert_eq!(history.last_seen_ms("scale:Major"), None);
        assert_eq!(history.last_seen_ms("chord:Minor 7"), Some(9_000));
        assert!(history.has_times(), "one dated engagement is enough to rank by age");

        // And it persists as a lineage from here on: no flat log is written back.
        let json = serde_json::to_string(&history).unwrap();
        assert!(!json.contains("next_sequence"));
        assert!(json.contains("visits"));
    }
}
