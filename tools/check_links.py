#!/usr/bin/env python3
"""Check every markdown link between the repo's documents.

Reports missing target files and `#anchor`s no heading generates. Anchors
follow GitHub: lower-case, keep word characters, spaces and hyphens, then
each space to a hyphen, so " — " gives "--".

    tools/check_links.py            # repo root
    tools/check_links.py docs/      # a subtree
"""

import re
import sys
from pathlib import Path

LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*$", re.MULTILINE)
SKIP = ("http://", "https://", "mailto:")

# Directories holding game data, build output or agent worktrees, not
# documentation.
PRUNE = {"target", "local", ".git", ".claude", "tmp"}


def anchor(heading: str) -> str:
    """GitHub's slug for a heading, after stripping inline markdown."""
    text = re.sub(r"`([^`]*)`", r"\1", heading)
    text = re.sub(r"\*\*([^*]*)\*\*", r"\1", text)
    text = re.sub(r"\*([^*]*)\*", r"\1", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)

    slug = re.sub(r"[^\w\s-]", "", text.lower())

    return re.sub(r"\s", "-", slug.strip())


def anchors_of(path: Path) -> set[str]:
    try:
        body = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return set()

    found: set[str] = set()
    for _, heading in HEADING.findall(body):
        slug = anchor(heading)
        # A repeated heading gets -1, -2 ... suffixes, so accept those too.
        if slug in found:
            for n in range(1, 10):
                if f"{slug}-{n}" not in found:
                    found.add(f"{slug}-{n}")
                    break
        else:
            found.add(slug)

    return found


def markdown_files(root: Path) -> list[Path]:
    return sorted(
        p
        for p in root.rglob("*.md")
        if not PRUNE & set(p.relative_to(root).parts)
    )


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    cache: dict[Path, set[str]] = {}
    bad = 0

    for doc in markdown_files(root):
        body = doc.read_text(encoding="utf-8", errors="replace")

        for lineno, line in enumerate(body.splitlines(), 1):
            for target in LINK.findall(line):
                if target.startswith(SKIP):
                    continue

                file_part, _, frag = target.partition("#")
                dest = doc.parent / file_part if file_part else doc

                if not dest.exists():
                    print(f"{doc.relative_to(root)}:{lineno}: no such file: {target}")
                    bad += 1
                    continue

                if not frag or dest.suffix != ".md":
                    continue

                if dest not in cache:
                    cache[dest] = anchors_of(dest)

                if frag.lower() not in cache[dest]:
                    print(f"{doc.relative_to(root)}:{lineno}: no such anchor: {target}")
                    bad += 1

    print(f"\n{len(markdown_files(root))} documents, {bad} broken links")

    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
