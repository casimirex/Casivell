#!/usr/bin/env python3
"""Structural accessibility checks for the browser build.

A real audit needs axe-core and a headless browser, and this repository has no external
dependencies. What can be checked without one is the class of failure that is *structural* —
visible in the markup, and the kind that gets reintroduced by a well-meaning edit months later.

The list below is what an audit of this page actually found, turned into assertions:

  1. A click handler on a non-interactive element. The explainability panel was opened by a
     handler on `<tr>`, which no keyboard user could reach — WCAG 2.1.1 Level A, and the most
     serious defect the page had.
  2. A disclosure without `aria-expanded`, which leaves a screen reader unable to tell whether
     the panel is open.
  3. A table without a caption, or headers without `scope`.
  4. Results that change with no live region, so a screen reader user hears nothing.
  5. Interactive controls with no visible focus style.
  6. A missing `lang`, or a control with no accessible name.

It cannot check colour contrast, focus order, or whether the prose makes sense. Those are
named in `web/README.md` as reviewed by hand.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAGE = ROOT / "web" / "index.html"


def check(html: str) -> list[str]:
    problems: list[str] = []

    # 1. Handlers must be attached to interactive elements. `closest("tr...")` in a click
    #    handler is the exact shape of the defect this page had.
    for match in re.finditer(r'closest\((["\'])([^"\']+)\1\)', html):
        selector = match.group(2)
        if not re.match(r"^\s*(button|a|input|select|textarea|\[role)", selector):
            problems.append(
                f"a click handler resolves to {selector!r}, which is not an interactive "
                "element — keyboard users cannot reach it (WCAG 2.1.1)"
            )

    # 2. A disclosure control must carry its state.
    if "class=\"why\"" in html and "aria-expanded" not in html:
        problems.append("the explainability control has no aria-expanded")

    # 3. Tables need a caption, and header cells a scope.
    for table in re.findall(r"<table>(.*?)</table>", html, re.S):
        if "<caption" not in table:
            problems.append("a table has no <caption>")
        for header in re.findall(r"<th\b[^>]*>", table):
            if "scope=" not in header:
                problems.append(f"a header cell has no scope: {header.strip()}")

    # 4. Figures that change need announcing.
    if 'aria-live' not in html:
        problems.append("results change with no aria-live region")

    # 5. Custom-styled controls need a visible focus ring.
    if ":focus-visible" not in html:
        problems.append("no :focus-visible style, so keyboard focus may be invisible")

    # 6. The basics.
    if not re.search(r'<html[^>]+\blang=', html):
        problems.append("<html> has no lang attribute")
    for control in re.findall(r"<button\b[^>]*>(.*?)</button>", html, re.S):
        text = re.sub(r"<[^>]+>", "", control).strip()
        if not text:
            problems.append("a <button> has no accessible name")

    return problems


def main() -> int:
    if not PAGE.exists():
        print(f"{PAGE} not found", file=sys.stderr)
        return 2

    problems = check(PAGE.read_text(encoding="utf-8"))
    if problems:
        print("Accessibility problems in web/index.html:\n", file=sys.stderr)
        for problem in sorted(set(problems)):
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print("OK: no structural accessibility problems in web/index.html.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
