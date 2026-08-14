import unittest

import benchmark_family_ab as harness


def valid_run(scored=15):
    scores = [f"{100 - i}.0e0" for i in range(min(15, scored))]
    feasible = scored + 5
    return {
        "exit": 0,
        "checked": 2_000,
        "feasible": feasible,
        "scored": scored,
        "gated": 2,
        "mana_reject": 2,
        "thresh_reject": 1,
        "bound_pruned": 10,
        "rate": 2_000,
        "elapsed": 1.0,
        "wall": 1.1,
        "top15_count": len(scores),
        "top_scores": scores,
        "top_items": [f"items-{i}" for i in range(len(scores))],
    }


class BenchmarkEvidenceTests(unittest.TestCase):
    def test_cli_validation_reconciles_funnel_and_top_count(self):
        result = harness.validate_cli_run(valid_run(), "scenario", "current")
        self.assertEqual(result["checked"], 2_000)
        self.assertEqual(len(result["scores"]), 15)
        self.assertFalse(result["valid_empty_top"])

        missing = valid_run()
        del missing["top15_count"]
        with self.assertRaisesRegex(harness.EvidenceError, "top15_count"):
            harness.validate_cli_run(missing, "scenario", "current")

        malformed = valid_run()
        malformed["top_scores"].pop()
        malformed["top_items"].pop()
        with self.assertRaisesRegex(harness.EvidenceError, "declared"):
            harness.validate_cli_run(malformed, "scenario", "current")

    def test_empty_top_is_valid_only_when_explicit_and_reconciled(self):
        empty = valid_run(scored=0)
        result = harness.validate_cli_run(empty, "scenario", "current")
        self.assertEqual(result["scores"], ())
        self.assertTrue(result["valid_empty_top"])

        inconsistent = valid_run(scored=0)
        inconsistent["top15_count"] = 1
        with self.assertRaises(harness.EvidenceError):
            harness.validate_cli_run(inconsistent, "scenario", "current")

    def test_work_budget_and_pair_delta_fail_closed(self):
        accepted = {"checked": 2_001_000}
        work = harness.validate_work(
            accepted, "scenario", "current", 2_000_000, 10_000_000,
            10_000, 0.1,
        )
        self.assertEqual(work["overshoot"], 1_000)

        with self.assertRaisesRegex(harness.EvidenceError, "below required target"):
            harness.validate_work(
                {"checked": 1_999_999}, "scenario", "current",
                2_000_000, 10_000_000, 10_000, 0.1,
            )
        with self.assertRaisesRegex(harness.EvidenceError, "overshoot"):
            harness.validate_work(
                {"checked": 2_003_000}, "scenario", "current",
                2_000_000, 10_000_000, 10_000, 0.1,
            )

        scores = ("1.0e0",)
        pair = harness.validate_pair(
            "scenario",
            {"checked": 2_000_000, "credited_rate": 2_000_000.0, "scores": scores},
            {"checked": 2_001_000, "credited_rate": 1_000_500.0, "scores": scores},
            2_000_000, 10_000, 0.1,
        )
        self.assertAlmostEqual(pair["speedup"], 2_000_000 / 1_000_500)

    def test_config_order_is_deterministic_and_alternating(self):
        self.assertEqual(
            [harness.paired_config_order("current", i)[0] for i in range(4)],
            ["current", "prior_safe", "current", "prior_safe"],
        )
        self.assertEqual(
            [harness.paired_config_order("prior_safe", i)[0] for i in range(4)],
            ["prior_safe", "current", "prior_safe", "current"],
        )


if __name__ == "__main__":
    unittest.main()
