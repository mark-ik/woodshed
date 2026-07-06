//! Corpus search (S4 slice 11): one query over every named catalog —
//! scales, chords, arpeggios, progressions, exercises, practice recipes,
//! tunings. A [`SearchHit`] names the target; the view layer applies it
//! (jump to the lens/tab, fill the set, swap the tuning).

use woodshedding::chord::catalog as chord_catalog;
use woodshedding::exercise::catalog as exercise_catalog;
use woodshedding::practice::catalog as practice_catalog;
use woodshedding::progression::catalog as progression_catalog;
use woodshedding::scale::catalog as scale_catalog;
use woodshedding::tuning::catalog as tuning_catalog;

/// Where a search result points. The index is into the matching catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchHit {
    Scale(usize),
    Chord(usize),
    Arpeggio(usize),
    Progression(usize),
    Exercise(usize),
    Recipe(usize),
    Tuning(usize),
}

/// One ranked search result: the display label, a short kind tag, and the
/// target to jump to.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub label: String,
    pub kind: &'static str,
    pub hit: SearchHit,
}

/// Rank a match: 0 = exact, 1 = prefix, 2 = word-start, 3 = substring.
/// Lower is better; `None` = no match. Case-insensitive.
fn score(name: &str, needle: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    if n == needle {
        Some(0)
    } else if n.starts_with(needle) {
        Some(1)
    } else if n.split_whitespace().any(|w| w.starts_with(needle)) {
        Some(2)
    } else if n.contains(needle) {
        Some(3)
    } else {
        None
    }
}

/// Search every catalog for `query`, returning up to `limit` results
/// ranked by match quality then kind order. Empty/blank query → empty.
pub fn search_corpus(query: &str, limit: usize) -> Vec<SearchResult> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    // (score, tie-break-order, result). tie-break keeps a stable,
    // catalog-grouped order among equal scores.
    let mut hits: Vec<(u8, usize, SearchResult)> = Vec::new();
    let mut push = |name: &str, kind: &'static str, hit: SearchHit, order: usize| {
        if let Some(s) = score(name, &needle) {
            hits.push((
                s,
                order,
                SearchResult {
                    label: name.to_string(),
                    kind,
                    hit,
                },
            ));
        }
    };

    for (i, f) in scale_catalog().iter().enumerate() {
        push(f.name, "scale", SearchHit::Scale(i), 0);
    }
    for (i, c) in chord_catalog().iter().enumerate() {
        push(c.name, "chord", SearchHit::Chord(i), 1);
        // The same catalog powers the arpeggio lens; offer it as a
        // second target so "minor 7" can land on either.
        push(c.name, "arpeggio", SearchHit::Arpeggio(i), 6);
    }
    for (i, p) in progression_catalog().iter().enumerate() {
        push(p.name, "progression", SearchHit::Progression(i), 2);
    }
    for (i, e) in exercise_catalog().iter().enumerate() {
        push(e.name, "exercise", SearchHit::Exercise(i), 3);
    }
    for (i, ps) in practice_catalog().iter().enumerate() {
        push(&ps.name, "recipe", SearchHit::Recipe(i), 4);
    }
    for (i, t) in tuning_catalog().iter().enumerate() {
        push(t.name, "tuning", SearchHit::Tuning(i), 5);
    }

    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    hits.truncate(limit);
    hits.into_iter().map(|(_, _, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_no_results() {
        assert!(search_corpus("", 10).is_empty());
        assert!(search_corpus("   ", 10).is_empty());
    }

    #[test]
    fn finds_across_catalogs_and_ranks() {
        let r = search_corpus("dorian", 10);
        assert!(r.iter().any(|x| x.label == "Dorian" && x.kind == "scale"));
        // Exact match ranks ahead of "Dorian #4" etc.
        assert_eq!(r[0].label.to_ascii_lowercase(), "dorian");
    }

    #[test]
    fn matches_recipe_and_tuning() {
        assert!(search_corpus("caged", 20)
            .iter()
            .any(|x| x.kind == "recipe"));
        assert!(search_corpus("standard", 20)
            .iter()
            .any(|x| x.kind == "tuning"));
    }

    #[test]
    fn respects_limit() {
        assert!(search_corpus("m", 5).len() <= 5);
    }

    #[test]
    fn chord_offers_both_chord_and_arpeggio() {
        let r = search_corpus("major", 40);
        assert!(r.iter().any(|x| x.kind == "chord"));
        assert!(r.iter().any(|x| x.kind == "arpeggio"));
    }
}
