# Wynncraft Functional Build Research Database

This directory is a dated, reproducible research corpus for end-game class building. It does not claim that every linked build is best-in-slot. The forum guide calls content-tagged entries suitable for the named content, but its separate meta tracker was still unpopulated when captured.

## Current scope

- Local builder data: current WynnBuilder data version 2.2.3.0, refreshed 13 August 2026.
- Community corpus: first post of [The Ultimate Build Guide](https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/), last edited 11 August 2026.
- 129 unique direct WynnBuilder records: Archer 26, Assassin 26, Mage 36, Shaman 21, Warrior 20.
- All 55 unique current Mythic weapon names have at least one direct forum WynnBuilder example.
- 75 links decode fully enough for equipment-pattern analysis. The other 54 are preserved as functional source links but currently fail local decoding, mainly because they contain crafted items or an incompatible tree encoding.
- Nori corpus: all 140 live catalogue rows, including 139 WynnBuilder links and one RawFish solver preset. Every WynnBuilder link was migrated and executed against the current official builder in an isolated browser page.
- Conservative Nori acceptance: 66 current links render a structurally legal tree, 16 retain at least 80% of available AP, and 9 are end-game noncrafted median-ID-feasible solver seeds. The 80% setting is configurable through `NORI_ATREE_COMPLETENESS`.

## Files

| File | Purpose |
|---|---|
| `functional-builds.json` | Source build records, all unique direct WynnBuilder links from the first post, declared archetypes/engines, configurable threshold-profile references, and partial decoded details. |
| `mythics.json` | Official live Mythic snapshot, including all item records, Ascensions, requirements, elements, IDs, Major IDs, lore, and base values. |
| `weapon-build-families.json` | One row per unique Mythic weapon, observed forum families and archetypes, current examples, plus clearly labelled property-based candidate families. |
| `threshold-profiles.json` | Layered research-default acceptance gates for build families, archetypes, and signature weapons. |
| `element-patterns.json` | Reproducible level 90+ item-correlation analysis for Earth, Thunder, Water, Fire, and Air. |
| `build-patterns.json` | Cross-build requirement-code, item-frequency, engine, archetype, and threshold-profile aggregates. |
| `ability-trees.json` | Complete raw official ability trees for all five classes. |
| `aspects.json` | Complete raw official Aspect portfolios for all five classes. |
| `nori-builds.json` | Lossless normalized snapshot of all 140 Nori source rows, URL formats, hashes, hosts, and duplicate audit. |
| `nori-browser-validation.json` | Exhaustive live-browser migration and current WynnBuilder rendering evidence for every Nori row. |
| `nori-validation.json` | Combined compatibility grades, current URLs, AP completeness, local 2.2.3.0 decode results, family tags, and solver-seed eligibility. |
| `nori-build-status.csv` | Flat 140-row status table for filtering, statistical analysis, and manual review. |
| `family-seed-browser-validation.json` | Live current-builder evidence for the six optimizer family reference builds. |

## Interpretation rules

1. A weapon's damage elements, its equipment skill requirements, and its combat engine are different fields.
2. `ETA`, `ETWFA`, and similar codes use Earth, Thunder, Water, Fire, Air order and mean that the decoded equipment contains those skill requirements. They do not say that the weapon deals those elements.
3. Element tendencies are correlations, not item-design laws. Multi-element items are counted in each applicable group.
4. `observed_*` values come from the dated forum corpus. `inferred_candidate_families` are hypotheses based on current item properties and still require a legal tree, rotation simulation, and in-game test.
5. Threshold profiles are configurable research defaults. They are neither official rules nor universal meta cutoffs.
6. Major IDs are fixed mechanics that can change an ability or rotation. Minor identifications are the ordinary rolled or fixed stats on equipment and must be evaluated using actual rolls where thresholds are tight.

## Reproduction

From the repository root:

```powershell
rtk python research\build-database\generate_catalog.py
rtk python research\build-database\fetch_live_abilities.py
rtk python research\build-database\extract_forum_builds.py
rtk node research\build-database\decode_builds.js
rtk python research\build-database\summarize_database.py
rtk python research\build-database\validate_database.py
rtk node research\build-database\fetch_nori_builds.js
rtk node research\build-database\fetch_wynnbuilder_version.js 2.2.3.0
rtk npx.cmd --yes --package @playwright/cli playwright-cli --session nori-audit open about:blank
rtk pwsh -NoProfile -Command '& npx.cmd --yes --package @playwright/cli playwright-cli --session nori-audit run-code --filename research/build-database/playwright_validate_nori.code.js | node research/build-database/capture_playwright_result.js'
rtk npx.cmd --yes --package @playwright/cli playwright-cli --session family-audit open about:blank
rtk pwsh -NoProfile -Command '& npx.cmd --yes --package @playwright/cli playwright-cli --session family-audit run-code --filename research/build-database/playwright_validate_family_seeds.code.js | node research/build-database/capture_playwright_result.js family-seed-browser-validation.json'
rtk node research\build-database\validate_nori_builds.js
```

`extract_forum_builds.py` needs `lxml`; install it into the selected Python environment before running that step. Every generated JSON file stores its source and snapshot metadata.

## Adding a measured build

Keep source facts separate from evaluation:

1. Add or import the WynnBuilder URL and its forum attribution.
2. Record the declared activity, archetype, engine, crafted status, and content tags.
3. Attach baseline, family, archetype, and weapon threshold profiles.
4. Decode the equipment and tree where supported.
5. Store measured rotation, hit-rate, sustain, state-uptime, and survivability results as a new evidence layer. Do not overwrite the source record with a subjective verdict.

## Whole-build statistics

The derived family, class, archetype, threshold, and identification-roll
analysis lives in `../build-analysis/`. Regenerate it with:

```powershell
rtk node research\build-analysis\analyze_builds.js
rtk node research\build-analysis\id_roll_probability.js --write-examples
```

The evaluator reports unsupported, stale, or locally infeasible links as data.
It never substitutes a maximum-roll total for a missing physical roll record.
