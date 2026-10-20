# PwnForge

**Forge your wins.**

PwnForge is a collaborative platform for CTF teams and offensive security professionals, built for teams that actually compete, not for organizers. It replaces the usual improvised stack of Notion, Discord, and Google Sheets with a single tool designed around how CTF teams actually work: challenges, writeups, notes, and real-time collaboration in one place.

Self-hostable, and built to scale from a solo player to a full team.

---

## Features

- **Workspaces**: one workspace per CTF or platform, solo or team, with granular permissions
- **Challenges & tags**: system and custom tags, status tracking, assignment, attachments, private notes
- **Writeups**: Markdown split-pane editor, draft or published, per-team visibility
- **Teams & invitations**: expirable invite links, role-based access, multi-workspace membership
- **Real-time collaboration**: WebSocket-backed updates across the team
- **Stats & activity feed**: track solves, progress, and team activity over time

## Tech stack

- **Backend**: Rust, Axum, SeaORM, PostgreSQL, Redis
- **Frontend**: React, TypeScript, Vite, Tailwind CSS, shadcn/ui
- **Infra**: Docker Compose, WebSockets

## Quick start (self-hosting)

```bash
git clone https://github.com/getpwnforge/pwnforge.git
cd pwnforge
cp .env.example .env

# Fill in .env before the first start; the comments in the file explain each
# value. At least:
#   - a value of its own, from `openssl rand -hex 32`, for each placeholder
#     secret: POSTGRES_PASSWORD, REDIS_PASSWORD, JWT_SECRET, ENCRYPTION_KEY
#     and PWNFORGE_S3_SECRET_ACCESS_KEY;
#   - PUBLIC_URL and CORS_ORIGIN, both the address the instance is served on,
#     and BACKEND_ENV=production;
#   - EMAIL_BACKEND, EMAIL_FROM and the SMTP_* or RESEND_API_KEY values they
#     need. Without them the stack refuses to start.

# Prepare the host for the bundled object store (COMPOSE_PROFILES=garage, the
# default): docker-compose.yml bind-mounts these two directories, and Garage
# runs as uid 1001, so they must exist and belong to it before the first start.
sudo mkdir -p /data/pwnforge/garage/meta /data/pwnforge/garage/data
sudo chown 1001:1001 /data/pwnforge/garage/meta /data/pwnforge/garage/data

# Garage's own configuration, gitignored because it holds the cluster's admin
# credentials. Put one generated value in rpc_secret and another in
# admin_token:
cp ops/garage/garage.toml.example ops/garage/garage.toml
openssl rand -hex 32   # -> rpc_secret
openssl rand -hex 32   # -> admin_token

docker compose up -d
```

If you bring your own S3 instead, clear `COMPOSE_PROFILES` in `.env` and fill in the `PWNFORGE_S3_*` variables: neither the two directories nor `garage.toml` are needed then.

The account you create on the `/setup` page is the instance's administration account. Create a separate account for everyday use.

### Serve it over HTTPS

The stack serves plain HTTP on `HTTP_PORT`: it does not terminate TLS itself. Put a TLS-terminating reverse proxy in front of it (Caddy, Traefik, Nginx Proxy Manager, Cloudflare...), set `PUBLIC_URL` and `CORS_ORIGIN` to the `https://` address that proxy serves, and set `TRUSTED_PROXY_COUNT=2` (see `.env.example`).

Do it before opening `/setup`. That page sends the setup token and the administrator's password, and over plain HTTP they cross the network in clear, as every later password and session cookie would. Keep `COOKIE_SECURE=true`: browsers then refuse the session cookies over plain HTTP, so an instance reached without TLS keeps no one signed in rather than exposing their sessions. The backend logs a warning at startup when `PUBLIC_URL` is not an `https://` address.

### Enable Turnstile (optional)

[Cloudflare Turnstile](https://developers.cloudflare.com/turnstile/) adds a bot check to sign-in, registration, password reset requests and the contact form. It is off until you configure it.

1. In the Cloudflare dashboard, create a Turnstile widget for the domain your instance is served on. It gives you a site key and a secret key.
2. Set both in `.env`. They go together: the backend refuses to start with only one of them.

   ```bash
   TURNSTILE_SITE_KEY=your_site_key
   TURNSTILE_SECRET_KEY=your_secret_key
   ```

3. Apply the change with `docker compose up -d`, which recreates the backend with the new values. The frontend reads the site key from the backend, so the published images work as they are.

The backend must be able to reach `challenges.cloudflare.com` over HTTPS. Verification fails closed: if it cannot, every protected form is refused, so an instance without outbound Internet access must leave Turnstile off.

### Stop, update and restart

`docker compose stop`, `docker compose down` and the recreation `docker compose up -d` does after a `docker compose pull` all send the backend SIGTERM. It then stops accepting connections, answers the requests already under way, and waits for the work they left running: the verification and password reset mails sent after the answer, and the cleanup pass if one is running, which finishes its current batch and starts no other. It gives all of it `SHUTDOWN_GRACE_SECS` (25 seconds by default, see `.env.example`), then exits anyway and logs how many tasks it abandoned. Docker waits 30 seconds (`stop_grace_period` in `docker-compose.yml`) before it kills the process, so keep `SHUTDOWN_GRACE_SECS` below that. The logs show each step: the signal received, the number of tasks waited for, and `shutdown complete`.

A stop with no notice is not covered: a crash, `docker kill`, the kernel's out-of-memory killer or a power loss end the process at once. The database stays consistent, as Postgres rolls back any transaction left open, but a mail under way is not sent: whoever was waiting for it asks for a new link.

> **Important**: `docker-compose.yml` is the **production** configuration. It pulls pre-built images from GHCR and does not reflect local source changes. If you're contributing code or want hot-reload, use `docker-compose.dev.yml` instead (see [CONTRIBUTING.md](CONTRIBUTING.md)).

## Self-hosted or cloud

This repository is complete and free to self-host, licensed under the AGPL v3, nothing held back behind a separate build. A managed cloud instance is also available at [pwnforge.app](https://pwnforge.app) if you'd rather not run the infrastructure yourself.

## Documentation

- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and our DCO-based contribution process (no CLA; pseudonyms accepted, valid email required).

## Contributors

Thanks to everyone who has contributed to PwnForge:

<a href="https://github.com/getpwnforge/pwnforge/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=getpwnforge/pwnforge" alt="Contributors" />
</a>

## Security

Found a vulnerability? Please see [SECURITY.md](SECURITY.md) for responsible disclosure instructions. Do not open a public issue.

## License

PwnForge is licensed under the [GNU AGPL v3](LICENSE).
