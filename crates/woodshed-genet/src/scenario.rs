//! The scenario lane: woodshed drives itself.
//!
//! Woodshed had no self-drive hook, so every headed receipt was SendKeys plus a
//! desktop grab: synthetic input that loses the foreground race whenever the
//! machine is in use, and captures that can silently photograph the wrong
//! window. This lane replaces both. The generic half (parsing, the verb loop,
//! selector resolution, assertions) is [`genet_probe`]; what lives here is only
//! what is woodshed's: which surfaces it has, what it can be asked to observe,
//! its named commands, and how a frame becomes a PNG.
//!
//! Since the host extraction it no longer owns *routing*. A delivered point is
//! queued as a [`HostPointer`] and the host runs it through the same hit test,
//! capture, and dispatch a real mouse takes — so a receipt exercises the
//! shipping path rather than an app-local imitation of it.
//!
//! Two env vars turn it on:
//!
//! - `WOODSHED_SCENARIO` — path to a `.scn` file (grammar in `genet_probe`).
//! - `WOODSHED_CAPTURE_DIR` — where `capture <name>` writes `<name>.png` and,
//!   at the end, `scenario.done` whose first line is `RESULT ok` or
//!   `RESULT fail`.
//!
//! Captures are in-process readbacks of the same rasterized view the frame
//! presented, so they need no compositor, no foreground, and no ffmpeg, and
//! they cannot photograph another window by mistake.
//!
//! `WOODSHED_STATE` overrides the session file. A scenario run must set it:
//! without it the run would read, and then overwrite, the real practice
//! session.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cambium_genet_winit_host::{read_frame, Frame, HostPointer};
use genet_probe::{
    Automatable, AutomatableExt, Driveable, ProbeSnapshot, ProbeSurface, Progress, Scenario,
    Selector,
};
use woodshed_core::Lens;
use woodshed_views::stage::{
    set_graph_relation_choices, set_graph_snapshot, set_graph_swatch_from_snapshot, UiState,
};

use crate::shared::Shared;
use crate::sync::Ctx;

/// A running scenario. Where its receipts go lives on [`Shared`], not here: the
/// scenario is moved out of the lane for the duration of a tick (so the driver
/// can hold everything else), which would otherwise make the capture directory
/// unreachable from inside a `capture` step.
pub struct ScenarioLane {
    scenario: Option<Scenario>,
    /// Set once the outcome has been written, so the sentinel is written once.
    finished: bool,
}

impl ScenarioLane {
    /// Build the lane from the environment, or `None` for an ordinary run.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("WOODSHED_SCENARIO").ok()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[woodshed-genet] scenario '{path}' unreadable: {e}");
                return None;
            }
        };
        let scenario = match Scenario::parse(&text) {
            Ok(scenario) => scenario,
            Err(e) => {
                eprintln!("[woodshed-genet] scenario '{path}' rejected: {e}");
                return None;
            }
        };
        eprintln!("[woodshed-genet] scenario lane armed: {path}");
        Some(Self {
            scenario: Some(scenario),
            finished: false,
        })
    }

    /// Where captures and the sentinel go, created if needed.
    pub fn capture_dir_from_env() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var("WOODSHED_CAPTURE_DIR").ok()?);
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    }

    /// Write the `scenario.done` sentinel the harness waits on.
    fn write_outcome(&mut self, outcome: genet_probe::Outcome, capture_dir: Option<&Path>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let result = if outcome.ok {
            "RESULT ok"
        } else {
            "RESULT fail"
        };
        let body = std::iter::once(result.to_string())
            .chain(outcome.log.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        eprintln!("[woodshed-genet] scenario {result}");
        for line in &outcome.log {
            eprintln!("[woodshed-genet]   {line}");
        }
        if let Some(dir) = capture_dir {
            let _ = std::fs::write(dir.join("scenario.done"), body);
        }
    }
}

/// The state a scenario asserts against, sampled each time it may have changed.
/// Kept beside the events it produces so a transition is reported once, with
/// what it was and what it became.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Observed {
    pub cards: usize,
    pub cursor_id: u64,
    pub cursor_number: usize,
    pub relations: String,
    pub edges: usize,
}

