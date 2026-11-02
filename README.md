# TurtleMeet

Self-hosted video conferencing: a Jitsi Meet alternative built on a Rust API and the
[LiveKit](https://livekit.io) SFU. Share a link, join from the browser, no downloads.

> **Status: MVP in development.** Phases 1–6 are done (auth, meetings, video, chat,
> moderation, lobby). One-command production deployment (Phase 7) and load testing
> (Phase 8) are next. Capacity has only been tested with 2 participants so far, so
> no capacity claims are made yet.

## Features

| Area | What works today |
|---|---|
| Meetings | Create, invite link (`/m/abc-defg-hij`), join as signed-in user or named guest, meeting history |
| Media | Audio/video, screen share, active speaker, connection quality, simulcast + adaptive stream + dynacast |
| Chat | Persisted chat with live delivery and unread badge |
| Host controls | Lobby (admit/deny), meeting password, mute one / mute everyone, remove, end for all |
| Everyone | Raise hand, emoji reactions, People panel |
| Accounts | Register/login with Argon2id, HTTP-only cookie sessions, optional meetings without an account (`ALLOW_GUEST_CREATE`) |
| Security | Rate limiting (Redis), audit log, CSRF protection, room-scoped tokens, LiveKit secrets never reach the browser |

Not yet: production Docker deployment, recording, embeddable iframe API, E2EE,
background blur, i18n, mobile layout. See [Roadmap](#roadmap).

## Architecture

```
Browser ── HTTPS ──► Next.js (apps/web) ── /api/* proxy ──► Rust API (apps/server) ──► PostgreSQL
   │                                                              │    │
   │                                                              │    └──────────► Redis (rate limits)
   │                                                              │ server SDK: rooms, tokens, moderation
   └────────── WebRTC media (never via the API) ──────────► LiveKit SFU ── webhooks ──► Rust API
```

- **The Rust API** owns accounts, meetings, permissions, chat and moderation. It mints
  short-lived LiveKit tokens and never touches media.
- **LiveKit** routes audio and video between participants (SFU), with simulcast.
- **Next.js** serves the UI and proxies `/api/*`, so the session cookie stays same-origin.

Design decisions and their reasons are in [docs/architecture.md](docs/architecture.md).

## Quick start (local development)

Requirements: Rust (edition 2024, stable), Node.js 20+, LiveKit server
(`brew install livekit`), and either Docker or a local PostgreSQL 13+ and Redis/Valkey.

```bash
make setup        # creates .env from .env.example, installs dependencies
make infra-up     # Postgres + Redis in Docker (skip if you run them locally)
make db-create    # creates the turtlemeet_test database for integration tests
make dev          # LiveKit + API + web together; Ctrl-C stops all
```

Open http://localhost:3000. To try it with two people on one machine, use a normal
window and a private/incognito window (separate sessions). Wear headphones to avoid echo.

**Using your own Postgres/Redis instead of Docker:** create the databases
(`createdb turtlemeet && createdb turtlemeet_test`), then point `DATABASE_URL`,
`TEST_DATABASE_URL` and `REDIS_URL` in `.env` at them. Migrations run automatically
when the API starts.

> Other devices on your network can't join yet: LiveKit dev mode listens on 127.0.0.1,
> and browsers only allow camera access over HTTPS or on `localhost`. Phase 7 adds HTTPS.

Run `make help` for every command.

## Configuration

All settings are environment variables (see [`.env.example`](.env.example)).

| Variable | Purpose | Production |
|---|---|---|
| `DATABASE_URL` | PostgreSQL connection | required |
| `REDIS_URL` | Redis/Valkey for rate limiting | required |
| `LIVEKIT_API_URL` | LiveKit server API, reached from the Rust API | required |
| `LIVEKIT_PUBLIC_URL` | LiveKit WebSocket URL handed to browsers | `wss://…` |
| `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET` | LiveKit credentials, server-side only | strong random secret |
| `COOKIE_SECURE` | Send session cookie only over HTTPS | `true` |
| `ALLOW_GUEST_CREATE` | Allow creating meetings without an account | your choice |
| `SESSION_TTL_HOURS` | Session lifetime (default 168) | |
| `BIND_ADDR`, `DB_MAX_CONNECTIONS`, `RUST_LOG` | Server tuning and logging | |
| `API_URL` | Where Next.js proxies `/api/*` | internal URL of the API |

## Testing

```bash
make check   # rustfmt, clippy -D warnings, ESLint, TypeScript, all Rust tests
make e2e     # two-browser test with real media (needs `make dev` + Google Chrome)
```

Details, including what each suite covers, are in [docs/testing.md](docs/testing.md).

## Project structure

```
apps/
  server/          Rust API (Axum): auth, meetings, chat, participant auth
  web/             Next.js app: landing, auth pages, meeting room (LiveKit React)
crates/
  database/        PostgreSQL pool and embedded migrations
  livekit-client/  Thin wrapper over the official LiveKit Rust SDK
  telemetry/       Structured JSON logging
  common/          Reserved for shared types (currently empty)
migrations/        SQL migrations, applied on API startup
infrastructure/    LiveKit dev config
docs/              Architecture, API reference, testing
```

API endpoints are listed in [docs/api.md](docs/api.md).

## Roadmap

1. **Phase 7: one-command deployment.** Dockerfiles, a full Compose stack (Traefik with
   automatic HTTPS, coturn TURN, LiveKit), and a VPS guide for Ubuntu 24.04.
2. **Phase 8: load test and security review.** Tests at 10/25/50/100 participants,
   measuring CPU, memory, bandwidth, latency and packet loss.
3. **Next:** dashboard and admin, recording (LiveKit Egress to MinIO), an embeddable
   iframe API with external JWT auth, background blur, E2EE, Indonesian/English UI,
   and a mobile layout.
