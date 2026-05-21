// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Combobox — thin Woodshed adapter over the shared
//! [`xilem_components::combobox`].
//!
//! The widget itself (trigger + toggling option panel) now lives in
//! the shared `xilem-components` crate, generic over host state. This
//! adapter pins it to Woodshed's `AppState.open_combobox` field so the
//! existing call sites — which pass the current `open_combobox` as
//! `open_state` — keep working unchanged.

use masonry::core::ArcStr;
use xilem::WidgetView;

use crate::AppState;

/// Build a combobox bound to Woodshed's shared `open_combobox` state.
///
/// `open_state` is the app's current open-combobox id (`AppState.
/// open_combobox`); this combobox renders open when it equals `id`.
/// All other parameters match [`xilem_components::combobox`].
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
    xilem_components::combobox(
        id,
        prefix,
        options,
        selected,
        is_open,
        |s: &mut AppState, v| s.open_combobox = v,
        on_select,
    )
}
