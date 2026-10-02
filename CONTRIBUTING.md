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

# The bundled object store needs its own config, which is gitignored because
# it holds the cluster's admin credentials. Generate the two secrets:
cp ops/garage/garage.toml.example ops/garage/garage.toml
openssl rand -hex 32   # -> rpc_secret
openssl rand -hex 32   # -> admin_token

docker compose -f docker-compose.dev.yml up -d
```

The object store (`garage`, `garage-init`) sits behind the `garage` Compose
profile, which `.env.example` enables by default. If you already run an S3,
clear `COMPOSE_PROFILES` and fill in the `PWNFORGE_S3_*` variables instead -
no `garage.toml` is needed then.

`docker-compose.dev.yml` builds from source and enables hot-reload for both backend and frontend. **Do not use `docker-compose.yml` for development.** That file pulls pre-built production images from GHCR and will not reflect your local changes.

- Backend: <http://localhost:3000>
- Frontend: <http://localhost:5173>
- Object store (S3 API): <http://127.0.0.1:3900>

### Inspecting object storage

Garage has no web console - this is the one thing lost in the move off MinIO
(see [ADR 0002](docs/adr/0002-replacing-minio.md)). Two ways to look
inside a local instance.

**The Garage CLI**, through the running container. The image is `FROM scratch`,
so there is no shell: invoke the binary directly.

```bash
# Cluster health, node id, disk available
docker compose -f docker-compose.dev.yml exec garage /garage status

# Buckets, their size and object count
docker compose -f docker-compose.dev.yml exec garage /garage bucket list
docker compose -f docker-compose.dev.yml exec garage /garage bucket info pwnforge-avatars

# Which keys can reach what
docker compose -f docker-compose.dev.yml exec garage /garage key list

# Cap a bucket's size. Refusals surface as HTTP 507 storage_quota_exceeded,
# not as a generic outage.
docker compose -f docker-compose.dev.yml exec garage /garage bucket set-quotas \
    pwnforge-attachments --max-size 50G
```

**Any S3 client**, against `http://127.0.0.1:3900`. Note `--region garage`:
Garage signs SigV4 against the region in `garage.toml`, and the default
`us-east-1` produces a signature mismatch rather than a useful error.

```bash
export AWS_ACCESS_KEY_ID=$(grep ^PWNFORGE_S3_ACCESS_KEY_ID .env | cut -d= -f2)
export AWS_SECRET_ACCESS_KEY=$(grep ^PWNFORGE_S3_SECRET_ACCESS_KEY .env | cut -d= -f2)

aws --endpoint-url http://127.0.0.1:3900 --region garage \
    s3 ls s3://pwnforge-avatars --recursive
```

To start over from an empty store, run `make reset-storage`. To reset the
database, run `make reset-db`; you will want that after a migration is edited
in place on a branch, since a database that already applied the old version
never sees the change. Both are destructive and prompt for three seconds first.

## Running tests

```bash
# Backend, against the dev stack
make test-backend

# Frontend
pnpm test
```

Use `make test-backend` rather than `cargo test` directly. Parts of the suite
talk to a real Postgres, Redis and object store, and the target starts them,
loads the root `.env` and points the endpoints at the host. A bare `cargo test`
reads none of that and fails on a missing variable, which reads like a bad
value rather than a file nothing opened. `make test` runs this plus the
frontend half.

## Before opening a PR

- [ ] Commits are signed off (`-s`)
- [ ] `make test-backend` and `pnpm test` pass locally
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
