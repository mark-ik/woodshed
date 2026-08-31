//! Woodshed's small workspace model over Genet's reusable tab/split core.
//!
//! The shared component owns tab and split mechanics. Woodshed owns which product surfaces
//! occupy those tabs, the stable ids that identify them, and its snapshot/restore policy. The
//! panel contents are deliberately open lanes: they are existing Woodshed views, not documents
//! or a new page system.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use workbench::{
    ContentSource, SplitAxis, TabStack, Tile, TileBranch, TileEvent, TileId, TileTree, Workbench,
    WorkbenchEffect, WorkbenchOutcome,
};

/// A Woodshed surface placed in the shared workspace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePanel {
    Practice,
    Set,
    Related,
    Settings,
}

impl WorkspacePanel {
    pub const ALL: [Self; 4] = [Self::Practice, Self::Set, Self::Related, Self::Settings];

    pub const fn tile_id(self) -> TileId {
        match self {
            Self::Practice => TileId(0x574f_5052),
            Self::Set => TileId(0x574f_5345),
            Self::Related => TileId(0x574f_5245),
            Self::Settings => TileId(0x574f_5347),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Practice => "Practice",
            Self::Set => "Set",
            Self::Related => "Related",
            Self::Settings => "Settings",
        }
    }

    fn lane(self) -> (&'static str, &'static str) {
        match self {
            Self::Practice => ("woodshed.practice", "stage"),
            Self::Set => ("woodshed.set", "rehearsal"),
            Self::Related => ("woodshed.related", "stage"),
            Self::Settings => ("woodshed.settings", "settings"),
        }
    }

    fn tile(self) -> Tile {
        let (kind, id) = self.lane();
        Tile {
            id: self.tile_id(),
            title: self.label().to_string(),
            content: ContentSource::Open {
                kind: kind.to_string(),
                id: id.to_string(),
            },
            accent: None,
        }
    }

    fn from_tile(tile: &Tile) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|panel| panel.tile_id() == tile.id)
    }

    fn validates_tile(self, tile: &Tile) -> bool {
        let expected = self.tile();
        tile.id == expected.id
            && tile.title == expected.title
            && tile.content == expected.content
            && tile.accent.is_none()
    }
}

/// A typed workspace command that Woodshed routes to the reusable reducer.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceEvent {
    Tile(TileEvent),
}

impl WorkspaceEvent {
    pub fn activate(panel: WorkspacePanel) -> Self {
        Self::Tile(TileEvent::Activated(panel.tile_id()))
    }
}

/// A Woodshed-owned host decision requested by a workspace gesture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceEffect {
    TearOut { panel: WorkspacePanel },
}

/// Result of one typed workspace event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceOutcome {
    Activated(WorkspacePanel),
    Changed,
    Unchanged,
    Effect(WorkspaceEffect),
}

/// Woodshed's snapshot of the shared component's presentation state.
///
/// This is intentionally a host type. It captures the current tree without claiming that the
/// shared component owns Woodshed session storage or its migration policy.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceSnapshot {
    tree: TileTree,
    active_panel: Option<WorkspacePanel>,
}

/// Errors at Woodshed's durable workspace boundary.
#[derive(Debug)]
pub enum WorkspaceSnapshotError {
    Encode(serde_json::Error),
    Decode(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for WorkspaceSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(error) => write!(f, "could not encode Woodshed workspace: {error}"),
            Self::Decode(error) => write!(f, "could not decode Woodshed workspace: {error}"),
            Self::Invalid(reason) => write!(f, "invalid Woodshed workspace: {reason}"),
        }
    }
}

impl std::error::Error for WorkspaceSnapshotError {}