impl Observed {
    fn read(ui: &UiState) -> Self {
        let graph = ui
            .set
            .graph()
            .with_relations(&ui.app_settings.stage.visible_relations());
        Self {
            cards: ui.set.cards.len(),
            cursor_id: ui.set.cursor_id().map(|id| id.0).unwrap_or_default(),
            cursor_number: if ui.set.cards.is_empty() {
                0
            } else {
                ui.set.cursor.min(ui.set.cards.len() - 1) + 1
            },
            relations: relation_summary(ui),
            edges: graph.edges.len(),
        }
    }
}

fn relation_summary(ui: &UiState) -> String {
    let visible = ui.app_settings.stage.visible_relations();
    if visible.is_empty() {
        return "none".to_string();
    }
    visible
        .iter()
        .map(|kind| kind.label())
        .collect::<Vec<_>>()
        .join(",")
}

/// Sample the observed state and emit an event for anything that moved. Events
/// are the app's own transitions, not the driver's intentions, so a scenario
/// asserting one is asserting that the app really did it.
pub fn note_events(shared: &mut Shared, ui: &UiState) {
    let now = Observed::read(ui);
    let before = std::mem::replace(&mut shared.observed, now.clone());
    if before == now {
        return;
    }
    if before.cards != now.cards {
        shared.events.push(format!("set-size {}", now.cards));
    }
    if before.cursor_id != now.cursor_id || before.cursor_number != now.cursor_number {
        shared.events.push(format!(
            "set-cursor id={} number={}",
            now.cursor_id, now.cursor_number
        ));
    }
    if before.relations != now.relations {
        shared
            .events
            .push(format!("relations-visible {}", now.relations));
    }
    if before.edges != now.edges {
        shared.events.push(format!("graph-edges {}", now.edges));
    }
}

thread_local! {
    /// The capture armed for the next presented frame: where it goes and the
    /// readback the host will drop into it. A thread-local because the capture
    /// closure outlives the tick that armed it, and the whole application runs
    /// on one thread.
    static PENDING: RefCell<Option<(PathBuf, Rc<RefCell<Option<Frame>>>)>> =
        const { RefCell::new(None) };
}

/// Pump the scenario one step, after a presented frame. Called from the host's
/// `after_frame` hook, so every assertion reads a state that was actually
/// rendered.
pub fn drive(shared: &mut Shared, ctx: &mut Ctx<'_>) {
    if shared.scenario.is_none() {
        return;
    }
    write_pending_capture();
    note_events(shared, ctx.runner.state());
    let Some(mut scenario) = shared.scenario.as_mut().and_then(|l| l.scenario.take()) else {
        return;
    };
    let progress = {
        let mut probe = Probe { ctx, shared };
        scenario.tick(&mut probe)
    };
    // Hold the sentinel until every armed capture has actually been written, or
    // the receipt would claim a screenshot that does not exist.
    if progress == Progress::Done && PENDING.with(|p| p.borrow().is_none()) {
        let mut outcome = scenario.finish();
        if shared.drag_frame_metrics.samples > 0 {
            outcome.log.push(shared.drag_frame_metrics.summary());
        }
        let capture_dir = shared.capture_dir.clone();
        if let Some(lane) = shared.scenario.as_mut() {
            lane.write_outcome(outcome, capture_dir.as_deref());
        }
        *ctx.close = true;
    }
    if let Some(lane) = shared.scenario.as_mut() {
        lane.scenario = Some(scenario);
    }
    // A scenario run must keep frames coming: every step is pumped by one, and
    // an idle app would stall the run rather than finish it.
    if let Some(window) = ctx.window {
        window.request_redraw();
    }
}

/// Encode whatever readback the last armed capture produced.
fn write_pending_capture() {
    let taken = PENDING.with(|p| p.borrow_mut().take());
    let Some((path, sink)) = taken else { return };
    let frame = sink.borrow_mut().take();
    match frame {
        Some(frame) => {
            if !write_png(&frame, &path) {
                eprintln!("[woodshed-genet] capture failed: {}", path.display());
            }
        }
        // Not presented yet: put it back and try again next frame.
        None => PENDING.with(|p| *p.borrow_mut() = Some((path, sink))),
    }
}

/// Write a read-back frame as a PNG. The same pixels the frame presented, so the
/// receipt is the frame.
fn write_png(frame: &Frame, path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(file)
        .write_image(
            &frame.rgba,
            frame.width,
            frame.height,
            image::ExtendedColorType::Rgba8,
        )
        .is_ok()
}

