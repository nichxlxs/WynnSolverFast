#!/usr/bin/env python3
"""Unit tests for anytime.py's metric math. Run: python3 test_anytime.py"""

import unittest

from anytime import metrics, primal_gap


class PrimalGap(unittest.TestCase):
    def test_no_incumbent_is_one(self):
        self.assertEqual(primal_gap(100.0, None), 1.0)

    def test_optimum_is_zero(self):
        self.assertEqual(primal_gap(100.0, 100.0), 0.0)

    def test_relative(self):
        self.assertAlmostEqual(primal_gap(100.0, 75.0), 0.25)

    def test_negative_scores_use_magnitude(self):
        self.assertAlmostEqual(primal_gap(-50.0, -100.0), 0.5)


class Metrics(unittest.TestCase):
    def test_piecewise_integral(self):
        # No incumbent for 1 s (gap 1), 50% for 2 s, optimum from t = 3.
        m = metrics([(1.0, 50.0), (3.0, 100.0)], 100.0, 10.0)
        self.assertAlmostEqual(m["primal_integral"], 1.0 + 0.5 * 2.0)
        self.assertEqual(m["t100"], 3.0)
        self.assertEqual(m["t99"], 3.0)
        self.assertEqual(m["final_gap"], 0.0)
        self.assertFalse(m["above_ref"])

    def test_never_reached(self):
        m = metrics([(2.0, 99.5)], 100.0, 4.0)
        self.assertAlmostEqual(m["primal_integral"], 2.0 + 0.005 * 2.0)
        self.assertEqual(m["t99"], 2.0)
        self.assertIsNone(m["t100"])
        self.assertAlmostEqual(m["final_gap"], 0.005)

    def test_events_after_horizon_are_clamped(self):
        m = metrics([(5.0, 100.0)], 100.0, 4.0)
        self.assertAlmostEqual(m["primal_integral"], 4.0)
        self.assertEqual(m["final_gap"], 0.0)

    def test_no_events(self):
        m = metrics([], 100.0, 3.0)
        self.assertAlmostEqual(m["primal_integral"], 3.0)
        self.assertIsNone(m["t99"])

    def test_above_reference_is_flagged(self):
        # A "best known" reference that a run beats must be reported.
        m = metrics([(1.0, 101.0)], 100.0, 2.0)
        self.assertTrue(m["above_ref"])


if __name__ == "__main__":
    unittest.main()
