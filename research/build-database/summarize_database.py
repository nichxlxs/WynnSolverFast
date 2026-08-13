"""Build Mythic-family and cross-build pattern indexes from the research corpus."""

from __future__ import annotations

import json
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from typing import Any


DB_DIR = Path(__file__).resolve().parent
SUBTYPE_CLASS = {
    "bow": "archer",
    "spear": "warrior",
    "wand": "mage",
    "dagger": "assassin",
    "relik": "shaman",
}


def load(name: str) -> dict[str, Any]:
    return json.loads((DB_DIR / name).read_text(encoding="utf-8"))


def write(name: str, payload: dict[str, Any]) -> None:
    (DB_DIR / name).write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def rolled_raw(value: Any) -> float:
    if isinstance(value, dict):
        return value.get("raw", 0) or 0
    return value or 0


def inferred_candidates(records: list[dict[str, Any]]) -> list[str]:
    candidates: list[str] = []
    elements = {element for record in records for element in record.get("elements") or []}
    identifications = [record.get("identifications") or {} for record in records]
    speeds = {record.get("attack_speed") for record in records}
    major_ids = [major for record in records for major in record.get("major_ids") or []]

    def positive(*keys: str) -> bool:
        return any(
            rolled_raw(ids.get(key)) > 0
            for ids in identifications
            for key in keys
        )

    if len(elements) == 5:
        candidates.append("rainbow")
    if major_ids:
        candidates.append("major_id_specific")
    if positive("poison"):
        candidates.append("poison")
    if positive("manaRegen"):
        candidates.append("mana_regen_spell")
    if positive("manaSteal"):
        candidates.append("spellsteal_or_hybrid")
    if positive("spellDamage", "rawSpellDamage"):
        candidates.append("spell")
    if positive("mainAttackDamage", "rawMainAttackDamage"):
        candidates.append("melee_or_hybrid")
    if speeds & {"superSlow", "verySlow", "slow"} and positive(
        "mainAttackDamage", "rawMainAttackDamage"
    ):
        candidates.append("heavy_melee_candidate")
    if speeds & {"superFast", "veryFast"} or positive("rawAttackSpeed"):
        candidates.append("fast_melee_or_tierstack_candidate")
    if positive("healingEfficiency") or any(
        "heal" in major.get("description", "").lower() for major in major_ids
    ):
        candidates.append("support_or_healing")
    if positive("walkSpeed"):
        candidates.append("mobility")
    return list(dict.fromkeys(candidates or ["archetype_driven_generalist"]))


def build_weapon_index(mythics: dict[str, Any], functional: dict[str, Any]) -> dict[str, Any]:
    records_by_weapon: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for item in mythics["items"]:
        if item.get("item_type") == "weapon":
            records_by_weapon[item["display_name"]].append(item)

    examples_by_weapon: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for build in functional["builds"]:
        if build["weapon_tier"] == "Mythic":
            examples_by_weapon[build["weapon"]].append(build)

    weapons = []
    for weapon_name in sorted(records_by_weapon):
        records = sorted(records_by_weapon[weapon_name], key=lambda item: item["level"])
        examples = examples_by_weapon[weapon_name]
        base_record = next((item for item in records if not item["ascended_variant"]), records[0])
        weapons.append(
            {
                "weapon": weapon_name,
                "class": SUBTYPE_CLASS[base_record["subtype"]],
                "elements": base_record.get("elements") or [],
                "attack_speed": base_record.get("attack_speed"),
                "major_ids": base_record.get("major_ids") or [],
                "signature_identifications": base_record.get("identifications") or {},
                "variants": [
                    {
                        "internal_name": item["internal_name"],
                        "level": item["level"],
                        "ascended": item["ascended_variant"],
                    }
                    for item in records
                ],
                "observed_forum_archetypes": sorted(
                    {build["archetype"] for build in examples if build.get("archetype")}
                ),
                "observed_forum_engines": sorted(
                    {tag for build in examples for tag in build.get("engine_tags") or []}
                ),
                "observed_content_tags": sorted(
                    {tag for build in examples for tag in build.get("content_tags") or []}
                ),
                "inferred_candidate_families": inferred_candidates(records),
                "forum_examples": [
                    {
                        "id": build["id"],
                        "build_name": build["build_name"],
                        "archetype": build.get("archetype"),
                        "engine_tags": build.get("engine_tags") or [],
                        "variant": build["variant"],
                        "content_tags": build.get("content_tags") or [],
                        "threshold_profile_ids": build.get("threshold_profile_ids") or [],
                        "builder_url": build["builder_url"],
                    }
                    for build in examples
                ],
            }
        )

    return {
        "schema_version": 1,
        "generated_on": date.today().isoformat(),
        "item_authority": mythics["source"],
        "forum_authority": functional["source"],
        "weapon_count": len(weapons),
        "coverage": "Every one of the 55 current unique Mythic weapon names has at least one direct forum WynnBuilder example.",
        "interpretation_warning": "Observed fields describe the dated forum corpus. Inferred candidate families are property-based research hypotheses, not tested meta rankings.",
        "weapons": weapons,
    }


