"""MkDocs hook that expands the feature board placeholder."""

from __future__ import annotations

import sys
from pathlib import Path

# MkDocs loads hooks by path, not as a package.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from render_html import render  # noqa: E402
from schema import load_items  # noqa: E402

PAGE = "features.md"
MARKER = "<!-- FEATURE_BOARD -->"


def on_page_markdown(markdown: str, page, config, files) -> str | None:
    if page.file.src_uri != PAGE:
        return None
    if MARKER not in markdown:
        raise RuntimeError(f"{PAGE} no longer contains the {MARKER} placeholder")
    return markdown.replace(MARKER, render(load_items()))
