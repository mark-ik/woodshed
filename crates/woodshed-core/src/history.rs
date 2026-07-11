//! Portable practice-engagement history.

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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PracticeEvent {
    pub subject_id: String,
    pub kind: EngagementKind,
    #[serde(default)]
    pub from_id: Option<String>,
    /// Persisted local ordering. A host can enrich this with wall-clock time
    /// when history gains calendar views.
    pub sequence: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PracticeHistory {
    pub events: Vec<PracticeEvent>,
    next_sequence: u64,
}

impl PracticeHistory {
    pub fn record(
        &mut self,
        subject_id: impl Into<String>,
        kind: EngagementKind,
        from_id: Option<String>,
    ) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.events.push(PracticeEvent {
            subject_id: subject_id.into(),
            kind,
            from_id,
            sequence,
        });
    }

    pub fn related_transition_count(&self, from_id: &str, subject_id: &str) -> usize {
        self.events
            .iter()
            .filter(|event| {
                event.subject_id == subject_id
                    && event.from_id.as_deref() == Some(from_id)
                    && matches!(
                        event.kind,
                        EngagementKind::Staged
                            | EngagementKind::Rehearsed
                            | EngagementKind::Completed
                            | EngagementKind::Looped
                            | EngagementKind::Recorded
                    )
            })
            .count()
    }

    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &PracticeEvent> {
        self.events.iter().rev().take(limit)
    }
}

pub fn catalog_id_for_card(card: &Card) -> String {
    match &card.material {
        Material::Scale { name, .. } => woodshed_graph::scale_id(name),
        Material::Chord { name, .. } if matches!(card.touch, Touch::Arpeggiate { .. }) => {
            woodshed_graph::arpeggio_id(name)
        }
        Material::Chord { name, .. } => woodshed_graph::chord_id(name),
        Material::Riff { name } => woodshed_graph::exercise_id(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_transitions_ignore_preview_only_events() {
        let mut history = PracticeHistory::default();
        history.record(
            "chord:Minor",
            EngagementKind::Previewed,
            Some("scale:Dorian".into()),
        );
        history.record(
            "chord:Minor",
            EngagementKind::Staged,
            Some("scale:Dorian".into()),
        );
        assert_eq!(history.related_transition_count("scale:Dorian", "chord:Minor"), 1);
        assert_eq!(history.events[0].sequence, 0);
        assert_eq!(history.events[1].sequence, 1);
    }
}
