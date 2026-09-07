# Time to a good build

The completed implementation and 1,209-run evidence are summarized in
[ANYTIME_OPTIMIZATION_RESULTS.md](../../ANYTIME_OPTIMIZATION_RESULTS.md).

This suite compares time to the **same score target**, not heuristic runtime
against exhaustive completion time. It preserves timeout, failed export, failed
process, and no-result observations. A target attained only in successful random
restarts is not reported as a universally attained speedup.

## Recovered workload coverage

`quality_suite.json` catalogs 132 queries:

* 105 queries recovered from `codex/current-family-benchmarks` at
  `58c0e3dc5f5ebc2419a636b54601e0247d4e26c6`: fifteen class/archetype profiles,
  each with the known build and one through six slots removed.
* 18 existing cancelstack, heavy-melee, tierstack, spellsteal, sustained-spell,
  and hybrid family queries at small/medium/large sizes.
* Nine existing Gaia, EHP, spell, healing, and XP-bonus queries.

The other inspected branches, `agent/add-family-benchmark-variations` at
`3a992a8e69b4fa45d4101828fa663d97d61ca850` and the performance trace branch at
`7080f4cd15f68cdac5891a3e43ccf78c0997da71`, contributed no missing snapshot
definitions beyond those already in the baseline. No old benchmark results or
solver implementations were transplanted.

Recovered snapshots retain source URLs, item lists, class/archetype labels,
ability-tree contracts, objective definitions, mana restrictions, deterministic
removal masks, and roll assumptions. Historical target scores and freshness
hashes were removed. The imported source profiles sometimes label an older
build standardized to current game data; that provenance remains visible.

Selectors:

| Selector | Queries |
|---|---:|
| `all` | 132 |
| `all_searches` | 117, excluding the fifteen fully supplied seeds |
| `meta` | 105 |
| `meta_small` | 45 with one through three slots removed |
| `meta_large` | 45 with four through six slots removed |
| `known_controls` | 15 fully supplied recovered seed builds |
| `families` | 18 |
| `legacy` | 9 |
| `wide_screen` | 57: remove-three/remove-six profiles plus family and legacy cases |
| `confirmation` | 22: all fifteen remove-six profiles, all six original large families, Gaia all-free |

Individual manifest names also work. These are saved-build improvement
workloads, with their original fixed weapon/tree and remaining locked items.
They are not evidence for unconstrained selection of every weapon/tree or for
cold-start optimization with no build information.

## Generate current, shared fixtures

From the repository root:

```bash
python3 rust/sp_kernel/export_quality_fixtures.py \
  --scenarios all --out /tmp/wynn-quality-fixtures \
  --dominance off --prechecks disabled --samples 1
```

The exporter executes this checkout's real JavaScript pool builder and Rust
fixture bridge. It validates recovered spell-row contracts, records snapshot and
fixture SHA-256 hashes, records generation time separately, and restores any
freshness fields written by the old test runner. Increase `--samples` for score
parity fixtures; one sample is only a smoke witness, not an evaluator oracle.
`--resume` retries missing or failed queries and reuses matching validated pairs.

The default comparison uses raw pools (`--dominance off`). Historical family
combination bands were calibrated after older pruning, so the export explicitly
records new observed counts without applying those old bands. Ordinary test
invocations still enforce their existing bands.

`--prechecks disabled` relaxes only the known unproved early rejection tests:
raw-stat minimum thresholds become `-1e300` while retaining the PC schema, and
the raw HP / 100-effective-SP EHP flags are disabled. Finalized-stat restrictions
in the scoring JSON are unchanged and still apply. This permits a fair common
search problem for every arm. It does not prove all remaining bounds, SP
allocation, Major-ID handling, or game mechanics exact.

Use explicit `--dominance legacy --prechecks production` in a **separate output
directory** only for a labelled current-production comparison. Do not mix such
fixtures into a same-query speedup or optimum reference.

## Run a campaign

Write a config containing absolute binary paths and optional environment/CLI
overrides. For example:

