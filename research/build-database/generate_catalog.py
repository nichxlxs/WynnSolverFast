"""Build reproducible Mythic and elemental-pattern snapshots for research."""

from __future__ import annotations

import argparse
import html
import json
import re
import statistics
import urllib.request
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
DB_DIR = Path(__file__).resolve().parent
ITEMS_PATH = ROOT / "data" / "2.2.2.0" / "items.json"
MAJOR_IDS_PATH = ROOT / "data" / "2.2.2.0" / "majid.json"
API_URL = "https://api.wynncraft.com/v3/item/database?fullResult"
ELEMENTS = ("earth", "thunder", "water", "fire", "air")
ATTACK_SPEED_RANK = {
    "SUPER_SLOW": 0,
    "VERY_SLOW": 1,
    "SLOW": 2,
    "NORMAL": 3,
    "FAST": 4,
    "VERY_FAST": 5,
    "SUPER_FAST": 6,
}
META_KEYS = {
    "displayName",
    "name",
    "category",
    "type",
    "icon",
    "emblem",
    "tier",
    "atkSpd",
    "averageDps",
    "drop",
    "elements",
    "lvl",
    "classReq",
    "set",
    "majorIds",
    "slots",
    "lore",
    "id",
    "quest",
    "restrict",
    "material",
    "color",
}
REQUIREMENT_KEYS = ("strReq", "dexReq", "intReq", "defReq", "agiReq")


def strip_markup(value: str) -> str:
    value = re.sub(r"<[^>]+>", "", value)
    value = html.unescape(value)
    return " ".join(value.split())


def raw_value(value: Any) -> Any:
    if isinstance(value, dict) and "raw" in value:
        return value["raw"]
    return value


def load_local() -> tuple[list[dict[str, Any]], dict[str, Any], str]:
    item_payload = json.loads(ITEMS_PATH.read_text(encoding="utf-8"))
    major_ids = json.loads(MAJOR_IDS_PATH.read_text(encoding="utf-8"))
    return item_payload["items"], major_ids, ITEMS_PATH.parent.name


def load_live() -> tuple[list[dict[str, Any]], str, str]:
    request = urllib.request.Request(
        API_URL,
        headers={"User-Agent": "WynnSolverFast-class-building-research"},
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response), response.headers.get("Version", "unknown"), response.headers.get("Date", "")


def normalize_live_mythic(item: dict[str, Any]) -> dict[str, Any]:
    majors = [
        {"name": name, "description": strip_markup(description)}
        for name, description in (item.get("majorIds") or {}).items()
    ]
    requirements = item.get("requirements") or {}
    return {
        "display_name": item.get("displayName"),
        "internal_name": item.get("internalName"),
        "ascended_variant": str(item.get("internalName", "")).startswith("Masterwork "),
        "item_type": item.get("type"),
        "subtype": item.get("subType"),
        "level": requirements.get("level"),
        "class_requirement": requirements.get("classRequirement"),
        "skill_requirements": {
            key: value
            for key, value in requirements.items()
            if key not in {"level", "classRequirement"}
        },
        "elements": item.get("elements") or [],
        "attack_speed": item.get("attackSpeed"),
        "powder_slots": item.get("powderSlots"),
        "base": item.get("base") or {},
        "identifications": item.get("identifications") or {},
        "major_ids": majors,
        "drop_restriction": item.get("dropRestriction"),
        "lore": strip_markup(item.get("lore") or ""),
    }


def median(values: Iterable[float]) -> float | None:
    values = list(values)
    return round(statistics.median(values), 3) if values else None


def mean(values: Iterable[float]) -> float | None:
    values = list(values)
    return round(statistics.fmean(values), 3) if values else None


def presence_rate(items: list[dict[str, Any]], key: str, predicate) -> float:
    if not items:
        return 0.0
    return round(100 * sum(1 for item in items if predicate(raw_value(item.get(key, 0)) or 0)) / len(items), 1)


