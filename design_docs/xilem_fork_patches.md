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
