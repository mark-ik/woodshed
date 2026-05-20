// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Combobox widget — a button that toggles an inline list of
//! mutually-exclusive options.
//!
//! Xilem does not ship a combobox or dropdown widget as of the alpha
//! we're tracking. Built here on top of `text_button` + `flex_col`
//! with a single shared "which combobox is open" field on
//! [`AppState`]. Open state is centralized so opening one combobox
//! automatically closes any other — no state-shape gymnastics inside
//! the consumer.
//!
//! Inline expansion (v1) — the option list renders directly under
//! the trigger button when open, pushing later siblings down. A
//! future v2 can swap to a `zstack`-based overlay popup without
//! changing this module's public surface; consumers should not
//! depend on the visual layout, only on the trigger / select
//! semantics.

use masonry::core::ArcStr;
use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};
use xilem::WidgetView;
use xilem::core::one_of::OneOf2;
use xilem::style::Style;
use xilem::view::{
    AnyFlexChild, FlexExt, flex_col, label, portal, sized_box, text_button,
};

use crate::AppState;

/// Threshold past which the option panel switches to a fixed-height
/// scrollable region. Below this count the panel sizes to content;
/// above it, the panel is capped at [`OPTION_PANEL_TALL_HEIGHT_PX`]
/// and the inner portal scrolls. Picked so the 12 chromatic pitch-
/// class roots render uncapped (they're already a tidy 12 lines)
/// while the ~30-entry scales catalog gets the scroll affordance.
const OPTION_PANEL_SCROLL_THRESHOLD: usize = 14;

/// Fixed height applied when the option list exceeds
/// [`OPTION_PANEL_SCROLL_THRESHOLD`]. Roughly ~10 button-row heights
/// at the default text size.
const OPTION_PANEL_TALL_HEIGHT_PX: f64 = 260.0;

/// Build a combobox.
///
/// `id` is the stable identifier the open-state field on [`AppState`]
/// uses to recognize this combobox. By convention `"<tab>.<field>"`
/// — e.g. `"progressions.key"` — but the only hard requirement is
/// uniqueness across the application. Mismatched IDs silently break
/// the open/close toggle, so prefer module-level `const` IDs over
/// inline literals when reusing across the same site.
///
/// `prefix` is rendered before the current value on the trigger
/// (e.g. `"Key: "`); pass `""` to suppress.
///
/// `options` is the visible labels — the index passed to `on_select`
/// is the position in this slice. The slice is borrowed only for the
/// duration of the call (each label is cloned into the closure that
/// fires on click).
///
/// `selected` is the currently-highlighted option index. Out-of-range
/// values render the trigger as `"<prefix>(unknown)"`; the option
/// list still renders normally.
///
/// `on_select(state, index)` runs when the user clicks an option.
/// The callback should mutate state to record the new selection;
/// the combobox itself handles closing.
pub fn combobox<P, F>(
    id: &'static str,
    prefix: P,
    options: &[ArcStr],
    selected: usize,
    open_state: Option<&'static str>,
    on_select: F,
) -> impl WidgetView<AppState> + use<P, F>
where
    P: Into<ArcStr>,
    F: Fn(&mut AppState, usize) + Send + Sync + Clone + 'static,
{
    let is_open = matches!(open_state, Some(open) if open == id);
    let prefix: ArcStr = prefix.into();
    let current_label: ArcStr = options
        .get(selected)
        .cloned()
        .unwrap_or_else(|| ArcStr::from("(unknown)"));
    let arrow = if is_open { " ▲" } else { " ▼" };
    let trigger_label: ArcStr =
        ArcStr::from(format!("{}{}{}", prefix, current_label, arrow));

    // Trigger toggles the shared open-state. Clicking the same
    // trigger again closes; clicking a different combobox's trigger
    // implicitly closes this one (the shared field can only hold
    // one ID at a time).
    let trigger = text_button(trigger_label, move |s: &mut AppState| {
        s.open_combobox = if matches!(s.open_combobox, Some(open) if open == id) {
            None
        } else {
            Some(id)
        };
    });

    // Build option buttons up-front (always — the type of the option
    // panel needs to be stable; we just hide the panel with OneOf2
    // when closed).
    let mut option_views: Vec<AnyFlexChild<AppState>> = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        let on_select = on_select.clone();
        let active = i == selected;
        let lbl: ArcStr = ArcStr::from(format!(
            "{}{}",
            if active { "● " } else { "  " },
            opt
        ));
        option_views.push(
            text_button(lbl, move |s: &mut AppState| {
                on_select(s, i);
                s.open_combobox = None;
            })
            .into_any_flex(),
        );
    }
    // Short lists render at natural height; long lists get a fixed
    // height + portal so they scroll instead of pushing siblings
    // off-screen. OneOf2 keeps the closed branch a stable type
    // alongside the open one. The open branch is itself a OneOf2
    // between short-list and tall-list shapes — nested OneOf2 keeps
    // each branch's concrete view type fixed even as the open/closed
    // state and option count vary across rebuilds.
    let option_views_len = options.len();
    let short_panel = flex_col(option_views)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(masonry::layout::Length::px(2.0));

    // v2: every open panel — short or long — gets a fixed max
    // height with internal scroll. Bounds the worst-case push-down
    // to a predictable amount instead of letting a 12-option
    // chromatic picker shove ~360px of content off-screen.
    //
    // Real popup overlay would need either masonry-level support
    // for paint-beyond-layout-bounds (`sized_box.fixed_height(0)`
    // tried but the layout system collapsed siblings into the
    // zero-height slot rather than overlaying them), or upstream
    // popup-positioning primitives. Deferred until those land.
    let _ = (option_views_len, OPTION_PANEL_SCROLL_THRESHOLD);
    let open_panel = sized_box(
        portal(short_panel).constrain_horizontal(true),
    )
    .fixed_height(masonry::layout::Length::px(OPTION_PANEL_TALL_HEIGHT_PX));

    let panel: OneOf2<_, _> = if is_open {
        OneOf2::A(open_panel)
    } else {
        OneOf2::B(
            sized_box(label(""))
                .fixed_height(masonry::layout::Length::px(0.0)),
        )
    };

    flex_col((trigger, panel))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start)
        .gap(masonry::layout::Length::px(2.0))
}
