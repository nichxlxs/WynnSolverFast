"""Shared query selection for quality exports and timed campaigns."""
from pathlib import Path
import json

HERE = Path(__file__).resolve().parent
DEFAULT_MANIFEST = HERE / "quality_suite.json"


def select_scenarios(manifest, names):
    queries = manifest["scenarios"]
    selected = []
    for name in names:
        if name == "all":
            matches = queries
        elif name == "all_searches":
            matches = [q for q in queries if q.get("variant") != "known_good"]
        elif name == "known_controls":
            matches = [q for q in queries if q.get("variant") == "known_good"]
        elif name == "confirmation":
            matches = [q for q in queries if
                       (q["group"] == "meta" and q.get("variant") == "remove_6")
                       or (q["group"] == "families" and q.get("variant") == "large")
                       or q["name"] == "gaia_all_free"]
        elif name == "wide_screen":
            matches = [q for q in queries if q["group"] != "meta"
                       or q.get("variant") in ("remove_3", "remove_6")]
        elif name in ("meta_small", "meta_large"):
            variants = ("remove_1", "remove_2", "remove_3") if name == "meta_small" else ("remove_4", "remove_5", "remove_6")
            matches = [q for q in queries if q["group"] == "meta" and q.get("variant") in variants]
        elif name in ("meta", "families", "legacy"):
            matches = [q for q in queries if q["group"] == name]
        else:
            matches = [q for q in queries if q["name"] == name or q["snapshot"] == name]
        if not matches:
            raise ValueError(f"unknown or empty scenario selection: {name}")
        selected.extend(matches)
    return list({q["name"]: q for q in selected}.values())


def load_manifest(path=DEFAULT_MANIFEST):
    return json.loads(Path(path).read_text())
