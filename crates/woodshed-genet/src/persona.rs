//! Whether to ask which persona is practising, and what to do with the answer.
//!
//! The view half is `woodshed_views::persona`. This half owns the two acts a
//! view must not perform: deciding, before the window exists, that the question
//! needs asking at all; and writing the answer into the shared vault before
//! reopening the practice store under it.
//!
//! **Nobody is asked who has already answered.** `PERSONAE_PROFILE`, a
//! remembered choice, a vault holding exactly one persona, and a machine with
//! no vault backend all keep the silent path they had before this module
//! existed. What is left is the one case the convention cannot decide: several
//! personas, none of them chosen.

use personae::bootstrap::{self, Unlock};
use personae::roster::{self, Roster};
use personae::vault::ProfileId;
use persona_picker::PickerEvent;
use woodshed_views::persona::{PersonaPick, PickPurpose};

use crate::shared::Shared;
use crate::storage::open_store_as;
use crate::sync::Ctx;

/// The roster to ask about, or `None` when the convention already decides.
///
/// Reads the vault but never opens a profile in it, which is the whole point of
/// running before the store: [`roster::open_shared`] would mint a `default`
/// persona beside the ones the user has, and seal the practice session to it.
pub fn pending_roster() -> Option<Roster> {
    pending_roster_at(&bootstrap::default_vault_dir(), Unlock::from_env())
}

/// [`pending_roster`] against a named vault directory, for tests.
pub fn pending_roster_at(dir: &std::path::Path, unlock: Unlock) -> Option<Roster> {
    if roster::chosen_profile(dir).is_some() {
        return None;
    }
    // A vault that will not open is not a question to put to the user: the
    // store's own fallback says so out loud and practice proceeds unsealed.
    let opened = bootstrap::open_storage(dir, unlock).ok()?;
    let roster = roster::read_roster(&*opened.storage, dir, opened.description).ok()?;
    (roster.entries.len() > 1).then_some(roster)
}

/// The whole roster, whatever the convention would decide (P2).
///
/// [`pending_roster`] asks whether the question is worth putting; this answers
/// the question the user asked for by name, so a remembered choice, a sole
/// persona, and `PERSONAE_PROFILE` are all beside the point.
pub fn roster_now() -> Result<Roster, personae::IdentityError> {
    let dir = bootstrap::default_vault_dir();
    let opened = bootstrap::open_storage(&dir, Unlock::from_env())?;
    roster::read_roster(&*opened.storage, &dir, opened.description)
}

/// Act on a gate the user has answered, if they have, and raise one if the
/// Settings row asked for it.
///
/// Runs at the head of the dispatch tail so the rest of it (the audio seam, the
/// skin, persistence) sees the restored session in the same beat the choice
/// lands, rather than one frame later.
pub fn after_dispatch(shared: &mut Shared, ctx: &mut Ctx<'_>) {
    let mut answer = None;
    let mut purpose = PickPurpose::Startup;
    let mut requested = false;
    ctx.runner.update(|ui| {
        requested = std::mem::take(&mut ui.persona_switch_requested);
        if let Some(pick) = ui.persona.as_mut() {
            answer = pick.outcome.take();
            purpose = pick.purpose;
        }
    });
    if requested {
        raise_switch(ctx);
        return;
    }
    let Some(answer) = answer else {
        return;
    };
    let chosen = match answer {
        PickerEvent::Chose(id) => Some(id),
        PickerEvent::Dismissed => match purpose {
            // Escape at startup: practise with no persona at all.
            PickPurpose::Startup => {
                decline(ctx);
                return;
            }
            // Escape on a switch changes nothing: the store that is open stays
            // open, and re-settling on the convention here would quietly move
            // the user off the persona they are already practising as.
            PickPurpose::Switch => {
                ctx.runner.update(|ui| ui.persona = None);
                return;
            }
        },
        // The view answers this one itself and keeps the gate open (P3 wires
        // the create flow), so it never reaches here.
        PickerEvent::CreateRequested => return,
    };
    settle(shared, ctx, chosen.as_ref(), purpose);
}

