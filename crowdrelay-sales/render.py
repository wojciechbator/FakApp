#!/usr/bin/env python3
"""Render an offer HTML file to PDF via headless Chromium.

Usage: python3 render.py offers/<file>.html [--out out.pdf]
"""
import argparse
import pathlib
import sys

from playwright.sync_api import sync_playwright


def main() -> int:
    parser = argparse.ArgumentParser(description="Render offer HTML to PDF")
    parser.add_argument("input", type=pathlib.Path, help="offer .html file")
    parser.add_argument("--out", type=pathlib.Path, help="output .pdf path")
    args = parser.parse_args()

    src = args.input.expanduser().resolve()
    if not src.is_file():
        print(f"not a file: {src}", file=sys.stderr)
        return 1
    if src.suffix.lower() != ".html":
        print(f"not an .html file: {src}", file=sys.stderr)
        return 1

    out = (args.out or src.with_suffix(".pdf")).expanduser().resolve()
    if out.is_dir():
        print(f"output path is a directory: {out}", file=sys.stderr)
        return 1
    out.parent.mkdir(parents=True, exist_ok=True)
    url = src.as_uri()

    with sync_playwright() as p:
        browser = p.chromium.launch()
        try:
            page = browser.new_page()
            page.goto(url, wait_until="networkidle")
            page.wait_for_load_state("load")
            page.emulate_media(media="print")
            page.wait_for_timeout(300)  # let fonts settle
            page.pdf(
                path=str(out),
                format="A4",
                print_background=True,
                margin={"top": "0", "right": "0", "bottom": "0", "left": "0"},
            )
        finally:
            browser.close()

    size = out.stat().st_size
    print(f"{out} ({size/1024:.0f} KiB)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