#[derive(Deserialize, Serialize)]
struct WorkspaceSnapshotDto {
    tree: WorkspaceTreeDto,
    active_panel: Option<WorkspacePanel>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkspaceTreeDto {
    Split {
        axis: WorkspaceAxisDto,
        children: Vec<WorkspaceBranchDto>,
    },
    Stack {
        tabs: Vec<WorkspaceTileDto>,
        active: usize,
    },
}

#[derive(Deserialize, Serialize)]
struct WorkspaceBranchDto {
    fraction: f32,
    tree: WorkspaceTreeDto,
}

#[derive(Deserialize, Serialize)]
struct WorkspaceTileDto {
    panel: WorkspacePanel,
    id: u64,
    title: String,
    kind: String,
    content_id: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkspaceAxisDto {
    Row,
    Column,
}

/// The host-owned workspace around Woodshed's existing views.
#[derive(Clone, Debug, PartialEq)]
pub struct WoodshedWorkspace {
    workbench: Workbench,
    /// The panel Woodshed most recently routed. A split has several stack-local active indices,
    /// so that presentation state cannot be inferred from the first tree branch.
    active_panel: Option<WorkspacePanel>,
}

impl Default for WoodshedWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl WoodshedWorkspace {
    /// Start with one tab stack for the four existing product surfaces.
    pub fn new() -> Self {
        Self {
            workbench: Workbench::new(TileTree::stack(
                WorkspacePanel::ALL
                    .into_iter()
                    .map(WorkspacePanel::tile)
                    .collect(),
                0,
            )),
            active_panel: Some(WorkspacePanel::Practice),
        }
    }

    pub fn tree(&self) -> &TileTree {
        self.workbench.tree()
    }

    pub fn active_panel(&self) -> Option<WorkspacePanel> {
        self.active_panel
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            tree: self.tree().clone(),
            active_panel: self.active_panel,
        }
    }

    pub fn restore(snapshot: WorkspaceSnapshot) -> Self {
        Self {
            workbench: Workbench::new(snapshot.tree),
            active_panel: snapshot.active_panel,
        }
    }

    /// Serialize Woodshed's allowed workspace presentation state for the non-browser host.
    ///
    /// This DTO intentionally records only the four known open lanes, their stable ids, split
    /// ratios, and active indices. It does not make the shared component responsible for
    /// Woodshed's session format or accept arbitrary host content on restore.
    pub fn to_snapshot_json(&self) -> Result<String, WorkspaceSnapshotError> {
        let dto = WorkspaceSnapshotDto::from_workspace(self)?;
        serde_json::to_string(&dto).map_err(WorkspaceSnapshotError::Encode)
    }

    /// Restore a Woodshed workspace from its durable host DTO.
    ///
    /// Unknown panels, altered ids or lanes, duplicate panels, malformed active indices, and
    /// non-canonical split fractions fail closed rather than creating a new product surface.
    pub fn from_snapshot_json(json: &str) -> Result<Self, WorkspaceSnapshotError> {
        let dto = serde_json::from_str::<WorkspaceSnapshotDto>(json)
            .map_err(WorkspaceSnapshotError::Decode)?;
        let mut panels = BTreeSet::new();
        let tree = dto.tree.into_tree(&mut panels, true)?;
        if let Some(panel) = dto.active_panel {
            if !panels.contains(&panel.tile_id().0) {
                return Err(WorkspaceSnapshotError::Invalid(format!(
                    "active panel {} is not present in the workspace",
                    panel.label()
                )));
            }
        } else if !panels.is_empty() {
            return Err(WorkspaceSnapshotError::Invalid(
                "a non-empty workspace must name its active panel".to_string(),
            ));
        }
        Ok(Self {
            workbench: Workbench::new(tree),
            active_panel: dto.active_panel,
        })
    }