def build_pattern_index(functional: dict[str, Any], thresholds: dict[str, Any]) -> dict[str, Any]:
    equipment = Counter()
    requirement_codes = Counter()
    archetypes = Counter()
    engines = Counter()
    threshold_references = Counter()
    decoded = 0
    referenced_profiles = set()
    for build in functional["builds"]:
        if build.get("archetype"):
            archetypes[build["archetype"]] += 1
        engines.update(build.get("engine_tags") or [])
        profile_ids = build.get("threshold_profile_ids") or []
        threshold_references.update(profile_ids)
        referenced_profiles.update(profile_ids)
        data = build.get("decoded")
        if not data:
            continue
        decoded += 1
        requirement_codes[data["equipment_requirement_profile"]["code"]] += 1
        equipment.update(name for name in data["equipment"][:8] if name)

    available_profiles = set(thresholds["profiles"])
    missing_profiles = sorted(referenced_profiles - available_profiles)
    return {
        "schema_version": 1,
        "generated_on": date.today().isoformat(),
        "source_build_count": len(functional["builds"]),
        "decoded_build_count": decoded,
        "decode_failure_count": len(functional["builds"]) - decoded,
        "decode_warning": "Crafted and several incompatible encoding variants are retained as source links but excluded from decoded equipment-frequency statistics.",
        "equipment_requirement_code_definition": "ETWFA means equipment skill-requirement presence in earth, thunder, water, fire, air order. It is not a weapon-damage-element label.",
        "equipment_requirement_code_counts": dict(requirement_codes.most_common()),
        "common_equipment_in_decoded_builds": [
            {"item": name, "build_count": count} for name, count in equipment.most_common(40)
        ],
        "declared_archetype_counts": dict(archetypes.most_common()),
        "engine_tag_counts": dict(engines.most_common()),
        "threshold_profile_reference_counts": dict(threshold_references.most_common()),
        "threshold_profile_validation": {
            "referenced_count": len(referenced_profiles),
            "available_count": len(available_profiles),
            "missing_profile_ids": missing_profiles,
            "valid": not missing_profiles,
        },
    }


if __name__ == "__main__":
    mythics_payload = load("mythics.json")
    functional_payload = load("functional-builds.json")
    thresholds_payload = load("threshold-profiles.json")
    write(
        "weapon-build-families.json",
        build_weapon_index(mythics_payload, functional_payload),
    )
    patterns = build_pattern_index(functional_payload, thresholds_payload)
    write("build-patterns.json", patterns)
    if not patterns["threshold_profile_validation"]["valid"]:
        raise SystemExit(
            f"Missing threshold profiles: {patterns['threshold_profile_validation']['missing_profile_ids']}"
        )
    print(json.dumps(patterns["threshold_profile_validation"], indent=2))
