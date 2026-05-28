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

## Upstream PR drafts (2026-05-28)

These two patches are the *only* upstream divergence Woodshed actually needs
(it does not need the wgpu-29 pins or the external-compositor work — those are
the mere/serval ecosystem's). Both are small, in-use, human-authored API
additions; offering them upstream would let Woodshed ride stock `linebender/xilem`.

Source: fork commit `129d330e` on `mere-wgpu-29-vello-0-9`. To prepare each PR,
cherry-pick that commit's hunks **for the listed files only**, drop the
incidental `masonry/src/widgets/label.rs` rustfmt reflow (not semantic), one
commit per PR with a DCO `Signed-off-by` line. SPDX headers (Apache-2.0) are
already present. Run the canonical Linebender lint set + rustfmt before opening,
and ping the Linebender Zulip first (both are public-API additions). No
published Linebender AI policy exists as of this date; disclose AI assistance in
the PR if asked.

### PR 1 — Runtime-swappable default properties

Files: `masonry_core/src/app/render_root.rs`, `xilem/src/window_view.rs`.
Title + body to paste into the PR:

```markdown
masonry/xilem: allow swapping tree-wide default properties at runtime

## Motivation

`DefaultProperties` is fixed at construction. A host that wants a light/dark
toggle to re-color widgets relying on the *default* `ContentColor` /
`Background` (bare labels, buttons, prose, rather than per-widget overrides)
can't today without rebuilding the app. `render_root.rs` already carries a
`// TODO: Find better ways to customize default property set`; this is one way.

## Change

- `RenderRoot::set_default_properties(Arc<DefaultProperties>)`: swaps the
  arena's default set and `request_render_all()`. Color-only swaps don't affect
  layout, so paint-invalidation is enough (doc note: request layout separately
  if a future swap changes layout-affecting defaults).
- `WindowView` gains a reactive `default_properties: Option<Arc<DefaultProperties>>`
  and `with_default_properties(..)`, applied in `rebuild` only when the `Arc`
  identity changes (`Arc::ptr_eq`) so steady-state frames don't re-apply.
  `None` (default) leaves the startup set untouched.

## Notes

- API shape is deliberately minimal; glad to align it with whatever you'd
  prefer for the existing "customize default property set" TODO.
- Pointer-identity comparison assumes the host caches the `Arc` and rebuilds it
  only on theme change (documented on the field).
- In use in a guitar-practice app for a no-restart theme toggle.
```

### PR 2 — Report split-bar drag position

Files: `masonry/src/widgets/split.rs`, `xilem_masonry/src/view/split.rs`.
Title + body to paste into the PR:

```markdown
masonry/xilem: emit an action when the user drags the Split bar

## Motivation

`Split` owns its drag position with no way to read it back, so a host can't
persist a user-adjusted split (across views or restarts). Its `Action` was
`NoAction`.

## Change

- `masonry`: `Split::Action = SplitDragged` (new `pub struct SplitDragged(pub f64)`,
  the effective fraction `0.0..=1.0`). A `dragging` flag is set during
  pointer-move; on pointer-up after a real drag (not a bare click),
  `ctx.submit_action(SplitDragged(effective_fraction))`.
- `xilem`: the `split` view gains `on_split_changed(impl Fn(&mut State, f64) -> Action)`;
  build wraps the pod via `with_action_widget`, teardown calls
  `teardown_action_source`, and `message` routes the `SplitDragged` action (the
  no-child-id branch).

## Notes

- Fires only after an actual drag, so a click that doesn't move the bar stays a
  no-op.
- Backwards-compatible: callers that don't set the callback get
  `MessageResult::Nop`.
- In use to share/persist a pane split across tabs.
```

**Resolved (2026-05-28):** diffed the fork's split files against `upstream/main`.
The only fork commit touching them is `129d330e`, and its only additions over
upstream are the `SplitDragged` action + `on_split_changed` (PR 2). The
commit message over-claimed: `split_point`, `split_point_from_start`,
`split_point_from_end`, `min_lengths`, and `SplitPoint::FromStart/FromEnd` are
**already upstream** (confirmed present in `upstream/main`). So there is no
third patch.

**Bottom line:** Woodshed's entire upstream divergence is exactly these two PRs
(runtime default-properties + `on_split_changed`). It needs no wgpu-29 pin and
none of the external-compositor work. With PR 1 + PR 2 merged, Woodshed builds
on stock `linebender/xilem`.