    /// Apply a typed gesture, retaining actual tile custody until Woodshed accepts a tear-out.
    pub fn apply(&mut self, event: WorkspaceEvent) -> WorkspaceOutcome {
        let WorkspaceEvent::Tile(event) = event;
        if !workspace_event_is_canonical(&self.workbench, &event) {
            return WorkspaceOutcome::Unchanged;
        }
        // A split has several locally active stacks. The shared reducer correctly treats
        // re-selecting a visible tab as a tree no-op, but Woodshed still needs to route that
        // deliberate selection as the host's active surface.
        let activated_panel = match event {
            TileEvent::Activated(tile) => {
                self.tree().find(tile).and_then(WorkspacePanel::from_tile)
            }
            _ => None,
        };
        match self.workbench.apply(&event) {
            WorkbenchOutcome::Applied => match event {
                TileEvent::Activated(tile) => {
                    let panel = activated_panel
                        .or_else(|| self.tree().find(tile).and_then(WorkspacePanel::from_tile));
                    self.active_panel = panel;
                    panel.map_or(WorkspaceOutcome::Changed, WorkspaceOutcome::Activated)
                }
                TileEvent::Closed(tile) => {
                    if self
                        .active_panel
                        .is_some_and(|panel| panel.tile_id() == tile)
                    {
                        self.active_panel = first_active_panel(self.tree());
                    }
                    WorkspaceOutcome::Changed
                }
                TileEvent::Dragged { .. } | TileEvent::DividerMoved { .. } => {
                    WorkspaceOutcome::Changed
                }
            },
            WorkbenchOutcome::Unchanged => {
                activated_panel.map_or(WorkspaceOutcome::Unchanged, |panel| {
                    self.active_panel = Some(panel);
                    WorkspaceOutcome::Activated(panel)
                })
            }
            WorkbenchOutcome::Effect(WorkbenchEffect::TearOut { tile }) => self
                .tree()
                .find(tile)
                .and_then(WorkspacePanel::from_tile)
                .map_or(WorkspaceOutcome::Unchanged, |panel| {
                    WorkspaceOutcome::Effect(WorkspaceEffect::TearOut { panel })
                }),
        }
    }
}

impl WorkspaceSnapshotDto {
    fn from_workspace(workspace: &WoodshedWorkspace) -> Result<Self, WorkspaceSnapshotError> {
        let mut panels = BTreeSet::new();
        WorkspaceTreeDto::validate_tree(workspace.tree(), &mut panels, true)?;
        if let Some(panel) = workspace.active_panel {
            if !panels.contains(&panel.tile_id().0) {
                return Err(WorkspaceSnapshotError::Invalid(format!(
                    "active panel {} is not present in the workspace",
                    panel.label()
                )));
            }
        } else if !panels.is_empty() {
            return Err(WorkspaceSnapshotError::Invalid(
                "a non-empty workspace must name its active panel".to_string(),
            ));
        }
        Ok(Self {
            tree: WorkspaceTreeDto::from_tree(workspace.tree())?,
            active_panel: workspace.active_panel,
        })
    }
}

impl WorkspaceTreeDto {
    fn from_tree(tree: &TileTree) -> Result<Self, WorkspaceSnapshotError> {
        match tree {
            TileTree::Split { axis, children } => Ok(Self::Split {
                axis: match axis {
                    SplitAxis::Row => WorkspaceAxisDto::Row,
                    SplitAxis::Column => WorkspaceAxisDto::Column,
                },
                children: children
                    .iter()
                    .map(|branch| {
                        Ok(WorkspaceBranchDto {
                            fraction: branch.fraction,
                            tree: Self::from_tree(&branch.tree)?,
                        })
                    })
                    .collect::<Result<Vec<_>, WorkspaceSnapshotError>>()?,
            }),
            TileTree::Stack(stack) => Ok(Self::Stack {
                tabs: stack
                    .tabs
                    .iter()
                    .map(|tile| {
                        let panel = WorkspacePanel::from_tile(tile)
                            .expect("validated Woodshed workspace contains only known panels");
                        let ContentSource::Open { kind, id } = &tile.content else {
                            unreachable!("known Woodshed panels always use open lanes");
                        };
                        WorkspaceTileDto {
                            panel,
                            id: tile.id.0,
                            title: tile.title.clone(),
                            kind: kind.clone(),
                            content_id: id.clone(),
                        }
                    })
                    .collect(),
                active: stack.active,
            }),
        }
    }

