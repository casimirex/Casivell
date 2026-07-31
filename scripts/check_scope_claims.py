#!/usr/bin/env python3
"""Fails if a crate's documentation claims to lack something the crate implements.

Casivell's crate docs carry a "Scope" or "Not modelled" section, and those sections are the
honest counterweight to everything else the documentation claims. They also drift: three of
them silently went stale while features were added underneath, and nothing caught it — the
docs said the Faktorverfahren, Elterngeld, the annual assessment, buying property and
außergewöhnliche Belastungen were all absent, long after each had been built.

A doc that understates the engine is a smaller sin than one that overstates it, but it is
still wrong, and it erodes the reason to believe the *other* claims in the same paragraph.

The check is deliberately narrow. It knows a fixed list of features, each with a phrase that
would appear in a "not modelled" sentence and a symbol whose presence proves the feature
exists. If both are found, the claim is stale. It cannot detect a feature nobody listed here,
which is why the list is part of the check rather than derived: adding a feature means adding
its entry, and that is the moment to look at the scope sections.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"

# (feature, phrase that would appear in an absence claim, symbol proving it exists)
FEATURES: list[tuple[str, str, str]] = [
    ("Faktorverfahren (§ 39f)", "Faktorverfahren", "pub fn faktorverfahren"),
    ("Außergewöhnliche Belastungen (§§ 33, 33b)", "Außergewöhnliche Belastungen",
     "pub fn extraordinary_burden"),
    ("Progressionsvorbehalt (§ 32b)", "Progressionsvorbehalt", "pub fn progression_tax"),
    ("Capital income (§ 32d)", "Kapitalerträge", "pub fn capital_income_tax"),
    ("Elterngeld (BEEG)", "Elterngeld", "pub fn elterngeld"),
    ("The annual assessment", "annual assessment", "pub fn assess"),
    ("Buying property", "Buying property", "PropertyPurchase {"),
    ("Kindererziehungszeiten (§ 56 SGB VI)", "Kindererziehungszeiten", "pub const fn child_raising"),
]

# A sentence claiming absence looks like one of these. Kept loose: the point is to notice a
# stale claim, and a false positive costs one reading of a paragraph.
ABSENCE = re.compile(
    r"not (modelled|implemented)|deliberately absent|is absent|are absent|not available|"
    r"not yet|is not covered",
    re.IGNORECASE,
)


def doc_lines(path: Path) -> list[str]:
    """The `//!` and `///` lines of a source file, which is where scope claims live."""
    return [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip().startswith("//!") or line.strip().startswith("///")
    ]


def main() -> int:
    sources = sorted(CRATES.glob("*/src/**/*.rs"))
    if not sources:
        print("no sources found; is this the right directory?", file=sys.stderr)
        return 2

    everything = "\n".join(p.read_text(encoding="utf-8") for p in sources)
    implemented = {
        feature for feature, _, symbol in FEATURES if symbol in everything
    }

    stale: list[str] = []
    for path in sources:
        lines = doc_lines(path)
        for index, line in enumerate(lines):
            if not ABSENCE.search(line):
                continue
            # A claim may run over two lines, so look at this one and the next.
            window = line + " " + (lines[index + 1] if index + 1 < len(lines) else "")
            for feature, phrase, _ in FEATURES:
                if feature in implemented and phrase.lower() in window.lower():
                    stale.append(
                        f"{path.relative_to(ROOT)}: claims {feature} is absent, but it is "
                        f"implemented\n    {line}"
                    )

    if stale:
        print("Stale scope claims — the docs understate what the engine does:\n", file=sys.stderr)
        for entry in stale:
            print(f"  {entry}\n", file=sys.stderr)
        return 1

    print(
        f"OK: no stale scope claims ({len(sources)} files, "
        f"{len(implemented)}/{len(FEATURES)} tracked features implemented)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
