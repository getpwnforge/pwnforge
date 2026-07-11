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
docker compose up -d
```

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