```json
{
  "variants": [
    {"name": "safe_baseline", "kind": "enum", "binary": "/tmp/enum_kernel", "env": {"RESULT_COUNT": "15", "RETAIN_WARM": "0"}},
    {"name": "retained_top1", "kind": "enum", "binary": "/tmp/enum_kernel", "env": {"RESULT_COUNT": "1", "RETAIN_WARM": "1"}},
    {"name": "lns_top1", "kind": "lns", "binary": "/tmp/anytime_kernel", "args": ["--repair-budget", "100000"]}
  ]
}
```

The baseline binary must contain trace-only instrumentation or the common
instrumented kernel with optimizations disabled. An old binary that only prints
results at completion cannot provide a fair time-to-discovery baseline. Warm
build discoveries count even when old production code would discard their
witnesses; this makes the algorithm comparison conservative about the UI gain
from publishing warm results.

```bash
python3 rust/sp_kernel/benchmark_quality.py \
  --config /tmp/quality-config.json --fixtures /tmp/wynn-quality-fixtures \
  --scenarios all --seconds 2 --repeat 1 --seeds 1 \
  --out /tmp/wynn-quality-screen

python3 rust/sp_kernel/benchmark_quality.py \
  --config /tmp/quality-config.json --fixtures /tmp/wynn-quality-fixtures \
  --scenarios all_searches --seconds 10 --repeat 3 --seeds 1,2,3 \
  --out /tmp/wynn-quality-repeats
```

Timed subprocesses run sequentially. The first arm rotates between queries and
the order reverses between repeats. Avoid compiling or running other CPU-heavy
work during measurements. Native single-thread results do not automatically
predict browser/WASM speed or thermal behaviour.

## Timing, reference, and censoring rules

The parent process starts an independent monotonic wall timer before spawning
each binary. The budget includes launch, parsing, scoring compilation, warm
search, and the main algorithm. The parent observes and records flushed JSONL
incumbent events every five milliseconds; those observation times determine
reported target times. Native timestamps remain in the trace for diagnostics.
Process startup, event delivery, polling uncertainty, and trace overhead are
therefore included. Tiny cases near this resolution need careful interpretation.

At the common wall deadline the parent kills a non-cooperative process. Valid
streamed witnesses already observed remain usable; results first observed after
the deadline do not count. Kernel internal time caps cannot silently give one
algorithm extra user-visible time. Fixture preparation is excluded from kernel
times and separately recorded; end-to-end UI latency would also need that stage.

Without `--references`, each query's reference is the maximum found by any arm
and repeat in this campaign, explicitly labelled **in-sample best-known**.
Reaching 99% of this score is not a certificate of 99% of the true optimum.
Completing an enumerator with the current greedy SP evaluator does not
automatically upgrade that label.

An optional reference JSON maps query names to objects containing `kind`
(`best_known` or `known_optimum`), `score`, `source`, `enum_sha256`, and
`score_sha256`. A known optimum needs independent evidence for exactly those
fixtures and the documented evaluator scope. The runner rejects mismatched
hashes and scores exceeding a claimed optimum. Multiplicative quality metrics
are undefined for non-positive reference scores and are left unreported.

Summary JSON includes T95/T99/T99.9, success denominators, final scores,
fixed-time score checkpoints, completion counts, and every censored observation.
An unconditional median treats a non-attainment as infinity and displays a
censored median where appropriate. Success-only medians are retained for
diagnostics but do not support the headline speedup. A numeric T99 ratio is
reported only if **all repeats of both arms attain the same target**. One-run
screening ratios are diagnostic, not repeat-validated performance claims.

`campaign.json`, append-only `runs.jsonl`, raw kernel traces, parent-observed
traces, stdout/stderr logs, `summary.json`, and `report.md` make each result
auditable. `--summarize-only --out <existing directory>` regenerates summaries,
optionally against a supplied independent reference. `--resume` requires the
same binaries, variant settings, fixture index, repeat count, seeds, and budget.

## Independent small-query references

The campaign's in-sample maximum can hide a shared failure to find a stronger
build. Run a separate enumeration control on the 45 remove-one/remove-two/
remove-three queries and the fifteen fully supplied seeds. Use the same raw-pool
fixtures with unproved early predicates disabled. The config writer turns off
warm search, SP feasibility pruning, subtree/cluster/tail score bounds, and the
leaf score ceiling gate:

```bash
python3 rust/sp_kernel/extract_quality_references.py config \
  --enum-bin /tmp/enum_kernel --out /tmp/quality-oracle-config.json

python3 rust/sp_kernel/benchmark_quality.py \
  --config /tmp/quality-oracle-config.json --fixtures /tmp/wynn-quality-fixtures \
  --scenarios meta_small known_controls --seconds 30 --repeat 1 --seeds 1 \
  --out /tmp/wynn-quality-oracle

python3 rust/sp_kernel/extract_quality_references.py extract \
  --campaign /tmp/wynn-quality-oracle/campaign.json \
  --out /tmp/wynn-quality-references.json

python3 rust/sp_kernel/benchmark_quality.py \
  --summarize-only --out /tmp/wynn-quality-repeats \
  --references /tmp/wynn-quality-references.json
```

The required settings are `RESULT_COUNT=1`, `RETAIN_WARM=0`, `WARM_K=0`,
`SP_BOUND_OFF=1`, `BOUND_DEPTH=0`, `BOUND_TAIL=0`, `BOUND_CLUSTER=0`, and
`SCORE_CEILING_GATE=0`. The binary must implement all these switches; an older
binary that ignores the leaf-gate switch is unsuitable. One selected result is
sufficient because score pruning is disabled in this control. Candidate-domain
construction, item exclusions, ring symmetry, and the final evaluator still
define the configured query.

Only a completed, successful run with a finite score observed within the wall
budget contributes a reference. The extractor verifies the required variant
settings, matching fixture hashes, valid traces, and agreement of any completed
repeats. Timeouts, no-result runs, malformed controls, and disagreements remain
in an adjacent audit JSON with explicit reasons. A timeout does not establish an
optimum. Queries without an eligible reference continue using a labelled
in-sample best-known score when the main campaign is summarized.

Extracted references deliberately use `kind: best_known` and the more precise
`evidence_kind: exhaustive_under_current_evaluator`. The provenance records
campaign and binary hashes, environment switches, completed run IDs and scores.
This is evidence for complete equipment enumeration under the current fixed
tree, skill-point-allocation and scoring semantics. It does not establish game-
global optimality, and it does not silently upgrade greedy SP allocation to
exhaustive SP optimization.

## Class/removal aggregates and confirmation

The read-only cohort analyzer reports class, family, and removal-count groups.
It includes explicit no-result and censored T99 denominators, paired endpoint
score ratios, common-attainment median T99 speedups, and an independently
labelled conservative lower bound when every baseline run misses the target
before its deadline but every candidate run reaches it. A score gain is never
labelled a runtime gain. Partial query blocks are omitted from paired aggregates
and listed; a partial campaign is explicitly provisional.
Completion counts are reported separately. A matched completion-time ratio
requires both enumeration arms to finish every repeat and to request the same
number of returned results. Top-one versus top-fifteen and heuristic versus
exhaustive completion times are not presented as like-for-like speedups.

```bash
python3 rust/sp_kernel/analyze_quality_campaign.py \
  --campaign /tmp/wynn-quality-screen/campaign.json \
  --expected-scenarios all --baselines safe_baseline15 best1_warm3 \
  --candidates alns1 --out /tmp/wynn-quality-screen/cohorts.json
```

`--references` accepts the independently extracted references. Without it,
references are the same labelled in-sample best-known scores as the main
summary. Class cohorts apply to the recovered profiles with explicit class
metadata; the original six-family cohorts remain separate. Both totals
including seed controls and search-only totals are emitted.

The fixed confirmation cohort consists of all fifteen recovered archetypes at
remove-six, the six original large family cases, and Gaia all-free. It is chosen
by workload definitions, not by which scenarios won the screening run. A
five-second, three-repeat confirmation uses fresh seeds `101,202,303` and the
same safe-baseline, small-warm-start-only, and ALNS configurations. An optional
fourth ALNS arm with `--warm-k 6` tests the prespecified tradeoff between stronger
initial coverage and less time for neighbourhood repairs; apply it to the whole
cohort, not only selected winners. Do not change configurations mid-campaign.

