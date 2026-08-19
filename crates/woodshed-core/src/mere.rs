//! Woodshed's mere: a joined, read-only projection of catalog structure and
//! personal practice lineage.
//!
//! Chartulary's `GraphLog` remains catalog authority and Stemma remains history
//! authority. A mere is the spatial read model where their shared catalog ids
//! become one graph. It can disclose the whole catalog or a bounded N-depth
//! neighborhood without creating another persistence format.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use woodshed_graph::{
    catalog_materials, related_neighbors, CatalogKind, MaterialRelation, RelationKind,
};

use crate::history::{PracticeHistory, PracticeTransition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MereNode {
    pub id: String,
    pub title: String,
    pub kind: CatalogKind,
    pub engagement_count: u64,
    pub practiced_ms: u64,
    pub last_seen_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WoodshedMereSnapshot {
    pub nodes: Vec<MereNode>,
    pub relations: Vec<MaterialRelation>,
}

impl WoodshedMereSnapshot {
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|node| node.id == id)
    }

    pub fn edges(&self) -> Vec<(usize, usize)> {
        self.relations
            .iter()
            .filter_map(|relation| {
                Some((
                    self.node_index(&relation.source)?,
                    self.node_index(&relation.target)?,
                ))
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MereScope<'a> {
    Whole,
    Selection { center: &'a str, depth: u8 },
}

/// Project the catalog and the current person's practice lineage into one mere.
pub fn woodshed_mere(history: &PracticeHistory, scope: MereScope<'_>) -> WoodshedMereSnapshot {
    let materials = catalog_materials();
    let by_id = materials
        .iter()
        .map(|material| (material.id.as_str(), material))
        .collect::<BTreeMap<_, _>>();
    let transitions = history.transitions();

    let ids = match scope {
        MereScope::Whole => materials
            .iter()
            .map(|material| material.id.clone())
            .collect::<Vec<_>>(),
        MereScope::Selection { center, depth } => {
            selection_ids(center, depth.min(12), &transitions)
        }
    };
    let included = ids.iter().cloned().collect::<BTreeSet<_>>();
    let nodes = ids
        .into_iter()
        .filter_map(|id| {
            let material = by_id.get(id.as_str())?;
            Some(MereNode {
                title: material.name.clone(),
                kind: material.kind,
                engagement_count: history.engagement_count(&id),
                practiced_ms: history.total_practiced_ms(&id),
                last_seen_ms: history.last_seen_ms(&id),
                id,
            })
        })
        .collect::<Vec<_>>();

    let mut relations = Vec::new();
    let mut seen = BTreeSet::new();
    for source in &nodes {
        for neighbor in related_neighbors(&source.id, usize::MAX) {
            if !included.contains(&neighbor.id) {
                continue;
            }
            for relation in neighbor.relations {
                let key = (
                    relation.source.clone(),
                    relation.target.clone(),
                    relation.kind,
                );
                if seen.insert(key) {
                    relations.push(relation);
                }
            }
        }
    }
    for transition in transitions {
        if !included.contains(&transition.from_id) || !included.contains(&transition.to_id) {
            continue;
        }
        let key = (
            transition.from_id.clone(),
            transition.to_id.clone(),
            RelationKind::PracticedAfter,
        );
        if seen.insert(key) {
            let times = transition.traversal_count;
            relations.push(MaterialRelation::evidence(
                transition.from_id,
                transition.to_id,
                RelationKind::PracticedAfter,
                (90 + times.min(10)) as u16,
                if times == 1 {
                    "Practiced next once".to_string()
                } else {
                    format!("Practiced next {times} times")
                },
            ));
        }
    }

    WoodshedMereSnapshot { nodes, relations }
}

fn selection_ids(center: &str, depth: u8, history: &[PracticeTransition]) -> Vec<String> {
    let mut found = BTreeSet::from([center.to_string()]);
    let mut order = vec![center.to_string()];
    let mut queue = VecDeque::from([(center.to_string(), 0_u8)]);
    while let Some((current, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        let mut neighbors = related_neighbors(&current, usize::MAX)
            .into_iter()
            .map(|neighbor| neighbor.id)
            .collect::<Vec<_>>();
        neighbors.extend(history.iter().filter_map(|transition| {
            if transition.from_id == current {
                Some(transition.to_id.clone())
            } else if transition.to_id == current {
                Some(transition.from_id.clone())
            } else {
                None
            }
        }));
        neighbors.sort();
        neighbors.dedup();
        for neighbor in neighbors {
            if found.insert(neighbor.clone()) {
                order.push(neighbor.clone());
                queue.push_back((neighbor, current_depth + 1));
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EngagementKind;
    use woodshed_graph::{chord_id, scale_id};

    #[test]
    fn selection_depth_expands_the_same_mere() {
        let history = PracticeHistory::default();
        let center = chord_id("Major 7");
        let one = woodshed_mere(
            &history,
            MereScope::Selection {
                center: &center,
                depth: 1,
            },
        );
        let two = woodshed_mere(
            &history,
            MereScope::Selection {
                center: &center,
                depth: 2,
            },
        );
        assert!(one.nodes.len() > 1);
        assert!(two.nodes.len() >= one.nodes.len());
        assert_eq!(one.nodes[0].id, center);
    }

    #[test]
    fn stemma_movements_join_catalog_relations_as_evidence() {
        let mut history = PracticeHistory::default();
        let major = scale_id("Major");
        let chord = chord_id("Major 7");
        history.record(
            Some(1),
            major.clone(),
            EngagementKind::Rehearsed,
            None,
            None,
        );
        history.record(
            Some(2),
            chord.clone(),
            EngagementKind::Rehearsed,
            None,
            None,
        );

        let mere = woodshed_mere(
            &history,
            MereScope::Selection {
                center: &major,
                depth: 1,
            },
        );
        assert!(mere.relations.iter().any(|relation| {
            relation.source == major
                && relation.target == chord
                && relation.kind == RelationKind::PracticedAfter
        }));
    }
}
