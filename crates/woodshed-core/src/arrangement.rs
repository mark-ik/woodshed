//! Deterministic graph arrangements shared by the full Woodshed mere and its
//! bounded swatches.
//!
//! This is product policy over a small graph-shaped input. Scenograph still
//! owns the scene contract and can later absorb an arrangement once another
//! product proves the same semantics. Keeping the input to node count, edges,
//! and focus makes that promotion mechanical rather than speculative.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Ten useful ways to arrange either a whole mere or a selected neighborhood.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphArrangement {
    Grid,
    #[default]
    Snake,
    Spiral,
    Circle,
    Radial,
    Tree,
    Layers,
    Arc,
    Timeline,
    Force,
}

impl GraphArrangement {
    pub const ALL: [Self; 10] = [
        Self::Grid,
        Self::Snake,
        Self::Spiral,
        Self::Circle,
        Self::Radial,
        Self::Tree,
        Self::Layers,
        Self::Arc,
        Self::Timeline,
        Self::Force,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::Snake => "Snake",
            Self::Spiral => "Spiral",
            Self::Circle => "Circle",
            Self::Radial => "Radial",
            Self::Tree => "Tree",
            Self::Layers => "Layers",
            Self::Arc => "Arc",
            Self::Timeline => "Timeline",
            Self::Force => "Force",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|item| *item == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// Arrange a graph into normalized `0..=1` coordinates.
pub fn arrange_graph(
    arrangement: GraphArrangement,
    count: usize,
    edges: &[(usize, usize)],
    focus: Option<usize>,
) -> Vec<(f32, f32)> {
    if count == 0 {
        return Vec::new();
    }
    let focus = focus.filter(|index| *index < count).unwrap_or(0);
    match arrangement {
        GraphArrangement::Grid => grid(count, false),
        GraphArrangement::Snake => grid(count, true),
        GraphArrangement::Spiral => spiral(count),
        GraphArrangement::Circle => circle(count),
        GraphArrangement::Radial => radial(count, edges, focus),
        GraphArrangement::Tree => tree(count, edges, focus, false),
        GraphArrangement::Layers => tree(count, edges, focus, true),
        GraphArrangement::Arc => arc(count),
        GraphArrangement::Timeline => timeline(count),
        GraphArrangement::Force => force(count, edges),
    }
}

fn grid(count: usize, snake: bool) -> Vec<(f32, f32)> {
    let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(columns).max(1);
    (0..count)
        .map(|index| {
            let row = index / columns;
            let raw = index % columns;
            let column = if snake && row % 2 == 1 {
                columns - 1 - raw
            } else {
                raw
            };
            (unit_slot(column, columns), unit_slot(row, rows))
        })
        .collect()
}

fn spiral(count: usize) -> Vec<(f32, f32)> {
    const GOLDEN_ANGLE: f32 = 2.399_963_1;
    (0..count)
        .map(|index| {
            let t = if count == 1 {
                0.0
            } else {
                index as f32 / (count - 1) as f32
            };
            let radius = 0.42 * t.sqrt();
            let angle = index as f32 * GOLDEN_ANGLE;
            (0.5 + angle.cos() * radius, 0.5 + angle.sin() * radius)
        })
        .collect()
}

fn circle(count: usize) -> Vec<(f32, f32)> {
    if count == 1 {
        return vec![(0.5, 0.5)];
    }
    (0..count)
        .map(|index| {
            let angle =
                index as f32 / count as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            (0.5 + angle.cos() * 0.42, 0.5 + angle.sin() * 0.42)
        })
        .collect()
}

fn radial(count: usize, edges: &[(usize, usize)], focus: usize) -> Vec<(f32, f32)> {
    let depths = depths(count, edges, focus, false);
    let max_depth = depths.iter().copied().max().unwrap_or(0).max(1);
    let mut positions = vec![(0.5, 0.5); count];
    for depth in 1..=max_depth {
        let ring = depths
            .iter()
            .enumerate()
            .filter_map(|(index, item_depth)| (*item_depth == depth).then_some(index))
            .collect::<Vec<_>>();
        let radius = 0.42 * depth as f32 / max_depth as f32;
        for (rank, index) in ring.iter().enumerate() {
            let angle = rank as f32 / ring.len().max(1) as f32 * std::f32::consts::TAU
                - std::f32::consts::FRAC_PI_2;
            positions[*index] = (0.5 + angle.cos() * radius, 0.5 + angle.sin() * radius);
        }
    }
    positions
}

fn tree(count: usize, edges: &[(usize, usize)], focus: usize, directed: bool) -> Vec<(f32, f32)> {
    let depths = depths(count, edges, focus, directed);
    let max_depth = depths.iter().copied().max().unwrap_or(0);
    let mut positions = vec![(0.5, 0.5); count];
    for depth in 0..=max_depth {
        let layer = depths
            .iter()
            .enumerate()
            .filter_map(|(index, item_depth)| (*item_depth == depth).then_some(index))
            .collect::<Vec<_>>();
        for (rank, index) in layer.iter().enumerate() {
            positions[*index] = (
                unit_slot(rank, layer.len()),
                unit_slot(depth, max_depth + 1),
            );
        }
    }
    positions
}

fn arc(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|index| {
            let x = unit_slot(index, count);
            let centred = (x - 0.5) * 2.0;
            (x, 0.20 + centred * centred * 0.62)
        })
        .collect()
}

fn timeline(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|index| {
            let x = unit_slot(index, count);
            let lane = match index % 3 {
                0 => 0.32,
                1 => 0.50,
                _ => 0.68,
            };
            (x, lane)
        })
        .collect()
}

