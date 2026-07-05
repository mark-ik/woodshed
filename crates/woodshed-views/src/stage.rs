//! The Stage lens over live [`StageState`] (S1).
//!
//! Pills nav (Stage active; the other tabs are inert until S4 brings their
//! screens), the scale-catalog sidebar (click selects), and the fretboard
//! rendered as DOM dots from [`StageState::dots`].

use std::collections::HashMap;

use woodshed_core::StageState;
use xilem_serval::{clickable, el, text, AnyView, ServalCtx, ServalElement};

/// Boxed heterogeneous child view over [`StageState`].
pub type StageChild = Box<dyn AnyView<StageState, (), ServalCtx, ServalElement>>;

fn pill(label: &str, active: bool) -> StageChild {
    Box::new(el("span", text(label.to_string())).attr(
        "class",
        if active { "pill pill-active" } else { "pill" },
    ))
}

fn sidebar(state: &StageState) -> StageChild {
    let items: Vec<StageChild> = state
        .scales()
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let class = if i == state.scale_idx {
                "side-item side-active"
            } else {
                "side-item"
            };
            Box::new(clickable(
                el("div", text(s.name)).attr("class", class),
                move |st: &mut StageState, _| {
                    st.select_scale(i);
                },
            )) as StageChild
        })
        .collect();
    Box::new(el("div", items).attr("class", "side"))
}

fn board(state: &StageState) -> StageChild {
    let dots: HashMap<(usize, u8), (bool, String)> = state
        .dots()
        .into_iter()
        .map(|d| ((d.string_index, d.fret), (d.is_root, d.label)))
        .collect();
    let rows: Vec<StageChild> = (0..state.string_count())
        .map(|string_index| {
            let cells: Vec<StageChild> = (0..=state.fret_count)
                .map(|fret| {
                    // Space the nut (fret 0) apart from the fretted columns.
                    let cell_class = if fret == 0 { "fret nut-gap" } else { "fret" };
                    match dots.get(&(string_index, fret)) {
                        Some((is_root, label)) => {
                            let dot_class = if *is_root { "dot root-dot" } else { "dot" };
                            Box::new(
                                el(
                                    "div",
                                    (el("div", text(label.clone())).attr("class", dot_class),),
                                )
                                .attr("class", cell_class),
                            ) as StageChild
                        }
                        None => {
                            Box::new(el("div", ()).attr("class", cell_class)) as StageChild
                        }
                    }
                })
                .collect();
            Box::new(el("div", cells).attr("class", "string")) as StageChild
        })
        .collect();
    Box::new(
        el(
            "div",
            (
                el("div", rows),
                el(
                    "div",
                    text(format!(
                        "{} {} — {} positions",
                        state.root.name,
                        state.scale().name,
                        dots.len()
                    )),
                )
                .attr("class", "scale-name"),
            ),
        )
        .attr("class", "board"),
    )
}

/// The Stage screen root. Boxed so hosts can name the runner's view type
/// on stable Rust (`fn(&StageState) -> StageChild`).
pub fn stage_root(state: &StageState) -> StageChild {
    Box::new(
        el(
            "div",
            (
                el("div", text("Woodshed")).attr("class", "title"),
                el(
                    "div",
                    (
                        pill("Stage", true),
                        pill("Practice", false),
                        pill("Song", false),
                        pill("Rehearsal", false),
                        pill("Settings", false),
                    ),
                )
                .attr("class", "pills"),
                el("div", (sidebar(state), board(state))).attr("class", "body"),
                el(
                    "div",
                    text("serval host S1 — click a scale in the sidebar"),
                )
                .attr("class", "caption"),
            ),
        )
        .attr("class", "root"),
    )
}
