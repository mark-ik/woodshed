//! Combobox — a trigger button that toggles an inline list of
//! mutually-exclusive options.
//!
//! Xilem ships no combobox/dropdown as of the alpha we track, so this
//! is built on `text_button` + `flex_col` + `portal`. Generalized from
//! Woodshed's app-coupled original: instead of reaching into a fixed
//! `AppState.open_combobox` field, the caller passes the current
//! `is_open` flag and a `set_open` setter, so any host state shape
//! works. Centralizing "which combobox is open" in one host field
//! still gives the nice property that opening one closes the others.
//!
//! Inline expansion (v1): the option list renders directly under the
//! trigger when open, inside a fixed-height scrolling portal so a long
//! list can't shove siblings off-screen. A future overlay-popup
//! version can swap the internals without changing this signature —
//! consumers depend only on the trigger/select semantics.

use masonry::core::ArcStr;
use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};
use xilem::core::one_of::OneOf2;
use xilem::style::Style;
use xilem::view::{flex_col, label, portal, sized_box, text_button, AnyFlexChild, FlexExt};
use xilem::WidgetView;

/// Fixed height of the open option panel; its inner portal scrolls
/// when the list is taller. Bounds the worst-case push-down to a
/// predictable amount instead of letting a long picker shove content
/// off-screen.
const OPTION_PANEL_HEIGHT_PX: f64 = 260.0;

/// Build a combobox.
///
/// - `id` — stable identifier the host's open-state uses to recognize
///   this combobox. By convention `"<area>.<field>"`; the only hard
///   requirement is uniqueness across the app.
/// - `prefix` — rendered before the current value on the trigger (e.g.
///   `"Key: "`); pass `""` to suppress.
/// - `options` — visible labels; the index passed to `on_select` is the
///   position in this slice. Borrowed only for the call (labels are
///   cloned into the per-option closures).
/// - `selected` — currently-highlighted option index. Out-of-range
///   renders the trigger value as `"(unknown)"`.
/// - `is_open` — whether *this* combobox is the open one. The caller
///   computes it (typically `host.open_id == Some(id)`).
/// - `set_open(state, Option<id>)` — writes the host's open-state
///   field. The trigger calls it with `Some(id)` / `None` to toggle;
///   selecting an option calls it with `None` to close.
/// - `on_select(state, index)` — runs when an option is clicked; record
///   the new selection. Closing is handled here via `set_open`.
pub fn combobox<State, P, S, F>(
    id: &'static str,
    prefix: P,
    options: &[ArcStr],
    selected: usize,
    is_open: bool,
    set_open: S,
    on_select: F,
) -> impl WidgetView<State> + use<State, P, S, F>
where
    State: 'static,
    P: Into<ArcStr>,
    S: Fn(&mut State, Option<&'static str>) + Send + Sync + Clone + 'static,
    F: Fn(&mut State, usize) + Send + Sync + Clone + 'static,
{
    let prefix: ArcStr = prefix.into();
    let current_label: ArcStr = options
        .get(selected)
        .cloned()
        .unwrap_or_else(|| ArcStr::from("(unknown)"));
    let arrow = if is_open { " ▲" } else { " ▼" };
    let trigger_label: ArcStr = ArcStr::from(format!("{prefix}{current_label}{arrow}"));

    // Trigger toggles the shared open-state through the caller's setter.
    let toggle_set = set_open.clone();
    let trigger = text_button(trigger_label, move |s: &mut State| {
        toggle_set(s, if is_open { None } else { Some(id) });
    });

    // Build option buttons up-front so the panel's type is stable; the
    // closed panel is a fixed alternative branch via OneOf2.
    let mut option_views: Vec<AnyFlexChild<State>> = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        let on_select = on_select.clone();
        let set_open = set_open.clone();
        let active = i == selected;
        let lbl: ArcStr = ArcStr::from(format!("{}{}", if active { "● " } else { "  " }, opt));
        option_views.push(
            text_button(lbl, move |s: &mut State| {
                on_select(s, i);
                set_open(s, None);
            })
            .into_any_flex(),
        );
    }

    let list = flex_col(option_views)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(masonry::layout::Length::px(2.0));

    let open_panel = sized_box(portal(list).constrain_horizontal(true))
        .fixed_height(masonry::layout::Length::px(OPTION_PANEL_HEIGHT_PX));

    let panel: OneOf2<_, _> = if is_open {
        OneOf2::A(open_panel)
    } else {
        OneOf2::B(sized_box(label("")).fixed_height(masonry::layout::Length::px(0.0)))
    };

    flex_col((trigger, panel))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(masonry::layout::Length::px(2.0))
}
