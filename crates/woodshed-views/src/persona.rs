//! The startup persona pick (P1).
//!
//! A vault holding more than one persona, with nobody chosen, is the one case
//! where Woodshed cannot answer "whose practice is this?" on its own. It asks,
//! once, before the practice store opens — because opening it would mint a
//! `default` persona beside the two the user already has, and seal the session
//! to a stranger.
//!
//! The list itself is not woodshed's: [`persona_picker`] renders the roster the
//! same way in every Merely application, so a persona reads the same here as it
//! does in Turnstone. What is woodshed's is where it appears (a gate screen in
//! place of the product root) and what happens next (`woodshed-genet` writes the
//! choice and reopens the store).
//!
//! **The gate is not a lock.** Escape dismisses it and practice proceeds on the
//! convention choice, the same doctrine as "sealing is not a gate": a tuner has
//! to open on a machine whose vault will not.

use cambium::{el, lens, map_action, text, CommandState};
use personae::roster::Roster;
use persona_picker::{persona_picker, picker_state, PickerEvent};

use crate::stage::{UiChild, UiState};

/// What the gate is waiting on: the roster to choose from, the picker's own
/// interaction state, and the answer once there is one.
pub struct PersonaPick {
    /// The vault's personas, read before the store opened.
    pub roster: Roster,
    /// Selection and keyboard state for the shared picker.
    pub picker: CommandState,
    /// The answer, consumed by the host after dispatch. One-shot: the host
    /// takes it, acts, and clears the whole pick.
    pub outcome: Option<PickerEvent>,
    /// Said back to the user when the picker asks for something this slice does
    /// not do yet. Empty for the ordinary path.
    pub notice: Option<String>,
}

impl PersonaPick {
    pub fn new(roster: Roster) -> Self {
        Self {
            roster,
            picker: picker_state(),
            outcome: None,
            notice: None,
        }
    }

    /// Record what the picker reported, for the host to act on.
    ///
    /// `CreateRequested` is P3's flow and is answered here rather than dropped:
    /// a row that silently does nothing reads as a broken application, so the
    /// gate says where a persona actually comes from today and stays open.
    pub fn record(&mut self, event: PickerEvent) {
        match event {
            PickerEvent::CreateRequested => {
                self.notice = Some(
                    "Woodshed cannot mint a persona yet. Create one with the \
                     personae-vault tool, or choose one below."
                        .into(),
                );
            }
            chosen_or_dismissed => {
                self.notice = None;
                self.outcome = Some(chosen_or_dismissed);
            }
        }
    }
}

/// The gate screen: who is practising, and the roster to answer with.
///
/// Rendered in place of the product root, not over it. Nothing is behind it
/// yet — the session has not been read, because the persona that would decrypt
/// it is what this screen is asking for — so a scrim over an empty stage would
/// be a lie about what is loaded.
pub fn persona_gate(pick: &PersonaPick) -> UiChild {
    let roster = pick.roster.clone();
    let picker = map_action(
        lens(
            move |state: &mut CommandState| persona_picker(state, &roster),
            |ui: &mut UiState| {
                // The gate is only rendered while `persona` is `Some`, so this
                // projection cannot be reached without it.
                &mut ui
                    .persona
                    .as_mut()
                    .expect("the persona gate renders only while a pick is open")
                    .picker
            },
        ),
        |ui: &mut UiState, event: PickerEvent| {
            if let Some(pick) = ui.persona.as_mut() {
                pick.record(event);
            }
        },
    );
    let notice = pick.notice.as_ref().map(|notice| {
        el("div", text(notice.clone()))
            .attr("class", "persona-notice")
            .attr("role", "status")
    });
    Box::new(
        el(
            "div",
            el(
                "div",
                (
                    el("div", text("Who is practising?")).attr("class", "persona-title"),
                    // What protects the vault, as the backend reports it. Shown
                    // rather than guessed: it is the honest answer to "where did
                    // these come from", and it changes per machine.
                    el("div", text(pick.roster.description.clone()))
                        .attr("class", "persona-vault"),
                    Box::new(picker) as UiChild,
                    notice,
                    el(
                        "div",
                        text("Escape practises as the usual persona without choosing."),
                    )
                    .attr("class", "persona-hint"),
                ),
            )
            .attr("class", "persona-card")
            .attr("role", "dialog")
            .attr("aria-label", "Choose a persona"),
        )
        .attr("class", "root persona-gate"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use personae::roster::RosterEntry;
    use personae::vault::ProfileId;

    pub(crate) fn roster(entries: &[(&str, usize, bool)]) -> Roster {
        let entries: Vec<RosterEntry> = entries
            .iter()
            .map(|(id, slots, chosen)| RosterEntry {
                id: ProfileId((*id).into()),
                display_name: (*id).into(),
                slot_count: *slots,
                chosen: *chosen,
            })
            .collect();
        let chosen = entries
            .iter()
            .find(|entry| entry.chosen)
            .map(|entry| entry.id.clone())
            .unwrap_or(ProfileId("default".into()));
        Roster {
            entries,
            chosen,
            description: "test vault".into(),
        }
    }

    #[test]
    fn a_choice_is_recorded_for_the_host_to_act_on() {
        let mut pick = PersonaPick::new(roster(&[("work", 2, false), ("alt", 0, false)]));
        pick.record(PickerEvent::Chose(ProfileId("alt".into())));
        assert_eq!(pick.outcome, Some(PickerEvent::Chose(ProfileId("alt".into()))));
    }

    #[test]
    fn dismissal_is_an_answer_too_so_the_host_can_proceed_unblocked() {
        // Escape must reach the host, not just close the screen: the host is
        // what opens the store on the convention choice.
        let mut pick = PersonaPick::new(roster(&[("work", 2, false), ("alt", 0, false)]));
        pick.record(PickerEvent::Dismissed);
        assert_eq!(pick.outcome, Some(PickerEvent::Dismissed));
    }

    #[test]
    fn asking_for_a_new_persona_says_what_to_do_instead_of_nothing() {
        // The create row is the shared picker's, and P1 does not implement it.
        // It must not read as a dead control.
        let mut pick = PersonaPick::new(roster(&[("work", 2, false), ("alt", 0, false)]));
        pick.record(PickerEvent::CreateRequested);
        assert!(pick.outcome.is_none(), "the gate stays open");
        assert!(pick.notice.as_deref().is_some_and(|n| n.contains("personae-vault")));
    }

    #[test]
    fn a_later_choice_clears_an_earlier_notice() {
        let mut pick = PersonaPick::new(roster(&[("work", 2, false), ("alt", 0, false)]));
        pick.record(PickerEvent::CreateRequested);
        pick.record(PickerEvent::Chose(ProfileId("work".into())));
        assert!(pick.notice.is_none());
    }
}
