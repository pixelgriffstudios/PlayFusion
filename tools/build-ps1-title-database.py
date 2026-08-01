#!/usr/bin/env python3
"""Build a compact PlayFusion serial/name-to-title lookup table."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


SERIAL_RE = re.compile(r"[A-Z]{4}-\d{5}")


def normalized_serial(serial: str) -> str:
    return re.sub(r"[^a-z0-9]", "", serial.lower())


def normalized_title(title: str) -> str:
    base = re.sub(r"\s+\(.*$", "", title)
    return "name" + re.sub(r"[^a-z0-9]", "", base.lower())


def parse_dat(path: Path, platform: str) -> dict[str, str]:
    titles: dict[str, str] = {}
    current_name: str | None = None

    with path.open("r", encoding="utf-8-sig", errors="replace") as source:
        for raw_line in source:
            line = raw_line.rstrip("\r\n")
            if line == "game (":
                current_name = None
                continue
            if current_name is None:
                match = re.match(r'^\s*name "(.*)"\s*$', line)
                if match:
                    current_name = (
                        match.group(1)
                        .replace(r"\"", '"')
                        .replace(r"\\", "\\")
                        .replace("\t", " ")
                    )
                continue
            if re.match(r"^\s*serial ", line):
                for serial in SERIAL_RE.findall(line):
                    titles.setdefault(normalized_serial(serial), current_name)
                serial_value = line.split('"', 2)[1]
                for serial in re.findall(r"[A-Z0-9]+-[A-Z0-9()\-]+", serial_value):
                    titles.setdefault(normalized_serial(serial), current_name)
                    if platform == "dreamcast":
                        titles.setdefault(
                            normalized_serial(serial.split("(", 1)[0]), current_name
                        )
                if platform in {"wii", "gamecube"}:
                    match = re.search(
                        rf"(?:RVL|DOL)-([A-Z0-9]{{4}})", serial_value
                    )
                    if match:
                        titles.setdefault(normalized_serial(match.group(1)), current_name)
                titles.setdefault(normalized_title(current_name), current_name)

    return titles


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--platform", default="ps1")
    args = parser.parse_args()

    titles = parse_dat(args.source, args.platform)
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    with args.destination.open("w", encoding="utf-8", newline="\n") as output:
        output.write(
            "# Generated from libretro-database's Sony - PlayStation DAT.\n"
            "# Source: https://github.com/libretro/libretro-database\n"
            "# License: CC-BY-SA-4.0\n"
        )
        for serial, title in sorted(titles.items()):
            output.write(f"{serial}\t{title}\n")

    print(f"Wrote {len(titles)} {args.platform} metadata keys to {args.destination}")


if __name__ == "__main__":
    main()
