# Optional GPU runner — staged plan

Goal: an OPTIONAL local acceleration path that uses the machine's GPU on top
of full CPU-thread utilization. Nothing requires a GPU: detection picks a
tier automatically and the CPU path remains the reference.

## Detection (DONE — `cargo run --release --features gpu --bin gpu_probe`)

wgpu adapter enumeration over all backends (Vulkan/DX12/Metal/GL), ranking
real GPUs only (software rasterizers and virtual devices never qualify):

| Hardware | Expected tier |
|---|---|
| Discrete GPU with fp64 shaders (e.g. RTX 3060 Ti via Vulkan) | **exact-f64** |
| Integrated graphics (typically no `SHADER_F64`) | **prescreen-f32** |
| No adapter / llvmpipe | **cpu-only** (unchanged behavior) |

## Why two GPU tiers can both stay EXACT

The bit-exactness contract only constrains what gets *pruned* and how
survivors are *scored*:

- **exact-f64**: IEEE fp64 add/mul on GPUs are bit-identical to the CPU
  when the evaluation order is preserved and FMA contraction is disabled.
  A ceiling kernel that replays the dense evaluation order exactly can
  gate/prune with the same margin contract as the CPU path.
- **prescreen-f32**: compute batch ceilings in f32, but prune only when
  `ceiling_f32 * (1 + EPS) < cutoff`, with EPS a conservative bound on
  worst-case relative f32 accumulation error for the scenario's circuit
  (path length is known at compile time; EPS ~1e-4 covers thousands of
  ops with headroom). Everything the prescreen cannot prove dead is
  re-checked exactly on the CPU. Wrong-prune impossible; results stay
  bit-exact on ANY GPU — including WebGPU, which has no f64 at all.

## Staged integration (follow-up work)

1. **Batch extraction**: the last-slot cluster loop already produces
   (prefix, delta-set) ceiling queries; buffer them per node/band into
   batches of ~64K instead of evaluating inline.
2. **Scenario → kernel compilation**: the dense evaluation is a fixed
   arithmetic circuit per scenario (DenseCtx indices + plans + row cache
   structure). Lower it once to a WGSL/SPIR-V compute shader; per-query
   inputs are the leaf vector deltas (~few hundred f64/f32 each).
3. **Async pipeline**: CPU threads keep enumerating and doing SP/greedy/
   mana work while GPU batches are in flight; batch results arrive as
   prune bitmaps. Stale cutoffs are safe (a lower cutoff only prunes
   less).
4. **Fallback parity harness**: every GPU-pruned cluster re-checked on CPU
   under a `GPU_CHECK=1` tripwire, mirroring SCORE_DENSE_CHECK.

Expected value: the ceiling/cluster evals are ~30-40% of CPU time on
damage objectives at 4 threads (more at higher thread counts, since bound
evals scale with coverage). A 3060 Ti at fp64 (~1/64 rate ≈ 0.25 TFLOP/s
fp64, but thousands of parallel lanes and small per-query state) should
comfortably absorb the entire bound workload, freeing CPU threads for
enumeration + survivors — plausibly 1.5-2.5x end-to-end on top of full
CPU threading, more on tight-bound objectives (xpb-style) where bounds
dominate. Not pursued server-side for now per project direction.
