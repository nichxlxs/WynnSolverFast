# Benchmarking the solver

Two tools. `bench.py` measures the solver itself; `gpu_bench` answers
whether a GPU offload would be worth building on your machine.

## Fixtures first

`fixtures/` is gitignored — everything in it is derived — so a clean checkout
has none. Export the base ones with the `SOLVER_EXPORT_RUST` /
`SOLVER_EXPORT_SCORE` command in [README.md](README.md), then:

```bash
node js/solver/tests/gen_bench_fixtures.js
```

That adds six scenario shapes the exported corpus does not contain, most
derived from `score_spell2.json` and pairing with `enum_spell2.txt` (the two
hp_casting ones name their own base and pairing):

| Fixture | Shape | What it measures |
|---|---|---|
| `score_slider.json` | a buff state declaring a slider | the dynamic-row path (A6/B9) |
| `score_blend_pos.json` | weights all non-negative | the two-sided ceiling must be a no-op here |
| `score_blend_neg.json` | one negative weight | B2 |
| `score_blend_mixed.json` | both signs | the shape the gate was unsound on |
| `score_hpcast.json` | HP-cost casting, no declared slider | the ceiling gate on `hp_casting` (B4); pairs with `enum_spell_wide.txt` |
| `score_hpcast_2m.json` | the same, on a space that completes | for exhaustive on/off comparison |

They are **benchmark-only** — their `cases[]` is empty on purpose, because the
scenario is a synthetic edit and the exported per-case expectations no longer
describe it. `score_kernel` will tell you so rather than validate them.
Correctness for these shapes is covered against the JS elsewhere:
`mana_sim_check`, `SCORE_DENSE_CHECK=1` (dense against Obj on every trial) and
`SCORE_DENSE=0` (the whole Obj path).

Reproducing B9's 16.7x, for example:

```bash
./target/release/enum_kernel fixtures/enum_spell2.txt 1 fixtures/score_slider.json
```

## bench.py — scenario × configuration matrix

```bash
cargo build --release
./bench.py                                   # default scenarios, 20s each
./bench.py --time 60 --repeat 5              # longer runs, median of 5
./bench.py --threads 1,2,4,8,16              # CPU scaling sweep
./bench.py --layers                          # ablate each optimization
./bench.py --scenarios spell_wide ehp xpb
./bench.py --json results.json               # machine-readable output
```

Columns:

| Column | Meaning |
|---|---|
| rate | leaves covered per second (higher is better) |
| speedup | versus the first row of that scenario |
| checked | leaves covered within the time cap |
| gpu-offloadable | share of CPU time in batch-shaped ceiling work — the *only* part a GPU could take |
| ±spread | half-range across repeats; treat differences smaller than this as noise |

**Correctness is checked, not assumed.** Every configuration of a scenario
must produce the same top-15 *score set*; a difference is reported as FAIL
and exits non-zero. Item lists may differ only where scores tie (documented
insertion-order behavior) — that prints as a `~` note, not a failure. Only
configurations that covered the same number of leaves are compared, since
time-capped runs otherwise explore different amounts of space.

**Thread counts**: omit `--threads` to use every core. Oversubscribing past
the physical core count measurably *hurts* (~19% on a 4-core box).

## gpu_bench — is a GPU offload worth building?

```bash
cargo build --release --features gpu
./target/release/gpu_probe                     # what hardware is here?
./target/release/gpu_bench --share 0.29        # go/no-go projection
```

`gpu_probe` reports the tier: **exact-f64** (discrete GPU with fp64
shaders — bit-exact offload eligible), **prescreen-f32** (typical
integrated graphics — sound only with a widened prune margin plus exact CPU
re-check), or **cpu-only**. Both tools exit cleanly on machines with no GPU.

`gpu_bench` runs the same dependent multiply-add chain on GPU and CPU,
reports the ratio at several batch sizes (including upload/readback — the
real cost), and projects end-to-end speedup:

```
end_to_end = 1 / ((1 - share) + share / gpu_ratio)
```

Pass the `--share` value that `bench.py` printed as **gpu-offloadable** for
the scenario you care about. The tool also prints the Amdahl ceiling
(`1/(1-share)`) — the speedup an *infinitely fast* GPU would give.

### What the harness found immediately

Ablation with `--repeat 5` showed the two coarse bound layers were
scenario-dependent and no fixed default was right for both:

| Layer disabled | spell_wide | xpb |
|---|---:|---:|
| super-cluster bound | **1.04x faster** (layer was costing) | 1.04x (noise) |
| tail bound | **1.06x faster** (layer was costing) | **0.91x** (layer was paying) |
| cluster bound | 0.49x | 0.28x (always pays) |
| dense vectors | 0.02x | 0.00x (always pays) |

So both layers now **self-tune at runtime** (`AdaptiveBound`): each measures
pruned-leaves-per-eval, switches itself off when it stops earning its cost,
and re-samples later since a tightening cutoff can make it profitable
mid-run. This is a speed-only heuristic — it changes how much work is
skipped, never what a surviving leaf scores.

Result: self-tuning beats every fixed configuration —
spell_wide 1.31M/s → **1.56M/s** (+19%, and +12% over the best fixed
ablation), xpb 29.5M/s → **30.2M/s** while keeping the 10% the tail bound
earns there.

### What the measurements said here (4-core Xeon, no GPU)

| Scenario | gpu-offloadable | Amdahl ceiling |
|---|---:|---:|
| spell_wide (damage) | ~29% | 1.41x |
| xpb (combat XP) | ~1.4% | 1.01x |
| ehp (effective HP) | ~1.3% | 1.01x |

The arithmetic a GPU is good at is a minority of the work: enumeration
(a branchy tree walk) dominates spell searches, and ehp is dominated by the
mana simulation. So a *direct* offload of today's workload is capped
around 1.4x on damage goals and is worthless on the others.

The interesting angle the numbers suggest instead: if bound evaluations
were nearly free, finer-grained bounds become affordable (per-item instead
of per-4-item clusters), which prunes far more enumeration — that changes
the operating point rather than just shaving the 29%. That hypothesis is
testable today without any GPU: run `BOUND_CLUSTER=1` and compare pruning
against the eval-cost increase.
