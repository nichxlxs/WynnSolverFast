"""Extract the current Ultimate Build Guide's WynnBuilder links into JSON."""

from __future__ import annotations

import html
import json
import re
import subprocess
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from urllib.parse import urlparse

import lxml.html


DB_DIR = Path(__file__).resolve().parent
FORUM_URL = "https://forums.wynncraft.com/threads/the-ultimate-build-guide.320092/"
THREAD_LAST_EDITED = "2026-08-11"
WARP_TIMERIFT_URL = "https://wynnbuilder-beta.github.io/builder/#CT0620-CHWHoX16ZQYaXanXY40NDY0saG7s4s4y3sa9b482OJ2TOJOJmFOJcKIW8WD9qXDXD1-WD9J91eT22RAo990mxf+wGR+F3"
NON_MYTHIC_CLASS = {
    "Abyssopelagic": "assassin",
    "Petal Blade": "assassin",
    "Departure": "archer",
    "Siege Vent": "archer",
    "Forsaken": "warrior",
    "Trikaya": "warrior",
    "Theatre Cane": "mage",
    "Futulism": "mage",
    "Event Horizon": "mage",
    "Panic Zealot": "shaman",
    "Latent Disorder": "shaman",
}
SUBTYPE_CLASS = {
    "bow": "archer",
    "spear": "warrior",
    "wand": "mage",
    "dagger": "assassin",
    "relik": "shaman",
}
ARCHETYPES = {
    "archer": ("boltslinger", "sharpshooter", "trapper"),
    "warrior": ("fallen", "battle monk", "paladin"),
    "mage": ("riftwalker", "light bender", "arcanist"),
    "assassin": ("shadestepper", "trickster", "acrobat"),
    "shaman": ("summoner", "ritualist", "acolyte"),
}
SPELLSTEAL_WEAPONS = {
    "Lament",
    "Oblivion",
    "Nirvana",
    "Idol",
    "Hadal",
    "Grandmother",
}


def slug(value: str) -> str:
    value = value.lower().replace("'", "")
    value = re.sub(r"[^a-z0-9]+", "-", value)
    return value.strip("-")


def clean_text(value: str) -> str:
    value = re.sub(r"<br\s*/?>", "\n", value, flags=re.I)
    value = re.sub(r"<[^>]+>", "", value)
    return "\n".join(line.strip() for line in html.unescape(value).splitlines() if line.strip())


def fetch_html() -> bytes:
    result = subprocess.run(
        [
            "curl.exe",
            "-L",
            "--compressed",
            "--max-time",
            "60",
            "-A",
            "Mozilla/5.0",
            "-s",
            FORUM_URL,
        ],
        capture_output=True,
        check=True,
    )
    return result.stdout


def mythic_index() -> tuple[dict[str, str], dict[str, list[str]]]:
    payload = json.loads((DB_DIR / "mythics.json").read_text(encoding="utf-8"))
    class_by_name: dict[str, str] = {}
    elements_by_name: dict[str, list[str]] = {}
    for item in payload["items"]:
        if item["item_type"] != "weapon":
            continue
        class_by_name[item["display_name"]] = SUBTYPE_CLASS[item["subtype"]]
        elements_by_name.setdefault(item["display_name"], item.get("elements") or [])
    return class_by_name, elements_by_name


def infer_archetype(player_class: str, heading: str) -> str | None:
    normalized = heading.lower().replace("lightbender", "light bender").replace("battlemonk", "battle monk")
    for archetype in ARCHETYPES[player_class]:
        if archetype in normalized:
            return archetype.title()
    return None


def infer_engine(heading: str) -> list[str]:
    normalized = heading.lower()
    tags: list[str] = []
    phrases = (
        ("heavy melee", "heavy_melee"),
        ("tier stack", "tierstack"),
        ("tierstack", "tierstack"),
        ("melee", "melee"),
        ("spell", "spell"),
        ("hybrid", "hybrid"),
        ("summoner", "summoner"),
        ("trapper", "trapper"),
        ("mob grinding", "poison_grinding"),
        ("surf", "surf"),
        ("big mac", "bash_melee"),
        ("upperbash", "upperbash"),
        ("upperscream", "upperscream"),
        ("bash cut", "bash_cut"),
    )
    for phrase, tag in phrases:
        if phrase in normalized and tag not in tags:
            tags.append(tag)
    return tags or ["archetype_default"]


