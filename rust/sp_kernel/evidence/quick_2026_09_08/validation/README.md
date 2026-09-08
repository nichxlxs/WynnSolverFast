# Quick search validation evidence

The real Chromium job passed 60 assertions at published commit
`d9b1d383c599cf02c7bff9994fa2c1b3eebfc1bb` in
[Actions run 34288116399](https://github.com/nichxlxs/WynnSolverFast/actions/runs/34288116399).
It loaded the actual solver page, bridge and shipped WASM worker. The small
five-second case first published a worker result at 192 ms and stopped at
5,043 ms; the immediate restart stopped at 5,050 ms. These are lifecycle
observations on a CI runner, not a broad performance benchmark.

`browser-ci-d9b1d383/` contains the complete browser-job log, job statuses,
artifact metadata, and result measurements. The full 19,230,712-byte browser
artifact contains the Playwright trace and static-server log. Its SHA-256 was
verified after download:

```
ebfd404247a11e4f64200d1f0fda79209f86516c11bd816e29a6433aa8ed5a81
```

The full ZIP is preserved in the working checkpoint at
`/workspace/scratch/b17a901c79d8/wynn-quick-validation/browser-ci-d9b1d383/evidence.zip`.
It is deliberately omitted from git. GitHub's copy expires September 22, 2026;
its artifact URL and exact metadata are in `manifest.json`.

The final local Rust run passed 35 tests. The local JavaScript run passed 575
assertions, with four browser-file failures and two warnings. One failed file
was the new Quick browser test, which subsequently passed in real Chromium CI.
The three legacy browser guards remained unexecuted in that CI job; this is
not an all-browser-suite pass.

The WASM/native differential campaigns used the same current evaluator,
seed 707, top 15, and fixed work budgets: 132 scenarios at 2,000 evaluated-build
budget each, plus three elite-pool cases at 100,000 each. All 135 comparisons
passed; six of the 132 small-budget scenarios returned no result on both
backends. This establishes bounded backend parity, not feasibility of every
query or optimality of returned builds. The clock case separately tests the
WASM time limit. Complete logs and the clock JSON are included.

`manifest.json` records source headers, scope, file hashes and preservation
status. The full parity JSON reports are preserved as deterministic gzip files,
with compact per-case summaries beside them. Their combined uncompressed size
is 8,125,813 bytes and combined compressed size is 280,582 bytes.

The six-run follow-up uses Node-hosted WASM with the actual browser profile
(top 15, warm width 6, maximum 10,000 repairs), a 15-second host cap and one
fresh seed, 1217. It includes startup and fixture reading but excludes prior
fixture generation and the page UI. All six runs reached the host deadline,
retained 15 complete witnesses, and reported zero validation errors. No
exhaustive completion or native differential comparison is claimed for this
follow-up.

| Query | Enumeration T99 | Quick T99 |
|---|---:|---:|
| Medium spellsteal | Not reached by 15 s | 0.903 s |
| Arcanist, six slots free | Not reached by 15 s | 1.810 s |
| Light Bender, six slots free | Not reached by 15 s | 6.576 s |

T99 is time to 99% of a previously frozen same-fixture best-known score, not
99% of a proven global optimum. These are single-seed observations; enumeration
results are censored at the deadline. The complete follow-up is preserved in
`wasm-long6.json.gz` and summarized in JSON and CSV.

`build-provenance.json` ties the timed native binary hashes to all campaign
records and verifies that local source commit `1479d683` and published CI
commit `d9b1d383` have the identical Git tree
`311bc693942ce8d4881b35278e065694851ea836`. It records 35 source blob IDs, the
shipped WASM hash, exact installed toolchain versions and recovered reproduction
commands. Original build logs do not include their shell commands or Git tree
IDs, so this is recovered provenance, not a hermetic build attestation. Earlier
WASM report headers retain their pre-commit `source_dirty` values unchanged.

The integrator also reported final focused gates passing: 17 Python benchmark-
harness tests and 11 Node-WASM harness self-check assertions. Commands and the
provenance of those results are recorded in `final-focused-checks.json`.