    fn validate_tree(
        tree: &TileTree,
        panels: &mut BTreeSet<u64>,
        is_root: bool,
    ) -> Result<(), WorkspaceSnapshotError> {
        match tree {
            TileTree::Split { children, .. } => {
                if children.len() < 2 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "a split must have at least two children".to_string(),
                    ));
                }
                let mut fraction_sum = 0.0;
                for child in children {
                    if !child.fraction.is_finite() || child.fraction <= 0.0 {
                        return Err(WorkspaceSnapshotError::Invalid(
                            "split fractions must be finite and positive".to_string(),
                        ));
                    }
                    fraction_sum += child.fraction;
                    Self::validate_tree(&child.tree, panels, false)?;
                }
                if (fraction_sum - 1.0).abs() > 0.0001 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "split fractions must sum to one".to_string(),
                    ));
                }
            }
            TileTree::Stack(stack) => {
                if stack.tabs.is_empty() && !is_root {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "only the root stack may be empty".to_string(),
                    ));
                }
                if stack.active >= stack.tabs.len() && !stack.tabs.is_empty() {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "active tab index is outside its stack".to_string(),
                    ));
                }
                if stack.tabs.is_empty() && stack.active != 0 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "an empty stack must have active index zero".to_string(),
                    ));
                }
                for tile in &stack.tabs {
                    let Some(panel) = WorkspacePanel::from_tile(tile) else {
                        return Err(WorkspaceSnapshotError::Invalid(
                            "an unregistered panel appears in the workspace".to_string(),
                        ));
                    };
                    if !panels.insert(tile.id.0) {
                        return Err(WorkspaceSnapshotError::Invalid(
                            "a panel appears more than once".to_string(),
                        ));
                    }
                    if !panel.validates_tile(tile) {
                        return Err(WorkspaceSnapshotError::Invalid(format!(
                            "{} does not match Woodshed's registered lane",
                            panel.label()
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn into_tree(
        self,
        panels: &mut BTreeSet<u64>,
        is_root: bool,
    ) -> Result<TileTree, WorkspaceSnapshotError> {
        match self {
            Self::Split { axis, children } => {
                if children.len() < 2 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "a split must have at least two children".to_string(),
                    ));
                }
                let mut fraction_sum = 0.0;
                let mut branches = Vec::with_capacity(children.len());
                for child in children {
                    if !child.fraction.is_finite() || child.fraction <= 0.0 {
                        return Err(WorkspaceSnapshotError::Invalid(
                            "split fractions must be finite and positive".to_string(),
                        ));
                    }
                    fraction_sum += child.fraction;
                    branches.push(TileBranch::new(
                        child.fraction,
                        child.tree.into_tree(panels, false)?,
                    ));
                }
                if (fraction_sum - 1.0).abs() > 0.0001 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "split fractions must sum to one".to_string(),
                    ));
                }
                Ok(TileTree::split(
                    match axis {
                        WorkspaceAxisDto::Row => SplitAxis::Row,
                        WorkspaceAxisDto::Column => SplitAxis::Column,
                    },
                    branches,
                ))
            }
            Self::Stack { tabs, active } => {
                if tabs.is_empty() && !is_root {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "only the root stack may be empty".to_string(),
                    ));
                }
                if active >= tabs.len() && !tabs.is_empty() {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "active tab index is outside its stack".to_string(),
                    ));
                }
                if tabs.is_empty() && active != 0 {
                    return Err(WorkspaceSnapshotError::Invalid(
                        "an empty stack must have active index zero".to_string(),
                    ));
                }
                let mut restored = Vec::with_capacity(tabs.len());
                for tile in tabs {
                    let candidate = Tile {
                        id: TileId(tile.id),
                        title: tile.title,
                        content: ContentSource::Open {
                            kind: tile.kind,
                            id: tile.content_id,
                        },
                        accent: None,
                    };
                    if !tile.panel.validates_tile(&candidate) {
                        return Err(WorkspaceSnapshotError::Invalid(format!(
                            "{} does not match Woodshed's registered lane",
                            tile.panel.label()
                        )));
                    }
                    if !panels.insert(candidate.id.0) {
                        return Err(WorkspaceSnapshotError::Invalid(
                            "a panel appears more than once".to_string(),
                        ));
                    }
                    restored.push(candidate);
                }
                Ok(TileTree::Stack(TabStack {
                    tabs: restored,
                    active,
                }))
            }
        }
    }
}

