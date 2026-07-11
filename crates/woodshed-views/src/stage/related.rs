use xilem_serval::{chisel_leaf, clickable, el, text};

use super::{UiChild, UiState, NEIGHBORHOOD_LEAF_KEY};

pub(super) fn panel(ui: &UiState) -> UiChild {
    let suggestions = ui
        .stage
        .related_material_with_history(&ui.practice_history, 5);
    let rows: Vec<UiChild> = suggestions
        .into_iter()
        .map(|item| {
            let select_target = item.target;
            let stage_target = item.target;
            Box::new(
                el(
                    "div",
                    (
                        clickable(
                            el(
                                "div",
                                (
                                    el("div", text(format!("{} · {}", item.kind, item.title)))
                                        .attr("class", "related-title"),
                                    el(
                                        "div",
                                        text(format!("{} · {}", item.score, item.reason)),
                                    )
                                    .attr("class", "related-reason"),
                                ),
                            )
                            .attr("class", "related-copy"),
                            move |ui: &mut UiState, _| {
                                ui.stage.select_related(select_target);
                            },
                        ),
                        clickable(
                            el("div", text("Stage")).attr("class", "related-stage"),
                            move |ui: &mut UiState, _| {
                                let from_id = ui.stage.catalog_id();
                                ui.stage.select_related(stage_target);
                                ui.stage_current(from_id);
                            },
                        ),
                    ),
                )
                .attr("class", "related-item"),
            ) as UiChild
        })
        .collect();

    let body: UiChild = if rows.is_empty() {
        Box::new(
            el("div", text("Choose material with catalog relations to see suggestions."))
                .attr("class", "related-empty"),
        )
    } else {
        Box::new(el("div", rows).attr("class", "related-list"))
    };

    let recent: Vec<UiChild> = ui
        .practice_history
        .recent(4)
        .map(|event| {
            let title = event
                .subject_id
                .split_once(':')
                .map(|(_, title)| title)
                .unwrap_or(event.subject_id.as_str());
            Box::new(
                el(
                    "div",
                    (
                        el("span", text(event.kind.label())).attr("class", "history-kind"),
                        el("span", text(title.to_string())).attr("class", "history-title"),
                    ),
                )
                .attr("class", "history-item"),
            ) as UiChild
        })
        .collect();
    let history: UiChild = if recent.is_empty() {
        Box::new(el("div", ()).attr("class", "history-empty"))
    } else {
        Box::new(
            el(
                "div",
                (
                    el("div", text("Recent")).attr("class", "history-heading"),
                    el("div", recent).attr("class", "history-list"),
                ),
            )
            .attr("class", "related-history"),
        )
    };

    Box::new(
        el(
            "aside",
            (
                el("div", text("Related")).attr("class", "related-heading"),
                el("div", text("What might I stage next?"))
                    .attr("class", "related-subtitle"),
                el(
                    "div",
                    chisel_leaf::<UiState, ()>(NEIGHBORHOOD_LEAF_KEY, 232, 112),
                )
                .attr("class", "related-graph"),
                history,
                body,
            ),
        )
        .attr("class", "related-panel"),
    )
}
