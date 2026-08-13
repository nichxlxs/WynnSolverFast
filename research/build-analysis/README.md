# Wynncraft build-family analysis

This directory turns the collected WynnBuilder links into reproducible build
statistics, empirical family summaries, roll-sensitivity tables, and proposed
optimizer guardrails.

## Reproduce

```bash
node research/build-analysis/analyze_builds.js
node research/build-analysis/id_roll_probability.js --write-examples
```

The analysis uses the exact stat assembly, ability-tree decoding, spell damage,
defense, and roll interpolation functions in WynnSolverFast. Its checked-in
item authority is data version `2.2.3.0`. The forum catalogue was last edited
on 2026-08-11, so exclusions and drift are reported rather than silently filled
with web values.

## Outputs

| File | Purpose |
|---|---|
| `build-stats.json` | Full evaluated build records, four roll profiles, and explicit failures |
| `build-stats.csv` | Flat whole-build attributes for notebooks, R, Python, or Excel |
| `family-summary.json` | Min, p10, p25, median, p75, p90, max, mean, sample SD, and MAD by family, class, archetype, and roll profile |
| `family-summary.csv` | Tidy form of the family summary |
| `threshold-candidates.json` | Median-roll p10, p50, and p75 guardrail candidates |
| `roll-sensitivity.json` | Per-build and per-family movement from median to upper-quartile proxy to maximum rolls |
| `roll-sensitivity.csv` | Tidy roll-sensitivity records |
| `id-roll-probability-examples.json` | Exact discrete displayed-value probability examples |
| `build-family-analysis.md` | Interpretation, limitations, and implementation recommendations |

## Roll profiles

| Profile | Solver interpolation | Intended use |
|---|---:|---|
| `median` | 50% for all groups | Primary acquisition-oriented baseline |
| `upper_quartile_proxy` | 75% for all groups | Sensitivity and better-copy comparison |
| `solver_default` | Damage 85, mana 100, healing 85, misc 85 | Current program behavior comparison |
| `maximum` | 100% for all groups | WynnBuilder template comparison only |

These are deterministic per-ID profiles. They are not whole-item or whole-build
percentiles. If eight independent positive IDs each need at least the 75%
profile point, the joint chance is about 1 in 51,854 before rerolls or
Amplifiers. Use the 75% profile to identify roll-sensitive breakpoints, not as
the default ownership assumption.

## Statistical policy

The 129 source links are curated examples, not an independent random sample.
The evaluated family strata are often small and multimodal. This project uses
empirical quantiles and median absolute deviation. It does not fit a normal
distribution or use Gaussian confidence intervals. A family threshold is:

- `provisional` at eight or more evaluated builds;
- `exploratory` at four to seven builds;
- `insufficient_sample` below four builds.

The p10 value is a permissive regression guardrail, the median is a typical
observed target, and p75 is a strong observed target. Damage thresholds remain
secondary to the exact ability state, combo, and content assumptions.
