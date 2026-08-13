"""Fetch complete current ability-tree and Aspect snapshots from the official API."""

from __future__ import annotations

import json
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


DB_DIR = Path(__file__).resolve().parent
CLASSES = ("archer", "warrior", "mage", "assassin", "shaman")
API_ROOT = "https://api.wynncraft.com/v3"


def fetch(path: str) -> tuple[object, dict[str, str]]:
    request = urllib.request.Request(
        f"{API_ROOT}/{path}",
        headers={"User-Agent": "WynnSolverFast-class-building-research"},
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        headers = {
            "api_version": response.headers.get("Version", "unknown"),
            "response_date": response.headers.get("Date", ""),
        }
        return json.load(response), headers


def write(name: str, endpoint_template: str) -> None:
    classes = {}
    headers = {}
    for player_class in CLASSES:
        payload, response_headers = fetch(endpoint_template.format(player_class=player_class))
        classes[player_class] = payload
        headers[player_class] = response_headers
    output = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_template": f"{API_ROOT}/{endpoint_template}",
        "authority": "official_live_api",
        "classes": classes,
        "response_headers": headers,
    }
    (DB_DIR / name).write_text(
        json.dumps(output, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    write("ability-trees.json", "ability/tree/{player_class}")
    write("aspects.json", "aspects/{player_class}")
    print("wrote ability-trees.json and aspects.json")
