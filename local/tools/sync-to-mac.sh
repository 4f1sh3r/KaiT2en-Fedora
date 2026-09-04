#!/usr/bin/env bash
set -euo pipefail

MAC_HOST="mac-t2"
LOCAL_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_PATH="/home/alex/Git/priv/kait2en/KaiT2en-Fedora"

LOCAL_BRANCH="$(git -C "$LOCAL_REPO" rev-parse --abbrev-ref HEAD)"
if [ "$LOCAL_BRANCH" = "HEAD" ]; then
  echo "✗ Lokal im detached-HEAD-Zustand, bitte erst einen Branch auschecken." >&2
  exit 1
fi

echo "→ Prüfe Branch auf $MAC_HOST ..."
MAC_BRANCH="$(ssh "$MAC_HOST" "git -C '$REMOTE_PATH' rev-parse --abbrev-ref HEAD" 2>/dev/null || echo "")"

if [ "$MAC_BRANCH" != "$LOCAL_BRANCH" ]; then
  echo "→ Mac ist auf Branch '${MAC_BRANCH:-<unbekannt>}', lokal aber auf '$LOCAL_BRANCH' – gleiche an..."
  ssh "$MAC_HOST" bash -s <<EOF
set -e
cd '$REMOTE_PATH'
git fetch origin --quiet || true
if git rev-parse --verify --quiet '$LOCAL_BRANCH' >/dev/null; then
  git checkout '$LOCAL_BRANCH'
elif git rev-parse --verify --quiet 'origin/$LOCAL_BRANCH' >/dev/null; then
  git checkout -b '$LOCAL_BRANCH' 'origin/$LOCAL_BRANCH'
else
  git checkout -b '$LOCAL_BRANCH'
fi
EOF
fi

echo "→ Synce Arbeitsbaum (inkl. uncommitteter Änderungen) nach $MAC_HOST:$REMOTE_PATH ..."
rsync -avz --delete \
  --exclude ".git/" \
  --exclude-from="$LOCAL_REPO/.gitignore" \
  "$LOCAL_REPO/" "$MAC_HOST:$REMOTE_PATH/"

echo "✅ Sync fertig. Zum Bauen/Testen einloggen mit: ssh $MAC_HOST"