def infer_threshold_profiles(
    weapon_name: str,
    archetype: str | None,
    engine_tags: list[str],
    variant: str,
) -> list[str]:
    """Attach layered, configurable acceptance profiles to a source build.

    These are research defaults, not claims made by the forum author. Consumers
    should merge the profiles in order and may override any threshold.
    """
    profiles = ["baseline_general_combat"]
    engine_set = set(engine_tags)
    if variant == "lootrun":
        profiles.append("family_lootrun")
    if "heavy_melee" in engine_set:
        profiles.append("family_heavy_melee")
    elif "tierstack" in engine_set:
        profiles.append("family_tierstack")
    elif "melee" in engine_set or "bash_melee" in engine_set:
        profiles.append("family_cancelstack")
    if engine_set & {"spell", "upperbash", "upperscream", "bash_cut", "surf"}:
        profiles.append(
            "family_spellsteal" if weapon_name in SPELLSTEAL_WEAPONS else "family_spell_sustained"
        )
    if "hybrid" in engine_set:
        profiles.append("family_hybrid")
    if "poison_grinding" in engine_set:
        profiles.append("family_poison")
    if weapon_name == "Guardian":
        profiles.append("family_support")

    if archetype:
        profiles.append(f"archetype_{slug(archetype).replace('-', '_')}")

    special_profiles = {
        "Warp": "weapon_warp",
        "Singularity": "weapon_singularity",
        "Grandmother": "weapon_grandmother",
        "Vengeance": "weapon_vengeance",
        "Revolution": "weapon_revolution",
        "Guardian": "weapon_guardian_support",
        "Toxoplasmosis": "weapon_toxoplasmosis",
    }
    if weapon_name == "Gaia" and "melee" in engine_set:
        profiles.append("weapon_gaia_melee")
    elif weapon_name in special_profiles:
        profiles.append(special_profiles[weapon_name])

    return list(dict.fromkeys(profiles))


def infer_variant(context: str, anchor_text: str) -> tuple[str, bool | None]:
    text = f"{context}\n{anchor_text}".lower()
    normalized = text.replace("non-crafted", "non crafted")
    protected = normalized.replace("non crafted", "__nc__").replace("uncrafted", "__nc__")
    positions = {
        "non_crafted": protected.rfind("__nc__"),
        "crafted": protected.rfind("crafted"),
        "lootrun": protected.rfind("lootrun"),
        "glass": protected.rfind("glass"),
    }
    variant, position = max(positions.items(), key=lambda item: item[1])
    if position < 0:
        return "standard", None
    if variant == "non_crafted":
        return variant, False
    if variant == "crafted":
        return variant, True
    if variant == "glass":
        return variant, False
    return variant, None


def parse_content_tags(target_html: str) -> list[str]:
    match = re.search(r"Uses:\s*([^<\n]+)", target_html, flags=re.I)
    if not match:
        return []
    raw = html.unescape(match.group(1))
    return [part.strip().upper() for part in raw.split(",") if part.strip()]


