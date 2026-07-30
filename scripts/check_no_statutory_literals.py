#!/usr/bin/env python3
"""Enforce rule D2: no statutory constant outside `casivell-lawdata`.

Statutory figures belong in cited tables in `casivell-lawdata`, never inline in a
calculation crate. The original project specification embedded figures from four
different tax years in one file with nothing to reveal it; see
docs/ROADMAP_ERRATA.md §F2. This check makes a recurrence a build failure.

Test code is exempt. A boundary test *should* name the boundary — asserting that
12 348 EUR of income attracts no tax in 2026 is the test doing its job. So the
scanner skips `#[cfg(test)]` modules, tracking brace depth to find where each one
ends rather than assuming it runs to end-of-file.

Known limitation: brace counting does not exclude braces inside string literals or
comments. No current source file has an unbalanced brace in either, and a
miscount fails closed (reporting a hit inside a test module), which is noisy rather
than unsafe. If that ever bites, the fix is to consume the output of
`rustc --pretty=expanded` instead.

Usage
-----
    python3 scripts/check_no_statutory_literals.py
    python3 scripts/check_no_statutory_literals.py --list   # show what is watched
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Directories that must contain no statutory constants. `casivell-lawdata` is
# deliberately absent: that is where these figures are supposed to live.
SCANNED_DIRS = [
    Path("crates/casivell-core/src"),
    Path("crates/casivell-tax/src"),
]

# Figures that caused, or narrowly avoided causing, a defect recorded in the
# errata. Not an exhaustive list of German statutory constants — an exhaustive
# list is not achievable — but every one of these appearing in a calculation crate
# is a real regression of the specific mistake this project was rebuilt to fix.
WATCHED = {
    # Grundfreibetrag and tariff Eckwerte, § 32a EStG
    "12_096": "Grundfreibetrag 2025",
    "12_348": "Grundfreibetrag 2026",
    "17_443": "top of zone 2, 2025",
    "17_799": "top of zone 2, 2026",
    "68_480": "top of zone 3, 2025",
    "69_878": "top of zone 3, 2026",
    "277_825": "top of zone 4",
    "277_826": "start of zone 5",
    # Pension, SGB VI
    "43_142": "Durchschnittsentgelt as wrongly specified",
    "50_493": "Durchschnittsentgelt 2025",
    "51_944": "Durchschnittsentgelt 2026",
    "96_600": "pension contribution ceiling 2025",
    "101_400": "pension contribution ceiling 2026",
    # Solidaritaetszuschlag, SolzG
    "19_950": "Soli Freigrenze 2025",
    "20_350": "Soli Freigrenze 2026",
}

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")


def test_line_numbers(lines: list[str]) -> set[int]:
    """Return the 1-based line numbers that fall inside a `#[cfg(test)]` item."""
    exempt: set[int] = set()
    index = 0
    while index < len(lines):
        if not CFG_TEST.search(lines[index]):
            index += 1
            continue

        # Walk forward to the opening brace of the annotated item, then track depth
        # until it closes. Everything in between is test code.
        depth = 0
        started = False
        cursor = index
        while cursor < len(lines):
            exempt.add(cursor + 1)
            for char in lines[cursor]:
                if char == "{":
                    depth += 1
                    started = True
                elif char == "}":
                    depth -= 1
            if started and depth <= 0:
                break
            cursor += 1
        index = cursor + 1
    return exempt


def scan(path: Path) -> list[tuple[int, str, str]]:
    """Return (line number, matched literal, source line) for each violation."""
    lines = path.read_text(encoding="utf-8").splitlines()
    exempt = test_line_numbers(lines)
    hits: list[tuple[int, str, str]] = []
    for number, line in enumerate(lines, start=1):
        if number in exempt:
            continue
        # A figure quoted in a doc comment or a plain comment is documentation,
        # not a constant the compiler will use.
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        for literal in WATCHED:
            # Word boundaries so that 12_348 does not match 112_348.
            if re.search(rf"(?<![\w.]){re.escape(literal)}(?![\w])", line):
                hits.append((number, literal, line.strip()))
    return hits


def main() -> int:
    """Scan the calculation crates and report any statutory literal found."""
    if "--list" in sys.argv:
        print("Watched statutory literals (must appear only in casivell-lawdata):\n")
        for literal, meaning in sorted(WATCHED.items()):
            print(f"  {literal:>10}  {meaning}")
        return 0

    missing = [d for d in SCANNED_DIRS if not d.is_dir()]
    if missing:
        print(f"error: run from the repository root; not found: {missing}", file=sys.stderr)
        return 2

    violations = 0
    for directory in SCANNED_DIRS:
        for path in sorted(directory.rglob("*.rs")):
            for number, literal, source in scan(path):
                meaning = WATCHED[literal]
                print(f"{path}:{number}: statutory literal {literal} ({meaning})")
                print(f"    {source}")
                violations += 1

    if violations:
        print(
            f"\n{violations} statutory literal(s) found outside casivell-lawdata.\n"
            "Statutory figures belong in a cited table in casivell-lawdata.\n"
            "See docs/CODING_STANDARD.md rule D2.",
            file=sys.stderr,
        )
        return 1

    scanned = sum(len(list(d.rglob("*.rs"))) for d in SCANNED_DIRS)
    print(f"OK: no statutory literals outside casivell-lawdata ({scanned} files scanned).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
