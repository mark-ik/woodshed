import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from audio_analysis_bench import ValidationError, score_documents, validate_document


def note(onset: float, offset: float, midi: float, confidence: float = 1.0):
    return {
        "onset_seconds": onset,
        "offset_seconds": offset,
        "midi": midi,
        "confidence": confidence,
    }


def document(notes, **extra):
    return {"schema_version": 1, "notes": notes, **extra}


class AudioAnalysisBenchTests(unittest.TestCase):
    def test_matching_is_one_to_one(self):
        reference = document([note(0.0, 0.5, 60)], catalog_targets=["chord:Major"])
        prediction = document(
            [note(0.0, 0.5, 60), note(0.01, 0.49, 60)],
            catalog_candidates=[{"id": "chord:Major", "score": 0.9}],
        )
        result = score_documents(reference, prediction)
        self.assertEqual(result["note_onset"]["matches"], 1)
        self.assertEqual(result["note_onset"]["precision"], 0.5)
        self.assertTrue(result["catalog_retrieval"]["hit_at_1"])

    def test_offset_metric_is_stricter_than_onset_metric(self):
        reference = document([note(0.0, 1.0, 64)])
        prediction = document([note(0.01, 1.5, 64)])
        result = score_documents(reference, prediction)
        self.assertEqual(result["note_onset"]["matches"], 1)
        self.assertEqual(result["note_onset_offset"]["matches"], 0)

    def test_pitch_tolerance_is_in_cents(self):
        reference = document([note(0.0, 0.5, 60.0)])
        prediction = document([note(0.0, 0.5, 60.5)])
        result = score_documents(reference, prediction)
        self.assertEqual(result["note_onset"]["matches"], 1)

    def test_invalid_confidence_is_rejected(self):
        with self.assertRaisesRegex(ValidationError, "confidence"):
            validate_document(document([note(0.0, 0.5, 60, confidence=1.1)]))


if __name__ == "__main__":
    unittest.main()