def extract() -> dict:
    class_by_name, elements_by_name = mythic_index()
    document = lxml.html.fromstring(fetch_html())
    containers = document.xpath(
        '//div[contains(concat(" ", normalize-space(@class), " "), " bbCodeSpoilerContainer ")]'
    )
    builds: list[dict] = []
    dedupe: set[tuple[str, str]] = set()

    for container in containers:
        targets = container.xpath(
            './div[contains(concat(" ", normalize-space(@class), " "), " SpoilerTarget ")]'
        )
        if not targets:
            continue
        target = targets[0]
        nested_spoilers = target.xpath(
            './/div[contains(concat(" ", normalize-space(@class), " "), " bbCodeSpoilerContainer ")]'
        )
        if nested_spoilers:
            continue
        target_html = lxml.html.tostring(target, encoding="unicode")
        chunks = re.split(r"<b>(.*?)</b>", target_html, flags=re.I | re.S)
        if len(chunks) < 3:
            continue
        content_tags = parse_content_tags(target_html)
        for index in range(1, len(chunks), 2):
            heading = clean_text(chunks[index])
            segment = chunks[index + 1] if index + 1 < len(chunks) else ""
            known_weapons = tuple(class_by_name) + tuple(NON_MYTHIC_CLASS)
            weapon_name = next(
                (
                    name
                    for name in sorted(known_weapons, key=len, reverse=True)
                    if heading == name or heading.startswith(f"{name} ") or heading.startswith(f"{name} -")
                ),
                heading.split(" - ", 1)[0].strip(),
            )
            player_class = class_by_name.get(weapon_name) or NON_MYTHIC_CLASS.get(weapon_name)
            if not player_class:
                continue

            link_pattern = re.compile(
                r'<a\s+href="([^"]*wynnbuilder[^\"]*)"[^>]*>(.*?)</a>',
                flags=re.I | re.S,
            )
            previous_end = 0
            for link_index, match in enumerate(link_pattern.finditer(segment), start=1):
                builder_url = html.unescape(match.group(1))
                if not urlparse(builder_url).fragment:
                    continue
                anchor_text = clean_text(match.group(2))
                context = clean_text(segment[previous_end : match.start()])
                previous_end = match.end()
                variant, crafted = infer_variant(context, anchor_text)
                unique_key = (heading, builder_url)
                if unique_key in dedupe:
                    continue
                dedupe.add(unique_key)
                is_mythic = weapon_name in class_by_name
                entry_id = slug(f"ubg-{weapon_name}-{heading}-{variant}-{link_index}")
                archetype = infer_archetype(player_class, heading)
                engine_tags = infer_engine(heading)
                builds.append(
                    {
                        "id": entry_id,
                        "class": player_class,
                        "weapon": weapon_name,
                        "weapon_tier": "Mythic" if is_mythic else "non-Mythic",
                        "weapon_elements": elements_by_name.get(weapon_name, []),
                        "build_name": heading,
                        "archetype": archetype,
                        "engine_tags": engine_tags,
                        "threshold_profile_ids": infer_threshold_profiles(
                            weapon_name,
                            archetype,
                            engine_tags,
                            variant,
                        ),
                        "variant": variant,
                        "crafted": crafted,
                        "content_tags": content_tags,
                        "community_status": (
                            "content-tagged community recommendation"
                            if content_tags and "GENERAL USE" not in content_tags
                            else "community functional example"
                        ),
                        "builder_url": builder_url,
                        "forum_url": FORUM_URL,
                        "forum_thread_last_edited": THREAD_LAST_EDITED,
                        "source_note": "Extracted from the first post of The Ultimate Build Guide. Treat as a dated community snapshot, not an official tier list.",
                    }
                )

    # The Warp spoiler also contains one auxiliary Timerift link in its notes,
    # outside the bold build-heading segments handled above.
    if not any(build["builder_url"] == WARP_TIMERIFT_URL for build in builds):
        builds.append(
            {
                "id": "ubg-warp-arcanist-timerift",
                "class": "mage",
                "weapon": "Warp",
                "weapon_tier": "Mythic",
                "weapon_elements": elements_by_name["Warp"],
                "build_name": "Warp - Arcanist - Timerift",
                "archetype": "Arcanist",
                "engine_tags": ["spell", "mobility"],
                "threshold_profile_ids": infer_threshold_profiles(
                    "Warp", "Arcanist", ["spell", "mobility"], "timerift"
                ),
                "variant": "timerift",
                "crafted": None,
                "content_tags": ["TNA", "TCC", "NOL", "NOTG", "WTP"],
                "community_status": "auxiliary community functional example",
                "builder_url": WARP_TIMERIFT_URL,
                "forum_url": FORUM_URL,
                "forum_thread_last_edited": THREAD_LAST_EDITED,
                "source_note": "Extracted from the Timerift note in the Warp spoiler of The Ultimate Build Guide.",
            }
        )

    builds.sort(key=lambda item: (item["class"], item["weapon"], item["build_name"], item["id"]))
    counts = Counter(item["class"] for item in builds)
    mythic_coverage: dict[str, set[str]] = defaultdict(set)
    for item in builds:
        if item["weapon_tier"] == "Mythic":
            mythic_coverage[item["class"]].add(item["weapon"])
    return {
        "schema_version": 1,
        "generated_on": date.today().isoformat(),
        "source": FORUM_URL,
        "source_last_edited": THREAD_LAST_EDITED,
        "scope": "All directly linked WynnBuilder builds in the first post that could be mapped to a current Mythic or named non-Mythic class example.",
        "interpretation_warning": "The author says a content tag denotes meta suitability for that content. The separate meta tracker is currently unpopulated, so these records are community-curated functional examples, not a complete ordered tier list.",
        "counts": {
            "build_records": len(builds),
            "by_class": dict(sorted(counts.items())),
            "unique_mythic_weapons_covered": sum(len(names) for names in mythic_coverage.values()),
            "mythic_weapon_coverage_by_class": {
                player_class: len(names) for player_class, names in sorted(mythic_coverage.items())
            },
        },
        "builds": builds,
    }


if __name__ == "__main__":
    payload = extract()
    (DB_DIR / "functional-builds.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(payload["counts"], indent=2))
