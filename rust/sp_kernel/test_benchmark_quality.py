#!/usr/bin/env python3
"""Evidence integrity regressions; no performance claims from these tests."""
import json
from pathlib import Path
import tempfile
import unittest

import benchmark_quality as bq
from quality_suite import load_manifest, select_scenarios
from export_quality_fixtures import disable_unproved_prechecks
from extract_quality_references import ORACLE_ENV, extract_references, oracle_config
from analyze_quality_campaign import analyze
from prepare_quality_confirmation import confirmation_config, freeze_screen_references


def run(variant, events, status="capped", budget=2.):
    return {"scenario": "q", "variant": variant, "enum_sha256": "enum", "score_sha256": "score",
            "status": status, "complete": status == "completed", "trajectory": events,
            "best_score": max([e["score"] for e in events if e["observed_seconds"] <= budget], default=None),
            "budget_seconds": budget}


def event(seconds, score, phase="enumerate"):
    return {"observed_seconds": seconds, "score": score, "phase": phase}


class QualityEvidenceTests(unittest.TestCase):
    def test_safe_fixture_relaxes_only_early_predicates(self):
        source = "PRECHECKS 1\nPC mr 20 3\nEHP 1 20000 0 .27\nEHPNA 1 12000 0 .27\nTHP 1 5000 1000\nNSLOTS 1\nITEM 0 10 20\n"
        changed = disable_unproved_prechecks(source)
        self.assertIn("PRECHECKS 1\nPC mr -1e300 3\n", changed)
        self.assertIn("EHP 0 0 0 0\n", changed)
        self.assertIn("EHPNA 0 0 0 0\n", changed)
        self.assertIn("THP 0 0 0\n", changed)
        self.assertTrue(changed.endswith("NSLOTS 1\nITEM 0 10 20\n"))

    def test_warm_time_is_charged_and_old_score_rediscovery_deduplicated(self):
        events = bq.canonical_trace([event(.15, 80, "warm"), event(.6, 100), event(.7, 90), event(.8, 100)])
        self.assertEqual(len(events), 2)
        self.assertEqual(bq.quality_time(events, 100, .95), .6)
        self.assertEqual(bq.quality_time(events, 80, 1.), .15)

    def test_target_uses_same_score_not_exhaustive_time(self):
        record = {"variants": [{"name": "base"}, {"name": "lns"}], "runs": [
            run("base", [event(.1, 100), event(1.9, 100)], "completed"),
            run("lns", [event(.2, 100)])]}
        row = bq.summarize(record)["rows"][0]
        self.assertEqual(row["comparisons"][0]["t99_speedup"], .5)
        self.assertEqual(row["reference"]["kind"], "best_known")

    def test_timeout_and_no_result_stay_in_denominator(self):
        record = {"variants": [{"name": "base"}, {"name": "lns"}], "runs": [
            run("base", [event(.1, 100)]), run("base", [event(.1, 100)]), run("base", [event(.1, 100)]),
            run("lns", [event(.01, 100)]), run("lns", [], "timeout"), run("lns", [], "timeout")]}
        row = bq.summarize(record)["rows"][0]
        lns = row["variants"][1]
        self.assertEqual(lns["targets"]["0.99"]["success_rate"], 1/3)
        self.assertIsNone(lns["targets"]["0.99"]["median_seconds"])
        self.assertIsNone(row["comparisons"][0]["t99_speedup"])
        self.assertEqual(lns["no_result"], 2)

    def test_post_budget_result_does_not_count(self):
        events = [event(.1, 50), event(2.1, 100)]
        self.assertIsNone(bq.quality_time(events, 100, .99, limit=2))
        self.assertEqual(bq.score_at(events, 2), 50)

    def test_bad_reference_is_rejected(self):
        rows = [run("base", [event(.1, 100)])]
        ref = {"q": {"kind": "known_optimum", "score": 100, "enum_sha256": "wrong", "score_sha256": "score", "source": "oracle"}}
        with self.assertRaisesRegex(ValueError, "does not match"):
            bq.reference_for("q", rows, ref)
        ref["q"]["enum_sha256"] = "enum"
        ref["q"]["score"] = 90
        with self.assertRaisesRegex(ValueError, "exceeds claimed optimum"):
            bq.reference_for("q", rows, ref)

    def test_nonpositive_objective_never_gets_percent_quality(self):
        self.assertIsNone(bq.quality_time([event(.1, -1)], -1, .99))
        self.assertIsNone(bq.quality_time([event(.1, 0)], 0, .99))

    def test_full_branch_coverage(self):
        manifest = load_manifest()
        self.assertEqual(len(select_scenarios(manifest, ["all"])), 132)
        self.assertEqual(len(select_scenarios(manifest, ["all_searches"])), 117)
        self.assertEqual(len(select_scenarios(manifest, ["meta"])), 105)
        self.assertEqual(len(select_scenarios(manifest, ["meta_small"])), 45)
        self.assertEqual(len(select_scenarios(manifest, ["wide_screen"])), 57)
        self.assertEqual(len(select_scenarios(manifest, ["meta_small", "known_controls"])), 60)
        self.assertEqual(len(select_scenarios(manifest, ["confirmation"])), 22)

    def test_cohort_analysis_retains_partial_queries_and_censored_counts(self):
        query = {"name": "q", "group": "meta", "variant": "remove_6", "class": "archer", "family": "hybrid"}
        missing = {**query, "name": "pending"}
        record = {"repeat": 1, "variants": [{"name": "safe_baseline15"}, {"name": "alns1"}],
                  "runs": [run("safe_baseline15", [event(.1, 50)], "timeout"), run("alns1", [event(.1, 100)])]}
        result = analyze(record, {"scenarios": [query, missing]}, expected=["q", "pending"])
        self.assertFalse(result["complete_for_expected_query_set"])
        self.assertEqual(result["pending_or_partial_queries"], ["pending"])
        row = next(x for x in result["aggregates"] if x["cohort"] == "meta_remove_6")
        self.assertEqual(row["baseline_t99_censored_runs"], 1)
        self.assertEqual(row["candidate_t99_attained_runs"], 1)
        self.assertIsNone(row["t99_median_speedup_common_attainment"])
        self.assertEqual(row["t99_censored_lower_bound_at_least_10x_queries"], 1)
        self.assertEqual(row["endpoint_geomean_ratio_paired_results"], 2)

    def test_confirmation_freezes_only_complete_screen_and_preserves_three_arms(self):
        variants = [{"name": "safe_baseline15", "kind": "enum"}, {"name": "best1_warm3", "kind": "enum"},
                    {"name": "alns1", "kind": "lns", "args": ["--warm-k", "3"]}]
        screen = {"repeat": 1, "variants": variants, "runs": [run("safe_baseline15", [event(.1, 95)]),
                  run("best1_warm3", [event(.1, 99)]), run("alns1", [event(.2, 100)])]}
        config = confirmation_config(screen)
        self.assertEqual(config["variants"][:3], variants)
        self.assertEqual(config["variants"][3]["args"], ["--warm-k", "6"])
        self.assertEqual(variants[2]["args"], ["--warm-k", "3"])
        with self.assertRaisesRegex(ValueError, "incomplete"):
            freeze_screen_references(screen, ["q", "unfinished"], ["q"], "test")
        references = freeze_screen_references(screen, ["q"], ["q"], "test")
        self.assertEqual(references["q"]["score"], 100)
        self.assertEqual(references["q"]["evidence_kind"], "frozen_screening_best_known")

    def test_reference_extractor_requires_completed_oracle_configuration(self):
        completed = run("oracle", [event(.1, 100)], "completed")
        record = {"variants": [{"name": "oracle", "kind": "enum", "env": dict(ORACLE_ENV)}],
                  "runs": [completed]}
        references, audit = extract_references(record, "test campaign")
        self.assertEqual(references["q"]["kind"], "best_known")
        self.assertEqual(references["q"]["evidence_kind"], "exhaustive_under_current_evaluator")
        self.assertEqual(audit["references_extracted"], 1)
        record["variants"][0]["env"].pop("SCORE_CEILING_GATE")
        references, audit = extract_references(record, "test campaign")
        self.assertFalse(references)
        self.assertIn("SCORE_CEILING_GATE", audit["scenarios"][0]["rejected_runs"][0]["reason"])

    def test_reference_extractor_retains_censoring_and_rejects_disagreement(self):
        record = {"variants": [{"name": "oracle", "kind": "enum", "env": dict(ORACLE_ENV)}],
                  "runs": [run("oracle", [event(.1, 100)], "timeout")]}
        references, audit = extract_references(record, "test campaign")
        self.assertFalse(references)
        self.assertEqual(audit["scenarios"][0]["considered_runs"], 1)
        record["runs"] = [run("oracle", [event(.1, 100)], "completed"), run("oracle", [event(.1, 110)], "completed")]
        references, audit = extract_references(record, "test campaign")
        self.assertFalse(references)
        self.assertIn("disagree", audit["scenarios"][0]["reason"])
        self.assertEqual(oracle_config("/tmp/enum")["variants"][0]["env"], ORACLE_ENV)

    def test_lns_trace_and_stdout_formats(self):
        self.assertEqual(bq.event_score({"elapsed_secs": .1, "best_score": 13}), 13)
        self.assertEqual(bq.event_score({"top": [{"score": 12}, {"score": 13}]}), 13)
        self.assertEqual(bq.parse_json_stdout('diagnostic\n{"best_score":13}\n')["best_score"], 13)

    def test_independent_wall_observer_retains_timeout_witness(self):
        # A dummy kernel emits an incumbent, then ignores its budget. The
        # parent must kill it while retaining the witness and charging startup.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "dummy.py"
            script.write_text("#!/usr/bin/env python3\nimport json,os,time\nf=open(os.environ['QUALITY_TRACE_PATH'],'w')\nf.write(json.dumps({'event':'incumbent','wall_seconds':0,'score':100})+'\\n'); f.flush()\ntime.sleep(5)\n")
            script.chmod(0o755)
            (root / "enum.txt").write_text("fixture")
            (root / "score.json").write_text("{}")
            fixture = {"enum_file": "enum.txt", "score_file": "score.json", "enum_sha256": "a", "score_sha256": "b"}
            variant = {"name": "dummy", "kind": "enum", "binary": str(script), "env": {}}
            row = bq.run_once({"name": "dummy", "group": "test"}, fixture, variant, root, root / "results", .2, 1, 0, 1)
            self.assertEqual(row["status"], "timeout")
            self.assertEqual(row["best_score"], 100)
            self.assertGreater(row["trajectory"][0]["observed_seconds"], 0)
            self.assertGreaterEqual(row["wall_seconds"], .2)
            self.assertLess(row["wall_seconds"], 1.)


if __name__ == "__main__":
    unittest.main()
