#!/usr/bin/env bash
# ops/scripts/refresh-disposable-domains.sh
# Run from the repo root. Fetches the upstream disposable-email-domains list,
# validates it, and replaces backend/data/disposable_email_blocklist.conf.
# A rebuild/redeploy is still required afterwards: the file is embedded at
# compile time via `include_str!` in crates/seed/src/lib.rs, not read at
# runtime by the running backend.

set -euo pipefail

LIST_URL="https://raw.githubusercontent.com/disposable-email-domains/disposable-email-domains/main/disposable_email_blocklist.conf"
TARGET="backend/data/disposable_email_blocklist.conf"
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

echo "Fetching latest disposable domains list..."
if ! curl -sSf "$LIST_URL" -o "$TMP_FILE"; then
  echo "error: failed to fetch list from $LIST_URL" >&2
  exit 1
fi

# Sanity check on count: catches a truncated download or an upstream format
# change before it ever reaches the repo.
LINE_COUNT=$(grep -cv -e '^[[:space:]]*$' -e '^[[:space:]]*#' "$TMP_FILE")

if [ "$LINE_COUNT" -lt 3000 ] || [ "$LINE_COUNT" -gt 10000 ]; then
  echo "error: fetched list has $LINE_COUNT entries, expected between 3000 and 10000 — refusing to replace" >&2
  exit 1
fi

# Sanity check on format: every non-blank, non-comment line should look like
# a bare domain, nothing else.
if grep -qvE '^[[:space:]]*(#.*)?$|^[a-z0-9.-]+\.[a-z]{2,}[[:space:]]*$' "$TMP_FILE"; then
  echo "error: fetched list contains lines that don't look like domains — refusing to replace" >&2
  exit 1
fi

if diff -q "$TMP_FILE" "$TARGET" >/dev/null 2>&1; then
  echo "Disposable email domains list is already up-to-date. No changes made."
  exit 0
fi

echo "Disposable email domains list has changed ($LINE_COUNT entries). Updating $TARGET..."
cp "$TMP_FILE" "$TARGET"

echo "Do you want to commit the updated list to the repo? (y/N)"
read -r answer
if [[ "$answer" =~ ^[Yy]$ ]]; then
  git add "$TARGET"
  git commit -m "chore(security): refresh disposable email domains"
else
  echo "Not committing changes. You can commit manually later if desired."
fi

echo "Done. Rebuild and redeploy the backend to embed the updated list."
