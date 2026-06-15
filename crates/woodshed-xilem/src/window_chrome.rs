// Copyright 2026 the Woodshed Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client-side window decorations (CSD).
//!
//! With native decorations off (`WindowOptions::with_decorations(false)`),
//! the app draws its own title-bar affordances. This is a small
//! role-parameterized masonry widget plus its Xilem view:
//!
//! - [`ChromeRole::Drag`]: a transparent grab region; a press starts an OS
//!   window-move drag (`EventCtx::drag_window`), so the header can be dragged
//!   to move the window.
//! - [`ChromeRole::Minimize`] / [`ChromeRole::Maximize`]: call
//!   `EventCtx::minimize` / `toggle_maximized` directly.
//! - [`ChromeRole::Close`]: emits [`WindowCloseRequested`], routed by the view
//!   to a host callback (Woodshed flips `AppState::running`, stopping the loop).
//!
//! All through masonry's public CSD context methods — no fork patch needed.
//! Each role is its own flex child, so the flex layout *is* the hit-testing.

use std::marker::PhantomData;

use masonry::accesskit::{Action as AccessAction, Node, Role};
use tracing::{Span, trace_span};

use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, QueryCtx,
    RegisterCtx, ResizeDirection, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Line, Point, Rect, Size, Stroke};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Which piece of window chrome a widget is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChromeRole {
    /// Transparent grab region: a press starts an OS window-move drag.
    Drag,
    /// Minimize the window.
    Minimize,
    /// Toggle maximized / restored.
    Maximize,
    /// Request close (routed to the host via [`WindowCloseRequested`]).
    Close,
}

/// Emitted when the close control is activated. Minimize/maximize act on the
/// window directly via the context, so they need no host round-trip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowCloseRequested;

// --- MARK: WIDGET

/// A single piece of client-side window chrome (see [`ChromeRole`]).
pub struct WindowChromeWidget {
    role: ChromeRole,
    /// Glyph color (themed; refreshed from the palette on a theme switch).
    fg: Color,
}

impl WindowChromeWidget {
    /// Create a chrome widget of the given role, with the given glyph color.
    pub fn new(role: ChromeRole, fg: Color) -> Self {
        Self { role, fg }
    }

    /// Re-color the glyph (called from the view when the palette changes).
    pub fn set_fg(this: &mut WidgetMut<'_, Self>, fg: Color) {
        this.widget.fg = fg;
        this.ctx.request_paint_only();
    }
}

