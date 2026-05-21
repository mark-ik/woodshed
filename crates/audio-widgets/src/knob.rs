//! Rotary knob — a vertical-drag continuous control rendered as an arc.
//!
//! Unlike [`crate::fader`] (which wraps masonry's horizontal `Slider`),
//! there's no rotary widget to reuse, so this is a from-scratch masonry
//! `Widget` + Xilem `View`, modeled on masonry's `Slider`. Interaction
//! is **vertical drag** (drag up = increase), the convention DAW knobs
//! use — true rotary dragging is fiddly and worse ergonomically.
//!
//! Colors are stored on the widget (constructor params), not pulled
//! from the masonry property system, to keep the widget self-contained
//! and theme-location-independent — same choice as the other widgets
//! here.
//!
//! NOTE: the drag math + arc rendering are compile-verified but want a
//! runtime eyeball (drag feel, arc orientation) — the project's
//! validate-by-running pattern, same as the waveform/meter.

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::pointer::PointerButton;
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButtonEvent,
    PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Arc, Circle, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Preferred (square) edge of the knob, in px.
const KNOB_SIZE: f64 = 48.0;
/// Vertical drag distance (px) that spans the full `min..max` range.
const DRAG_RANGE_PX: f64 = 200.0;
/// Arc geometry: a 270° gauge with the gap at the bottom. Start at the
/// lower-left (135°), sweep clockwise 270° to the lower-right.
const ARC_START: f64 = std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_4; // 135°
const ARC_SWEEP: f64 = std::f64::consts::PI * 1.5; // 270°

// =================================================================
// Widget
// =================================================================

/// A knob was moved.
#[derive(PartialEq, Debug)]
pub struct KnobMoved {
    /// The new value.
    pub value: f64,
}

/// The masonry widget backing [`knob`].
pub struct KnobWidget {
    min: f64,
    max: f64,
    value: f64,
    track_color: Color,
    fill_color: Color,
    indicator_color: Color,
    /// `(pointer_y_at_press, value_at_press)` while dragging.
    drag_anchor: Option<(f64, f64)>,
}

impl KnobWidget {
    fn new(min: f64, max: f64, value: f64, track: Color, fill: Color, indicator: Color) -> Self {
        Self {
            min,
            max,
            value: value.clamp(min, max),
            track_color: track,
            fill_color: fill,
            indicator_color: indicator,
            drag_anchor: None,
        }
    }

    /// Normalized `0..=1` position of the current value.
    fn progress(&self) -> f64 {
        if self.max <= self.min {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }

    /// Setter used by the view's `rebuild`.
    fn set_value(this: &mut WidgetMut<'_, Self>, value: f64) {
        let v = value.clamp(this.widget.min, this.widget.max);
        if (v - this.widget.value).abs() > f64::EPSILON {
            this.widget.value = v;
            this.ctx.request_render();
        }
    }
}

impl Widget for KnobWidget {
    type Action = KnobMoved;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if ctx.is_disabled() {
            return;
        }
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary) | None,
                state,
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                let local = ctx.local_position(state.position);
                self.drag_anchor = Some((local.y, self.value));
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                if let Some((anchor_y, anchor_value)) = self.drag_anchor {
                    let local = ctx.local_position(current.position);
                    // Drag up (smaller y) increases the value.
                    let delta_px = anchor_y - local.y;
                    let span = self.max - self.min;
                    let new_value =
                        (anchor_value + (delta_px / DRAG_RANGE_PX) * span).clamp(self.min, self.max);
                    if (new_value - self.value).abs() > f64::EPSILON {
                        self.value = new_value;
                        ctx.submit_action::<Self::Action>(KnobMoved { value: self.value });
                        ctx.request_render();
                    }
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
                self.drag_anchor = None;
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: masonry::kurbo::Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::MinContent | LenReq::MaxContent => Length::const_px(KNOB_SIZE),
            LenReq::FitContent(space) => space,
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let size = ctx.content_box_size();
        let cx = size.width / 2.0;
        let cy = size.height / 2.0;
        let center = (cx, cy);
        let radius = (size.width.min(size.height) / 2.0 - 4.0).max(1.0);
        let radii = Vec2::new(radius, radius);
        let arc_thickness = (radius * 0.22).max(2.0);
        let stroke = Stroke::new(arc_thickness);

        // Track arc (full gauge sweep).
        let track = Arc::new(center, radii, ARC_START, ARC_SWEEP, 0.0);
        painter.stroke(&track, &stroke, self.track_color).draw();

        // Value arc (gauge start → current progress).
        let progress = self.progress();
        if progress > 0.0 {
            let value_arc = Arc::new(center, radii, ARC_START, ARC_SWEEP * progress, 0.0);
            painter.stroke(&value_arc, &stroke, self.fill_color).draw();
        }

        // Indicator dot at the current value angle.
        let angle = ARC_START + ARC_SWEEP * progress;
        let ix = cx + angle.cos() * radius;
        let iy = cy + angle.sin() * radius;
        let dot = Circle::new((ix, iy), arc_thickness * 0.7);
        painter.fill(&dot, self.indicator_color).draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Slider
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        node.set_value(self.value.to_string());
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// =================================================================
// View
// =================================================================

/// A rotary knob view spanning `[min, max]`, currently at `value`.
/// `on_change(state, value)` fires as the user drags vertically.
/// `track`/`fill`/`indicator` are the gauge background, the filled
/// portion, and the position marker.
pub fn knob<State, F>(
    min: f64,
    max: f64,
    value: f64,
    track: Color,
    fill: Color,
    indicator: Color,
    on_change: F,
) -> Knob<State, F>
where
    State: 'static,
    F: Fn(&mut State, f64) + Send + Sync + 'static,
{
    Knob {
        min,
        max,
        value,
        track,
        fill,
        indicator,
        on_change,
        phantom: PhantomData,
    }
}

/// View for [`knob`].
pub struct Knob<State, F> {
    min: f64,
    max: f64,
    value: f64,
    track: Color,
    fill: Color,
    indicator: Color,
    on_change: F,
    phantom: PhantomData<fn(State)>,
}

impl<State, F> ViewMarker for Knob<State, F> {}
impl<State, F> View<State, (), ViewCtx> for Knob<State, F>
where
    State: 'static,
    F: Fn(&mut State, f64) + Send + Sync + 'static,
{
    type Element = Pod<KnobWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(KnobWidget::new(
                    self.min,
                    self.max,
                    self.value,
                    self.track,
                    self.fill,
                    self.indicator,
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.value != self.value {
            KnobWidget::set_value(&mut element, self.value);
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        match message.take_message::<KnobMoved>() {
            Some(moved) => {
                (self.on_change)(app_state, moved.value);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}