/// Put the switch gate up, or say why it cannot go up.
///
/// A vault that will not open is reported on the gate itself rather than
/// swallowed: the row was a deliberate act, and a control that answers a click
/// with nothing reads as broken.
fn raise_switch(ctx: &mut Ctx<'_>) {
    let pick = match roster_now() {
        Ok(roster) => PersonaPick::switch(roster),
        Err(error) => {
            eprintln!("[woodshed] cannot read the persona roster: {error}");
            PersonaPick::switch(Roster {
                entries: Vec::new(),
                chosen: ProfileId(String::new()),
                description: "no vault on this machine".into(),
            })
            .with_notice(format!(
                "The identity vault would not open ({error}). Practice continues \
                 as the current persona."
            ))
        }
    };
    // Taken rather than moved: the runner's callback is `FnMut`, and the pick
    // is not `Copy`.
    let mut pick = Some(pick);
    ctx.runner.update(move |ui| ui.persona = pick.take());
}

/// Practise with no persona: no store, nothing saved, and the nav row says so.
///
/// The alternative was to open on the convention, which is what this did until
/// the shape was thought through. On the only vault that reaches this screen —
/// several personas, none chosen — the convention resolves to `default` and
/// *mints it*, so declining would have added a third identity beside the user's
/// two and sealed their practice to one they never picked. Opening nothing is
/// both the honest reading of "no thanks" and the only one that leaves the
/// vault as it was found.
///
/// The cost is stated where it lands rather than buried: `Shared.storage` stays
/// `None`, every save in the dispatch tail is skipped, and `practice_saved`
/// turns the nav-row notice on for the rest of the session.
fn decline(ctx: &mut Ctx<'_>) {
    eprintln!("[woodshed] no persona chosen; this session is not saved");
    ctx.runner.update(woodshed_views::persona::practise_unsaved);
}

/// Open the store on the settled persona, restore the session into it, and take
/// the gate down.
fn settle(
    shared: &mut Shared,
    ctx: &mut Ctx<'_>,
    chosen: Option<&ProfileId>,
    purpose: PickPurpose,
) {
    if let Some(id) = chosen {
        // Remembered first, so every other application in the family opens the
        // same persona next time. The store opens on the id either way: a vault
        // directory that refuses the write must not silently reroute this
        // session to somebody else.
        if let Err(error) = roster::remember_profile(&bootstrap::default_vault_dir(), id) {
            eprintln!(
                "[woodshed] chose persona {:?} but could not remember it ({error}); \
                 this session practises as it, the next one will ask again",
                id.0
            );
        }
    }
    let (storage, seal) = open_store_as(chosen);
    ctx.runner.update(|ui| {
        if purpose == PickPurpose::Switch {
            // The whole session goes, not just the parts the incoming persona
            // happens to have stored. `restore` returns early on a store with
            // no session, so anything left standing would be the OUTGOING
            // persona's practice — and the next frame's save would write it
            // into this persona's store. Host-fed fields (the MIDI port lists,
            // latency) refill on the next dispatch.
            *ui = woodshed_views::stage::UiState::new();
        }
        crate::session::restore(&storage, ui);
        ui.persona = None;
        // After the reset above, so a switch does not wipe the seal it just
        // established. Cloned rather than moved: the callback is `FnMut`.
        ui.seal = Some(seal.clone());
    });
    // Both from the one value, so `Shared` and the view cannot disagree about
    // who is practising.
    shared.seal = Some(seal);
    shared.storage = Some(storage);
}

/// Seed the gate onto a fresh [`UiState`], if one is pending.
pub fn seed(shared: &mut Shared, ui: &mut woodshed_views::stage::UiState) {
    ui.persona = shared.pending_roster.take().map(PersonaPick::new);
}


