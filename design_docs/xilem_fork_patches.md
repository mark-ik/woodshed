# Carried xilem fork patches

## Current footing (2026-06-15): Woodshed rides its own lean worktree

Woodshed no longer depends on the shared `mere-wgpu-29-vello-0-9` fork. It
path-deps a dedicated worktree at `crates/xilem-woodshed` on branch
`woodshed-theme`, which is **`upstream/main` plus exactly one commit**: the
PR #1822 patch `WindowView::with_default_properties` (the no-restart retheme
hook).

- **Masonry: resolved upstream.** `RenderRoot::set_default_properties` (PR
  #1821) merged to `linebender/xilem` main on 2026-05-31, so Woodshed carries
  no masonry patch.
- **Xilem: one carried commit.** PR #1822 is still open, and it cannot be
  replicated from Woodshed's own code, because Xilem's `MasonryDriver` is
  monomorphic on the concrete `WindowView` type (a vendored view will not
  satisfy the trait bound on stock xilem). When #1822 merges and a masonry
  release ships with #1821, Woodshed drops to crates.io with zero patches.
- **Renderer:** rides upstream's `wgpu-28 / vello-0.8 / parley-0.8`. The newer
  `wgpu-29 / vello-0.9` pins were mere/serval's; Woodshed never used them.
  Adapting to upstream cost two API renames: `content_box_size()` became
  `content_box().size()`, and parley `Layout::align` regained its leading
  `container_width` argument (passed `None`).

The sections below are **historical**. They describe the shared
`mere-wgpu-29-vello-0-9` fork (still Strophe's footing) and the original
two-PR plan, kept for reference and for the shared fork's maintenance.

Living ledger of the **meaningful** local edits in the shared `../xilem`
checkout (path-dep'd by Strophe; formerly by Woodshed too). The fork's working
tree also carries a lot of CRLF/format churn — ignore that; this tracks only
edits with semantic intent, so a future rebase-on-upstream knows what to
re-apply.

Base: forked at `785ab8a9 masonry_winit: port to wgpu 29 API` (plus the
wgpu29 / vello 0.9 / parley 0.9 pin work and the scrollbar auto-hide cherry
that predate our edits).

## Runtime theme switching (2026-05-20)

Goal: let a host swap the tree-wide default property set (and window base
color) at runtime so a light/dark toggle re-colors masonry-default-driven
widgets (bare labels, buttons, prose) without a restart.

- **`masonry_core/src/app/render_root.rs`** — added
  `RenderRoot::set_default_properties(Arc<DefaultProperties>)`: swaps
  `property_arena.default_properties`, then marks the whole tree for the
  update-properties pass and invalidates each widget's property cache, so the
  pass re-fires `Widget::property_changed` for every cached property (the
  proper reactive path). No explicit repaint: each widget's handler (e.g.
  `ContentColor::prop_changed` → `request_paint_only`, plus the built-in
  `core_property_changed`) requests whatever paint/layout that property needs.
  *(Revised 2026-05-28/29 per maintainer feedback on #1819: a bare repaint
  skips `property_changed`; a defaults swap doesn't change a defaulted
  property's resolved stack index, so `property_cache.invalidated` is what
  forces the re-fire; and `request_render_all()` was dropped on his note that
  it isn't needed once the per-property handlers run.)*
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

Per maintainer feedback on #1819 (PoignardAzur, 2026-05-28), this splits in two:
**PR 1a (Masonry) lands first**, then **PR 1b (Xilem)** builds on it. Feedback
also: do the real property propagation (not a bare repaint, item below), and
disclose AI assistance ("mostly fine if you disclose it ahead of time").

### PR 1a — Masonry: runtime-swappable default properties (first)

File: `masonry_core/src/app/render_root.rs`.
Title + body to paste into the PR:

```markdown
masonry: allow swapping tree-wide default properties at runtime

## Motivation

`DefaultProperties` is fixed at construction. A host that wants a light/dark
toggle to re-color widgets relying on the *default* `ContentColor` /
`Background` (bare labels, buttons, prose, rather than per-widget overrides)
can't today without rebuilding the app. `render_root.rs` already carries a
`// TODO: Find better ways to customize default property set`; this is one way.

## Change

`RenderRoot::set_default_properties(Arc<DefaultProperties>)`: swaps the arena's
default set, then marks the whole tree for the update-properties pass
(`needs_update_props` + `request_update_props`) and invalidates each widget's
`property_cache`, so the pass re-fires `Widget::property_changed` for every
cached property. A defaults swap doesn't change a defaulted property's resolved
stack index (only its fallback value), so the `invalidated` flag is what forces
`cached_props_changed` true and the per-entry re-fire. Each widget's handler
(e.g. `ContentColor::prop_changed` → `request_paint_only`, plus the built-in
`core_property_changed`) then requests whatever paint/layout that property
needs, so no separate repaint call is necessary.

## Notes

- AI-assisted (disclosing per the in-progress policy); I reviewed and verified
  the propagation against the update-props pass.
- API shape is deliberately minimal; glad to align it with whatever you'd
  prefer for the existing "customize default property set" TODO.
- Scope is the whole-set swap only. The same trigger path (mark + invalidate,
  let the pass re-fire `property_changed`) generalizes naturally to narrower
  scopes later: a per-widget-type default swap (`DefaultProperties` is keyed by
  widget `TypeId`), a per-class update (reusing the existing `ClassSet` /
  `relevant_classes` machinery), or a single-widget refresh. Out of scope here;
  flagging so the API reads as one rung of a ladder rather than a dead end, and
  happy to follow up. Related: #1765, #1786.
- In use in a guitar-practice app for a no-restart theme toggle.
```

### PR 1b — Xilem: expose it on `WindowView` (after 1a lands)

File: `xilem/src/window_view.rs`.
Title + body to paste into the PR:

```markdown
xilem: WindowView::with_default_properties for runtime theme swap

## Motivation

Surfaces the new `RenderRoot::set_default_properties` (PR #<1a>) reactively, so
a Xilem app can swap its theme's default property set mid-session without a
restart.

## Change

`WindowView` gains a reactive `default_properties: Option<Arc<DefaultProperties>>`
and `with_default_properties(..)`, applied in `rebuild` only when the `Arc`
identity changes (`Arc::ptr_eq`) so steady-state frames don't re-apply. `None`
(default) leaves the startup set untouched.

## Notes

- AI-assisted (disclosing per the in-progress policy); reviewed by me.
- Depends on PR #<1a>.
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

**Bottom line (revised 2026-05-28):** Woodshed's upstream divergence is the two
patches above (runtime default-properties + `on_split_changed`), and neither the
wgpu-29 pin nor the external-compositor work. But the split patch is now
**vendored** in-tree (`pane_split` / `pane_split_widget`, Phase 1, commit
`24a9180`), so Woodshed no longer needs PR 2 to land to ride stock upstream — PR
2 becomes an optional community offer, not load-bearing.

The theme patch *can't* be vendored (it's a `RenderRoot` method, not a widget),
so PR 1 (now 1a Masonry + 1b Xilem) is the one path to a fully fork-free
Woodshed *with* runtime retheme. The fork-free fallback that needs no PR at all:
keep startup theming (stock `Xilem::with_default_properties`), drop the runtime
swap, accept restart-to-retheme.
