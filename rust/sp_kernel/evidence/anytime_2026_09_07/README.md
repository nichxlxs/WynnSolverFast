# Recorded quality experiments, 2026-09-07

Start with [the implementation/results report](../../../../ANYTIME_OPTIMIZATION_RESULTS.md).
All campaigns used the same exported raw-pool fixtures with unproved early
HP/EHP/raw-stat prechecks disabled and final restrictions retained. Native
processes ran sequentially on one thread. The reference behavior uses the
common corrected scorer; it is not an untouched-master benchmark.

| Campaign | Queries | Arms | Repeats | Seconds per run | Runs | Reference |
|---|---:|---:|---:|---:|---:|---|
| Screen | 132 | 6 | 1 | 2 | 792 | In-sample maximum |
| Primary confirmation | 22 | 4 | 3 | 5 | 264 | Frozen screening score |
| Selected diagnostics | 3 | 3 | 3 | 15 | 27 | Frozen screening score |
| Independent enumeration | 60 | 1 | 1 | 30 | 60 | 58 completed evaluator maxima; two timeouts |
| Budget/scheduler follow-up | 22 | 1 | 3 | 5 | 66 | Frozen screening score |

The 132 queries comprise 117 searches and 15 fully supplied controls. Their
definitions and provenance are in `../../quality_suite.json`; generation
commands are in [the benchmark guide](../../QUALITY_BENCHMARKING.md).

Each `*_report.md` and `*_summary.csv` is readable without unpacking. The
`wynn-quality-*-review` files provide exact per-seed targets, endpoints and
independent analysis. Initial healing failures and final passing records are
both retained. `environment.json` identifies toolchains, machine limits,
baseline/source revisions, old/new binary hashes and the final shipped WASM.
`fixture_index.json` records all 132 exact input hashes and export results.

Each `*_raw.tar.gz` contains the complete campaign directory: normalized
variant configuration and binary hashes, `campaign.json`, `runs.jsonl`,
native traces, parent-observed traces, stdout/stderr logs, summary and report.
The oracle archive also contains independently extracted references and the
screen comparison against them. Targets and plans were frozen in the adjacent
`confirmation_setup`, `diagnostic_setup` and `followup_setup` directories.
Absolute paths inside historical records identify their original execution
location; regeneration should use paths in the new checkout.

To inspect a raw campaign from this directory:

```sh
tar -xzf confirmation_raw.tar.gz
python3 ../../analyze_quality_campaign.py \
  --campaign confirmation/campaign.json --expected-scenarios confirmation \
  --references confirmation_setup/screening_references.json \
  --baselines safe_baseline15 --candidates alns1 alns_warm6 \
  --out /tmp/confirmation-cohorts.json
```

`manifest.json` records SHA-256 and size for every committed evidence file
other than itself. Archives use normalized entry timestamps and ownership.
They preserve raw observations rather than only successful runs. External
deadline terminations are expected censored observations; a build streamed
before the deadline remains valid evidence, while later discoveries do not
count.

The final profile reached all frozen T99 milestones in 66/66 runs. This is a
post-hoc combined configuration follow-up on fresh seeds, not an independent
claim of 99% of the global optimum. The earlier selected-case timing gains
belong to their original heuristic configurations and are not transferred to
the changed profile. None of these campaigns certifies game-global optimality
or exhaustive optimization over all manual skill-point allocations.

The two `*_source.patch.gz` files preserve the complete local source revisions
used for the timed kernels, relative to baseline `027b0e8490432e6c87fe6c3024aed57fafcfb724`.
Apply either patch to a clean checkout of that baseline to rebuild the respective
source. They preserve provenance if GitHub publication creates a new commit
with the same final tree instead of transferring local commit history.

## Raw archives

The raw campaign archives this README links (`*.tar.gz`, `*.json.gz`,
`*.patch.gz`) were left out when this branch was integrated, to keep about
10 MB of binary data out of the source tree. They are preserved, at the
same paths, on `agent/anytime-neighborhood-benchmarks` at commit `bf9b753`:
`git checkout bf9b753 -- <path>` restores one.
