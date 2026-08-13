# Benchmarking the solver

Two tools. `bench.py` measures the solver itself; `gpu_bench` answers
whether a GPU offload would be worth building on your machine.

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
