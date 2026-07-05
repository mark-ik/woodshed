//! S0 static Stage sheet — the walking-skeleton content.
//!
//! Ported from serval's `examples/serval_web_smoke` (browser receipt,
//! 2026-07-04). Replaced by real `AppState`-driven views in S1; keep this
//! module free of app state on purpose so S0 only proves the host stack.

use xilem_serval::{el, text, AnyView, ServalCtx, ServalElement, View};

/// Boxed heterogeneous child view (the meerkat `NoteChild` pattern).
pub type Child = Box<dyn AnyView<(), (), ServalCtx, ServalElement>>;

/// Slate-flavored demo stylesheet. The real theme engine (OKLCH seeds → CSS
/// variables) replaces this in S1.
pub const DEMO_SHEET: &str = r#"
.root { width: 100%; height: 100%; background-color: #171a21; color: #d7dae0;
        font-family: sans-serif; font-size: 14px; padding: 16px; }
.title { font-size: 18px; color: #e8e2d4; margin-bottom: 12px; }
.pills { display: flex; margin-bottom: 16px; }
.pill { padding: 6px 14px; margin-right: 6px; border-radius: 14px; color: #9aa0ac; }
.pill-active { background-color: #2a2f3a; color: #e8b15c; }
.body { display: flex; }
.side { width: 200px; margin-right: 16px; }
.side-item { padding: 5px 10px; color: #9aa0ac; }
.side-active { background-color: #232833; color: #d7dae0; border-radius: 6px; }
.board { background-color: #1e222b; border-radius: 10px; padding: 14px; }
.string { display: flex; margin-bottom: 8px; }
.fret { width: 44px; height: 26px; }
.dot { width: 22px; height: 22px; border-radius: 11px; background-color: #4a90b8;
       color: #10131a; font-size: 11px; text-align: center; }
.root-dot { background-color: #e8b15c; }
.caption { margin-top: 12px; color: #6b7280; font-size: 12px; }
"#;

/// A minor-pentatonic-ish scatter: (string, fret, is_root) per dot.
const DOTS: &[(usize, usize, bool)] = &[
    (0, 0, false), (0, 3, false), (1, 0, false), (1, 3, true),
    (2, 0, false), (2, 2, true), (3, 0, false), (3, 2, false),
    (4, 0, true), (4, 3, false), (5, 0, false), (5, 3, false),
];

fn string_row(string: usize) -> Child {
    let frets: Vec<Child> = (0..6)
        .map(|fret| {
            let dot = DOTS.iter().find(|(s, f, _)| *s == string && *f == fret);
            match dot {
                Some((_, _, is_root)) => {
                    let class = if *is_root { "dot root-dot" } else { "dot" };
                    Box::new(
                        el(
                            "div",
                            (el("div", text(if *is_root { "R" } else { "" }))
                                .attr("class", class),),
                        )
                        .attr("class", "fret"),
                    ) as Child
                }
                None => Box::new(el("div", ()).attr("class", "fret")) as Child,
            }
        })
        .collect();
    Box::new(el("div", frets).attr("class", "string"))
}

fn pill(label: &str, active: bool) -> Child {
    Box::new(el("span", text(label.to_string())).attr(
        "class",
        if active { "pill pill-active" } else { "pill" },
    ))
}

fn side_item(label: &str, active: bool) -> Child {
    Box::new(el("div", text(label.to_string())).attr(
        "class",
        if active { "side-item side-active" } else { "side-item" },
    ))
}

/// [`demo_view`] boxed, so hosts can name the runner's view type on stable
/// Rust (`fn(&()) -> Child`).
pub fn demo_root(_: &()) -> Child {
    Box::new(demo_view())
}

/// The static Stage sheet: pills nav, catalog sidebar, fretboard dots.
pub fn demo_view() -> impl View<(), (), ServalCtx, Element = ServalElement> {
    el(
        "div",
        (
            el("div", text("Woodshed — serval host (S0)")).attr("class", "title"),
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
            el(
                "div",
                (
                    el(
                        "div",
                        (
                            side_item("Minor Pentatonic", true),
                            side_item("Major", false),
                            side_item("Dorian", false),
                            side_item("Mixolydian", false),
                        ),
                    )
                    .attr("class", "side"),
                    el(
                        "div",
                        (
                            string_row(0),
                            string_row(1),
                            string_row(2),
                            string_row(3),
                            string_row(4),
                            string_row(5),
                        ),
                    )
                    .attr("class", "board"),
                ),
            )
            .attr("class", "body"),
            el(
                "div",
                text("xilem-serval / serval-layout / netrender / winit host"),
            )
            .attr("class", "caption"),
        ),
    )
    .attr("class", "root")
}