/// The `Automatable` view of woodshed, borrowed for the duration of one tick.
///
/// The host owns the runner, so the application cannot hold a long-lived `&mut`
/// to it; it borrows the hook's context for exactly as long as the driver needs
/// and queues pointer delivery back through the host.
struct Probe<'a, 'c> {
    ctx: &'a mut Ctx<'c>,
    shared: &'a mut Shared,
}

impl Probe<'_, '_> {
    /// The `data-key` of the Set-graph node the canvas is currently painting
    /// as focused, or `"none"`. The canvas owns that emphasis, so the DOM is
    /// the authority for it.
    fn graph_focus_key(&self) -> String {
        use layout_dom_api::LayoutDom;
        let dom = self.ctx.runner.dom();
        let dom = dom.borrow();
        dom.all_with_class(dom.document(), "graph-canvas-swatch-node")
            .into_iter()
            .find(|node| {
                dom.attribute(
                    *node,
                    &layout_dom_api::Namespace::from(""),
                    &layout_dom_api::LocalName::from("class"),
                )
                .is_some_and(|class| class.split_whitespace().any(|token| token == "focused"))
            })
            .and_then(|node| {
                dom.attribute(
                    node,
                    &layout_dom_api::Namespace::from(""),
                    &layout_dom_api::LocalName::from("data-key"),
                )
                .map(str::to_owned)
            })
            .unwrap_or_else(|| "none".to_string())
    }

    fn graph_label_placements(&self) -> (usize, usize) {
        use layout_dom_api::LayoutDom;
        let dom = self.ctx.runner.dom();
        let dom = dom.borrow();
        dom.all_with_class(dom.document(), "graph-canvas-swatch-label")
            .into_iter()
            .fold((0, 0), |(above, side), node| {
                let placement = dom.attribute(
                    node,
                    &layout_dom_api::Namespace::from(""),
                    &layout_dom_api::LocalName::from("data-label-placement"),
                );
                match placement {
                    Some("above") => (above + 1, side),
                    Some("left" | "right") => (above, side + 1),
                    _ => (above, side),
                }
            })
    }

    fn graph_node_card_count(&self) -> usize {
        use layout_dom_api::LayoutDom;
        let dom = self.ctx.runner.dom();
        let dom = dom.borrow();
        dom.all_with_class(dom.document(), "set-graph-node-card")
            .len()
    }

    fn pointer_capture_class(&self) -> String {
        use layout_dom_api::LayoutDom;
        let Some(node) = self.ctx.runner.pointer_capture() else {
            return "none".to_string();
        };
        let dom = self.ctx.runner.dom();
        let dom = dom.borrow();
        dom.attribute(
            node,
            &layout_dom_api::Namespace::from(""),
            &layout_dom_api::LocalName::from("class"),
        )
        .unwrap_or("unclassed")
        .to_string()
    }

    fn stage_node_key(&self, index: usize) -> Option<String> {
        let ui = self.ctx.runner.state();
        let snapshot = set_graph_snapshot(ui);
        set_graph_swatch_from_snapshot(&snapshot, ui, ui.set_tray_expanded)
            .graph
            .nodes
            .get(index)
            .and_then(|node| node.key.clone())
    }

    fn stage_relation_id(&self, index: usize) -> Option<String> {
        let ui = self.ctx.runner.state();
        let snapshot = set_graph_snapshot(ui);
        set_graph_swatch_from_snapshot(&snapshot, ui, ui.set_tray_expanded)
            .relations
            .get(index)
            .map(|relation| relation.id.clone())
    }
}

