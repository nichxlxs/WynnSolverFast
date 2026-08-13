"""Validate the internal coverage and provenance contracts of the research database."""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path


DB_DIR = Path(__file__).resolve().parent


def load(name: str) -> dict:
    return json.loads((DB_DIR / name).read_text(encoding="utf-8"))


functional = load("functional-builds.json")
mythics = load("mythics.json")
families = load("weapon-build-families.json")
patterns = load("build-patterns.json")
thresholds = load("threshold-profiles.json")
abilities = load("ability-trees.json")
aspects = load("aspects.json")
elements = load("element-patterns.json")

builds = functional["builds"]
assert len(builds) == 129
assert len({build["id"] for build in builds}) == 129
assert len({build["builder_url"] for build in builds}) == 129
assert Counter(build["class"] for build in builds) == {
    "archer": 26,
    "assassin": 26,
    "mage": 36,
    "shaman": 21,
    "warrior": 20,
}

weapon_records = [item for item in mythics["items"] if item["item_type"] == "weapon"]
weapon_names = {item["display_name"] for item in weapon_records}
assert len(weapon_records) == 71
assert len(weapon_names) == 55
assert mythics["source_kind"] == "official_live_api"
assert mythics["official_api_version"] == "v3.7.2"

armour_names = {
    item["display_name"] for item in mythics["items"] if item["item_type"] == "armour"
}
assert len(armour_names) == 11
assert not [item for item in mythics["items"] if item["item_type"] == "accessory"]

covered_weapons = {
    build["weapon"] for build in builds if build["weapon_tier"] == "Mythic"
}
assert covered_weapons == weapon_names
assert families["weapon_count"] == 55
assert len(families["weapons"]) == 55
assert all(weapon["forum_examples"] for weapon in families["weapons"])

assert functional["decode"]["decoded_records"] == 75
assert functional["decode"]["failed_records"] == 54
assert patterns["source_build_count"] == 129
assert patterns["decoded_build_count"] == 75
assert patterns["threshold_profile_validation"]["valid"] is True

profile_names = set(thresholds["profiles"])
referenced_profiles = {
    profile for build in builds for profile in build["threshold_profile_ids"]
}
assert referenced_profiles <= profile_names
assert len(profile_names) == 33
gaia = thresholds["profiles"]["weapon_gaia_melee"]["hard_constraints"]
assert gaia["ehp_no_agi"]["min"] == 20_000
assert gaia["life_steal"]["min"] == 0
assert gaia["health_regen_raw"]["min"] == -100
assert gaia["main_attack_range_percent"]["min"] == -20

expected_classes = {"archer", "warrior", "mage", "assassin", "shaman"}
assert set(abilities["classes"]) == expected_classes
assert set(aspects["classes"]) == expected_classes
assert all(abilities["classes"][player_class] for player_class in expected_classes)
assert all(aspects["classes"][player_class] for player_class in expected_classes)
assert elements["snapshot_version"] == "2.2.2.0"
assert set(elements["elements"]) == {"earth", "thunder", "water", "fire", "air"}

print(
    json.dumps(
        {
            "valid": True,
            "functional_builds": len(builds),
            "decoded_builds": functional["decode"]["decoded_records"],
            "unique_mythic_weapons": len(weapon_names),
            "mythic_armour_names": len(armour_names),
            "threshold_profiles": len(profile_names),
            "ability_tree_classes": len(abilities["classes"]),
            "aspect_classes": len(aspects["classes"]),
        },
        indent=2,
    )
)
