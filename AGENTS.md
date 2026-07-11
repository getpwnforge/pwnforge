# AGENTS.md

PwnForge: a collaborative CTF/offensive-security platform. Rust/Axum backend, React/TypeScript/Vite frontend, self-hosted or cloud, AGPL v3. Users are security professionals who will test this adversarially. See [SECURITY.md](SECURITY.md) before touching auth, permissions, or file handling.

**You must never open, submit, or merge a Pull Request.** Only the human developer does that. Per [AI_POLICY.md](AI_POLICY.md), an AI agent may prepare code, commits, and messages, but submitting the PR itself is a human-only action, no exceptions. Everything below describes what should be true about the code and commits before the human does that, not a step the agent performs.

## Setup

```bash
docker compose -f docker-compose.dev.yml up -d
```

Never use `docker-compose.yml` for local changes: that's prod, pre-built GHCR images, no hot-reload.

## Commands

```bash
# backend/
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# frontend/
pnpm lint && pnpm typecheck && pnpm test && pnpm build
```

All eight must pass before the human submits a PR. This is exactly what `ci.yml` runs.

## Code style

- Commits: [Conventional Commits](https://www.conventionalcommits.org/), format `type(scope): summary`. Scopes: `auth teams workspaces challenges writeups ws stats ui db docker ci`.
- New primary keys: Postgres-native `uuidv7()`, not app-side UUID generation.
- Case-insensitive text (email, handle, slug): `CITEXT`, not `TEXT`.
- User-facing strings go through `t()` (i18next) even though only `en.json` exists today.
- API routes under `/api/v1/`; never break an existing v1 route.

## Testing instructions

- New endpoints need a positive test (correct role succeeds) and negative tests in `backend/tests/`. The expected status code for a negative test depends on what the requester should be allowed to know, not just whether they're blocked (see Permissions below):
  - A **member with an insufficient role** (e.g. Viewer trying to write) already knows the resource exists → `403`.
  - A **non-member** (or anyone who shouldn't know the resource exists at all) → `404`, never `403` — a `403` would confirm existence to someone who isn't supposed to know it.
- Migrations: test `down()`, not just `up()`.

## Permissions

**Allowed without asking**: reading code, running the commands above, editing existing files within `backend/` or `frontend/`.

**Ask first**: adding a new dependency, editing anything under `.github/workflows/`, changing `docker-compose*.yml`, database migrations that alter existing tables (new migrations for new tables are fine).

**Never do**: open, submit, comment as, or merge a Pull Request — that action is human-only, regardless of how much of the content you prepared; commit secrets or `.env` values; touch `docker-build.yml`'s publish/signing steps; add `Signed-off-by:` yourself (see below); write raw SQL string interpolation; put roles/permissions/PII into a JWT claim (see [SECURITY.md](SECURITY.md) §3); return `403` to a non-member where `404` is required to avoid confirming a resource exists (see [SECURITY.md](SECURITY.md) §4) — this only applies to hiding existence from someone with no access at all, not to a member who lacks a specific permission, see Testing instructions above.

## Before the human opens the PR (you don't do this step)

- Every command in the block above passes, not just for the files touched.
- If the change touches auth, permissions, file uploads, or webhooks, the matching section of [SECURITY.md](SECURITY.md) has been checked: most footguns there are non-obvious rather than things a linter catches.
- Commit message(s) follow the Conventional Commits format above.

## AI-assisted commits

If you materially wrote a commit, the human adds `Assisted-by: <tool>`. Never add `Signed-off-by:` yourself, only the human committer can certify the DCO. Full policy: [AI_POLICY.md](AI_POLICY.md).