impl Automatable for Probe<'_, '_> {
    fn with_surfaces<R>(&self, f: impl FnOnce(&[ProbeSurface<'_>]) -> R) -> R {
        let dom = self.ctx.runner.dom();
        let dom_ref = dom.borrow();
        let (w, h) = self.ctx.logical_size;
        f(&[ProbeSurface {
            name: "woodshed",
            dom: &dom_ref,
            // One runner covers the window, and the probe resolves in the same
            // logical coordinates the layout and the cursor use.
            rect: [0.0, 0.0, w, h],
            sheet: &self.shared.accessible_sheet(),
        }])
    }

    fn snapshot(&self) -> ProbeSnapshot {
        let ui = self.ctx.runner.state();
        let observed = Observed::read(ui);
        let stage_snapshot = set_graph_snapshot(ui);
        let stage_swatch =
            set_graph_swatch_from_snapshot(&stage_snapshot, ui, ui.set_tray_expanded);
        let routed_lanes = stage_swatch
            .projected_relations()
            .iter()
            .filter(|(_, route)| route.len() == 4)
            .count();
        let (card_incident_relations, card_anchored_relations) = stage_swatch
            .selected
            .as_ref()
            .and_then(|selected| {
                let region = stage_swatch.projected_node_footprint(selected)?;
                let right = region.left + region.width;
                let bottom = region.top + region.height;
                let mut incident = 0;
                let mut anchored = 0;
                for (relation, route) in stage_swatch.projected_relations() {
                    let endpoint = if &relation.from == selected {
                        route.first().copied()
                    } else if &relation.to == selected {
                        route.last().copied()
                    } else {
                        None
                    };
                    let Some(point) = endpoint else {
                        continue;
                    };
                    incident += 1;
                    let on_vertical =
                        (point.0 - region.left).abs() < 0.01 || (point.0 - right).abs() < 0.01;
                    let on_horizontal =
                        (point.1 - region.top).abs() < 0.01 || (point.1 - bottom).abs() < 0.01;
                    if (on_vertical && point.1 >= region.top && point.1 <= bottom)
                        || (on_horizontal && point.0 >= region.left && point.0 <= right)
                    {
                        anchored += 1;
                    }
                }
                Some((incident, anchored))
            })
            .unwrap_or_default();
        let (labels_above, labels_side) = self.graph_label_placements();
        let relation_choices = set_graph_relation_choices(&stage_snapshot, ui);
        let cursor_label = ui
            .set
            .cursor_id()
            .and_then(|id| ui.set.card(id))
            .map(|card| card.label.clone())
            .unwrap_or_default();
        let mut snap = ProbeSnapshot::default()
            .with_field("set-cards", observed.cards.to_string())
            .with_field("cursor-id", observed.cursor_id.to_string())
            .with_field("cursor-number", observed.cursor_number.to_string())
            .with_field("cursor-label", cursor_label.clone())
            .with_field("graph-nodes", ui.set.graph().nodes.len().to_string())
            .with_field("graph-edges", observed.edges.to_string())
            .with_field("graph-relations", stage_swatch.relations.len().to_string())
            .with_field(
                "graph-arrangement",
                ui.app_settings.stage.set_arrangement.label(),
            )
            .with_field("graph-routed-lanes", routed_lanes.to_string())
            .with_field("graph-labels-above", labels_above.to_string())
            .with_field("graph-labels-side", labels_side.to_string())
            .with_field("graph-width", stage_swatch.width.to_string())
            .with_field("graph-height", stage_swatch.height.to_string())
            .with_field("graph-node-cards", self.graph_node_card_count().to_string())
            .with_field(
                "graph-card-incident-relations",
                card_incident_relations.to_string(),
            )
            .with_field(
                "graph-card-anchored-relations",
                card_anchored_relations.to_string(),
            )
            .with_field("pointer-capture", self.pointer_capture_class())
            .with_field(
                "graph-visible-relations",
                relation_choices
                    .iter()
                    .filter(|relation| relation.visible)
                    .count()
                    .to_string(),
            )
            .with_field("graph-relation-choices", relation_choices.len().to_string())
            .with_field("graph-epoch", stage_snapshot.epoch().0.to_string())
            .with_field("graph-drag-active", ui.set_graph_drag_active.to_string())
            .with_field(
                "graph-selected-relation",
                ui.set_graph_relation.is_some().to_string(),
            )
            .with_field(
                "graph-moved-nodes",
                ui.set_graph_positions.len().to_string(),
            )
            .with_field(
                "view-only-dispatches",
                self.shared.view_only_dispatches.to_string(),
            )
            .with_field(
                "full-dispatch-syncs",
                self.shared.full_dispatch_syncs.to_string(),
            )
            .with_field(
                "drag-present-samples",
                self.shared.drag_frame_metrics.samples.to_string(),
            )
            .with_field(
                "drag-present-average-us",
                self.shared.drag_frame_metrics.average_us().to_string(),
            )
            .with_field(
                "drag-present-max-us",
                self.shared.drag_frame_metrics.max_us.to_string(),
            )
            .with_field(
                "drag-viewport-us",
                self.shared.drag_frame_metrics.viewport_us.to_string(),
            )
            .with_field(
                "drag-drive-us",
                self.shared.drag_frame_metrics.drive_us.to_string(),
            )
            .with_field(
                "drag-leaves-us",
                self.shared.drag_frame_metrics.leaves_us.to_string(),
            )
            .with_field(
                "drag-root-rebuilds",
                self.shared.drag_frame_metrics.root_rebuilds.to_string(),
            )
            .with_field(
                "drag-host-samples",
                self.shared.drag_frame_metrics.host_samples.to_string(),
            )
            .with_field(
                "drag-host-average-us",
                self.shared.drag_frame_metrics.host_average_us().to_string(),
            )
            .with_field(
                "drag-host-max-us",
                self.shared.drag_frame_metrics.host_max_us.to_string(),
            )
            .with_field(
                "drag-host-relayout-us",
                self.shared.drag_frame_metrics.host_relayout_us.to_string(),
            )
            .with_field(
                "drag-host-layout-update-us",
                self.shared
                    .drag_frame_metrics
                    .host_layout_update_us
                    .to_string(),
            )
            .with_field(
                "drag-host-layout-apply-us",
                self.shared
                    .drag_frame_metrics
                    .host_layout_apply_us
                    .to_string(),
            )
            .with_field(
                "drag-host-layout-mutations",
                self.shared
                    .drag_frame_metrics
                    .host_layout_mutations
                    .to_string(),
            )
            .with_field(
                "drag-host-layout-rebuilds",
                self.shared
                    .drag_frame_metrics
                    .host_layout_rebuilds
                    .to_string(),
            )
            .with_field(
                "drag-host-leaf-boxes-us",
                self.shared
                    .drag_frame_metrics
                    .host_leaf_boxes_us
                    .to_string(),
            )
            .with_field(
                "drag-host-leaf-render-us",
                self.shared
                    .drag_frame_metrics
                    .host_leaf_render_us
                    .to_string(),
            )
            .with_field(
                "drag-host-leaf-repaints",
                self.shared
                    .drag_frame_metrics
                    .host_leaf_repaints
                    .to_string(),
            )
            .with_field(
                "drag-host-fragments-us",
                self.shared.drag_frame_metrics.host_fragments_us.to_string(),
            )
            .with_field(
                "drag-host-emit-us",
                self.shared.drag_frame_metrics.host_emit_us.to_string(),
            )
            .with_field(
                "drag-host-raster-us",
                self.shared.drag_frame_metrics.host_raster_us.to_string(),
            )
            .with_field(
                "drag-host-acquire-us",
                self.shared.drag_frame_metrics.host_acquire_us.to_string(),
            )
            .with_field(
                "drag-host-clear-us",
                self.shared.drag_frame_metrics.host_clear_us.to_string(),
            )
            .with_field(
                "drag-host-compose-us",
                self.shared.drag_frame_metrics.host_compose_us.to_string(),
            )
            .with_field(
                "drag-host-present-us",
                self.shared.drag_frame_metrics.host_present_us.to_string(),
            )
            .with_field(
                "drag-host-a11y-us",
                self.shared.drag_frame_metrics.host_a11y_us.to_string(),
            )
            .with_field(
                "drag-raster-inner-us",
                self.shared.drag_frame_metrics.raster_inner_us.to_string(),
            )
            .with_field(
                "drag-raster-invalidate-us",
                self.shared
                    .drag_frame_metrics
                    .tile_invalidate_us
                    .to_string(),
            )
            .with_field(
                "drag-raster-rebuild-us",
                self.shared
                    .drag_frame_metrics
                    .dirty_tile_rebuild_us
                    .to_string(),
            )
            .with_field(
                "drag-raster-master-us",
                self.shared.drag_frame_metrics.master_compose_us.to_string(),
            )
            .with_field(
                "drag-raster-vello-us",
                self.shared.drag_frame_metrics.vello_render_us.to_string(),
            )
            .with_field(
                "drag-dirty-tiles",
                self.shared.drag_frame_metrics.dirty_tiles.to_string(),
            )
            .with_field(
                "drag-max-dirty-tiles",
                self.shared.drag_frame_metrics.max_dirty_tiles.to_string(),
            )
            .with_field("relations", observed.relations)
            .with_field("tray-expanded", ui.set_tray_expanded.to_string())
            .with_field("card-expanded", ui.set_graph_card_expanded.to_string())
            // Graph focus emphasis belongs to the canvas component now, so it
            // is observed where it actually is — the focused node's rendered
            // `data-key` — rather than from a UiState field that would report
            // "none" forever.
            .with_field("graph-focus", self.graph_focus_key())
            .with_field("section", ui.section.label().to_string())
            .with_field("lens", format!("{:?}", ui.stage.lens))
            .with_field("material", ui.stage.material_name());
        // The Related frontier, observed as the app computes it: the top
        // neighbor and how many distinct relations connect it. A scenario can
        // then assert that multiplicity survived ranking, rather than sniffing
        // for a substring in the DOM.
        let related = ui.stage.related_material_configured(
            &ui.practice_history,
            &ui.app_settings.stage.related,
            8,
        );
        if let Some(top) = related.first() {
            snap = snap
                .with_field("related-top", top.title.clone())
                .with_field("related-top-relations", top.relation_count().to_string())
                .with_field(
                    "related-top-kinds",
                    top.relations
                        .iter()
                        .map(|relation| relation.kind.label())
                        .collect::<Vec<_>>()
                        .join(","),
                )
                .with_field(
                    "related-top-authorities",
                    top.relations
                        .iter()
                        .map(|relation| format!("{:?}", relation.authority))
                        .collect::<Vec<_>>()
                        .join(","),
                );
        }
        snap = snap
            .with_field("related-count", related.len().to_string())
            .with_field(
                "related-multi-count",
                related
                    .iter()
                    .filter(|neighbor| neighbor.relation_count() > 1)
                    .count()
                    .to_string(),
            );
        // The richest pair on the frontier: the one carrying the most relations
        // at once. A single top row can honestly have one reason (an arpeggio
        // realizes its chord, and that is all it does), so multiplicity is
        // asserted where it actually lives.
        if let Some(richest) = related
            .iter()
            .max_by_key(|neighbor| (neighbor.relation_count(), neighbor.score))
        {
            snap = snap
                .with_field("related-richest", richest.title.clone())
                .with_field(
                    "related-richest-relations",
                    richest.relation_count().to_string(),
                )
                .with_field(
                    "related-richest-kinds",
                    richest
                        .relations
                        .iter()
                        .map(|relation| relation.kind.label())
                        .collect::<Vec<_>>()
                        .join(","),
                )
                .with_field(
                    "related-richest-authorities",
                    richest
                        .relations
                        .iter()
                        .map(|relation| format!("{:?}", relation.authority))
                        .collect::<Vec<_>>()
                        .join(","),
                );
        }
        if !cursor_label.is_empty() {
            snap = snap.with_focus(cursor_label);
        }
        snap
    }

    fn drain_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.shared.events)
    }

