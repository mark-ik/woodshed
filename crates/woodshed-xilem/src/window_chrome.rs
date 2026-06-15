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
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId,
    WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Line, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

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
            (_, Axis::Horizontal) => Length::const_px(46.0),
            (_, Axis::Vertical) => Length::const_px(32.0),
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
        let r = (bb.width().min(bb.height()) * 0.28).clamp(4.0, 9.0);
        let stroke = Stroke::new(1.4);
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
