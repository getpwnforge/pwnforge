#!/usr/bin/env bash
#
# Resolves `uses: owner/repo@vX.Y.Z` references in .github/workflows/*.yml to full
# commit SHAs, and rewrites them in place as `uses: owner/repo@<sha> # vX.Y.Z`.
#
# Requires: GitHub CLI (`gh`), authenticated (`gh auth login`).
# Usage:
#   ./scripts/pin-actions.sh              # pin every unpinned action in .github/workflows/
#   ./scripts/pin-actions.sh ci.yml       # pin only that file

set -uo pipefail

WORKFLOW_DIR=".github/workflows"

if ! command -v gh >/dev/null 2>&1; then
  echo "error: GitHub CLI (gh) is required. Install: https://cli.github.com" >&2
  exit 1
fi

if ! command -v awk >/dev/null 2>&1; then
  echo "error: awk is required (should be preinstalled on macOS/Linux)." >&2
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "error: gh is not authenticated. Run 'gh auth login' first." >&2
  exit 1
fi

resolve_sha() {
  local repo="$1" ref="$2"
  gh api "repos/${repo}/commits/${ref}" --jq '.sha'
}

# Literal (non-regex) find-and-replace of the first occurrence of $2 with $3 on each
# line of $1, then strips $4 (also literal) if present on the resulting line.
# Using awk's index()/substr() instead of sed avoids all delimiter/regex-escaping
# issues (sed's behavior here differs annoyingly between GNU and BSD/macOS sed).
literal_replace_in_file() {
  local file="$1" old="$2" new="$3" strip="$4"
  awk -v old="$old" -v new="$new" -v strip="$strip" '
    {
      line = $0
      i = index(line, old)
      if (i > 0) {
        line = substr(line, 1, i - 1) new substr(line, i + length(old))
        j = index(line, strip)
        if (j > 0) {
          line = substr(line, 1, j - 1) substr(line, j + length(strip))
        }
      }
      print line
    }
  ' "$file" > "${file}.tmp" && mv "${file}.tmp" "$file"
}

pin_file() {
  local file="$1"

  if [[ ! -f "$file" ]]; then
    echo "error: ${file} not found" >&2
    return 1
  fi

  echo "== ${file} =="

  local refs
  refs="$(grep -ohE 'uses: [A-Za-z0-9._-]+/[A-Za-z0-9._-]+@[A-Za-z0-9._/-]+' "$file" \
    | sed 's/^uses: //' | sort -u)"

  if [[ -z "$refs" ]]; then
    echo "  (no action references found)"
    return 0
  fi

  while IFS= read -r ref_full; do
    [[ -z "$ref_full" ]] && continue

    repo="${ref_full%@*}"
    tag="${ref_full##*@}"

    if [[ "$tag" =~ ^[0-9a-f]{40}$ ]]; then
      echo "  ${repo}@${tag} already pinned, skipping"
      continue
    fi

    sha_output="$(resolve_sha "$repo" "$tag" 2>&1)"
    status=$?

    if [[ $status -ne 0 || -z "$sha_output" || "$sha_output" == "null" ]]; then
      echo "  ! could not resolve ${repo}@${tag} — skipping. gh said: ${sha_output}"
      continue
    fi

    sha="$sha_output"
    echo "  ${repo}@${tag} -> ${sha}"
    literal_replace_in_file "$file" \
      "uses: ${repo}@${tag}" \
      "uses: ${repo}@${sha} # ${tag}" \
      " # TODO: pin to commit SHA, see scripts/pin-actions.sh"
  done <<< "$refs"
}

if [[ $# -gt 0 ]]; then
  for f in "$@"; do
    pin_file "${WORKFLOW_DIR}/${f}"
  done
else
  for f in "${WORKFLOW_DIR}"/*.yml; do
    pin_file "$f"
  done
fi

echo
echo "Done. Review the diff (git diff .github/workflows/) before committing."
echo "Note: dtolnay/rust-toolchain@stable resolves fine here — pinning the action's own"
echo "commit doesn't lock the Rust toolchain version, since that's resolved at runtime"
echo "against rust-lang's release manifest, not baked into the action's code."