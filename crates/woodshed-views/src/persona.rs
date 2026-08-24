//! The startup persona pick (P1).
//!
//! A vault holding more than one persona, with nobody chosen, is the one case
//! where Woodshed cannot answer "whose practice is this?" on its own. It asks,
//! once, before the practice store opens — because opening it would mint a
//! `default` persona beside the two the user already has, and seal the session
//! to a stranger.
//!
//! The list itself is not woodshed's: [`persona_picker_focused`] renders the
//! roster the same way in every Merely application, so a persona reads the same
//! here as it does in Turnstone. What is woodshed's is where it appears (a gate screen in
//! place of the product root) and what happens next (`woodshed-genet` writes the
//! choice and reopens the store).
//!
//! **The gate is not a lock.** Escape practises with no persona at all: no key,
//! no store, nothing kept when the window closes. That is the same doctrine as
//! "sealing is not a gate" carried one step further — a tuner has to open on a
//! machine whose vault will not, and it has to open for somebody who does not
//! want to say who they are.

use cambium::{el, lens, map_action, text, CommandState};
use persona_picker::{persona_picker_focused, picker_state, PickerEvent};
use personae::roster::Roster;

use crate::stage::{UiChild, UiState};

/// Why the gate is up, which is what declining it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickPurpose {
    /// Startup, with nothing loaded behind the gate. Declining practises with
    /// no persona: no store opens, and nothing is saved.
    Startup,
    /// A deliberate switch from Settings, with a session already open and
    /// sealed to somebody. Declining changes nothing at all.
    Switch,
}

/// What the gate is waiting on: the roster to choose from, the picker's own
/// interaction state, and the answer once there is one.
pub struct PersonaPick {
    /// Startup or a deliberate switch. The host reads it to know what a
    /// dismissal means; the view reads it to say so on screen.
    pub purpose: PickPurpose,
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
    /// The startup gate: asked once, before the store opens.
    pub fn new(roster: Roster) -> Self {
        Self::for_purpose(roster, PickPurpose::Startup)
    }

    /// The switch gate, raised from Settings while a session is open.
    pub fn switch(roster: Roster) -> Self {
        Self::for_purpose(roster, PickPurpose::Switch)
    }

    fn for_purpose(roster: Roster, purpose: PickPurpose) -> Self {
        Self {
            purpose,
            roster,
            picker: picker_state(),
            outcome: None,
            notice: None,
        }
    }

