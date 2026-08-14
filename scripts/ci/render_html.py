"""Render the feature board as an HTML block for the landing page."""

from __future__ import annotations

import datetime
from collections.abc import Callable
from html import escape

from schema import STATE_LABELS, STATES, UPSTREAM, UPSTREAM_LABELS, sorted_for_docs

# How a `link:` naming a page below docs/ turns into a URL. The site builder
# replaces this because the documentation is a single page of anchors.
Resolver = Callable[[str], str]


def _format_date(value: str) -> str:
    return datetime.date.fromisoformat(value).strftime("%d %b %Y")


def _default_resolve(link: str) -> str:
    return f"{link[: -len('.md')]}.html"


def _href(item: dict, resolve: Resolver) -> str:
    link = item.get("link", "")
    if not link or link.startswith("https://"):
        return escape(link, quote=True)
    return escape(resolve(link), quote=True)


def _external(item: dict) -> bool:
    return item.get("link", "").startswith("https://")


def _target(item: dict) -> str:
    return ' target="_blank" rel="noopener"' if _external(item) else ""


def _feature_cell(item: dict, resolve: Resolver) -> str:
    title = escape(item["title"])
    if item.get("link"):
        head = (
            f'<a class="board-title" href="{_href(item, resolve)}"'
            f"{_target(item)}>{title}</a>"
        )
    else:
        head = f'<span class="board-title">{title}</span>'
    if item.get("help"):
        head += '<span class="board-help" title="Help wanted">Help wanted</span>'

    parts = [head]
    if item.get("notes"):
        parts.append(f'<span class="board-notes">{escape(item["notes"])}</span>')
    if item.get("help"):
        parts.append(f'<span class="board-help-text">🙋 {escape(item["help"])}</span>')

    meta = [escape(item[f]) for f in ("project", "subsystem", "version") if item.get(f)]
    if item.get("authors"):
        meta.append(", ".join(f"@{escape(a)}" for a in item["authors"]))
    if meta:
        parts.append(f'<span class="board-meta">{" · ".join(meta)}</span>')
    return "".join(parts)


def _upstream_cell(item: dict, resolve: Resolver) -> str:
    upstream = item["upstream"]
    pill = f'<span class="board-pill board-pill--{upstream}">{escape(UPSTREAM_LABELS[upstream])}</span>'
    if not item.get("link"):
        return pill
    return (
        f'<a class="board-upstream-link" href="{_href(item, resolve)}"{_target(item)}>{pill}'
        f'<span class="board-ext" aria-hidden="true">{"↗" if _external(item) else "→"}</span></a>'
    )


def _row(item: dict, resolve: Resolver) -> str:
    state = item["state"]
    return (
        f'<tr class="board-row" data-state="{state}" data-upstream="{item["upstream"]}"'
        f' data-help="{"yes" if item.get("help") else "no"}">'
        f'<td class="board-cell-feature">{_feature_cell(item, resolve)}</td>'
        f'<td class="board-cell-state">'
        f'<span class="board-pill board-pill--{state}">{escape(STATE_LABELS[state])}</span></td>'
        f'<td class="board-cell-upstream">{_upstream_cell(item, resolve)}</td>'
        f'<td class="board-cell-updated">'
        f'<time datetime="{item["updated"]}">{_format_date(item["updated"])}</time>'
        f"</td>"
        f"</tr>"
    )


def _chip(filter_value: str, label: str, count: int, extra: str = "") -> str:
    active = ' is-active" aria-pressed="true' if filter_value == "all" else '" aria-pressed="false'
    return (
        f'<button class="board-chip{extra}{active}" type="button"'
        f' data-filter="{filter_value}">{escape(label)}'
        f'<span class="board-count">{count}</span></button>'
    )


def _group(label: str, chips: list[str]) -> str:
    return (
        f'<div class="board-filter-group" role="group" aria-label="Filter by {label.lower()}">'
        f'<span class="board-filter-label">{escape(label)}</span>'
        + "".join(chips)
        + "</div>"
    )


def _filters(items: list[dict]) -> str:
    chips = [_chip("all", "All", len(items))]
    help_wanted = sum(1 for item in items if item.get("help"))
    if help_wanted:
        chips.append(_chip("help", "🙋 Help wanted", help_wanted, " board-chip--help"))
    for state in STATES:
        count = sum(1 for item in items if item["state"] == state)
        if count:
            chips.append(
                _chip(f"state:{state}", STATE_LABELS[state], count, f" board-chip--{state}")
            )

    upstream_chips = []
    for upstream in UPSTREAM:
        count = sum(1 for item in items if item["upstream"] == upstream)
        if count:
            upstream_chips.append(
                _chip(
                    f"upstream:{upstream}",
                    UPSTREAM_LABELS[upstream],
                    count,
                    f" board-chip--{upstream}",
                )
            )

    return (
        '<div class="board-filters">'
        + _group("Status", chips)
        + _group("Upstream", upstream_chips)
        + "</div>"
    )


def render(items: list[dict], resolve: Resolver = _default_resolve) -> str:
    rows = "".join(_row(item, resolve) for item in sorted_for_docs(items))
    return (
        '<div class="board">'
        + _filters(items)
        + '<div class="board-table-wrap">'
        '<table class="board-table">'
        "<thead><tr>"
        '<th scope="col">Feature</th>'
        '<th scope="col">Status</th>'
        '<th scope="col">Upstream</th>'
        '<th scope="col">Updated</th>'
        "</tr></thead>"
        f"<tbody>{rows}</tbody>"
        "</table></div>"
        '<p class="board-empty" hidden>Nothing matches this filter.</p>'
        "</div>"
    )
