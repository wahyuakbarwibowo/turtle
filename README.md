# TurtleMeet

Self-hosted video conferencing: a Jitsi Meet alternative built on a Rust API and the
[LiveKit](https://livekit.io) SFU. Share a link, join from the browser, no downloads.

> **Status: MVP in development.** Auth, meetings, video, chat, moderation, lobby and
> the integration API + embed SDK are done. One-command production deployment
> (Phase 7) and load testing (Phase 8) are next. Capacity has only been tested with 2 participants so far, so
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
| Integrations | API keys for other platforms, embed links, iframe SDK with events/commands, HMAC-signed webhooks ([docs](docs/integrations.md)) |
| Security | Rate limiting (Redis), audit log, CSRF protection, room-scoped tokens, LiveKit secrets never reach the browser |

Not yet: recording, E2EE,
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

## Embed in your platform

Other apps can create meetings server-to-server and show the call in an iframe,
like Jitsi's External API:

```js
// backend: POST /api/integrations/meetings/{code}/tokens  (Bearer tmk_…) → embed_url
const meet = new TurtleMeetEmbed(document.getElementById("call"), { url: embedUrl });
meet.on("readyToClose", () => meet.dispose());
meet.execute("toggleAudio");
```

Keys, endpoints, events and webhook signatures: [docs/integrations.md](docs/integrations.md).

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
| `PUBLIC_WEB_URL` | Base of embed links returned to integrations | `https://…` |
| `API_URL` | Where Next.js proxies `/api/*` | internal URL of the API |
| `EMBED_ALLOWED_ORIGINS` | Who may iframe `/embed/*` (web app) | your integrators' origins |

## Testing

```bash
make check   # rustfmt, clippy -D warnings, ESLint, TypeScript, all Rust tests
make e2e     # two-browser test with real media (needs `make dev` + Google Chrome)
```

Details, including what each suite covers, are in [docs/testing.md](docs/testing.md).

## Project structure

```
apps/
  server/          Rust API (Axum): auth, meetings, chat, integrations, CLI
  web/             Next.js app: landing, auth, meeting room, /embed page
    public/sdk/    turtlemeet-embed.js (iframe SDK for integrators)
crates/
  database/        PostgreSQL pool and embedded migrations
  livekit-client/  Thin wrapper over the official LiveKit Rust SDK
  telemetry/       Structured JSON logging
  common/          Reserved for shared types (currently empty)
migrations/        SQL migrations, applied on API startup
infrastructure/    LiveKit dev config
docs/              See the table below
```

## Documentation

| Doc | For |
|---|---|
| [architecture.md](docs/architecture.md) | Design decisions and why |
| [api.md](docs/api.md) | HTTP endpoints of the web app's API |
| [integrations.md](docs/integrations.md) | Embedding TurtleMeet in another platform |
| [testing.md](docs/testing.md) | Test suites and how to run them |
| [security.md](docs/security.md) | Security review: findings and their status |
| [load-test.md](docs/load-test.md) | Load test script and results so far |
| [deploy.md](docs/deploy.md) | Docker stack: local test and VPS with HTTPS |

## Roadmap

1. **Phase 7: one-command deployment** (in progress): Docker stack in `deploy/`, see
   [docs/deploy.md](docs/deploy.md). Not yet tested on a real VPS.
2. **Phase 8: load test and security review.** Tests at 10/25/50/100 participants,
   measuring CPU, memory, bandwidth, latency and packet loss.
3. **Next:** dashboard and admin, recording (LiveKit Egress to MinIO), background blur,
   E2EE, Indonesian/English UI, and a mobile layout.

## License

[MIT](LICENSE)