    fn act(&mut self, label: &str) -> bool {
        if label == "reset-dispatch-sync-counts" {
            self.shared.view_only_dispatches = 0;
            self.shared.full_dispatch_syncs = 0;
            self.shared.drag_frame_metrics.reset();
            self.shared.scenario_drag_origin = None;
            return true;
        }
        let mut known = true;
        self.ctx.runner.update(|ui| match label {
            "stage-current" => ui.stage_current(None),
            "stage-related-pair" => {
                ui.stage.set_lens(Lens::Chords);
                if let Some(major) = ui
                    .stage
                    .chords()
                    .iter()
                    .position(|chord| chord.name == "Major")
                {
                    ui.stage.select_chord(major);
                    ui.stage_current(None);
                }
                if let Some(major_seven) = ui
                    .stage
                    .chords()
                    .iter()
                    .position(|chord| chord.name == "Major 7")
                {
                    ui.stage.select_chord(major_seven);
                    ui.stage_current(None);
                }
                ui.set_tray_expanded = true;
            }
            "hide-first-stage-relation" => {
                let snapshot = set_graph_snapshot(ui);
                if let Some(key) = set_graph_relation_choices(&snapshot, ui)
                    .first()
                    .map(|relation| relation.key.clone())
                {
                    ui.toggle_set_graph_relation(&snapshot, key);
                }
            }
            "hide-all-stage-relations" => {
                let snapshot = set_graph_snapshot(ui);
                ui.hide_all_set_graph_relations(&snapshot);
            }
            "show-all-stage-relations" => ui.show_all_set_graph_relations(),
            "expand-tray" => ui.set_tray_expanded = true,
            "collapse-tray" => ui.set_tray_expanded = false,
            "duplicate-selected" => {
                let cursor = ui.set.cursor;
                ui.set.duplicate(cursor);
            }
            "move-selected-down" => {
                let cursor = ui.set.cursor;
                ui.set.move_card(cursor, 1);
            }
            "move-selected-up" => {
                let cursor = ui.set.cursor;
                ui.set.move_card(cursor, -1);
            }
            "remove-selected" => {
                let cursor = ui.set.cursor;
                ui.set.remove(cursor);
            }
            _ => known = false,
        });
        if known {
            crate::sync::after_dispatch(self.shared, self.ctx);
            note_events(self.shared, self.ctx.runner.state());
        }
        known
    }

