# Contributing to PwnForge

Thanks for considering a contribution. This document covers the practical setup and the rules for getting a PR merged.

## Developer Certificate of Origin (DCO)

We use **DCO**, not a CLA. Every commit must be signed off, certifying you have the right to submit the contribution under the project's license (AGPL v3).

```bash
git commit -s -m "fix: correct challenge status transition"
```

This appends a `Signed-off-by: Your Name <your.email@example.com>` line to the commit message. A valid email is required; pseudonyms are perfectly fine as the name.

PRs with unsigned commits will fail the DCO check and won't be merged until fixed. You can sign off retroactively with:

```bash
git rebase --signoff HEAD~<n>
```

## Local development setup

Requirements: Docker, Docker Compose, Git.

```bash
git clone https://github.com/getpwnforge/pwnforge.git
cd pwnforge
cp .env.example .env
docker compose -f docker-compose.dev.yml up -d
```

`docker-compose.dev.yml` builds from source and enables hot-reload for both backend and frontend. **Do not use `docker-compose.yml` for development.** That file pulls pre-built production images from GHCR and will not reflect your local changes.

- Backend: <http://localhost:3000>
- Frontend: <http://localhost:5173>

## Running tests

```bash
# Backend
cargo test

# Frontend
pnpm test
```

## Before opening a PR

- [ ] Commits are signed off (`-s`)
- [ ] `cargo test` and `pnpm test` pass locally
- [ ] `cargo audit` and `pnpm audit` are clean (or documented if not)
- [ ] New auth/permissions logic includes tests for the relevant role transitions
- [ ] Lockfiles (`Cargo.lock`, `pnpm-lock.yaml`) are committed if dependencies changed

## Code style

- Rust: `cargo fmt` and `cargo clippy` clean before submitting
- TypeScript/React: project ESLint/Prettier config, no `dangerouslySetInnerHTML`, no ad-hoc permission checks outside the `Role` methods

## Commit message convention

We loosely follow [Conventional Commits](https://www.conventionalcommits.org/):

```md
feat(challenges): add bulk tag assignment
fix(auth): correct refresh token reuse detection
chore(security): refresh disposable email domains
docs(readme): clarify dev vs prod compose files
```

## Reporting security issues

**Do not open a public issue for security vulnerabilities.** See [SECURITY.md](SECURITY.md) for the responsible disclosure process.

## Questions

Open a [Discussion](https://github.com/getpwnforge/pwnforge/discussions) or an issue using the appropriate template.

## AI-assisted contributions

See [AI_POLICY.md](AI_POLICY.md) for the full policy. Short version: AI tools are welcome, you must fully understand what you submit, and meaningfully AI-assisted commits get an `Assisted-by:` trailer alongside `Signed-off-by:`, never in place of it.