fn first_active_panel(tree: &TileTree) -> Option<WorkspacePanel> {
    match tree {
        TileTree::Stack(stack) => stack
            .tabs
            .get(stack.active)
            .and_then(WorkspacePanel::from_tile),
        TileTree::Split { children, .. } => children
            .iter()
            .find_map(|branch| first_active_panel(&branch.tree)),
    }
}

/// Reject divider requests that the shared reducer permits structurally but that cannot enter
/// Woodshed's durable presentation format. The component retains generic mechanics; this host
/// owns the canonical ratio policy required to save and restore its workspace.
fn workspace_event_is_canonical(workbench: &Workbench, event: &TileEvent) -> bool {
    let TileEvent::DividerMoved { split, fractions } = event else {
        return true;
    };
    workbench
        .tree()
        .fractions_at(split)
        .is_some_and(|existing| {
            existing.len() == fractions.len()
                && fractions
                    .iter()
                    .all(|fraction| fraction.is_finite() && *fraction > 0.0)
                && (fractions.iter().sum::<f32>() - 1.0).abs() <= 0.0001
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use workbench::{DropTarget, Edge, TilePath};

    fn split_workspace(workspace: &mut WoodshedWorkspace) {
        assert_eq!(
            workspace.apply(WorkspaceEvent::Tile(TileEvent::Dragged {
                tile: WorkspacePanel::Settings.tile_id(),
                to: DropTarget::Edge {
                    tile: WorkspacePanel::Practice.tile_id(),
                    edge: Edge::Right,
                },
            })),
            WorkspaceOutcome::Changed
        );
    }

    #[test]
    fn panels_use_stable_ids_and_open_content_lanes() {
        let workspace = WoodshedWorkspace::new();
        assert_eq!(
            workspace
                .tree()
                .tiles()
                .iter()
                .map(|tile| tile.id)
                .collect::<Vec<_>>(),
            vec![
                WorkspacePanel::Practice.tile_id(),
                WorkspacePanel::Set.tile_id(),
                WorkspacePanel::Related.tile_id(),
                WorkspacePanel::Settings.tile_id(),
            ]
        );
        for tile in workspace.tree().tiles() {
            let ContentSource::Open { kind, .. } = &tile.content else {
                panic!("{} must use an open Woodshed lane", tile.title);
            };
            assert!(kind.starts_with("woodshed."));
        }
    }

    #[test]
    fn activation_selects_the_existing_set_surface() {
        let mut workspace = WoodshedWorkspace::new();
        assert_eq!(workspace.active_panel(), Some(WorkspacePanel::Practice));
        assert_eq!(
            workspace.apply(WorkspaceEvent::activate(WorkspacePanel::Set)),
            WorkspaceOutcome::Activated(WorkspacePanel::Set)
        );
        assert_eq!(workspace.active_panel(), Some(WorkspacePanel::Set));
    }

    #[test]
    fn split_activation_retains_the_panel_routed_by_the_host() {
        let mut workspace = WoodshedWorkspace::new();
        split_workspace(&mut workspace);
        assert_eq!(workspace.active_panel(), Some(WorkspacePanel::Practice));

        assert_eq!(
            workspace.apply(WorkspaceEvent::activate(WorkspacePanel::Settings)),
            WorkspaceOutcome::Activated(WorkspacePanel::Settings)
        );
        assert_eq!(workspace.active_panel(), Some(WorkspacePanel::Settings));
    }

    #[test]
    fn outside_drop_requests_a_typed_tear_out_without_losing_the_panel() {
        let mut workspace = WoodshedWorkspace::new();
        let before = workspace.snapshot();
        assert_eq!(
            workspace.apply(WorkspaceEvent::Tile(TileEvent::Dragged {
                tile: WorkspacePanel::Related.tile_id(),
                to: DropTarget::Outside,
            })),
            WorkspaceOutcome::Effect(WorkspaceEffect::TearOut {
                panel: WorkspacePanel::Related,
            })
        );
        assert_eq!(
            workspace.snapshot(),
            before,
            "Woodshed retains content custody"
        );
    }

    #[test]
    fn host_snapshot_restore_is_deterministic() {
        let mut workspace = WoodshedWorkspace::new();
        workspace.apply(WorkspaceEvent::activate(WorkspacePanel::Settings));
        let snapshot = workspace.snapshot();
        let restored = WoodshedWorkspace::restore(snapshot.clone());
        assert_eq!(restored.snapshot(), snapshot);
        assert_eq!(restored.active_panel(), Some(WorkspacePanel::Settings));
    }

    #[test]
    fn host_snapshot_json_round_trips_the_tree_and_active_panel() {
        let mut workspace = WoodshedWorkspace::new();
        split_workspace(&mut workspace);
        workspace.apply(WorkspaceEvent::activate(WorkspacePanel::Settings));
        let json = workspace.to_snapshot_json().unwrap();
        assert!(json
            .as_bytes()
            .windows(b"woodshed.settings".len())
            .any(|window| { window == b"woodshed.settings" }));
        assert!(json.contains("\"axis\":\"row\""));
        assert!(json.contains("\"fraction\":0.5"));

        let restored = WoodshedWorkspace::from_snapshot_json(&json).unwrap();
        assert_eq!(restored.snapshot(), workspace.snapshot());
        assert_eq!(restored.active_panel(), Some(WorkspacePanel::Settings));
        assert_eq!(restored.to_snapshot_json().unwrap(), json);
    }

    #[test]
    fn malformed_divider_never_enters_or_restores_from_the_workspace_snapshot() {
        let mut workspace = WoodshedWorkspace::new();
        split_workspace(&mut workspace);
        let before = workspace.snapshot();
        assert_eq!(
            workspace.apply(WorkspaceEvent::Tile(TileEvent::DividerMoved {
                split: TilePath(vec![]),
                fractions: vec![0.7, 0.4],
            })),
            WorkspaceOutcome::Unchanged
        );
        assert_eq!(workspace.snapshot(), before);

        let mut malformed: serde_json::Value =
            serde_json::from_str(&workspace.to_snapshot_json().unwrap()).unwrap();
        malformed["tree"]["split"]["children"][0]["fraction"] = serde_json::json!(0.7);
        malformed["tree"]["split"]["children"][1]["fraction"] = serde_json::json!(0.4);
        assert!(matches!(
            WoodshedWorkspace::from_snapshot_json(&serde_json::to_string(&malformed).unwrap()),
            Err(WorkspaceSnapshotError::Invalid(_))
        ));
    }

    #[test]
    fn host_snapshot_json_rejects_an_unknown_or_altered_panel() {
        let json = WoodshedWorkspace::new().to_snapshot_json().unwrap();
        let unknown = json.replacen("woodshed.practice", "woodshed.unknown", 1);
        assert!(matches!(
            WoodshedWorkspace::from_snapshot_json(&unknown),
            Err(WorkspaceSnapshotError::Invalid(_))
        ));

        let malformed = json.replacen("\"practice\"", "\"unknown\"", 1);
        assert!(matches!(
            WoodshedWorkspace::from_snapshot_json(&malformed),
            Err(WorkspaceSnapshotError::Decode(_))
        ));
    }
}
