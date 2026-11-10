#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = ["pillow", "platformdirs"]
# ///
"""Cyberlore's unit cards as openregent portrait overrides.

Downloads each card, crops its portrait and writes it where openregent's
override folder reads it, named as `regent_cli ui-dump` names the portrait:

    <out>/portraits/<archive>/<IMAG>_1000_s0_f0.png

Set 1000 is a unit's selection portrait; the list icon is drawn from it.
A crop is two corners, top left and bottom right, (0, 0) the card's top
left; both pixels are inside it. It should be square: the game draws it
into a 100x100 rectangle.

Usage:

    uv run tools/cards.py [out]

`out` defaults to openregent's art folder in the user data folder:
`~/.local/share/openregent/art` on Linux, `~/Library/Application
Support/openregent/art` on macOS, `%APPDATA%\\openregent\\art` on Windows.
Downloads are kept in `<out>/../cards/` and fetched once.
"""

import io
import sys
import urllib.request
from pathlib import Path

import platformdirs
from PIL import Image

URL = "https://www.cyberlore.com/majesty/downloads/card-{}+.jpg"

# Every card is this size.
CARD = (840, 1110)

# The set a unit's selection portrait is in (`INTERFACE_PORTRAIT`).
PORTRAIT = 1000

# Expansion archive. Its copies of a record win when the expansion is loaded.
MX = "mx_maindata"

# The Wrath of Krolm quest's archive (`Quests/WrathOfKrolm.mqxml`).
KROLM = "mdl1_maindata"

# card name: (crop ((x, y), (x, y)) in card pixels, IMAG, archives)
# Fill in each crop; a card with `None` is skipped.
CARDS = {
    "adept": (((120,145), (619,644)), "AVA1", ["maindata", MX]),
    "barbarian": (((122,145), (621,644)), "AVB1", ["maindata", MX]),
    "cultist": (((122,145), (621,644)), "AVC1", ["maindata"]),
    "dirgo": (((120,145), (619,644)), "BVd1", ["maindata"]),
    "dwarf": (((122,145), (621,644)), "AVc1", ["maindata"]),
    "elf": (((122,145), (621,644)), "AVd1", ["maindata"]),
    "gnome": (((122,145), (621,644)), "AVe1", ["maindata"]),
    "healer": (((120,145), (619,644)), "AVD1", ["maindata"]),
    "krolm": (((120,145), (619,644)), "BV01", [KROLM]),
    "monk": (((120,146), (619,645)), "AVE1", ["maindata"]),
    "paladin": (((120,143), (619,642)), "AVF1", ["maindata"]),
    "phantom": (((120,143), (619,642)), "BVa1", ["maindata"]),
    "priestess": (((122,147), (621,646)), "AVG1", ["maindata"]),
    "ranger": (((122,149), (621,648)), "AVH1", ["maindata"]),
    "rhoden": (((120,145), (619,644)), "BVt1", [MX]),
    "rogue": (((121,145), (620,644)), "AVJ1", ["maindata"]),
    "solarus": (((120,146), (619,645)), "AVK1", ["maindata"]),
    "tax-collector": (((120,145), (619,644)), "AVg1", ["maindata"]),
    "vendral": (((120,145), (619,644)), "BVg1", ["maindata"]),
    "warrior": (((122,149), (621,648)), "AVL1", ["maindata"]),
    "wizard": (((122,147), (621,646)), "AVN1", ["maindata"]),
    "wod": ( ((120,143), (619,642)), "AVM1", ["maindata", MX]),
}

# The card back has no portrait to replace.


def default_out():
    """openregent's art folder, as the `dirs` crate's `data_dir` finds it.

    `roaming` picks `%APPDATA%` on Windows, as `dirs` does; no `appauthor`
    keeps a second `openregent` level out of the path.
    """
    data = platformdirs.user_data_dir("openregent", appauthor=False, roaming=True)

    return Path(data) / "art"


def card(name, cache):
    """The card's pixels, from the cache or downloaded into it."""
    path = cache / f"card-{name}+.jpg"

    if not path.exists():
        with urllib.request.urlopen(URL.format(name)) as response:
            body = response.read()

        cache.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)

    image = Image.open(io.BytesIO(path.read_bytes())).convert("RGB")

    if image.size != CARD:
        sys.exit(f"{path}: {image.size}, expected {CARD}")

    return image


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else default_out()
    cache = out.parent / "cards"
    filled = {k: v for k, v in CARDS.items() if v[0] is not None}

    for name, (crop, imag, archives) in filled.items():
        (left, top), (right, bottom) = crop
        portrait = card(name, cache).crop((left, top, right + 1, bottom + 1))

        if portrait.size[0] != portrait.size[1]:
            print(f"{name}: {portrait.size[0]}x{portrait.size[1]}, not square")

        for archive in archives:
            path = out / "portraits" / archive / f"{imag}_{PORTRAIT}_s0_f0.png"
            path.parent.mkdir(parents=True, exist_ok=True)
            portrait.save(path)
            print(f"{name}: {path} {portrait.size[0]}x{portrait.size[1]}")

    skipped = sorted(CARDS.keys() - filled.keys())

    if skipped:
        print(f"no crop, skipped: {', '.join(skipped)}")


if __name__ == "__main__":
    main()