fn force(count: usize, edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    let mut positions = circle(count);
    for _ in 0..36 {
        let mut delta = vec![(0.0_f32, 0.0_f32); count];
        // A bounded repulsion window keeps whole-catalog layouts linear-ish
        // while still breaking local clumps deterministically.
        for a in 0..count {
            for b in (a + 1)..(a + 25).min(count) {
                let dx = positions[a].0 - positions[b].0;
                let dy = positions[a].1 - positions[b].1;
                let distance_sq = (dx * dx + dy * dy).max(0.0025);
                let push = 0.00055 / distance_sq;
                delta[a].0 += dx * push;
                delta[a].1 += dy * push;
                delta[b].0 -= dx * push;
                delta[b].1 -= dy * push;
            }
        }
        for &(a, b) in edges {
            if a >= count || b >= count || a == b {
                continue;
            }
            let dx = positions[b].0 - positions[a].0;
            let dy = positions[b].1 - positions[a].1;
            let pull = 0.018;
            delta[a].0 += dx * pull;
            delta[a].1 += dy * pull;
            delta[b].0 -= dx * pull;
            delta[b].1 -= dy * pull;
        }
        for (position, movement) in positions.iter_mut().zip(delta) {
            position.0 = (position.0 + movement.0).clamp(0.06, 0.94);
            position.1 = (position.1 + movement.1).clamp(0.06, 0.94);
        }
    }
    positions
}

fn depths(count: usize, edges: &[(usize, usize)], focus: usize, directed: bool) -> Vec<usize> {
    let mut depth = vec![usize::MAX; count];
    depth[focus] = 0;
    let mut queue = VecDeque::from([focus]);
    while let Some(node) = queue.pop_front() {
        for &(from, to) in edges {
            let next = if from == node {
                Some(to)
            } else if !directed && to == node {
                Some(from)
            } else {
                None
            };
            let Some(next) = next.filter(|next| *next < count) else {
                continue;
            };
            if depth[next] == usize::MAX {
                depth[next] = depth[node] + 1;
                queue.push_back(next);
            }
        }
    }
    let connected_max = depth
        .iter()
        .filter(|depth| **depth != usize::MAX)
        .copied()
        .max()
        .unwrap_or(0);
    let mut orphan_depth = connected_max + 1;
    for item in &mut depth {
        if *item == usize::MAX {
            *item = orphan_depth;
            orphan_depth += 1;
        }
    }
    depth
}

fn unit_slot(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.5
    } else {
        0.08 + index as f32 / (count - 1) as f32 * 0.84
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_names_ten_distinct_arrangements() {
        let labels = GraphArrangement::ALL
            .into_iter()
            .map(GraphArrangement::label)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(labels.len(), 10);
    }

    #[test]
    fn every_arrangement_handles_the_same_graph_shape() {
        let edges = [(0, 1), (0, 2), (2, 3), (2, 4)];
        for arrangement in GraphArrangement::ALL {
            let positions = arrange_graph(arrangement, 5, &edges, Some(0));
            assert_eq!(positions.len(), 5, "{}", arrangement.label());
            assert!(positions.iter().all(|(x, y)| {
                x.is_finite() && y.is_finite() && (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y)
            }));
        }
    }
}