def element_pattern_snapshot(items: list[dict[str, Any]]) -> dict[str, Any]:
    endgame = [
        item
        for item in items
        if item.get("lvl", 0) >= 90
        and item.get("category") in {"weapon", "armor", "accessory"}
        and item.get("tier") not in {"Normal", "Unique"}
    ]
    result: dict[str, Any] = {}
    for element in ELEMENTS:
        group = [item for item in endgame if element in (item.get("elements") or [])]
        weapons = [item for item in group if item.get("category") == "weapon"]
        single_element_weapons = [
            item for item in weapons if (item.get("elements") or []) == [element]
        ]
        result[element] = {
            "sample_definition": "Level 90+ Rare-or-higher equipment whose WynnBuilder element list includes the element",
            "equipment_count": len(group),
            "weapon_count": len(weapons),
            "single_element_weapon_count": len(single_element_weapons),
            "weapon_attack_speed_distribution": dict(
                sorted(Counter(item.get("atkSpd", "UNKNOWN") for item in weapons).items())
            ),
            "median_weapon_attack_speed_rank": median(
                ATTACK_SPEED_RANK[item["atkSpd"]]
                for item in weapons
                if item.get("atkSpd") in ATTACK_SPEED_RANK
            ),
            "median_single_element_weapon_dps": median(
                item.get("averageDps", 0) for item in single_element_weapons
            ),
            "mean_single_element_weapon_dps": mean(
                item.get("averageDps", 0) for item in single_element_weapons
            ),
            "positive_id_presence_percent": {
                "poison": presence_rate(group, "poison", lambda value: value > 0),
                "mana_regen": presence_rate(group, "mr", lambda value: value > 0),
                "mana_steal": presence_rate(group, "ms", lambda value: value > 0),
                "life_steal": presence_rate(group, "ls", lambda value: value > 0),
                "raw_health_regen": presence_rate(group, "hprRaw", lambda value: value > 0),
                "health_bonus": presence_rate(group, "hpBonus", lambda value: value > 0),
                "walk_speed": presence_rate(group, "spd", lambda value: value > 0),
                "attack_speed_tier": presence_rate(group, "atkTier", lambda value: value > 0),
                "raw_main_attack": presence_rate(group, "mdRaw", lambda value: value > 0),
                "raw_spell_damage": presence_rate(group, "sdRaw", lambda value: value > 0),
            },
            "negative_id_presence_percent": {
                "raw_health_regen": presence_rate(group, "hprRaw", lambda value: value < 0),
                "walk_speed": presence_rate(group, "spd", lambda value: value < 0),
                "attack_speed_tier": presence_rate(group, "atkTier", lambda value: value < 0),
                "mana_regen": presence_rate(group, "mr", lambda value: value < 0),
            },
            "median_raw_ids": {
                key: median(raw_value(item.get(key, 0)) or 0 for item in group)
                for key in (
                    "poison",
                    "mr",
                    "ms",
                    "ls",
                    "hprRaw",
                    "hpBonus",
                    "spd",
                    "atkTier",
                    "mdRaw",
                    "sdRaw",
                )
            },
        }
    return {
        "source": str(ITEMS_PATH.relative_to(ROOT)).replace("\\", "/"),
        "snapshot_version": "2.2.2.0",
        "attack_speed_rank": ATTACK_SPEED_RANK,
        "overlap_warning": "An item can contribute to multiple element groups. These are descriptive correlations, not item design laws.",
        "elements": result,
    }


def normalize_local_mythic(item: dict[str, Any], major_ids: dict[str, Any]) -> dict[str, Any]:
    ids = {
        key: value
        for key, value in item.items()
        if key not in META_KEYS
        and key not in REQUIREMENT_KEYS
        and key not in {"nDam", "eDam", "tDam", "wDam", "fDam", "aDam", "hp", "eDef", "tDef", "wDef", "fDef", "aDef"}
    }
    return {
        "display_name": item.get("displayName"),
        "internal_name": item.get("name"),
        "ascended_variant": str(item.get("name", "")).startswith("Masterwork "),
        "item_type": item.get("category"),
        "subtype": item.get("type"),
        "level": item.get("lvl"),
        "class_requirement": item.get("classReq"),
        "skill_requirements": {
            key: item[key] for key in REQUIREMENT_KEYS if key in item
        },
        "elements": item.get("elements") or [],
        "attack_speed": item.get("atkSpd"),
        "average_dps": item.get("averageDps"),
        "powder_slots": item.get("slots"),
        "base": {
            key: item[key]
            for key in ("nDam", "eDam", "tDam", "wDam", "fDam", "aDam", "hp", "eDef", "tDef", "wDef", "fDef", "aDef")
            if key in item
        },
        "identifications": ids,
        "major_ids": [
            {
                "internal_name": major_id,
                "name": major_ids.get(major_id, {}).get("displayName", major_id),
                "description": major_ids.get(major_id, {}).get("description"),
            }
            for major_id in (item.get("majorIds") or [])
        ],
        "lore": item.get("lore"),
    }


def write_json(path: Path, payload: Any) -> None:
    path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--offline", action="store_true", help="Skip the official API refresh")
    args = parser.parse_args()

    local_items, local_major_ids, local_version = load_local()
    generated_at = datetime.now(timezone.utc).isoformat()

    live_error = None
    live_items: list[dict[str, Any]] = []
    api_version = None
    api_date = None
    if not args.offline:
        try:
            live_items, api_version, api_date = load_live()
        except Exception as error:  # pragma: no cover - network fallback
            live_error = f"{type(error).__name__}: {error}"

    if live_items:
        mythics = [normalize_live_mythic(item) for item in live_items if item.get("tier") == "mythic"]
        source = API_URL
        source_kind = "official_live_api"
    else:
        mythics = [
            normalize_local_mythic(item, local_major_ids)
            for item in local_items
            if item.get("tier") == "Mythic"
        ]
        source = str(ITEMS_PATH.relative_to(ROOT)).replace("\\", "/")
        source_kind = "local_wynnbuilder_snapshot"

    mythics.sort(
        key=lambda item: (
            item.get("item_type") or "",
            item.get("subtype") or "",
            item.get("display_name") or "",
            item.get("internal_name") or "",
        )
    )
    weapon_records = [item for item in mythics if item.get("item_type") == "weapon"]
    payload = {
        "schema_version": 1,
        "generated_at": generated_at,
        "source": source,
        "source_kind": source_kind,
        "official_api_version": api_version,
        "official_api_date": api_date,
        "local_wynnbuilder_version": local_version,
        "live_fetch_error": live_error,
        "counts": {
            "all_mythic_records": len(mythics),
            "mythic_weapon_records": len(weapon_records),
            "unique_mythic_weapon_names": len({item["display_name"] for item in weapon_records}),
        },
        "items": mythics,
    }
    write_json(DB_DIR / "mythics.json", payload)
    write_json(DB_DIR / "element-patterns.json", element_pattern_snapshot(local_items))
    print(json.dumps(payload["counts"]))
    print(f"source={source_kind} api_version={api_version} local_version={local_version}")


if __name__ == "__main__":
    main()