    /// Say something the gate could not do, and stay open. Used when the vault
    /// itself will not answer, so the row that raised the gate does not read
    /// as a dead control.
    pub fn with_notice(mut self, notice: impl Into<String>) -> Self {
        self.notice = Some(notice.into());
        self
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

/// What is protecting the practice session, as the store reported it opening.
///
/// The store knows this and used to only print it, which left Settings offering
/// to switch a persona it could not name. Reported rather than inferred: the
/// view cannot re-derive which persona opened or what the vault backend is,
/// and a settings page that guesses at whose practice this is would be worse
/// than one that says nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PracticeSeal {
    /// Sealed to a persona. `protection` is personae's own account of what
    /// holds the key at rest, passed through unedited.
    Sealed { persona: String, protection: String },
    /// Saved, but in the clear: no vault on this machine, or no key from it.
    /// Carries the reason, because "unsealed" without a why is not actionable.
    Unsealed { reason: String },
}

impl PracticeSeal {
    /// The one-line reading for Settings.
    pub fn summary(&self) -> String {
        match self {
            Self::Sealed {
                persona,
                protection,
            } => {
                format!("Practising as {persona}. Sealed with {protection}.")
            }
            Self::Unsealed { reason } => {
                format!("Not sealed: {reason}. Practice is saved in the clear.")
            }
        }
    }
}

/// Declining the gate, for a host that has to answer for the picker.
///
/// The picker reports its own Escape, but only once something is focused, and
/// at startup nothing is. A host with a window-wide key policy can record this
/// instead; the two paths are the same answer, so recording it twice is
/// harmless.
pub fn dismissed() -> PickerEvent {
    PickerEvent::Dismissed
}

/// Take the gate down and practise with no persona.
///
/// The state half of declining. The host owns the other half (opening no
/// store), but the flag and the pick both live here, so the shape of "nobody
/// is saving this" is stated once in the crate that draws it.
pub fn practise_unsaved(ui: &mut UiState) {
    ui.persona = None;
    ui.practice_saved = false;
}

/// The nav-row notice for a session nobody is saving.
///
/// A row rather than a one-off dialog: the choice is in force for as long as
/// the window is open, so saying it once at startup and then going quiet would
/// leave the honest fact where nobody can check it. Absent entirely on the
/// ordinary path, including the unsealed fallback a machine with no vault
/// takes, which does save.
pub fn unsaved_notice(ui: &UiState) -> Option<UiChild> {
    if ui.practice_saved {
        return None;
    }
    Some(Box::new(
        el("div", text("Not saving"))
            .attr("class", "unsaved")
            .attr("role", "status")
            .attr(
                "aria-label",
                "Practice is not being saved: no persona was chosen.",
            ),
    ))
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
            // The focused variant: this screen is the only thing on the window,
            // so the picker takes the caret as it appears and the arrows work
            // on the first press.
            move |state: &mut CommandState| persona_picker_focused(state, &roster),
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
                    el("div", text(pick.roster.description.clone())).attr("class", "persona-vault"),
                    Box::new(picker) as UiChild,
                    notice,
                    el(
                        "div",
                        text(match pick.purpose {
                            PickPurpose::Startup => {
                                "Escape practises without a persona. Nothing is saved."
                            }
                            // Nothing is pending behind a switch: the session
                            // on screen belongs to somebody already.
                            PickPurpose::Switch => {
                                "Escape keeps practising as the current persona."
                            }
                        }),
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
        assert_eq!(
            pick.outcome,
            Some(PickerEvent::Chose(ProfileId("alt".into())))
        );
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
        assert!(pick
            .notice
            .as_deref()
            .is_some_and(|n| n.contains("personae-vault")));
    }

    #[test]
    fn a_switch_gate_says_that_declining_keeps_the_current_persona() {
        // The two gates mean different things by Escape, and the screen has to
        // say which one it is: at startup nobody is chosen yet, during a switch
        // somebody already is.
        let entries = [("work", 2, true), ("alt", 0, false)];
        let startup = PersonaPick::new(roster(&entries));
        let switch = PersonaPick::switch(roster(&entries));
        assert_eq!(startup.purpose, PickPurpose::Startup);
        assert_eq!(switch.purpose, PickPurpose::Switch);
    }

    #[test]
    fn a_vault_that_will_not_answer_still_puts_its_reason_on_the_gate() {
        // The Settings row is a deliberate act; it cannot answer with nothing.
        let pick = PersonaPick::switch(roster(&[])).with_notice("the vault would not open");
        assert!(pick
            .notice
            .as_deref()
            .is_some_and(|n| n.contains("would not open")));
    }

    #[test]
    fn declining_takes_the_gate_down_and_stops_saving() {
        let mut ui = UiState::new();
        ui.persona = Some(PersonaPick::new(roster(&[
            ("work", 2, false),
            ("alt", 0, false),
        ])));
        assert!(ui.practice_saved, "an ordinary session saves");
        practise_unsaved(&mut ui);
        assert!(ui.persona.is_none(), "the gate does not stay up");
        assert!(!ui.practice_saved);
    }

    #[test]
    fn the_notice_appears_only_for_a_session_nobody_is_saving() {
        let mut ui = UiState::new();
        assert!(
            unsaved_notice(&ui).is_none(),
            "an ordinary session says nothing, including the unsealed fallback"
        );
        practise_unsaved(&mut ui);
        assert!(unsaved_notice(&ui).is_some());
    }

    #[test]
    fn a_later_choice_clears_an_earlier_notice() {
        let mut pick = PersonaPick::new(roster(&[("work", 2, false), ("alt", 0, false)]));
        pick.record(PickerEvent::CreateRequested);
        pick.record(PickerEvent::Chose(ProfileId("work".into())));
        assert!(pick.notice.is_none());
    }
}
