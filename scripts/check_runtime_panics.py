#!/usr/bin/env python3
"""FakApp runtime code must be panic-free.

A watchdog that dies takes its silence with it: no unwrap/expect on Option or
Result, no panic!/unreachable!/todo! outside test modules. `unwrap_or*` is
fine — it is total and cannot panic. Test modules are exempt: scanning stops
at the first genuine test module, which is a line holding exactly
`#[cfg(test)]` directly followed by a non-blank line starting with `mod`.
Mentions of the attribute inside comments or docstrings do not count.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]

FORBIDDEN = re.compile(
    r"\.(unwrap|expect)\(|\bpanic!\s*\(|\bunreachable!\s*\(|\btodo!\s*\(|\bunimplemented!\s*\("
)
# .unwrap_or(, .unwrap_or_else(, .unwrap_or_default() never match the pattern
# above because of the word boundary + open-paren requirement.


def runtime_line_count(lines):
    """How many leading lines are runtime code (before a real test module)."""
    for index, line in enumerate(lines):
        if line.strip() != "#[cfg(test)]":
            continue
        following = next((l for l in lines[index + 1:] if l.strip()), "")
        if following.lstrip().startswith("mod "):
            return index
    return len(lines)


failures = []
for path in sorted((ROOT / "src").rglob("*.rs")):
    lines = path.read_text(encoding="utf-8").splitlines()
    for number, line in enumerate(lines[: runtime_line_count(lines)], start=1):
        if line.lstrip().startswith("//"):
            continue
        if FORBIDDEN.search(line):
            failures.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()}")

if failures:
    print("RUNTIME_PANICS=FAIL — a watchdog must not die quietly:")
    for failure in failures:
        print(f"  {failure}")
    sys.exit(1)

print("RUNTIME_PANICS=PASS unwrap/expect/panic-free on all runtime paths")
