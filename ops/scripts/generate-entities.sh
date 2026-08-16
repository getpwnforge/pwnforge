#!/usr/bin/env bash
#
# Regenerates the SeaORM entities from the live database schema, then reapplies
# the manual patches the code generator gets wrong.
#
# Every patch below is a workaround, not a preference. Do not edit the files in
# crates/domain/src/entities by hand: this script is the single source of truth
# for those corrections, and any manual edit is lost on the next run.
#
# Usage (from anywhere):
#   ./ops/scripts/generate-entities.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
BACKEND_DIR="${REPO_ROOT}/backend"
ENTITIES_DIR="${BACKEND_DIR}/crates/domain/src/entities"

cd "${BACKEND_DIR}"

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

if [[ -z "${DATABASE_URL:-}" ]]; then
    if [[ -f "${REPO_ROOT}/.env" ]]; then
        set -a
        # shellcheck disable=SC1091
        source "${REPO_ROOT}/.env"
        set +a
    fi
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "error: DATABASE_URL is not set and backend/.env did not provide it" >&2
    exit 1
fi

# The compose service name is not resolvable from the host. Fail early with a
# clear message rather than on a connection timeout.
if [[ "${DATABASE_URL}" == *"@db:"* ]]; then
    echo "error: DATABASE_URL points at the compose service host 'db'." >&2
    echo "       Use 'localhost' when running this script from the host." >&2
    exit 1
fi

command -v sea-orm-cli >/dev/null 2>&1 || {
    echo "error: sea-orm-cli not found. Install it with:" >&2
    echo "       cargo install sea-orm-cli --locked" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Generation
# ---------------------------------------------------------------------------

echo "==> Generating entities from ${DATABASE_URL%%\?*}"

sea-orm-cli generate entity \
    -o "${ENTITIES_DIR}" \
    --database-url "${DATABASE_URL}" \
    --with-serde none

# Older invocations used --lib, which writes lib.rs instead of mod.rs. Handle
# both so the module tree stays valid either way.
if [[ -f "${ENTITIES_DIR}/lib.rs" ]]; then
    mv "${ENTITIES_DIR}/lib.rs" "${ENTITIES_DIR}/mod.rs"
fi

# ---------------------------------------------------------------------------
# Patch 1: citext columns are emitted with `ignore`
# ---------------------------------------------------------------------------
# The generator marks every citext column with `ignore`, which excludes the
# field from the Entity entirely: no Column variant, no ActiveModel field. It
# also omits `save_as = "citext"`, so writes are not cast and fail at runtime
# with "mismatched types ... TEXT is not compatible with citext".
#
# Upstream issue: <PASTE ISSUE URL HERE ONCE POSTED>
# Related: SeaQL/sea-orm#1643, SeaQL/sea-orm#2081
#
# This is generic: it covers every citext column, including the slugs added in
# later phases (teams, workspaces, tags, categories).

echo "==> Patch 1: removing 'ignore' and adding 'save_as' on citext columns"

find "${ENTITIES_DIR}" -name '*.rs' -type f -print0 |
    while IFS= read -r -d '' file; do
        # The attribute may be emitted on one line or split across several, so
        # the whole #[sea_orm(...)] block is matched rather than a single line.
        perl -0777 -pi -e '
          s{(\#\[sea_orm\(.*?\)\])}{
              my $b = $1;
              if ($b =~ /citext/) {
                  $b =~ s/^[ \t]*ignore,[ \t]*\n//m;
                  $b =~ s/\bignore,[ \t]*//;
                  $b =~ s/(select_as\s*=\s*"text")/$1, save_as = "citext"/ unless $b =~ /save_as/;
              }
              $b;
          }gse;
        ' "${file}"
    done

# ---------------------------------------------------------------------------
# Patch 2: spurious `unique` on user_emails.user_id
# ---------------------------------------------------------------------------
# The partial unique index on (user_id) WHERE is_primary is read as a plain
# unique constraint, so the generator models the relation as one to one. A user
# can have several emails, only one of which is primary.

echo "==> Patch 2: removing spurious 'unique' on user_emails.user_id"

perl -0777 -pi -e 's/^[ \t]*#\[sea_orm\(unique\)\]\n(?=[ \t]*pub user_id: Uuid,)//m' \
    "${ENTITIES_DIR}/user_emails.rs"

# ---------------------------------------------------------------------------
# Patch 3: users -> user_emails is has_many, not has_one
# ---------------------------------------------------------------------------
# Same root cause as patch 2, on the relation side this time.

echo "==> Patch 3: has_one -> has_many on users.user_emails"

sed -i 's/has_one = "super::user_emails::Entity"/has_many = "super::user_emails::Entity"/' \
    "${ENTITIES_DIR}/users.rs"

# ---------------------------------------------------------------------------
# Patch 4: readable relation names on instance_audit_logs
# ---------------------------------------------------------------------------
# The table has two foreign keys to users, which the generator names Users1 and
# Users2 in an order that is the opposite of what the names suggest. This is
# cosmetic but the confusion is a real source of bugs when writing audit joins.
#
# Note there is deliberately no Related impl on this entity: with two paths to
# the same target, SeaORM cannot pick one. Use explicit joins:
#   .join(JoinType::LeftJoin, Relation::Actor.def())

echo "==> Patch 4: renaming audit log relations to Actor / TargetUser"

AUDIT_FILE="${ENTITIES_DIR}/instance_audit_logs.rs"

if grep -q 'Users1' "${AUDIT_FILE}" || grep -q 'Users2' "${AUDIT_FILE}"; then
    sed -i 's/\bUsers1\b/TargetUser/g; s/\bUsers2\b/Actor/g' "${AUDIT_FILE}"
else
    echo "    warning: Users1 / Users2 not found. The generator may have changed" >&2
    echo "             its naming. Check the relation order in ${AUDIT_FILE}." >&2
fi

# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------

echo "==> Verifying"

if grep -rn 'ignore' "${ENTITIES_DIR}" >/dev/null 2>&1; then
    echo "    warning: 'ignore' still present in the generated entities:" >&2
    grep -rn 'ignore' "${ENTITIES_DIR}" >&2
    echo "             A new custom column type may need a patch above." >&2
fi

cargo check -p domain

echo
echo "Done. Review the diff before committing:"
echo "  git diff --stat crates/domain/src/entities"