    fn press(&mut self, x: f32, y: f32) {
        // Routed by the host: the same hit test, capture, and dispatch a real
        // pointer takes. Delivered once this hook returns, and observed by the
        // next frame's tick — which is where the scenario asserts anyway.
        self.ctx.pointer.push(HostPointer::Press(x, y));
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.ctx.pointer.push(HostPointer::Moved(x, y));
    }

    fn release(&mut self, x: f32, y: f32) {
        self.ctx.pointer.push(HostPointer::Release(x, y));
    }

    fn busy(&mut self) -> Option<bool> {
        // Woodshed has no fetch or actor round-trip in this lane: a step's
        // effect is in the state by the time the next frame lays out. Reporting
        // quiet is honest here, and keeps `wait` from burning its cap.
        Some(false)
    }
}

impl Driveable for Probe<'_, '_> {
    /// Stage-canvas receipt verbs resolve epoch-qualified identities from the
    /// live snapshot, then use the ordinary host pointer lifecycle. This keeps
    /// them distinct from the Related graph, which shares the same CSS classes.
    fn app_step(&mut self, line: &str) -> Result<(), String> {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("open-stage-arrangement") => {
                if parts.next().is_some() {
                    return Err("open-stage-arrangement takes no arguments".to_string());
                }
                let selector = Selector::role("combobox").containing("Set arrangement");
                self.click(&selector)
                    .then_some(())
                    .ok_or_else(|| "open-stage-arrangement missed the picker".to_string())
            }
            Some("choose-stage-arrangement") => {
                let label = parts
                    .next()
                    .ok_or("choose-stage-arrangement wants one arrangement label")?;
                if parts.next().is_some() {
                    return Err("choose-stage-arrangement takes one arrangement label".to_string());
                }
                let selector = Selector::role("option").containing(label);
                self.click(&selector)
                    .then_some(())
                    .ok_or_else(|| format!("choose-stage-arrangement missed {label}"))
            }
            Some("click-stage-node") => {
                let index: usize = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("click-stage-node wants a node index")?;
                if parts.next().is_some() {
                    return Err("click-stage-node takes one index".to_string());
                }
                let node_key = self
                    .stage_node_key(index)
                    .ok_or_else(|| format!("no Stage node at index {index}"))?;
                let selector =
                    Selector::class("graph-canvas-swatch-node").with_attr("data-key", node_key);
                self.click(&selector)
                    .then_some(())
                    .ok_or_else(|| format!("click-stage-node missed index {index}"))
            }
            Some("click-stage-relation") => {
                let index: usize = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("click-stage-relation wants a relation index")?;
                if parts.next().is_some() {
                    return Err("click-stage-relation takes one index".to_string());
                }
                let relation_id = self
                    .stage_relation_id(index)
                    .ok_or_else(|| format!("no Stage relation at index {index}"))?;
                let selector = Selector::class("graph-canvas-swatch-relation")
                    .with_attr("data-relation-id", relation_id);
                self.click(&selector)
                    .then_some(())
                    .ok_or_else(|| format!("click-stage-relation missed index {index}"))
            }
            Some("drag-stage-node") => {
                let index: usize = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("drag-stage-node wants a node index")?;
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("drag-stage-node wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("drag-stage-node wants a y delta")?;
                if parts.next().is_some() {
                    return Err("drag-stage-node takes exactly index, dx, and dy".to_string());
                }
                let node_key = self
                    .stage_node_key(index)
                    .ok_or_else(|| format!("no Stage node at index {index}"))?;
                let selector =
                    Selector::class("graph-canvas-swatch-node").with_attr("data-key", node_key);
                let hit = self
                    .resolve(&selector)
                    .ok_or_else(|| format!("drag-stage-node missed index {index}"))?;
                let start = hit.point;
                let end = (start.0 + dx, start.1 + dy);
                self.press(start.0, start.1);
                self.moved((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5);
                self.moved(end.0, end.1);
                self.release(end.0, end.1);
                Ok(())
            }
            Some("drag-stage-graph-resize") => {
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("drag-stage-graph-resize wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("drag-stage-graph-resize wants a y delta")?;
                if parts.next().is_some() {
                    return Err("drag-stage-graph-resize takes exactly dx and dy".to_string());
                }
                let selector = Selector::class("resize-handle");
                let hit = self
                    .resolve(&selector)
                    .ok_or("drag-stage-graph-resize missed the handle")?;
                let start = hit.point;
                let end = (start.0 + dx, start.1 + dy);
                self.press(start.0, start.1);
                self.moved((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5);
                self.moved(end.0, end.1);
                self.release(end.0, end.1);
                Ok(())
            }
            Some("press-stage-graph-resize") => {
                if parts.next().is_some() {
                    return Err("press-stage-graph-resize takes no arguments".to_string());
                }
                let selector = Selector::class("resize-handle");
                let hit = self
                    .resolve(&selector)
                    .ok_or("press-stage-graph-resize missed the handle")?;
                self.shared.scenario_drag_origin = Some(hit.point);
                self.press(hit.point.0, hit.point.1);
                Ok(())
            }
            Some("move-stage-graph-resize") => {
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("move-stage-graph-resize wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("move-stage-graph-resize wants a y delta")?;
                if parts.next().is_some() {
                    return Err("move-stage-graph-resize takes x and y deltas".to_string());
                }
                let origin = self
                    .shared
                    .scenario_drag_origin
                    .ok_or("move-stage-graph-resize has no pressed handle")?;
                self.moved(origin.0 + dx, origin.1 + dy);
                Ok(())
            }
            Some("release-stage-graph-resize") => {
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("release-stage-graph-resize wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("release-stage-graph-resize wants a y delta")?;
                if parts.next().is_some() {
                    return Err("release-stage-graph-resize takes x and y deltas".to_string());
                }
                let origin = self
                    .shared
                    .scenario_drag_origin
                    .take()
                    .ok_or("release-stage-graph-resize has no pressed handle")?;
                self.release(origin.0 + dx, origin.1 + dy);
                Ok(())
            }
            Some("press-stage-node") => {
                let index: usize = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("press-stage-node wants a node index")?;
                if parts.next().is_some() {
                    return Err("press-stage-node takes one index".to_string());
                }
                let node_key = self
                    .stage_node_key(index)
                    .ok_or_else(|| format!("no Stage node at index {index}"))?;
                let selector =
                    Selector::class("graph-canvas-swatch-node").with_attr("data-key", node_key);
                let hit = self
                    .resolve(&selector)
                    .ok_or_else(|| format!("press-stage-node missed index {index}"))?;
                self.shared.scenario_drag_origin = Some(hit.point);
                self.press(hit.point.0, hit.point.1);
                Ok(())
            }
            Some("move-stage-node") => {
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("move-stage-node wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("move-stage-node wants a y delta")?;
                if parts.next().is_some() {
                    return Err("move-stage-node takes x and y deltas".to_string());
                }
                let origin = self
                    .shared
                    .scenario_drag_origin
                    .ok_or("move-stage-node has no pressed node")?;
                self.moved(origin.0 + dx, origin.1 + dy);
                Ok(())
            }
            Some("release-stage-node") => {
                let dx: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("release-stage-node wants an x delta")?;
                let dy: f32 = parts
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or("release-stage-node wants a y delta")?;
                if parts.next().is_some() {
                    return Err("release-stage-node takes x and y deltas".to_string());
                }
                let origin = self
                    .shared
                    .scenario_drag_origin
                    .take()
                    .ok_or("release-stage-node has no pressed node")?;
                self.release(origin.0 + dx, origin.1 + dy);
                Ok(())
            }
            _ => Err(format!("unknown verb: {line}")),
        }
    }

    fn capture(&mut self, name: &str) -> bool {
        let Some(path) = self
            .shared
            .capture_dir
            .as_ref()
            .map(|dir| dir.join(format!("{name}.png")))
        else {
            // No capture dir: the run is an assertion-only receipt. Say so
            // rather than claiming a screenshot that does not exist.
            eprintln!("[woodshed-genet] capture '{name}' skipped: no WOODSHED_CAPTURE_DIR");
            return true;
        };
        let sink = Rc::new(RefCell::new(None::<Frame>));
        let out = sink.clone();
        *self.ctx.capture = Some(Box::new(move |surface, view, w, h| {
            *out.borrow_mut() = read_frame(surface, view, w, h);
        }));
        PENDING.with(|p| *p.borrow_mut() = Some((path, sink)));
        if let Some(window) = self.ctx.window {
            window.request_redraw();
        }
        true
    }
}