impl Widget for WindowChromeWidget {
    type Action = WindowCloseRequested;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => match self.role {
                // Hand off to the OS window-move drag immediately.
                ChromeRole::Drag => ctx.drag_window(),
                // Buttons act on release (like masonry's Button), so a press
                // that slides off the control doesn't fire.
                _ => {
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                }
            },
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    match self.role {
                        ChromeRole::Minimize => ctx.minimize(),
                        ChromeRole::Maximize => ctx.toggle_maximized(),
                        ChromeRole::Close => {
                            ctx.submit_action::<Self::Action>(WindowCloseRequested);
                        }
                        ChromeRole::Drag => {}
                    }
                }
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == AccessAction::Click {
            match self.role {
                ChromeRole::Minimize => ctx.minimize(),
                ChromeRole::Maximize => ctx.toggle_maximized(),
                ChromeRole::Close => ctx.submit_action::<Self::Action>(WindowCloseRequested),
                ChromeRole::Drag => {}
            }
        }
    }

    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _event: &Update) {}

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match (self.role, axis) {
            // The drag region has no intrinsic width; `.flex(1.0)` in the
            // header makes it eat the leftover space.
            (ChromeRole::Drag, Axis::Horizontal) => Length::const_px(8.0),
            (_, Axis::Horizontal) => Length::const_px(36.0),
            (_, Axis::Vertical) => Length::const_px(30.0),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.role == ChromeRole::Drag {
            return; // transparent grab region
        }
        let bb = ctx.border_box();
        let c = bb.center();
        let r = (bb.width().min(bb.height()) * 0.22).clamp(3.0, 6.5);
        let stroke = Stroke::new(1.3);
        match self.role {
            ChromeRole::Minimize => {
                painter
                    .stroke(Line::new((c.x - r, c.y), (c.x + r, c.y)), &stroke, self.fg)
                    .draw();
            }
            ChromeRole::Maximize => {
                painter
                    .stroke(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r), &stroke, self.fg)
                    .draw();
            }
            ChromeRole::Close => {
                painter
                    .stroke(Line::new((c.x - r, c.y - r), (c.x + r, c.y + r)), &stroke, self.fg)
                    .draw();
                painter
                    .stroke(Line::new((c.x - r, c.y + r), (c.x + r, c.y - r)), &stroke, self.fg)
                    .draw();
            }
            ChromeRole::Drag => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        match self.role {
            ChromeRole::Drag => Role::Unknown,
            _ => Role::Button,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if self.role != ChromeRole::Drag {
            node.add_action(AccessAction::Click);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("WindowChrome", id = id.trace())
    }
}

// --- MARK: VIEW

type CloseCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// The [`View`] created by [`window_chrome`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct WindowChrome<State, Action> {
    role: ChromeRole,
    fg: Color,
    on_close: Option<CloseCallback<State, Action>>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// A piece of client-side window chrome. `fg` is the (themed) glyph color.
pub fn window_chrome<State, Action>(role: ChromeRole, fg: Color) -> WindowChrome<State, Action> {
    WindowChrome {
        role,
        fg,
        on_close: None,
        phantom: PhantomData,
    }
}

impl<State, Action> WindowChrome<State, Action> {
    /// Callback fired when the close control is activated. Only meaningful for
    /// [`ChromeRole::Close`]; the host typically requests app shutdown.
    pub fn on_close(
        mut self,
        callback: impl Fn(&mut State) -> Action + Send + Sync + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(callback));
        self
    }
}

impl<State, Action> ViewMarker for WindowChrome<State, Action> {}
impl<State, Action> View<State, Action, ViewCtx> for WindowChrome<State, Action>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<WindowChromeWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let pod =
            ctx.with_action_widget(|ctx| ctx.create_pod(WindowChromeWidget::new(self.role, self.fg)));
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if prev.fg != self.fg {
            WindowChromeWidget::set_fg(&mut element, self.fg);
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<WindowCloseRequested>() {
            Some(_) => match &self.on_close {
                Some(callback) => MessageResult::Action(callback(app_state)),
                None => MessageResult::Nop,
            },
            None => MessageResult::Stale,
        }
    }
}

// --- MARK: RESIZE FRAME

/// Default thickness (logical px) of the edge band that starts a window
/// resize. Native decorations are off, so the OS provides no resize grips;
/// this band is our own. Wide enough to grab comfortably.
pub const RESIZE_MARGIN: f64 = 7.0;

/// Map a local pointer position within a `size` box to a window-edge resize
/// direction if it falls within `margin` of an edge (corners take priority).
/// `None` means the interior, where the wrapped content lives.
fn edge_resize_direction(pos: Point, size: Size, margin: f64) -> Option<ResizeDirection> {
    let west = pos.x <= margin;
    let east = pos.x >= size.width - margin;
    let north = pos.y <= margin;
    let south = pos.y >= size.height - margin;
    use ResizeDirection::*;
    Some(match (north, south, west, east) {
        (true, _, true, _) => NorthWest,
        (true, _, _, true) => NorthEast,
        (_, true, true, _) => SouthWest,
        (_, true, _, true) => SouthEast,
        (true, ..) => North,
        (_, true, ..) => South,
        (_, _, true, _) => West,
        (_, _, _, true) => East,
        _ => return None,
    })
}

/// The directional resize cursor for a window edge.
fn resize_cursor(dir: ResizeDirection) -> CursorIcon {
    use ResizeDirection::*;
    match dir {
        East | West => CursorIcon::EwResize,
        North | South => CursorIcon::NsResize,
        NorthWest | SouthEast => CursorIcon::NwseResize,
        NorthEast | SouthWest => CursorIcon::NeswResize,
    }
}

