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

use cambium_genet_winit_host::{Frame, HostPointer, read_frame};
use genet_probe::{Automatable, Driveable, ProbeSnapshot, ProbeSurface, Progress, Scenario};
use woodshed_views::stage::UiState;

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
        let result = if outcome.ok { "RESULT ok" } else { "RESULT fail" };
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
        let outcome = scenario.finish();
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
            .with_field("relations", observed.relations)
            .with_field("tray-expanded", ui.set_tray_expanded.to_string())
            .with_field("card-expanded", ui.set_graph_card_expanded.to_string())
            .with_field(
                "graph-focus",
                ui.set_graph_focus
                    .map(|id| id.0.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            )
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
        let mut known = true;
        self.ctx.runner.update(|ui| match label {
            "stage-current" => ui.stage_current(None),
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
