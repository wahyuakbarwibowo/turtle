# Architecture decisions

Short records of the choices that shape TurtleMeet, and why. Newest considerations are
folded into each entry rather than appended as history.

## 1. LiveKit as the SFU; media never goes through the Rust API

Writing an SFU is a multi-year project. LiveKit is open source, self-hostable, and ships
simulcast, adaptive streaming, dynacast, TURN support and official SDKs (Rust server SDK,
React components). The Rust API only authorizes participants and calls LiveKit's server
API. It never relays audio or video, so API load doesn't grow with media bandwidth.

**Room name = meeting code.** That avoids a mapping table, and the code is already the
public identifier.

## 2. Opaque server-side sessions, not JWT, for browser auth

Signing in creates a random 256-bit token. It goes into an HTTP-only, `SameSite=Lax`
cookie, and the database stores only its SHA-256. Logout and account disabling therefore
take effect immediately, and a database leak doesn't expose usable session tokens.
JWTs are used only where the receiver must verify them without our database (LiveKit).

**CSRF:** `SameSite=Lax` plus JSON-only request bodies. A cross-site form can't send
`application/json` without a CORS preflight, and we allow none. Next.js proxies `/api/*`,
so the browser only ever talks to one origin.

## 3. Two LiveKit-signed tokens per participant

`POST /api/meetings/{code}/join` returns:

- **Join token** (15 min): what LiveKit needs to connect. It is room-scoped, carries no
  admin rights, and has `can_update_own_metadata = false`. LiveKit refreshes it by itself
  during reconnects.
- **Participant token** (12 h): a bearer token for *our* in-meeting endpoints (chat,
  lobby, moderation, raise hand). It has no `room_join` grant, so LiveKit rejects it for
  media. It lets guests, who have no session, call the API, and it is scoped to one room.

Reusing LiveKit's signing key avoids a second secret. The cost is that rotating the
LiveKit secret also invalidates participant tokens. That's acceptable because both are
short-lived compared with a rotation cycle.

## 4. In-meeting authority comes from the database, not the token

Each token proves *who* the caller is (room plus identity). *What* they may do comes from
their latest `meeting_participants` row (`status`, `role`, `removed_at`). Revoking someone
(remove, deny) works instantly without token revocation lists, and guest hosts (meetings
created without an account) can moderate like account hosts.

## 5. Role and raised hand are server-written LiveKit attributes

The UI shows the Host badge and raised hands from participant attributes. If clients
could set their own attributes, anyone could display themselves as host. Join tokens
therefore deny `can_update_own_metadata`, and raising a hand goes through the API, which
updates attributes through LiveKit's server API.

## 6. Chat goes through the API, then LiveKit broadcasts it

Peer-to-peer data-channel chat can't be persisted or moderated reliably. Messages are
POSTed to the API, which checks that the sender is an admitted, non-removed participant,
stores the message, and then broadcasts it on the `turtlemeet.chat` topic. If the
broadcast fails, the message is still in history.

Reactions are the exception. They are ephemeral and harmless if lost, so clients send
them directly over a lossy data channel, and only whitelisted emoji are rendered.

## 7. Lobby by polling

A waiting participant polls `GET /lobby/me` every 2 s. The poll also works as a heartbeat:
the host's list hides requests not seen in the last 30 s, so people who give up drop off
automatically. Hosts get a data-channel ping on changes, with a 5 s poll as fallback.
WebSockets would cut latency, but polling is simple and good enough at lobby scale.

## 8. Migrations are embedded and applied on startup

`sqlx::migrate!` compiles the SQL into the binary, so a deployment is one artifact.
`crates/database/build.rs` tells Cargo to rebuild when `migrations/` changes. Without it,
a new migration file could ship in the repo but not in the binary.

All SQL is static (`&'static str`, enforced by sqlx 0.9) with bound parameters, so there
is no string-built SQL.

## 9. Rate limiting in Redis with fixed windows

Login and registration are limited per email, joins per meeting code, password attempts
per meeting, chat per participant. Fixed windows can allow a 2× burst at window edges.
Per-IP limits belong at the reverse proxy (Traefik, Phase 7), where the real client IP
is known.

## 10. Modular monolith

A single Rust binary with modules (`auth`, `meetings`, `chat`, `participant`), each split
into a `service` (use cases, no HTTP) and an `http` layer. Microservices would add
operational cost with no benefit at this size.

## 11. Integrations reuse LiveKit-signed tokens and a postMessage SDK

Embed links carry a third LiveKit-signed token kind (`tm_token=embed`, no room grant),
verified by the join endpoint like the participant token, so there is no extra secret
or JWT library. Integration meetings refuse every other way in. The SDK is one
dependency-free file served from `/sdk`; the iframe learns its parent's origin from a
`hello` handshake rather than trusting `document.referrer`. Webhooks are HMAC-signed and
retried in-process; a durable queue is deferred until someone needs guaranteed delivery.