```bash
python3 rust/sp_kernel/benchmark_quality.py \
  --config /tmp/quality-confirm-config.json --fixtures /tmp/wynn-quality-fixtures \
  --scenarios confirmation --seconds 5 --repeat 3 --seeds 101,202,303 \
  --out /tmp/wynn-quality-confirm
```

This confirms performance on new random seeds for a fixed workload cohort.
It is not a held-out set of previously unseen query definitions. Screening
and confirmation targets may differ if the longer runs find better builds;
use a shared reference file when directly comparing their target times.

The preparation helper copies the baseline, small-warm-only, and ALNS settings
from the completed screen and adds only the prespecified `--warm-k 6` ALNS arm.
It verifies unchanged binary hashes and freezes each confirmation query's
screening maximum before confirmation starts. It refuses an incomplete screen
or a query with no finite screening target. `--config-only` can prepare the
configuration early, but does not freeze references.

```bash
python3 rust/sp_kernel/prepare_quality_confirmation.py \
  --screen /tmp/wynn-quality-screen/campaign.json \
  --out /tmp/wynn-quality-confirm-setup

python3 rust/sp_kernel/benchmark_quality.py \
  --config /tmp/wynn-quality-confirm-setup/confirmation_config.json \
  --fixtures /tmp/wynn-quality-fixtures --scenarios confirmation \
  --seconds 5 --repeat 3 --seeds 101,202,303 \
  --references /tmp/wynn-quality-confirm-setup/screening_references.json \
  --out /tmp/wynn-quality-confirm
```

A separate `--diagnostic-scenarios <query>` option prepares a clearly labelled
screen-selected follow-up with safe-baseline/ALNS-warm3/ALNS-warm6 arms, fresh
seeds `1001,2002,3003`, and a suggested fifteen-second budget. For example,
Battle Monk remove-four can investigate a screening observation of a large
censored T99 lower bound. This follow-up is selected because of screening
results; do not combine it into the prespecified cohort as unbiased evidence.
Its references are still frozen screening targets. Distinguish an observed
finite speedup from a lower bound caused by the baseline missing the deadline.

## Harness checks

```bash
python3 -m unittest discover -s rust/sp_kernel -p test_benchmark_quality.py -v
```

These checks cover warm-time charging, shared-target comparisons, timeout/no-
result denominators, post-budget exclusion, fixture/reference identity, claimed
optimum contradictions, non-positive objectives, complete branch coverage,
trace normalization, an externally killed dummy kernel, and preservation of the
fixture schema when disabling unproved early predicates.

## Final semantic checks

The measured toolchain was Rust 1.98.1, wasm-bindgen 0.2.127; `wasm-opt` was
unavailable. Build/test commands from `rust/sp_kernel`:

```sh
cargo build --locked --release --bins
cargo test --locked --release --lib
python3 -m unittest test_benchmark_quality
bash build-wasm.sh
node check_quality_wasm.mjs ../.. /tmp/wynn-quality-fixtures /tmp/wasm-parity.json
python3 test_wide_bound_keys.py --score-base /tmp/wynn-quality-fixtures/score_spell_wide.json
```

The WASM check exhausts all 45 recovered control/remove-one/remove-two queries
and compares native/WASM top-15 scores, allowing equivalent tie identities.
It is a Node-hosted WASM check, not browser end-to-end validation. The PR workflow
runs Rust unit tests, evidence-integrity tests and a WASM-target compilation;
it does not enforce byte-identical builds across different environments.

To reproduce the six-arm screen with the checked-in relative binary paths,
run from `rust/sp_kernel` after exporting the shared fixtures:

```sh
python3 benchmark_quality.py --config quality_variants.json \
  --fixtures /tmp/wynn-quality-fixtures --scenarios all \
  --seconds 2 --repeat 1 --seeds 1 --out /tmp/quality-screen
```

The preferred experimental heuristic profile uses all these explicit arguments:
`--warm-k 6 --warm-budget 2000000 --repair-budget 100000 --max-repairs 10000 --cycle-stagnation 1`.
Its post-hoc 22-query follow-up used seeds `707,808,909` and five seconds. Earlier
screen/confirmation/diagnostic arms used the original default repair limit and
scheduler, and their timing ratios should not be attributed to the changed profile.