/// Wraps the whole UI and turns its outer `margin` band into window-resize
/// grips. The child is inset by `margin`, so pointer events in the band reach
/// this widget (not the child): they start an OS resize via
/// `EventCtx::drag_resize_window` and show a directional resize cursor.
pub struct WindowFrameWidget<C: Widget + ?Sized> {
    child: WidgetPod<C>,
    margin: f64,
}

impl<C: Widget + FromDynWidget + ?Sized> WindowFrameWidget<C> {
    /// Wrap `child`, reserving a `margin`-thick resize band around it.
    pub fn new(child: NewWidget<C>, margin: f64) -> Self {
        Self {
            child: child.to_pod(),
            margin,
        }
    }

    /// Mutable access to the wrapped child (for the view's rebuild).
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, C> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<C: Widget + ?Sized> Widget for WindowFrameWidget<C> {
    type Action = ();

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent { state, .. }) = event {
            let pos = ctx.local_position(state.position);
            let size = ctx.content_box().size();
            if let Some(dir) = edge_resize_direction(pos, size, self.margin) {
                ctx.set_handled();
                ctx.drag_resize_window(dir);
            }
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn update(&mut self, _ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, _event: &Update) {}

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto_length = len_req.into();
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(&mut self.child, auto_length, context_size, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let m = self.margin;
        let inner = Size::new((size.width - 2.0 * m).max(0.0), (size.height - 2.0 * m).max(0.0));
        ctx.run_layout(&mut self.child, inner);
        ctx.place_child(&mut self.child, Point::new(m, m));
        ctx.derive_baselines(&self.child);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _painter: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Unknown
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, _node: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let size = ctx.content_box().size();
        let local = ctx.to_local(pos);
        match edge_resize_direction(local, size, self.margin) {
            Some(dir) => resize_cursor(dir),
            None => CursorIcon::Default,
        }
    }

    fn make_trace_span(&self, id: WidgetId) -> Span {
        trace_span!("WindowFrame", id = id.trace())
    }
}

// --- MARK: RESIZE FRAME VIEW

/// Randomly chosen view id for the frame's single child.
const FRAME_CHILD_VIEW_ID: ViewId = ViewId::new(0x77ade_0001);

/// The [`View`] created by [`window_frame`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct WindowFrame<Child, State, Action> {
    child: Child,
    margin: f64,
    phantom: PhantomData<fn() -> (State, Action)>,
}

/// Wrap `child` (typically the whole app root) in a window-resize frame with
/// a `margin`-thick grip band. See [`RESIZE_MARGIN`] for a sensible default.
pub fn window_frame<Child, State, Action>(
    child: Child,
    margin: f64,
) -> WindowFrame<Child, State, Action> {
    WindowFrame {
        child,
        margin,
        phantom: PhantomData,
    }
}

impl<Child, State, Action> ViewMarker for WindowFrame<Child, State, Action> {}
impl<Child, State, Action> View<State, Action, ViewCtx> for WindowFrame<Child, State, Action>
where
    State: 'static,
    Action: 'static,
    Child: WidgetView<State, Action>,
{
    type Element = Pod<WindowFrameWidget<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) =
            ctx.with_id(FRAME_CHILD_VIEW_ID, |ctx| self.child.build(ctx, app_state));
        let pod = ctx.create_pod(WindowFrameWidget::new(child.new_widget, self.margin));
        (pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        ctx.with_id(FRAME_CHILD_VIEW_ID, |ctx| {
            let child_element = WindowFrameWidget::child_mut(&mut element);
            self.child
                .rebuild(&prev.child, view_state, ctx, child_element, app_state);
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(FRAME_CHILD_VIEW_ID, |ctx| {
            let child_element = WindowFrameWidget::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child_element);
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first() {
            Some(FRAME_CHILD_VIEW_ID) => {
                let child_element = WindowFrameWidget::child_mut(&mut element);
                self.child
                    .message(view_state, message, child_element, app_state)
            }
            _ => MessageResult::Stale,
        }
    }
}
