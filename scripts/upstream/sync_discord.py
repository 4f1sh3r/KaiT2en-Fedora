#!/usr/bin/env python3
"""Announce the upstream submissions from data/features.yml in Discord.

One webhook message per submission; the message id is remembered in
data/discord-state.json so a status change edits the existing message instead
of posting a new one.

Usage:
    DISCORD_UPSTREAM_WEBHOOK=https://discord.com/api/webhooks/... \\
        python3 scripts/upstream/sync_discord.py [--dry-run]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from render_discord import render  # noqa: E402
from schema import SUBMITTED_UPSTREAM, DATA_FILE, UpstreamDataError, load_items  # noqa: E402

STATE_FILE = DATA_FILE.parent / "discord-state.json"
STATE_VERSION = 1

WEBHOOK_PREFIXES = ("https://discord.com/api/webhooks/", "https://discordapp.com/api/webhooks/")

REQUEST_PAUSE = 0.6
MAX_ATTEMPTS = 5


class SyncError(Exception):
    pass


def _redact(url: str) -> str:
    head, _, _ = url.rpartition("/")
    return f"{head}/<token>"


def _request(url: str, payload: dict, method: str) -> dict:
    body = json.dumps(payload).encode("utf-8")
    for attempt in range(1, MAX_ATTEMPTS + 1):
        request = urllib.request.Request(
            url,
            data=body,
            method=method,
            headers={
                "Content-Type": "application/json",
                "User-Agent": "KaiT2en-upstream-sync/1.0 (+https://github.com/kaiT2en/KaiT2en-Fedora)",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                raw = response.read().decode("utf-8")
                return json.loads(raw) if raw else {}
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", "replace")[:400]
            if exc.code == 429 and attempt < MAX_ATTEMPTS:
                wait = 5.0
                try:
                    wait = float(json.loads(detail).get("retry_after", wait))
                except (ValueError, AttributeError):
                    pass
                print(f"  rate limited, retrying in {wait:.1f}s", flush=True)
                time.sleep(min(wait, 60.0) + 0.25)
                continue
            if exc.code >= 500 and attempt < MAX_ATTEMPTS:
                time.sleep(2.0 * attempt)
                continue
            raise SyncError(
                f"{method} {_redact(url)} failed with HTTP {exc.code}: {detail}"
            ) from None
        except urllib.error.URLError as exc:
            if attempt < MAX_ATTEMPTS:
                time.sleep(2.0 * attempt)
                continue
            raise SyncError(f"{method} {_redact(url)} failed: {exc.reason}") from None
    raise SyncError(f"{method} {_redact(url)} failed after {MAX_ATTEMPTS} attempts")


def _payload(content: str) -> dict:
    # The @handles are plain-text GitHub names and must never become real pings.
    return {"content": content, "allowed_mentions": {"parse": []}}


def post(webhook: str, content: str) -> str:
    separator = "&" if "?" in webhook else "?"
    result = _request(f"{webhook}{separator}wait=true", _payload(content), "POST")
    message_id = result.get("id")
    if not message_id:
        raise SyncError("Discord accepted the message but returned no message id")
    return str(message_id)


def patch(webhook: str, message_id: str, content: str) -> bool:
    """Return False if the message is gone and has to be posted again."""
    try:
        _request(f"{webhook}/messages/{message_id}", _payload(content), "PATCH")
        return True
    except SyncError as exc:
        if "HTTP 404" in str(exc):
            return False
        raise


def load_state() -> dict:
    if not STATE_FILE.exists():
        return {"version": STATE_VERSION, "messages": {}}
    try:
        state = json.loads(STATE_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SyncError(f"{STATE_FILE.name} is not valid JSON: {exc}") from None
    if state.get("version") != STATE_VERSION:
        raise SyncError(
            f"{STATE_FILE.name} has version {state.get('version')}, expected {STATE_VERSION}"
        )
    state.setdefault("messages", {})
    return state


def save_state(state: dict, order: list[str]) -> None:
    messages = state["messages"]
    ranked = sorted(messages, key=lambda i: (order.index(i) if i in order else len(order), i))
    state["messages"] = {key: messages[key] for key in ranked}
    STATE_FILE.write_text(json.dumps(state, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def sync(webhook: str, dry_run: bool) -> int:
    items = load_items()
    state = load_state()
    messages = state["messages"]

    created = updated = unchanged = 0
    changed_state = False

    for item in items:
        item_id = item["id"]
        # The channel announces submissions, not the board. Everything else is
        # documentation-site only, unless it has already been announced once.
        if item["upstream"] not in SUBMITTED_UPSTREAM and item_id not in messages:
            continue
        content = render(item)
        digest = hashlib.sha256(content.encode("utf-8")).hexdigest()
        known = messages.get(item_id)

        if known and known.get("hash") == digest:
            unchanged += 1
            continue

        action = "update" if known else "post"
        print(f"{'would ' if dry_run else ''}{action}: {item_id} [{item['upstream']}]", flush=True)
        if dry_run:
            created += action == "post"
            updated += action == "update"
            continue

        if known:
            if patch(webhook, known["message_id"], content):
                messages[item_id] = {"message_id": known["message_id"], "hash": digest}
                updated += 1
            else:
                print(f"  message {known['message_id']} is gone, posting a new one", flush=True)
                messages[item_id] = {"message_id": post(webhook, content), "hash": digest}
                created += 1
        else:
            messages[item_id] = {"message_id": post(webhook, content), "hash": digest}
            created += 1

        changed_state = True
        time.sleep(REQUEST_PAUSE)

    known_ids = {item["id"] for item in items}
    for orphan in sorted(set(messages) - known_ids):
        print(
            f"warning: '{orphan}' has a Discord message ({messages[orphan]['message_id']}) "
            f"but is no longer in {DATA_FILE.name}; the message is left untouched",
            file=sys.stderr,
        )

    if changed_state and not dry_run:
        save_state(state, [item["id"] for item in items])

    print(f"done: {created} posted, {updated} updated, {unchanged} unchanged")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="report what would be posted or edited without calling Discord",
    )
    args = parser.parse_args()

    webhook = os.environ.get("DISCORD_UPSTREAM_WEBHOOK", "").strip()
    if not webhook:
        if args.dry_run:
            webhook = "https://discord.com/api/webhooks/0/dry-run"
        else:
            print("error: DISCORD_UPSTREAM_WEBHOOK is not set", file=sys.stderr)
            return 1
    elif not webhook.startswith(WEBHOOK_PREFIXES):
        print(
            "error: DISCORD_UPSTREAM_WEBHOOK does not look like a Discord webhook URL",
            file=sys.stderr,
        )
        return 1

    try:
        return sync(webhook.rstrip("/"), args.dry_run)
    except (SyncError, UpstreamDataError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
