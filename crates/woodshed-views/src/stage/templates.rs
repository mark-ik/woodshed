use woodshed_core::{set_from_practice, storage::AppSection};
use xilem_serval::{clickable, el, text};

use super::{UiChild, UiState};

pub(super) fn screen(ui: &UiState) -> UiChild {
    let tiles: Vec<UiChild> = woodshedding::practice::catalog()
        .into_iter()
        .enumerate()
        .map(|(i, ps)| {
            let meta = format!("{} cards · tap to fill the set", ps.items.len());
            let name = ps.name.clone();
            let desc = ps.description.clone();
            let _ = i;
            Box::new(clickable(
                el(
                    "div",
                    (
                        el("div", text(name)).attr("class", "recipe-name"),
                        el("div", text(desc)).attr("class", "recipe-desc"),
                        el("div", text(meta)).attr("class", "recipe-meta"),
                    ),
                )
                .attr("class", "recipe-tile"),
                move |ui: &mut UiState, _| {
                    ui.set = set_from_practice(&ps);
                    ui.section = AppSection::Rehearsal;
                },
            )) as UiChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", text("Recipes")).attr("class", "settings-heading"),
                el("div", tiles).attr(
                    "class",
                    format!("recipe-grid recipe-grid-{}", ui.viewport.class()),
                ),
            ),
        )
        .attr("class", "board"),
    )
}