#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::Harness;
    use genet_probe::Selector;
    use winit::keyboard::NamedKey;
    use woodshed_views::stage::{UiChild, UiState};

    /// `PERSONAE_PROFILE` is process-wide, so every test that reads the vault
    /// serializes behind one lock: `chosen_profile` consults it, which makes an
    /// unrelated test's export enough to change this one's answer.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn unlock() -> Unlock {
        Unlock::passphrase(b"woodshed-test".to_vec())
    }

    /// A vault directory holding `names`, and nothing else.
    fn vault(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp vault");
        let opened = bootstrap::open_storage(dir.path(), unlock()).expect("open vault");
        for name in names {
            roster::create_profile(&*opened.storage, &ProfileId((*name).into()), *name)
                .expect("mint persona");
        }
        dir
    }

    #[test]
    fn two_personas_and_no_choice_is_the_one_case_worth_asking_about() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt"]);
        let roster = pending_roster_at(dir.path(), unlock()).expect("the gate must open");
        assert_eq!(roster.entries.len(), 2);
        assert_eq!(roster.description.is_empty(), false, "the vault says what protects it");
    }

    #[test]
    fn a_sole_persona_is_not_a_question() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["only"]);
        assert!(pending_roster_at(dir.path(), unlock()).is_none());
    }

    #[test]
    fn an_empty_vault_is_not_a_question_either() {
        // First run. `default` is minted on open, which is the silent path the
        // application had before the gate existed.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&[]);
        assert!(pending_roster_at(dir.path(), unlock()).is_none());
    }

    #[test]
    fn a_remembered_choice_is_not_asked_again() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt"]);
        roster::remember_profile(dir.path(), &ProfileId("alt".into())).expect("remember");
        assert!(pending_roster_at(dir.path(), unlock()).is_none());
    }

    #[test]
    fn a_forced_persona_is_not_asked_about() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = vault(&["work", "alt"]);
        std::env::set_var(roster::PROFILE_ENV, "work");
        let pending = pending_roster_at(dir.path(), unlock());
        std::env::remove_var(roster::PROFILE_ENV);
        assert!(pending.is_none(), "PERSONAE_PROFILE decides without asking");
    }

    #[test]
    fn a_vault_that_will_not_open_is_stepped_over_rather_than_asked_about() {
        // The done condition behind "sealing is not a gate": a machine with no
        // usable vault backend never sees the picker. Here, the wrong
        // passphrase stands in for a backend that will not unlock.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt"]);
        let wrong = Unlock::passphrase(b"not-the-passphrase".to_vec());
        assert!(pending_roster_at(dir.path(), wrong).is_none());
    }

    #[test]
    fn the_gate_asks_about_the_personas_the_vault_actually_holds() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt", "burner"]);
        let roster = pending_roster_at(dir.path(), unlock()).expect("the gate must open");
        let ids: Vec<&str> = roster.entries.iter().map(|entry| entry.id.0.as_str()).collect();
        // Sorted by id, so the list does not reorder itself between runs.
        assert_eq!(ids, ["alt", "burner", "work"]);
    }

    /// A harness over the real product root, so what is asserted is the DOM the
    /// shipping build lays out — not a view fn called in isolation.
    fn gated_harness(roster: Roster) -> Harness<UiState, crate::sync::Logic, UiChild> {
        let mut ui = UiState::new();
        ui.persona = Some(PersonaPick::new(roster));
        let logic: crate::sync::Logic = Box::new(woodshed_views::stage::stage_root);
        // The shipping Escape policy, not a copy of it. Everything else is
        // inert: this harness is about the gate, not the audio seam.
        let hooks = cambium_genet_winit_host::HostHooks {
            key_intercept: Box::new(crate::escape_policy),
            ..cambium_genet_winit_host::inert_hooks()
        };
        let mut harness = Harness::with_hooks(
            cambium_genet_winit_host::Init {
                state: ui,
                logic,
                sheet: woodshed_views::theme::slate_stage_css(),
            },
            hooks,
        );
        harness.layout_at(1_100.0, 664.0);
        harness
    }

    fn two_persona_roster() -> Roster {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["alt", "work"]);
        let opened = bootstrap::open_storage(dir.path(), unlock()).expect("reopen vault");
        roster::read_roster(&*opened.storage, dir.path(), opened.description).expect("roster")
    }

    #[test]
    fn the_gate_is_reachable_by_a_driver() {
        // The standing requirement for any new surface: genet-probe must be
        // able to find it through identity the DOM carries.
        let harness = gated_harness(two_persona_roster());
        assert!(
            harness.resolve(&Selector::class("command-picker")).is_some(),
            "the picker itself must resolve"
        );
        assert!(
            harness
                .resolve(&Selector::role("dialog").containing("Choose a persona"))
                .is_some(),
            "the gate announces itself as a dialog with a label"
        );
        assert!(
            harness
                .resolve(&Selector::class("command-item").with_attr("data-key", "work"))
                .is_some(),
            "each persona is addressable by its id, not by the name it shows"
        );
    }

    #[test]
    fn two_personas_sharing_a_name_are_still_told_apart() {
        // Why the rows carry a key at all. Display names are the user's and
        // need not be unique; the id is what `settle` opens the store on, so it
        // is what a driver has to be able to aim at.
        use personae::roster::RosterEntry;
        let twins = Roster {
            entries: vec![
                RosterEntry {
                    id: ProfileId("work-laptop".into()),
                    display_name: "Work".into(),
                    slot_count: 2,
                    chosen: false,
                },
                RosterEntry {
                    id: ProfileId("work-studio".into()),
                    display_name: "Work".into(),
                    slot_count: 1,
                    chosen: false,
                },
            ],
            chosen: ProfileId("work-laptop".into()),
            description: "test vault".into(),
        };

        fn chose(harness: &Harness<UiState, crate::sync::Logic, UiChild>) -> Option<PickerEvent> {
            harness
                .state()
                .persona
                .as_ref()
                .and_then(|pick| pick.outcome.clone())
        }

        // The route this replaces. Both twins answer to the same label, and a
        // driver gets whichever one comes first: the other is unreachable.
        let mut by_label = gated_harness(twins.clone());
        assert!(by_label.click_on(&Selector::class("command-label").containing("Work")));
        assert_eq!(
            chose(&by_label),
            Some(PickerEvent::Chose(ProfileId("work-laptop".into()))),
            "by name, the second twin cannot be reached at all"
        );

        // By key, each one resolves on its own.
        let mut by_key = gated_harness(twins);
        assert!(
            by_key.click_on(&Selector::class("command-item").with_attr("data-key", "work-studio"))
        );
        assert_eq!(
            chose(&by_key),
            Some(PickerEvent::Chose(ProfileId("work-studio".into()))),
            "and it is the one that was asked for, not its namesake"
        );
    }

    #[test]
    fn the_gate_stands_in_front_of_the_product_rather_than_over_it() {
        let harness = gated_harness(two_persona_roster());
        assert!(
            harness.resolve(&Selector::class("pills")).is_none(),
            "no product navigation while the session behind it is unread"
        );
    }

    #[test]
    fn clicking_a_persona_records_the_choice_the_host_acts_on() {
        let mut harness = gated_harness(two_persona_roster());
        assert!(
            harness.click_on(&Selector::class("command-item").with_attr("data-key", "work")),
            "the row must be clickable where the driver found it"
        );
        let pick = harness.state().persona.as_ref().expect("the gate is still up");
        assert_eq!(
            pick.outcome,
            Some(PickerEvent::Chose(ProfileId("work".into()))),
            "the answer is recorded by id, for `settle` to open the store on"
        );
    }

    #[test]
    fn asking_for_a_new_persona_keeps_the_gate_up_and_says_why() {
        let mut harness = gated_harness(two_persona_roster());
        // The create row's key is the picker's sentinel, which carries a NUL so
        // no profile id can collide with it. Matched on the readable tail.
        assert!(harness.click_on(
            &Selector::class("command-item").with_attr("data-key", "persona-picker:create")
        ));
        let pick = harness.state().persona.as_ref().expect("the gate stays up");
        assert!(pick.outcome.is_none(), "nothing for the host to settle");
        assert!(
            harness.resolve(&Selector::role("status")).is_some(),
            "the notice is on screen, not only in the state"
        );
    }

    #[test]
    fn the_gate_takes_the_keyboard_without_a_tab() {
        // A startup gate is the whole window, so the picker asks for the caret
        // as it appears. Asserted as behaviour rather than as "which node has
        // focus": what matters is that the arrows and Enter do something on the
        // first press, with no Tab in front of them.
        let mut harness = gated_harness(two_persona_roster());
        assert!(harness.focus().is_some(), "the picker took the caret unasked");

        // Rows are sorted by id, so selection starts on `alt` and one step down
        // is `work`.
        harness.key_named(NamedKey::ArrowDown);
        harness.key_named(NamedKey::Enter);
        harness.relayout();

        let pick = harness.state().persona.as_ref().expect("the host clears it");
        assert_eq!(
            pick.outcome,
            Some(PickerEvent::Chose(ProfileId("work".into()))),
            "arrow then Enter chose the second persona, cold"
        );
    }

    #[test]
    fn escape_dismisses_the_gate_so_practice_is_never_blocked() {
        // "Picking nobody must not block practice." Escape has to reach the
        // picker's own key handler, which means the gate has to be focusable
        // and reachable by the host's Tab traversal from a cold start.
        let mut harness = gated_harness(two_persona_roster());
        harness.key_named(NamedKey::Escape);
        harness.relayout();
        let pick = harness.state().persona.as_ref().expect("the host clears it, not the view");
        assert_eq!(
            pick.outcome,
            Some(PickerEvent::Dismissed),
            "the first Escape declines, through the window-wide policy"
        );
    }

    /// The switch gate is raised by a Settings row, not by the convention, so
    /// it must appear for the cases the startup gate deliberately skips.
    #[test]
    fn the_settings_row_asks_even_when_the_convention_would_not() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt"]);
        roster::remember_profile(dir.path(), &ProfileId("alt".into())).expect("remember");

        assert!(
            pending_roster_at(dir.path(), unlock()).is_none(),
            "a remembered choice is not a startup question"
        );
        let opened = bootstrap::open_storage(dir.path(), unlock()).expect("reopen vault");
        let roster = roster::read_roster(&*opened.storage, dir.path(), opened.description)
            .expect("the switch reads the roster regardless");
        assert_eq!(roster.entries.len(), 2, "both personas are offered to switch to");
    }

    /// The hazard P2 introduces that P1 could not have: at startup the state
    /// behind the gate is empty, but a switch happens over a loaded session,
    /// and `restore` returns early when the incoming store holds nothing.
    /// Without the reset, the next save would write the outgoing persona's
    /// practice into the incoming persona's store.
    #[test]
    fn switching_into_an_unused_persona_does_not_carry_the_last_one_in() {
        use woodshed_views::stage::UiState;

        let mut ui = UiState::new();
        ui.song.name = "the previous persona's song".into();

        // An empty store stands in for a persona who has never practised;
        // `restore` returns early on it, leaving the state exactly as found.
        let unused: woodshed_core::storage::SessionStore<crate::storage::HostBackend> =
            woodshed_core::storage::SessionStore::new(Box::new(muniment::MemoryBackend::default()));

        // Restoring without the reset keeps the outgoing persona's practice.
        crate::session::restore(&unused, &mut ui);
        assert_eq!(
            ui.song.name, "the previous persona's song",
            "the hazard is real: an empty store restores nothing over what is there"
        );

        // What `settle` does for a switch, in the order it does it.
        ui = UiState::new();
        crate::session::restore(&unused, &mut ui);
        assert_ne!(
            ui.song.name, "the previous persona's song",
            "a fresh persona opens on its own empty practice"
        );
    }

    #[test]
    fn declining_is_not_a_dead_end_the_settings_switch_starts_saving() {
        // The coupling between P1's decline and P2's switch, which nothing else
        // covers: `settle` resets the whole state for a switch, and the reset
        // is what turns saving back on. A declined session must be able to
        // adopt a persona from Settings rather than needing a restart.
        let mut ui = UiState::new();
        woodshed_views::persona::practise_unsaved(&mut ui);
        assert!(!ui.practice_saved);

        // What `settle` does on the switch path, before it opens the store.
        ui = UiState::new();
        assert!(
            ui.practice_saved,
            "a session that has just adopted a persona saves again"
        );
    }

    #[test]
    fn declining_at_startup_would_have_minted_a_third_identity() {
        // Why declining opens nothing rather than opening on the convention.
        // The only vault that reaches the gate is several personas with none
        // chosen, and the convention resolves that to `default` and mints it.
        // If personae's ladder ever changes, this fails and the reasoning in
        // `decline` gets re-read rather than quietly outliving its cause.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["alt", "work"]);

        let opened = roster::open_chosen(dir.path(), unlock()).expect("the convention open");
        assert_eq!(opened.profile.0, "default");
        assert!(opened.created, "and it was minted, not found");

        let reopened = bootstrap::open_storage(dir.path(), unlock()).expect("reopen");
        let after = roster::read_roster(&*reopened.storage, dir.path(), reopened.description)
            .expect("roster");
        assert_eq!(
            after.entries.len(),
            3,
            "a third identity now sits beside the two the user made"
        );
    }

    #[test]
    fn the_unsaved_notice_is_on_screen_for_a_declined_session() {
        let mut harness = gated_harness(two_persona_roster());
        assert!(
            harness.resolve(&Selector::class("unsaved")).is_none(),
            "nothing to say while the gate is still up"
        );
        harness.update(woodshed_views::persona::practise_unsaved);
        assert!(
            harness.resolve(&Selector::class("pills")).is_some(),
            "the product is reachable without a persona"
        );
        assert!(
            harness
                .resolve(&Selector::role("status").containing("not being saved"))
                .is_some(),
            "and it says, for the rest of the session, that nothing is kept"
        );
    }

    #[test]
    fn choosing_a_persona_opens_the_store_on_it_without_a_round_trip() {
        // What `settle` rests on: naming the persona in the open is what seals
        // the session, not the remembered file, which may fail to write.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(roster::PROFILE_ENV);
        let dir = vault(&["work", "alt"]);
        let opened = roster::open_profile(dir.path(), unlock(), &ProfileId("alt".into()))
            .expect("open on the chosen persona");
        assert_eq!(opened.profile.0, "alt");
        assert!(!opened.created, "an existing persona is loaded, never re-minted");
    }
}
