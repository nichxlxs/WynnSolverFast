# Nori Wynn build catalogue: enumeration, provenance, and validation evidence

**Audit date:** 2026-08-13, Australia/Sydney
**Live catalogue:** <https://nori.fish/wynn/build/>
**Public API:** `POST https://nori.fish/api/build/search`
**Nori source snapshot:** [`RawFish69/Nori@4b3a52f`](https://github.com/RawFish69/Nori/tree/4b3a52f00c4224c6fb100095417fa2a9a1dce877)
**Current WynnBuilder source snapshot used for validation:** [`wynnbuilder/wynnbuilder.github.io@245bf04`](https://github.com/wynnbuilder/wynnbuilder.github.io/tree/245bf0489f2e3166b37d4039e856ae425bb9645f)

## Executive result

Nori exposes a useful, contributor-curated discovery corpus, but it is not a versioned or prevalidated build benchmark.

- The complete live catalogue is enumerable with one documented public API request. The 2026-08-13 snapshot contained **140 builds**.
- The API has no pagination. The browser receives the complete filtered array and displays ten records per client-side page, so the unfiltered catalogue has **14 UI pages**.
- Every record exposes seven fields: `name`, `link`, `class`, `weapon`, `tag`, `icon`, and `credit`. It exposes no stable record ID, created or updated timestamp, Wynncraft version, builder version, validation result, or numerical build statistics.
- No exact duplicate title or exact duplicate link existed in the snapshot. That does not mean the builds are semantically unique. Many records are deliberate variants of the same weapon and archetype.
- Of 140 links, 111 target the stable WynnBuilder host, 28 target the unofficial beta host, and one targets RawFish's build solver. The beta site currently warns that it may be out of date and offers to redirect to stable WynnBuilder.
- None of the 139 binary or legacy WynnBuilder records is natively encoded for the current WynnBuilder data version `2.2.3.0`. The newest native catalogue hashes are four `2.2.1.0` links. A current builder can migrate many old links, but its own warning says migration may break the build and ability tree.
- HTTP 200 is not a build-validity check because WynnBuilder state is in the URL fragment and decoded by JavaScript. An exhaustive isolated-browser pass migrated all 139 WynnBuilder links to `2.2.3.0`: 66 rendered with no structural tree error, 62 rendered an explicit current ATree error, and 11 failed another current load requirement. Of the 66 structurally valid migrations, 50 retained less than 80% of available AP, including 20 empty trees. Only 16 passed the configurable 80% AP-completeness proxy.
- Nine of those 16 were level 100 or higher, decoded locally with current `2.2.3.0` data, used noncrafted complete equipment, matched the declared class, and remained skill-point feasible at median displayed ID rolls. Six more were current-builder-only rather than solver seeds, and one was a genuine level-81 leveling build.
- Nori's documented limit for build search is 3 requests per second. One API request is enough to enumerate all builds. Current `robots.txt` disallows crawling `/wynn/build/` and `/js_global/`, but does not list `/api/build/search`; it separately states `search=yes`, `ai-train=no`, and `use=reference`.

The safe use is: fetch one API snapshot, preserve all source rows, decode and validate each builder fragment against the current official WynnBuilder data and ability tree, retain its original version, and assign explicit compatibility grades.

## 1. Authority and source boundary

Nori is first-party authority for the catalogue it publishes, but not for Wynncraft game mechanics. Its public repository describes Nori as a Discord bot, web app, and API project and is licensed AGPL-3.0. The audited main commit is `4b3a52f00c4224c6fb100095417fa2a9a1dce877`, committed 2026-07-27; GitHub repository metadata reported `pushed_at` 2026-08-08. See the [repository README](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/README.md) and [commit](https://github.com/RawFish69/Nori/commit/4b3a52f00c4224c6fb100095417fa2a9a1dce877).

The source boundary is important:

- The static build-search front end and public API documentation are open source.
- The repository explicitly says the production database and API service are closed source and not included. It directs clients to the public API instead of private database files or generated caches. See [`src/db/API.md`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/db/API.md#L1-L10).
- The repository also says the bot and web app are separate and do not share a runtime. Therefore bot-side file helpers are useful evidence for the contributor data model, not proof of production ordering, storage, constraints, or moderation. See [`src/README.md`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/README.md).

The live `build_search.js` was byte-for-byte identical to the file at the audited Nori commit on 2026-08-13, SHA-256 `E2CF134EFB4A3834AD982CE9ACDDD011D9C1FC5593781FD3EAC39248D8B735CB`. The data returned by the API is runtime data and is not present in the repository.

## 2. Complete enumeration and pagination

### 2.1 Exact enumeration request

The documented endpoint is:

```http
POST https://nori.fish/api/build/search
Content-Type: application/json
```

The minimal request that returned the entire catalogue was:

```json
{
  "keyword": "",
  "class_types": []
}
```

The response is one JSON array. On 2026-08-13 at approximately 03:09 UTC it returned HTTP 200, `Content-Type: application/json`, 51,883 UTF-8 bytes, and 140 objects. The response headers supplied an HTTP `Date`, but no catalogue timestamp, ETag, Last-Modified value, standard rate-limit remaining value, or delta cursor.

Supplying all five class values also returned the same 140 records:

```json
{
  "keyword": "",
  "class_types": ["warrior", "mage", "archer", "assassin", "shaman"]
}
```

Nori's [public API documentation](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/docs/api.md#L463-L489) specifies this POST body and response schema. The front end makes the same request in [`build_search.js`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js#L16-L30).

### 2.2 There is no API pagination

`page` is browser UI state, not an API parameter or cursor. The browser:

1. Fetches the whole filtered result array.
2. Sets `buildsPerPage = 10`.
3. Calculates indexes locally.
4. Uses JavaScript `slice(startIndex, endIndex)`.
5. Calculates `Math.ceil(builds.length / buildsPerPage)` for page controls.

This behavior is explicit in [`build_search.js` line 11](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js#L11), [lines 82-87](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js#L82-L87), and [lines 114-154](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js#L114-L154). The shareable UI query format is:

```text
https://nori.fish/wynn/build/?keyword=<text>&class_types=<comma-separated classes>&page=<client page>
```

The source updates those three query parameters in browser history, but only `keyword` and `class_types` are sent to the API. See [`updateURL`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js#L65-L72).

### 2.3 Snapshot totals

| Class | Records | Pages when that class alone is displayed at 10 per page |
|---|---:|---:|
| Archer | 22 | 3 |
| Assassin | 17 | 2 |
| Mage | 52 | 6 |
| Shaman | 24 | 3 |
| Warrior | 25 | 3 |
| **All classes** | **140** | **14** |

The sum is exact. All 140 records had one of the five expected class labels.

### 2.4 Observed search semantics

These are live API observations, not guaranteed production implementation details because the production search code is closed source:

- An empty keyword and empty class list returns all builds through the API, although the web form prevents a user from submitting both empty.
- Multiple classes act as a union. Mage plus Warrior returned 77, equal to 52 plus 25.
- Class input was accepted case-insensitively in bounded tests.
- Keyword matching was case-insensitive.
- Keyword matching behaved like a contiguous phrase, rather than independently ANDing whitespace-separated terms. `heavy melee` and `HEAVY` returned the same three heavy-melee records, while `heavy raid` returned zero.
- Credit text is searchable. A contributor name can return records even when it is absent from the title and tags.

The open bot helper similarly builds one lowercased searchable string from title, tag, credit, and weapon, then requires every submitted keyword string to occur in it. See [`build_file_search`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/bot/lib/build_recipe_utils.py#L12-L41). This corroborates the observed data model, but it is not proof that the hosted API runs this exact helper.

## 3. Record schema and missing version evidence

Every live record contained exactly these seven nonempty fields:

```json
{
  "name": "Riptide pot-healing raid oriented",
  "link": "https://wynnbuilder-beta.github.io/builder/#CV013teF-Fs8vCnBbs9KJArnE4sXeW2n7HgPA+UrhgMrgMrsanicrfErgLDk303jrKZUwt7-rqwVS",
  "class": "Mage",
  "weapon": "Riptide",
  "tag": "High DPS, High EHP, pot healing, riptide, WNTF, WTF, arcanist, mage, raids",
  "icon": "https://cdn.wynncraft.com/nextgen/itemguide/3.3/wand.water3.webp",
  "credit": "IGN: MambaDuDakora (helped by the raider Nirnir) discord:@mamba6862"
}
```

| Field | Meaning | Validation concern |
|---|---|---|
| `name` | Contributor-assigned display title | Not a stable ID. Free text. |
| `link` | WynnBuilder or solver URL | Must be executed and decoded client-side. HTTP status alone proves nothing about the fragment. |
| `class` | One of five display classes | Should be checked against the decoded weapon type. |
| `weapon` | Free-text weapon name | Should be checked against the decoded weapon slot and current item database. |
| `tag` | Comma-separated free text | Carries archetype, content, budget, playstyle, and miscellaneous labels without a controlled schema. |
| `icon` | Wynncraft CDN image URL | Presentation metadata, not build validity. |
| `credit` | Free-text author/contributor attribution | Searchable, but not a stable contributor identity. |

The API does **not** expose:

- a stable record identifier;
- submission, creation, verification, or update timestamps;
- a Wynncraft patch or data version field;
- a WynnBuilder encoder/schema version field;
- a structured archetype or intended ability-tree field;
- Aspects, tomes, powders, skill points, equipment, or numerical metrics as separate fields;
- a last-known-good or broken-link state;
- an endorsement, benchmark result, or content-clear proof.

The bot's contributor-only submission command accepts a title, keyword string, link, weapon, and credit. It resolves the weapon against an item map to derive class and icon, but the shown code does not decode the submitted link or validate its version or ability tree before storing it. See [`BuildSubmit`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/bot/lib/commands/builds.py#L152-L172).

Consequently, `name`, `tag`, and catalogue order must not be treated as proof of freshness. `?v=16` in a link is a legacy WynnBuilder data-index parameter, not a Nori record version or timestamp.

## 4. Link inventory and builder-version audit

### 4.1 Destination hosts

| Destination | Count | Interpretation |
|---|---:|---|
| `wynnbuilder.github.io` | 111 | Stable WynnBuilder host |
| `wynnbuilder-beta.github.io` | 28 | Unofficial beta/fork host which currently offers redirect to stable |
| `rawfish69.github.io/build-solver` | 1 | Nori/RawFish solver state, not a WynnBuilder fragment |

All 140 links used HTTPS and were nonempty. Two beta links contain a doubled slash before `builder`, which browsers normalize: `Paladin Ascendancy` and `Moirai Revolution`.

The beta host's current `redirect_fork.js` calls itself an unofficial version that might not be fully updated, warns that redirecting may break the ability tree, and preserves the path, query, and hash when redirecting to `wynnbuilder.github.io`. See the live [beta redirect script](https://wynnbuilder-beta.github.io/js/redirect_fork.js).

### 4.2 Hash formats

| Link state format | Count |
|---|---:|
| Binary `#CM...` | 75 |
| Binary `#CN...` | 23 |
| Binary `#CT...` | 18 |
| Binary `#CU...` | 5 |
| Binary `#CG...` | 4 |
| Binary `#CV...` | 4 |
| Binary `#CO...` | 1 |
| Legacy numeric `#9_...` | 7 |
| Legacy numeric `#10_...` | 1 |
| Legacy numeric `#11_...` | 1 |
| RawFish solver `?wb=...` | 1 |
| **Total** | **140** |

There are 130 compressed binary WynnBuilder hashes and nine numeric legacy WynnBuilder hashes. Eight numeric links explicitly carry `?v=16`; the single `#11_...` link omits `?v`, so current WynnBuilder falls back to its last legacy-supported data index.

### 4.3 Encoded WynnBuilder data versions

Current WynnBuilder lists its data versions by index and identifies `2.2.3.0` as the newest at the audited commit. See [`load_item.js`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/load_item.js#L208-L247). For a binary hash, the decoder reads the version index from its header. For legacy hashes, it reads `?v=` and supports up to index 18. See [`decodeHeader` and `decodeHash`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L387-L395), [binary dispatch](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L615-L647), and [legacy version handling](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L683-L731).

Decoded native-version distribution:

| Native builder data version | Records | Hash evidence |
|---|---:|---|
| `2.1.1.6` | 12 | 4 binary `CG`, plus 8 legacy links with `?v=16` |
| `2.1.2.0` effective legacy fallback | 1 | Numeric `#11_...` without `?v`; current decoder falls back to legacy index 18 |
| `2.1.5.0` | 75 | Binary `CM` |
| `2.1.6.0` | 23 | Binary `CN` |
| `2.2.0.0` | 1 | Binary `CO` |
| `2.2.0.21` | 18 | Binary `CT` |
| `2.2.0.31` | 5 | Binary `CU` |
| `2.2.1.0` | 4 | Binary `CV` |
| Not a WynnBuilder hash | 1 | RawFish solver `?wb=` |
| **Total** | **140** | |

There were **zero** native `2.2.2.0` or `2.2.3.0` WynnBuilder records in this snapshot. This does not mean every build concept is obsolete, but it proves the catalogue carries no record whose fragment was authored against the current builder data version.

When an older binary version is loaded, current WynnBuilder asks whether to update and explicitly warns that updating may break the build and ability tree. See [`loadOlderVersion`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L37-L64).

## 5. Duplicate and identity semantics

The live snapshot contained:

- zero exact duplicate `name` values;
- zero duplicate names after Unicode-invariant lowercase comparison;
- zero exact duplicate `link` values.

These counts do not justify semantic deduplication. The catalogue intentionally contains multiple records for a weapon or concept, including crafted versus non-crafted, budget versus high-investment, raid versus lootrun, and alternate archetype variants. Examples include `Freedom Boltslinger` and `Freedom Boltslinger 2`, as well as differently titled Hadal Acolyte entries.

The open bot helper stores builds in a JSON object keyed by exact title. An exact key update replaces its value; a new key is prepended to the object. See [`build_file_updater`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/bot/lib/build_recipe_utils.py#L45-L65). Removal searches titles case-insensitively. Again, production storage is closed, so this is contributor-tool behavior, not a guaranteed hosted-database constraint.

Recommended downstream identity fields:

```text
source_record_fingerprint = SHA256(name + "\0" + link + "\0" + class + "\0" + weapon + "\0" + tag + "\0" + credit)
source_link_exact         = exact submitted URL
decoded_native_version   = version decoded from the fragment
normalized_current_hash  = current builder hash after an accepted, validated migration
semantic_fingerprint     = ordered equipment + powders + assigned SP + level + tomes + aspects + active current ability IDs
```

Preserve every source row. Use `semantic_fingerprint` only to identify equivalent decoded states, and retain variant titles and provenance. Do not deduplicate solely by weapon, archetype tag, exact old hash, or normalized current hash. An old URL may normalize to a different current hash, and a migration can silently lose or reinterpret ability-tree state.

## 6. Exhaustive current compatibility pass and representative cases

Browser checks used current stable WynnBuilder on 2026-08-13, whose repository head was `245bf0489f2e3166b37d4039e856ae425bb9645f`, dated 2026-08-10. Every source link was opened on a fresh blank page, beta redirects and old-version migration prompts were accepted, and the rendered error box, ATree warning, AP cost and cap, weapon field, skill-point summary, and calculated-stat summary were inspected. The current URL was retained. This executed the client-side fragment decoder and did not rely on HTTP status.

| Exhaustive result | Count | Interpretation |
|---|---:|---|
| Current functional solver seed | 9 | At least 80% AP retained, level 100+, complete noncrafted current equipment, matching class, and median-ID skill-point feasibility |
| Current functional builder only | 6 | Tree completeness passed, but crafted decoding or median-ID skill-point feasibility prevented safe solver seeding |
| Current functional non-endgame | 1 | Valid level-81 Warden leveling build |
| Migrated incomplete ATree | 50 | Page and tree structure rendered, but less than 80% of available AP survived migration |
| Broken current ATree | 62 | Current builder rendered explicit reachability, dependency, or blocker errors |
| Current load failure | 11 | Navigation, build-state, item, skill-point summary, error-box, or page-execution acceptance failed |
| Not a WynnBuilder link | 1 | RawFish solver preset requiring a separate validator |
| **Total** | **140** | |

The 80% AP rule is a configurable completeness proxy, not a WynnBuilder legality rule. It is deliberately conservative for end-game benchmark seeding. A migrated tree can be structurally legal while retaining too little of the authored build to be useful.

### 6.1 Modern compressed entry, old native version, apparently successful migration

Catalogue record: [Riptide pot-healing raid oriented](https://wynnbuilder-beta.github.io/builder/#CV013teF-Fs8vCnBbs9KJArnE4sXeW2n7HgPA+UrhgMrgMrsanicrfErgLDk303jrKZUwt7-rqwVS)

Observed behavior:

- The beta host offered to redirect to stable WynnBuilder.
- Stable WynnBuilder reported native version `2.2.1.0 < 2.2.3.0`.
- Accepting migration rewrote the fragment from `CV...` to a current `CX...` binary hash.
- Equipment decoded to Polyglot, Far Cosmos, Runebound Chains, Crusade Sabatons, two Prisms, Prowess, Compromise, and Riptide.
- The current tree showed 50 of 50 AP active and current calculated statistics rendered.

**Grade:** `migrates-current-syntactically`. This is not proof that the new abilities, damage, sustain, or intended play loop are semantically equivalent to the authored `2.2.1.0` build. That needs a current mechanic review.

### 6.2 Legacy equipment survives, ability-tree migration is materially degraded

Catalogue record: [Acrobat Cataclysm](https://wynnbuilder.github.io/builder?v=16#10_0Au01+0r50tv0uP0A50K40OH0QQ0O2C1a0O0e1g10006C00010036C0z0z0+0+0+0+0-1T2Y2Y2Z2Z2a2ajNZ-YzsO0)

Observed behavior:

- Stable WynnBuilder reported native version `2.1.1.6 < 2.2.3.0`.
- Migration rewrote the URL to a current compressed `CX...` hash.
- Equipment decoded, including Cumulonimbus, Aquarius, Vaward, Pro Tempore, Photon, Rune of Safe Passage, Prowess, Diamond Fusion Necklace, and Cataclysm.
- The migrated current Assassin ability tree showed only **11 of 45 AP** active, despite the catalogue title claiming Acrobat.

**Grade:** `equipment-decodes-tree-degraded`. It is not a valid current Acrobat build until a current tree is deliberately reconstructed and revalidated.

### 6.3 Accessible but broken/degraded re-encoding

Catalogue record: [boreal warp with discoverer](https://wynnbuilder.github.io/builder/#11_0eO0QU0r50QN0Fx0OW0K40OH0R10V0V0V1B2M1g1000FU01000FU1000FU1007lU0z0z0+0+0+0+0-1T2Y2Y2Z2Z2a2a401401401401401--PqclPB)

Observed behavior:

- Current WynnBuilder treated the link as legacy `2.1.2.0` and warned before migration.
- Gear names decoded, including The Modulator, Discoverer, Boreal, and Warp.
- Assigned skill points displayed as zero.
- Current WynnBuilder raised:

```text
TypeError: Cannot read properties of undefined (reading 'push')
    at collectPowders
    at encodePowders
    at encodeEquipment
    at encodeBuild
```

**Grade:** `degraded-broken`. The page being accessible and some gear rendering do not make the state safe for current numerical analysis or resharing.

### 6.4 Non-WynnBuilder entry

Catalogue record: [Boreal Ignis](https://rawfish69.github.io/build-solver/?wb=N4IgziBcoBZQLAJgGzIDQgMYIIwE54MAbKZABgA4MAjKHZCgdgwCccEBWRRVxT7mizoBmShgB2nPDgwB3OozLIAvsTqIyGbJHEBXIkQxwd%2BosqA)

This loaded RawFish's solver state without an observed console error. It must be validated through that solver's own schema and exported build state. It cannot be parsed as a WynnBuilder hash.

The examples above illustrate the failure classes found by the exhaustive pass.

## 7. Evidence-backed validation criteria

### 7.1 Compatibility grades

Use separate, machine-readable grades rather than a boolean `valid` field:

| Grade | Required evidence |
|---|---|
| `unfetched` | Source row exists but target has not been checked. |
| `transport-only` | Host returned HTTP success. Fragment has not been decoded. |
| `decodes-native-old` | The current client can load the original version without migrating; all expected state renders. Still not current. |
| `migrates-current-syntactically` | Migration completes, current hash is produced, all state renders, no hard tree error, and no console exception. Semantic equivalence remains unproven. |
| `current-validated` | Current data version plus all structural, ability-tree, and numerical checks below pass, followed by a human mechanic review for intended playstyle/content. |
| `equipment-decodes-tree-degraded` | Equipment survives but current ability tree is incomplete, invalid, or inconsistent with claimed archetype. |
| `degraded-broken` | Missing state, decoder/encoder exception, undefined item, invalid skill points, or other material failure. |
| `unsupported-link-type` | Link is not supported by the selected validator, such as RawFish solver state sent to a WynnBuilder-only decoder. |

### 7.2 Transport and URL checks

1. Require HTTPS and an allowlisted host.
2. Preserve the exact original URL.
3. Classify stable WynnBuilder, beta WynnBuilder, and RawFish solver separately.
4. Require a nonempty fragment for WynnBuilder links.
5. Normalize harmless path differences, but do not rewrite beta to stable without recording that migration.
6. Treat HTTP 200 only as `transport-only`.

### 7.3 Decode and state-integrity checks

Execute the current stable WynnBuilder JavaScript and require:

1. The fragment version is decoded and recorded before migration.
2. All nine equipment slots decode to defined current items, custom items, or crafted items.
3. The decoded weapon matches Nori's `weapon` text after controlled normalization.
4. Weapon type implies the same class as Nori's `class` field.
5. Powders decode in legal slots and tiers.
6. Assigned skill points, build level, tomes, Aspects, and ability-tree state decode without truncation.
7. Equip-order and skill-point requirements are valid under current items.
8. No JavaScript error, rejected promise, missing data fetch, undefined item, or re-encoding exception occurs.
9. Re-encoding produces a current hash that can be loaded again to the same semantic state.

The current decoder explicitly separates binary and legacy formats and loads version-specific item, tome, Aspect, Major ID, and ability-tree data. See [`decodeHash`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L615-L681).

### 7.4 Current ability-tree checks

Do not infer a tree from Nori's free-text archetype tag. Decode or deliberately reconstruct the tree against current class data, then require:

1. Selected node IDs exist in the current class tree.
2. Every selected node is reachable from an active parent.
3. All dependencies are active.
4. No selected blocker conflict is active.
5. Archetype-count requirements are satisfied.
6. Total AP does not exceed the level-dependent cap.
7. The builder reports no hard tree validation error.
8. Active AP is plausible for an end-game build. A migrated tree with 11 of 45 AP is not accepted merely because it renders.
9. The active nodes actually implement the catalogue's claimed archetype and playstyle.
10. Current Aspects exist for the decoded class and their selected tiers are legal.

WynnBuilder's validator checks missing dependencies, blockers, reachability, archetype requirements, and remaining points in [`abil_can_activate`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/atree.js#L299-L346). It totals AP against a level-dependent cap and reports over-allocation in [`atree_validate`](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/atree.js#L355-L440).

### 7.5 Numerical and semantic checks

After structural validation:

1. Recalculate current EHP, damage, spell costs, mana sustain, life sustain, walk speed, and relevant attack-speed breakpoints.
2. Check any roll-sensitive thresholds separately because normal WynnBuilder links do not encode a physical item's rolls and calculate ordinary items at favorable endpoints.
3. Confirm required consumables, tomes, crafted items, off-hands, raid buffs, and Aspect tiers are stated rather than silently assumed.
4. Confirm the stated use tag such as raid, lootrun, war, world event, leveling, heavy melee, or support is mechanically compatible with the build.
5. For archetype migrations, compare the authored old tree with the reconstructed current tree at the ability level. Matching equipment is insufficient.
6. Record the validator's current WynnBuilder commit, data version, audit time, and normalized current hash.

## 8. Robots, rate limits, and collection expectations

Nori's [current `robots.txt`](https://nori.fish/robots.txt) contains two relevant layers:

- A managed content signal for the general user agent: `search=yes,ai-train=no,use=reference`.
- Named crawler exclusions including `GPTBot`, plus a later `User-agent: *` block that disallows `/wynn/build/`, `/js_global/`, and several other interactive pages.

The file does **not** explicitly disallow `/api/build/search`. The API is publicly documented, and Nori's repository directs community developers to use it for data access. The appropriate collector is therefore the documented API, not HTML or script crawling.

Nori documents `POST /api/build/search` at **3 requests per second** in [`src/web/docs/api.md`](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/docs/api.md#L463-L489). A bounded four-request burst produced an HTTP rate-limit error body:

```json
{"error":"Rate limit exceeded: 3 per 1 second"}
```

Successful live responses were served through Cloudflare with `cf-cache-status: DYNAMIC` and did not expose standard `RateLimit-*` or `Retry-After` headers in this audit.

Recommended behavior:

- Use one unfiltered POST per snapshot.
- Run no faster than one request per second if filters are genuinely needed.
- Cache locally and compare snapshots rather than repeatedly querying.
- Back off on 429 or the JSON rate-limit error.
- Identify the client honestly.
- Use the data only as a reference/discovery corpus, consistent with the site's content signal.
- Do not crawl the disallowed build UI or shared JavaScript paths for routine ingestion.

## 9. Proposed machine-usable snapshot schema

```json
{
  "source": "nori",
  "source_endpoint": "https://nori.fish/api/build/search",
  "fetched_at": "2026-08-13T03:09:19Z",
  "source_row": {
    "name": "...",
    "link": "...",
    "class": "...",
    "weapon": "...",
    "tag": "...",
    "icon": "...",
    "credit": "..."
  },
  "link_type": "wynnbuilder-binary|wynnbuilder-legacy|rawfish-solver|unknown",
  "native_data_version": "2.2.1.0",
  "validation": {
    "validator_commit": "245bf0489f2e3166b37d4039e856ae425bb9645f",
    "validator_data_version": "2.2.3.0",
    "checked_at": "...",
    "grade": "migrates-current-syntactically",
    "decoded_weapon": "Riptide",
    "decoded_class": "Mage",
    "active_ap": 50,
    "ap_cap": 50,
    "hard_tree_error": false,
    "console_errors": [],
    "normalized_current_hash": "CX...",
    "semantic_review": "pending"
  }
}
```

## 10. Uncertainties and limits

1. **Production data lifecycle is undisclosed.** There is no public creation/update timestamp, change feed, deletion log, or moderation history for build records.
2. **Production persistence and duplicate constraints are closed source.** The bot's JSON helper cannot be assumed to be the hosted API implementation.
3. **Catalogue ordering has no documented meaning.** The first row was not current-version data, so position is not freshness evidence.
4. **No native current-version entries existed in the snapshot.** The audit can test migration behavior, but cannot point to a catalogue row authored directly under `2.2.3.0`.
5. **The browser pass proves current structural compatibility, not gameplay equivalence.** All 140 rows were executed or explicitly classified as non-WynnBuilder, but no controlled in-game benchmark proves that a migrated build still performs its authored rotation or content role.
6. **Syntactic migration is not semantic migration.** Current WynnBuilder can render and re-encode a build while its intended archetype, rotation, sustain, breakpoints, or content suitability has changed.
7. **Builder data version is not automatically the public game patch label.** This note reports WynnBuilder's own `wynn_version_names` values as compatibility identifiers.
8. **Free-text tags are community metadata.** Claims such as DPS, tank, raid, lootrun, or archetype are not independently benchmarked by Nori's API payload.
9. **The beta-host redirect is mutable.** Its current warning and redirect behavior should be rechecked if automated migration is performed later.

## Source ledger

| Source | Authority | Audited fact |
|---|---|---|
| [Nori build UI](https://nori.fish/wynn/build/) | Nori live site | Public discovery interface and shareable filter URLs |
| `POST https://nori.fish/api/build/search` | Nori live API | 140-record point-in-time payload and exact seven-field rows |
| [Nori API docs](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/docs/api.md#L463-L489) | Nori first-party docs | Method, body, response schema, 3/second limit |
| [Nori build front end](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/web/js_global/build_search.js) | Nori first-party source | Complete fetch, client pagination, URL query behavior |
| [Nori database notice](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/db/API.md#L1-L10) | Nori first-party source | Production API/database closed-source boundary |
| [Nori contributor build command](https://github.com/RawFish69/Nori/blob/4b3a52f00c4224c6fb100095417fa2a9a1dce877/src/bot/lib/commands/builds.py#L152-L172) | Nori bot source | Submission fields and lack of shown link/tree validation |
| [Nori robots.txt](https://nori.fish/robots.txt) | Nori live policy | Crawler exclusions and content signals |
| [WynnBuilder version list](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/load_item.js#L208-L247) | Current builder source | Version index mapping and current `2.2.3.0` identifier |
| [WynnBuilder decoder](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/build_encode_decode.js#L615-L731) | Current builder source | Binary versus legacy parsing, old-version loading, legacy limit |
| [WynnBuilder ability-tree validator](https://github.com/wynnbuilder/wynnbuilder.github.io/blob/245bf0489f2e3166b37d4039e856ae425bb9645f/js/builder/atree.js#L299-L440) | Current builder source | Dependency, blocker, reachability, archetype, and AP validation |
| [Beta redirect script](https://wynnbuilder-beta.github.io/js/redirect_fork.js) | Live beta builder | Unofficial-site warning and redirect semantics |
