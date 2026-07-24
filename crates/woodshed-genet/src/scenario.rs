//! The scenario lane: woodshed drives itself.
//!
//! Woodshed had no self-drive hook, so every headed receipt was SendKeys plus a
//! desktop grab: synthetic input that loses the foreground race whenever the
//! machine is in use, and captures that can silently photograph the wrong
//! window. This lane replaces both. The generic half (parsing, the verb loop,
//! selector resolution, assertions) is [`genet_probe`]; what lives here is only
//! what is woodshed's: which surfaces it has, what it can be asked to observe,
//! its named commands, how a delivered point routes, and how a frame is
//! captured.
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

use std::path::{Path, PathBuf};

use genet_probe::{Automatable, Driveable, ProbeSnapshot, ProbeSurface, Progress, Scenario};
use woodshed_views::stage::UiState;

use crate::App;

/// A running scenario. Where its receipts go lives on the app, not here: the
/// lane is moved out of the app for the duration of a tick (so the driver can
/// hold `&mut App`), which would otherwise make the capture directory
/// unreachable from inside a `capture` step.
pub struct ScenarioLane {
    scenario: Scenario,
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
            scenario,
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
    fn write_outcome(&mut self, capture_dir: Option<&Path>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let outcome = self.scenario.finish();
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

impl App {
    /// Sample the observed state and emit an event for anything that moved.
    /// Events are the app's own transitions, not the driver's intentions, so a
    /// scenario asserting one is asserting that the app really did it.
    pub(crate) fn note_events(&mut self) {
        let Some(runner) = self.runner.as_ref() else {
            return;
        };
        let now = Observed::read(runner.state());
        let before = std::mem::replace(&mut self.observed, now.clone());
        if before == now {
            return;
        }
        if before.cards != now.cards {
            self.events.push(format!("set-size {}", now.cards));
        }
        if before.cursor_id != now.cursor_id || before.cursor_number != now.cursor_number {
            self.events.push(format!(
                "set-cursor id={} number={}",
                now.cursor_id, now.cursor_number
            ));
        }
        if before.relations != now.relations {
            self.events
                .push(format!("relations-visible {}", now.relations));
        }
        if before.edges != now.edges {
            self.events.push(format!("graph-edges {}", now.edges));
        }
    }

    /// Pump the scenario one step, after a presented frame. The lane is moved
    /// out for the tick so the driver can hold `&mut self`.
    pub(crate) fn drive_scenario(&mut self) {
        let Some(mut lane) = self.scenario.take() else {
            return;
        };
        let progress = lane.scenario.tick(self);
        if progress == Progress::Done {
            lane.write_outcome(self.capture_dir.as_deref());
            self.close_requested = true;
        }
        self.scenario = Some(lane);
        if let Some(window) = self.window.as_ref() {
            // A scenario run must keep frames coming: every step is pumped by
            // one, and an idle app would stall the run rather than finish it.
            window.request_redraw();
        }
    }

    /// Perform a capture requested by the previous frame's `capture` step, using
    /// the view this frame rasterized. Called from `redraw` after present, so
    /// the pixels are exactly what was shown.
    pub(crate) fn take_pending_capture(&mut self) -> Option<PathBuf> {
        self.pending_capture.take()
    }
}

impl Automatable for App {
    fn with_surfaces<R>(&self, f: impl FnOnce(&[ProbeSurface<'_>]) -> R) -> R {
        let Some(runner) = self.runner.as_ref() else {
            return f(&[]);
        };
        let dom = runner.dom();
        let dom_ref = dom.borrow();
        f(&[ProbeSurface {
            name: "woodshed",
            dom: &dom_ref,
            // One runner covers the window, and the probe resolves in the same
            // logical coordinates the layout and the cursor use.
            rect: [0.0, 0.0, self.layout_size.0, self.layout_size.1],
            sheet: &self.sheet,
        }])
    }

    fn snapshot(&self) -> ProbeSnapshot {
        let Some(runner) = self.runner.as_ref() else {
            return ProbeSnapshot::default();
        };
        let ui = runner.state();
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
        snap = snap.with_field("related-count", related.len().to_string()).with_field(
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
        std::mem::take(&mut self.events)
    }

    fn act(&mut self, label: &str) -> bool {
        let Some(runner) = self.runner.as_mut() else {
            return false;
        };
        let mut known = true;
        runner.update(|ui| match label {
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
            self.after_dispatch();
            self.note_events();
        }
        known
    }

    fn press(&mut self, x: f32, y: f32) {
        // Woodshed dispatches on press, through the same hit-test path a real
        // pointer takes; `release` is where a real click ends, not where it
        // acts, so this is the honest routing rather than a shortcut.
        self.cursor = (x, y);
        self.click();
        self.note_events();
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.cursor = (x, y);
        self.hover();
    }

    fn release(&mut self, _x: f32, _y: f32) {}

    fn busy(&mut self) -> Option<bool> {
        // Woodshed has no fetch or actor round-trip in this lane: a step's
        // effect is in the state by the time the next frame lays out. Reporting
        // quiet is honest here, and keeps `wait` from burning its cap.
        Some(false)
    }
}

impl Driveable for App {
    fn capture(&mut self, name: &str) -> bool {
        let Some(path) = self
            .capture_dir
            .as_ref()
            .map(|dir| dir.join(format!("{name}.png")))
        else {
            // No capture dir: the run is an assertion-only receipt. Say so
            // rather than claiming a screenshot that does not exist.
            eprintln!("[woodshed-genet] capture '{name}' skipped: no WOODSHED_CAPTURE_DIR");
            return true;
        };
        // The capture runs in the next frame's redraw, where the rasterized
        // view still exists. Nothing between now and then changes the state:
        // the scenario pumps one step per frame.
        self.pending_capture = Some(path);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }
}

/// Compose `view` into an owned `COPY_SRC` target, read it back, and write a
/// PNG. The same view the frame presented, so the receipt is the frame.
pub(crate) fn capture_view(
    host: &genet_winit_host::SurfaceHost,
    view: &wgpu::TextureView,
    width: u32,
    height: u32,
    path: &Path,
) -> bool {
    use netrender::ExternalTexturePlacement;

    let target = host.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("woodshed scenario capture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    host.renderer().compose_external_texture(
        view,
        &target_view,
        wgpu::TextureFormat::Rgba8Unorm,
        width,
        height,
        ExternalTexturePlacement::new([0.0, 0.0, width as f32, height as f32]),
    );
    let rgba = read_texture_rgba(host.device(), host.queue(), &target, width, height);
    if rgba.is_empty() {
        return false;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(file)
        .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
        .is_ok()
}

/// Read a texture back as tightly packed RGBA8 (empty on failure). Standard
/// wgpu readback: copy into a row-aligned buffer, map it, strip the padding
/// each row carries.
fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("woodshed capture readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("woodshed capture readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    if device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .is_err()
    {
        eprintln!("[woodshed-genet] capture readback poll failed");
        return Vec::new();
    }
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}
