#!/usr/bin/env python3
"""Package complete native campaign evidence with deterministic archives.

Run only after all timed jobs have stopped. Raw directories remain available
locally, while compact archives and readable reports are committed to git.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import tarfile

from benchmark_quality import sha256
from quality_suite import HERE


COHORTS = {"long": ("long_setup", 48), "gaia": ("gaia_setup", 6),
           "new_seeds": ("new_seed_setup", 12)}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", type=Path, default=HERE / "evidence/quick_2026_09_08")
    args = ap.parse_args()
    root = args.root.resolve()
    records = []
    witness_count = 0
    for cohort, (setup, count) in COHORTS.items():
        folder = root / cohort
        campaign = json.loads((folder / "combined/campaign.json").read_text())
        analysis = json.loads((folder / "long_analysis.json").read_text())
        plan = json.loads((root / setup / "plan.json").read_text())
        references = json.loads((root / setup / "references.json").read_text())
        runs = campaign["runs"]
        if len(runs) != count or count != plan["planned_runs"] or not analysis["complete_matrix"]:
            raise ValueError(f"{cohort}: incomplete/failed campaign")
        if len({r["run_id"] for r in runs}) != count:
            raise ValueError(f"{cohort}: duplicate run IDs")
        for run in runs:
            if not run["has_result_within_budget"] or run.get("trace_errors"):
                raise ValueError(f"{run['run_id']}: missing result or invalid trajectory")
            ref = references[run["scenario"]]
            if any(run[k] != ref[k] for k in ("enum_sha256", "score_sha256")):
                raise ValueError(f"{run['run_id']}: wrong reference fixture")
            for event in run["trajectory"]:
                if event.get("event") != "incumbent":
                    continue
                if len(event.get("items", [])) != 8 or any(len(event.get(k, [])) != 5 for k in ("base_sp", "total_sp")):
                    raise ValueError(f"{run['run_id']}: incomplete retained witness")
                if abs(sum(event["base_sp"]) - event["assigned_sp"]) > 1e-8:
                    raise ValueError(f"{run['run_id']}: inconsistent assigned skill points")
                witness_count += 1
        manifest_file = folder / "archive_manifest.json"
        payloads = [p for p in sorted(folder.rglob("*")) if p.is_file() and p != manifest_file]
        members = [{"path": p.relative_to(root).as_posix(), "size_bytes": p.stat().st_size, "sha256": sha256(p)} for p in payloads]
        manifest_file.write_text(json.dumps({"schema_version": 1, "cohort": cohort, "runs": count,
            "scope": "Complete raw native logs, parent-observed trajectories and normalized campaign metadata; includes unsuccessful seeds and deadline terminations.",
            "files": members}, indent=2) + "\n")
        archive = root / f"{cohort}_raw.tar.gz"
        with archive.open("wb") as destination:
            with gzip.GzipFile(filename="", mode="wb", fileobj=destination, compresslevel=9, mtime=0) as zipped:
                with tarfile.open(fileobj=zipped, mode="w") as packed:
                    for p in sorted(folder.rglob("*")):
                        info = packed.gettarinfo(str(p), arcname=p.relative_to(root).as_posix())
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        info.mtime = 0
                        info.mode = 0o755 if p.is_dir() else 0o644
                        if p.is_file():
                            with p.open("rb") as content:
                                packed.addfile(info, content)
                        elif p.is_dir():
                            packed.addfile(info)
                        else:
                            raise ValueError(f"Unexpected non-regular evidence path: {p}")
        with tarfile.open(archive, "r:gz") as packed:
            for expected in members:
                content = packed.extractfile(expected["path"]).read()
                if len(content) != expected["size_bytes"] or hashlib.sha256(content).hexdigest() != expected["sha256"]:
                    raise ValueError(f"Archive failed verification: {expected['path']}")
        records.append({"cohort": cohort, "runs": count,
            "archive": archive.name, "archive_bytes": archive.stat().st_size,
            "archive_sha256": sha256(archive), "member_manifest": manifest_file.relative_to(root).as_posix(),
            "member_manifest_sha256": sha256(manifest_file), "payload_files": len(members),
            "payload_bytes": sum(m["size_bytes"] for m in members), "all_members_verified": True,
            "plan_sha256": sha256(root / setup / "plan.json"),
            "references_sha256": sha256(root / setup / "references.json"),
            "variants": campaign["variants"]})
        print(f"{archive.name}: {count} runs, {archive.stat().st_size} bytes, {len(members)} verified payload files")
    (root / "native_manifest.json").write_text(json.dumps({"schema_version": 1,
        "total_runs": sum(r["runs"] for r in records), "distinct_queries": 9,
        "all_runs_have_predeadline_result": True, "trace_errors": 0,
        "incumbent_witnesses_checked": witness_count,
        "witness_validation_scope": "Structural item/SP completeness and assigned-point conservation; the current evaluator supplies feasibility. This is not an independent game-accuracy certificate.",
        "compression": "gzip level9 mtime0; tar sorted paths, normalized owner/permissions/mtime",
        "archives": records}, indent=2) + "\n")


if __name__ == "__main__":
    main()
