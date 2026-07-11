#!/usr/bin/env python3
"""Score normalized audio-analysis predictions against known observations.

This smoke scorer is intentionally dependency-free. It gives model adapters a
stable local contract while mir_eval remains the reference for published AMT
metrics.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1


class ValidationError(ValueError):
    pass


def _finite_number(value: Any, field: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValidationError(f"{field} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValidationError(f"{field} must be finite")
    return result


def validate_document(document: Any, *, reference: bool = False) -> dict[str, Any]:
    if not isinstance(document, dict):
        raise ValidationError("document must be an object")
    if document.get("schema_version") != SCHEMA_VERSION:
        raise ValidationError(f"schema_version must be {SCHEMA_VERSION}")

    notes = document.get("notes", [])
    if not isinstance(notes, list):
        raise ValidationError("notes must be an array")
    for index, note in enumerate(notes):
        if not isinstance(note, dict):
            raise ValidationError(f"notes[{index}] must be an object")
        onset = _finite_number(note.get("onset_seconds"), f"notes[{index}].onset_seconds")
        offset = _finite_number(note.get("offset_seconds"), f"notes[{index}].offset_seconds")
        midi = _finite_number(note.get("midi"), f"notes[{index}].midi")
        confidence = _finite_number(note.get("confidence", 1.0), f"notes[{index}].confidence")
        if onset < 0.0:
            raise ValidationError(f"notes[{index}].onset_seconds must be non-negative")
        if offset <= onset:
            raise ValidationError(f"notes[{index}].offset_seconds must follow onset")
        if not 0.0 <= midi <= 127.0:
            raise ValidationError(f"notes[{index}].midi must be in 0..=127")
        if not 0.0 <= confidence <= 1.0:
            raise ValidationError(f"notes[{index}].confidence must be in 0..=1")

    candidates = document.get("catalog_candidates", [])
    if not isinstance(candidates, list):
        raise ValidationError("catalog_candidates must be an array")
    seen_candidates: set[str] = set()
    for index, candidate in enumerate(candidates):
        if not isinstance(candidate, dict):
            raise ValidationError(f"catalog_candidates[{index}] must be an object")
        candidate_id = candidate.get("id")
        if not isinstance(candidate_id, str) or not candidate_id:
            raise ValidationError(f"catalog_candidates[{index}].id must be a non-empty string")
        if candidate_id in seen_candidates:
            raise ValidationError(f"catalog candidate {candidate_id!r} is duplicated")
        seen_candidates.add(candidate_id)
        score = _finite_number(candidate.get("score"), f"catalog_candidates[{index}].score")
        if not 0.0 <= score <= 1.0:
            raise ValidationError(f"catalog_candidates[{index}].score must be in 0..=1")

    if reference:
        targets = document.get("catalog_targets", [])
        if not isinstance(targets, list) or any(not isinstance(item, str) or not item for item in targets):
            raise ValidationError("catalog_targets must be an array of non-empty strings")

    return document


def load_document(path: Path, *, reference: bool = False) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"cannot read {path}: {error}") from error
    return validate_document(document, reference=reference)


def _note_matches(
    expected: dict[str, Any],
    predicted: dict[str, Any],
    *,
    onset_tolerance: float,
    pitch_tolerance_cents: float,
    require_offset: bool,
    offset_ratio: float,
    offset_min_tolerance: float,
) -> bool:
    if abs(float(expected["onset_seconds"]) - float(predicted["onset_seconds"])) > onset_tolerance:
        return False
    if abs(float(expected["midi"]) - float(predicted["midi"])) * 100.0 > pitch_tolerance_cents:
        return False
    if not require_offset:
        return True
    duration = float(expected["offset_seconds"]) - float(expected["onset_seconds"])
    tolerance = max(offset_min_tolerance, duration * offset_ratio)
    return abs(float(expected["offset_seconds"]) - float(predicted["offset_seconds"])) <= tolerance


def _maximum_matching(
    expected: list[dict[str, Any]],
    predicted: list[dict[str, Any]],
    **match_options: Any,
) -> list[tuple[int, int]]:
    adjacency: list[list[int]] = []
    for note in expected:
        candidates = [
            index
            for index, prediction in enumerate(predicted)
            if _note_matches(note, prediction, **match_options)
        ]
        candidates.sort(
            key=lambda index: (
                abs(float(note["onset_seconds"]) - float(predicted[index]["onset_seconds"])),
                abs(float(note["midi"]) - float(predicted[index]["midi"])),
            )
        )
        adjacency.append(candidates)

    prediction_to_expected: dict[int, int] = {}

    def assign(expected_index: int, visited: set[int]) -> bool:
        for prediction_index in adjacency[expected_index]:
            if prediction_index in visited:
                continue
            visited.add(prediction_index)
            incumbent = prediction_to_expected.get(prediction_index)
            if incumbent is None or assign(incumbent, visited):
                prediction_to_expected[prediction_index] = expected_index
                return True
        return False

    for expected_index in range(len(expected)):
        assign(expected_index, set())

    return sorted((expected_index, prediction_index) for prediction_index, expected_index in prediction_to_expected.items())


def _f1_counts(matches: int, expected: int, predicted: int) -> dict[str, float | int]:
    precision = matches / predicted if predicted else (1.0 if expected == 0 else 0.0)
    recall = matches / expected if expected else (1.0 if predicted == 0 else 0.0)
    f1 = 2.0 * precision * recall / (precision + recall) if precision + recall else 0.0
    return {
        "matches": matches,
        "expected": expected,
        "predicted": predicted,
        "precision": precision,
        "recall": recall,
        "f1": f1,
    }


def _overlap(expected: dict[str, Any], predicted: dict[str, Any]) -> float:
    start = max(float(expected["onset_seconds"]), float(predicted["onset_seconds"]))
    end = min(float(expected["offset_seconds"]), float(predicted["offset_seconds"]))
    intersection = max(0.0, end - start)
    union_start = min(float(expected["onset_seconds"]), float(predicted["onset_seconds"]))
    union_end = max(float(expected["offset_seconds"]), float(predicted["offset_seconds"]))
    return intersection / (union_end - union_start)


def score_documents(
    reference: dict[str, Any],
    prediction: dict[str, Any],
    *,
    onset_tolerance: float = 0.05,
    pitch_tolerance_cents: float = 50.0,
    offset_ratio: float = 0.2,
    offset_min_tolerance: float = 0.05,
) -> dict[str, Any]:
    validate_document(reference, reference=True)
    validate_document(prediction)
    expected_notes = reference.get("notes", [])
    predicted_notes = prediction.get("notes", [])
    common = {
        "onset_tolerance": onset_tolerance,
        "pitch_tolerance_cents": pitch_tolerance_cents,
        "offset_ratio": offset_ratio,
        "offset_min_tolerance": offset_min_tolerance,
    }
    onset_pairs = _maximum_matching(
        expected_notes,
        predicted_notes,
        require_offset=False,
        **common,
    )
    offset_pairs = _maximum_matching(
        expected_notes,
        predicted_notes,
        require_offset=True,
        **common,
    )
    note_onset = _f1_counts(len(onset_pairs), len(expected_notes), len(predicted_notes))
    note_with_offset = _f1_counts(len(offset_pairs), len(expected_notes), len(predicted_notes))
    note_with_offset["average_overlap"] = (
        sum(_overlap(expected_notes[i], predicted_notes[j]) for i, j in offset_pairs) / len(offset_pairs)
        if offset_pairs
        else 0.0
    )

    target_ids = set(reference.get("catalog_targets", []))
    candidate_ids = [candidate["id"] for candidate in prediction.get("catalog_candidates", [])]
    catalog = {
        f"hit_at_{limit}": any(candidate in target_ids for candidate in candidate_ids[:limit])
        if target_ids
        else None
        for limit in (1, 3, 5)
    }

    return {
        "schema_version": SCHEMA_VERSION,
        "reference_source": reference.get("source_id"),
        "prediction_run": prediction.get("run", {}),
        "note_onset": note_onset,
        "note_onset_offset": note_with_offset,
        "catalog_retrieval": catalog,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("prediction", type=Path)
    parser.add_argument("--compact", action="store_true", help="emit one-line JSON")
    args = parser.parse_args()
    try:
        reference = load_document(args.reference, reference=True)
        prediction = load_document(args.prediction)
        result = score_documents(reference, prediction)
    except ValidationError as error:
        parser.error(str(error))
    print(json.dumps(result, indent=None if args.compact else 2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
