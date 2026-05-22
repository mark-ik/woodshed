# Carried xilem fork patches

Living ledger of the **meaningful** local edits we rely on in the shared
`../xilem` checkout (path-dep'd by both Woodshed and Strophe). The fork's
working tree also carries a lot of CRLF/format churn — ignore that; this
tracks only edits with semantic intent, so a future rebase-on-upstream knows
what to re-apply.

Base: forked at `785ab8a9 masonry_winit: port to wgpu 29 API` (plus the
wgpu29 / vello 0.9 / parley 0.9 pin work and the scrollbar auto-hide cherry
that predate our edits).

## Runtime theme switching (2026-05-20)

Goal: let a host swap the tree-wide default property set (and window base
color) at runtime so a light/dark toggle re-colors masonry-default-driven
widgets (bare labels, buttons, prose) without a restart.

- **`masonry_core/src/app/render_root.rs`** — added
  `RenderRoot::set_default_properties(Arc<DefaultProperties>)`: swaps
  `property_arena.default_properties` and calls `request_render_all()`.
  Color-only swaps don't need relayout, so paint-invalidate is enough.
- **`xilem/src/window_view.rs`** — `WindowView` gains a reactive
  `default_properties: Option<Arc<DefaultProperties>>` field +
  `with_default_properties(...)`. Applied in `rebuild` when the `Arc`
  identity changes (cheap `Arc::ptr_eq`), via `render_root().set_default_properties`.
  Base color was already reactive in `WindowView::rebuild`.

Consumer: Woodshed's `run()` uses the windowed `Xilem::new` API and feeds
`base_color` + `default_properties` from `state.palette` each frame; the
`Arc<DefaultProperties>` is cached in `AppState`, rebuilt only in `set_theme`.

Upstream note: Xilem's own code flags `// TODO: Find better ways to customize
default property set` — this is a candidate to offer upstream later, but per
the project's defer-upstreaming stance it stays a local patch for now.

## Split-bar drag callback (2026-05-20)

Goal: persist a draggable pane split (and share it across views). The split
widget owned its drag position with no way to read it back.

- **`masonry/src/widgets/split.rs`** — `Split` now has `type Action =
  SplitDragged` (new `pub struct SplitDragged(pub f64)`, the effective
  fraction). A `dragging` flag is set during pointer-move and, on pointer-up
  after a real drag, `ctx.submit_action::<SplitDragged>(..)` reports the final
  fraction. (Was `NoAction`.)
- **`xilem_masonry/src/view/split.rs`** — the `split` view gains
  `on_split_changed(Fn(&mut State, f64) -> Action)`; build wraps the pod in
  `with_action_widget`, teardown calls `teardown_action_source`, and `message`
  handles the `SplitDragged` action (the `None` / self branch).

Consumer: Woodshed's fretboard tabs share `AppState.split_ratio` (persisted in
`Settings`, default `0.0` = min fretboard); each `split(..)` does
`.split_point(state.split_ratio).on_split_changed(|s, f| s.split_ratio = f)`.
