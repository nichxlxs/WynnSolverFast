"""Provenance/integrity checks for freezing and reporting long campaigns."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

from analyze_long_campaign import expected_counts
from prepare_long_campaign import load_prior_campaign


class LongCampaignIntegrity(unittest.TestCase):
    def test_tracked_archive_wins_over_stale_unpacked_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            payload = b'{"runs": [{"score": 123}]}'
            archive = root / "screen_raw.tar.gz"
            with tarfile.open(archive, "w:gz") as packed:
                info = tarfile.TarInfo("screen/campaign.json")
                info.size = len(payload)
                packed.addfile(info, io.BytesIO(payload))
            (root / "screen").mkdir()
            (root / "screen/campaign.json").write_text('{"runs": [{"score": 999}]}')
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / "manifest.json").write_text(json.dumps({"files": [{"path": archive.name, "sha256": digest}]}))
            result = load_prior_campaign(root, "screen")
            self.assertEqual(result["runs"], [{"score": 123}])
            self.assertEqual(result["archive_sha256"], digest)
            self.assertEqual(result["sha256"], hashlib.sha256(payload).hexdigest())
            archive.write_bytes(archive.read_bytes() + b"changed")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                load_prior_campaign(root, "screen")

    def test_unpacked_fallback_explicitly_reports_missing_archive(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "screen").mkdir()
            (root / "screen/campaign.json").write_text('{"runs": []}')
            self.assertTrue(load_prior_campaign(root, "screen")["archive_unavailable"])

    def test_expected_counts_come_from_plan(self):
        counts = expected_counts({"baseline_repeats": 2, "wide_exact_repeats": 3,
                                  "heuristic_seeds": [4, 5, 6]})
        self.assertEqual(counts, {"current_exact15": 2, "wide_exact15": 3,
                                  "current_alns15": 3, "elite_alns15": 3})


if __name__ == "__main__":
    unittest.main()
