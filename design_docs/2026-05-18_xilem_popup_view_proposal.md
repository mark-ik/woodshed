# Xilem popup view proposal

Draft text for an upstream issue against `linebender/xilem`. File when ready;
maintainer's the right author since the GitHub identity matters.

markik: I say defer upstreaming pretty much anything. I certainly don't want to step on any toes re: the use of AI. I just want to make cool stuff, and I don't mind forking if needed, clearly. Save this for posterity, and if the maintainers ask. Plus, this is all open source anyway. I do my plotting in the sunlight.

Suggested title: **Add a `popup` view using the existing `VisualLayerPlan` overlay layers**

---

## Summary

Masonry already ships everything needed for popup-style overlays
(`PaintLayerMode::IsolatedScene`, `VisualLayer` with per-layer transforms,
`VisualLayerPlan::overlay_layers()`, `PreparedFrame::paint_into` replaying
overlays in painter order). What's missing at the `xilem_masonry` view
level is a **`popup`** abstraction that uses these primitives — a view
that lets you anchor content (an option list, a context menu, a tooltip)
to a trigger widget's window position, with the popup painting over
siblings regardless of where in the tree it was declared.

## What works today

End-to-end paint-beyond-layout already works:

- `passes/paint.rs` produces a `VisualLayerPlan` with `IsolatedScene`
  widgets getting their own scene + transform.
- `masonry_winit/event_loop_runner.rs::redraw` collects
  `visual_layers.overlay_layers()` into `Vec<ImagingLayer>`.
- `masonry_imaging::PreparedFrame::paint_into` replays each overlay
  with `scale * layer.transform` — overlays composite cleanly over
  the root scene at their own positions.

The `PaintLayerMode` doc comment ("Current hosts still flatten these
scene layers back together") appears to be stale — the host pipeline
above does honor overlay transforms.

## Why a `popup` view is still needed

Three things `IsolatedScene` alone doesn't solve, and that a downstream
consumer (Woodshed, Mere, anyone wanting dropdowns / context menus /
tooltips) keeps hitting:

### 1. Layer order vs. tree order

Layers are produced in tree-traversal order. A popup declared inside
the trigger widget's column gets its layer recorded **before** later
siblings in the outer tree. So even with `IsolatedScene`, later content
paints on top of the popup. To overlay everything, the popup needs to
emit its layer late in the traversal — practically, at or near the
app root.

This means the popup-trigger and the popup-content need to be at
**different positions in the tree**. The trigger lives at its call
site; the content needs to live at the root. State (or some other
mechanism) carries content + position between them.

### 2. Anchoring to trigger position

A popup needs its trigger's **window-coordinate position** to render
the content adjacent to it (below, right, etc.). `resize_observer`
gives sizes, not positions. There's no first-class "report my
`window_transform`" primitive at the view layer.

A position-tracker widget — analogous to `resize_observer` but
reporting `state.window_transform` instead of `state.size` — fills
this gap. Could be a sibling primitive to `resize_observer` or an
extension of it.

### 3. State plumbing between trigger and overlay slot

If the trigger is in tab content and the overlay slot is at the app
root, *something* has to communicate "popup open / position / content"
between them. Two natural options:

- **State-driven**: caller stores popup descriptor on AppState; root
  reads from there. Simple, but every consumer reinvents the slot.
- **Message-driven**: xilem-core provides a typed message channel
  ("PopupRequest") that any widget can publish and a root-level
  `popup_host` view subscribes to. More framework-native, more work.

## Proposed API sketch

```rust
// At app root: install the popup host. Required for popups to render.
fn app_view(state: &mut State) -> impl WidgetView<State> {
    popup_host(
        // Normal content tree
        my_tab_layout(state),
        // Reads the current popup from state, renders it as an overlay
        // anchored at its stored window-coordinate position. None = no
        // popup active.
        state.active_popup.as_ref().map(|p| popup_content(p)),
    )
}

// At call sites: a button that opens a popup, with content + anchoring.
fn key_picker(state: &mut State) -> impl WidgetView<State> {
    popup_anchor(
        "key.picker",  // id; matches state.active_popup if open
        text_button("Key: C ▼", |s| { s.active_popup = Some(/* … */); }),
    )
}

// Or: at a slightly lower level, just give us position tracking and
// IsolatedScene-with-transform-override. Consumers compose the rest.
fn track_position<V>(
    on_size: impl Fn(&mut State, kurbo::Affine),
    inner: V,
) -> impl WidgetView<State>;
```

Either shape is fine; the value is the **primitives**, not the exact
ergonomics. Most consumers will use the high-level `popup_anchor` /
`popup_host` pair; framework-level work like custom-menu systems might
reach for `track_position` and roll their own composition.

## Use cases backing this

- **Combobox / dropdown** (the obvious one — currently every
  Xilem-based combobox we've seen falls back to inline expansion
  that pushes sibling content offscreen)
- **Context menus** (right-click → menu appears at cursor)
- **Tooltips** (hover → tip floats over neighbors)
- **Command palette** (`⌘P` → centered floating modal)
- **Drag previews** (drag a tab → preview floats with cursor)

All of these are blocked at the view layer for the same reasons even
though `IsolatedScene` makes them implementable at the widget layer.

## What we'd contribute / prefer-to-receive

Happy to land an initial `popup_anchor` + `popup_host` + position-
tracker primitive against a design the maintainers signal off on.
Conversely if there's already a sketch / RFC / in-progress branch,
happy to consume that.

## Side note on `PaintLayerMode` doc comment

The doc comment on `PaintLayerMode` claims:

> Current hosts still flatten these scene layers back together, so
> changing this does not yet change runtime presentation behavior.

Reading `masonry_winit/event_loop_runner.rs::redraw` +
`masonry_imaging::PreparedFrame::paint_into`, this no longer appears
true — overlay layers do composite in painter order with their own
transforms. The doc comment looks stale and probably worth updating
to reflect the current end-to-end behavior.
