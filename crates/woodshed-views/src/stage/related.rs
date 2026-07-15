use cambium::{clickable, el, graph_canvas_swatch, on_hover, text, HoverEvent, HoverPhase};
use woodshed_core::RelatedTarget;

use super::{related_swatch, UiChild, UiState, RELATED_LIMIT};

pub(super) fn panel(ui: &UiState) -> UiChild {
    let suggestions = ui.stage.related_material_configured(
        &ui.practice_history,
        &ui.app_settings.stage.related,
        RELATED_LIMIT,
    );

    // The suggestions as a structured pane: one row each, columns for kind /
    // name+why / actions. Hovering a row (or its graph node) highlights the
    // other through `related_hover`; the copy navigates, Stage stages, × hides.
    let rows: Vec<UiChild> = suggestions
        .iter()
        .map(|item| {
            let target = item.target;
            let dismiss_id = ui.stage.related_target_id(target);
            let row_class = if ui.related_hover == Some(target) {
                "related-row hovered"
            } else {
                "related-row"
            };
            Box::new(on_hover(
                el(
                    "div",
                    (
                        clickable(
                            el(
                                "div",
                                (
                                    el("div", text(item.kind))
                                        .attr("class", format!("related-kind kind-{}", item.kind.to_lowercase())),
                                    el(
                                        "div",
                                        (
                                            el("div", text(item.title.clone()))
                                                .attr("class", "related-name"),
                                            el("div", text(item.reason.clone()))
                                                .attr("class", "related-why"),
                                        ),
                                    )
                                    .attr("class", "related-copy-text"),
                                ),
                            )
                            .attr("class", "related-copy"),
                            move |ui: &mut UiState, _| {
                                ui.stage.select_related(target);
                            },
                        ),
                        el(
                            "div",
                            (
                                clickable(
                                    el("div", text("Stage")).attr("class", "related-stage"),
                                    move |ui: &mut UiState, _| {
                                        let from_id = ui.stage.catalog_id();
                                        ui.stage.select_related(target);
                                        ui.stage_current(from_id);
                                    },
                                ),
                                clickable(
                                    el("div", text("×")).attr("class", "related-hide"),
                                    move |ui: &mut UiState, _| {
                                        let related = &mut ui.app_settings.stage.related;
                                        if !related.dismissed_ids.contains(&dismiss_id) {
                                            related.dismissed_ids.push(dismiss_id.clone());
                                        }
                                    },
                                ),
                            ),
                        )
                        .attr("class", "related-actions"),
                    ),
                )
                .attr("class", row_class),
                move |ui: &mut UiState, ev: HoverEvent| match ev.phase {
                    HoverPhase::Enter | HoverPhase::Move => ui.related_hover = Some(target),
                    HoverPhase::Leave => {
                        if ui.related_hover == Some(target) {
                            ui.related_hover = None;
                        }
                    }
                },
            )) as UiChild
        })
        .collect();

    let pane: UiChild = if rows.is_empty() {
        Box::new(
            el("div", text("Choose material with catalog relations to see suggestions."))
                .attr("class", "related-empty"),
        )
    } else {
        Box::new(el("div", rows).attr("class", "related-list"))
    };

    // The interactive graph swatch: click a node to navigate, hover to link it to
    // its row, Expand to grow the canvas. Node click/hover route through the same
    // dispatch the pane uses, so the two stay in sync.
    let graph: UiChild = if ui.app_settings.stage.related.show_neighborhood {
        let swatch = related_swatch(ui);
        Box::new(
            el(
                "div",
                graph_canvas_swatch(
                    &swatch,
                    |ui: &mut UiState, id: Option<RelatedTarget>| {
                        if let Some(target) = id {
                            ui.stage.select_related(target);
                        }
                    },
                    |ui: &mut UiState, id: Option<Option<RelatedTarget>>| {
                        ui.related_hover = id.flatten();
                    },
                    |ui: &mut UiState| {
                        ui.related_expanded = !ui.related_expanded;
                    },
                ),
            )
            .attr("class", "related-graph-col"),
        )
    } else {
        Box::new(el("div", ()))
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
                el("div", text("What might I stage next?")).attr("class", "related-subtitle"),
                el("div", (graph, el("div", pane).attr("class", "related-pane-col")))
                    .attr("class", "related-body"),
                history,
            ),
        )
        .attr("class", "related-panel"),
    )
}
