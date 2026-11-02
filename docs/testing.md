# Testing

| Suite | Command | Needs | Covers |
|---|---|---|---|
| Static checks | `make lint typecheck` | nothing | rustfmt, clippy `-D warnings`, ESLint, TypeScript |
| Rust unit tests | `make test-unit` | nothing | Password hashing, tokens, validation, LiveKit token grants (room-scoped, no admin rights, no self-set metadata) |
| Rust integration | `make test` | Postgres, Redis, LiveKit | Real HTTP router against real services (see below) |
| Browser E2E | `make e2e` | `make dev` running, Google Chrome | Two Chrome instances with fake camera/mic and real WebRTC media |

`make check` runs everything except E2E.

## Integration tests (`apps/server/tests`)

They use `TEST_DATABASE_URL`, a separate database (`make db-create`), plus Redis and
`livekit-server --dev`. Each test registers users with unique emails, so tests never
truncate tables and can run against a database that already has data.

- `auth.rs`: register, `me`, logout revokes the session, login, case-insensitive email
  uniqueness, validation errors.
- `meetings.rs`:
  - Host moderation with participant tokens: remove, mute, mute-all, end. Non-hosts get
    403, and the cookie alone isn't accepted.
  - Passwords: required and wrong password, host bypass, clearing the password.
  - Lobby: waiting participants can't chat or see the lobby; admit and deny; polling.
  - Guest-created meetings with a host secret, including a wrong secret.
  - The public config endpoint.
- `chat.rs`: send and history order, unauthenticated and invalid tokens, room scoping,
  removed participants can't post.

## Browser E2E (`apps/web/e2e/meeting.e2e.mjs`)

Drives system Chrome via `playwright-core` with `--use-fake-device-for-media-stream`.
It needs the API running with `ALLOW_GUEST_CREATE=true` (the `.env.example` default).

Steps:
1. Host registers, creates a meeting and joins.
2. Host turns on the lobby and sets a password.
3. Guest is rejected with a wrong password and waits in the lobby with the right one.
4. Host admits the guest. The test checks that real remote video frames arrive.
5. Raise hand, a reaction, and host mute.
6. Chat both ways with the unread badge.
7. Screen share switches the stage to the focus layout.
8. Host removes the guest, who isn't offered a rejoin.
9. Chat history survives a rejoin.
10. End for all; the meeting link then shows "ended".
11. A visitor without an account creates a meeting and is its host.

Screenshots go to `apps/web/e2e/screenshots/` (gitignored).

## Not yet tested

- **Load:** 10/25/50/100 participants (Phase 8). There are no capacity numbers yet.
- **Other browsers and devices:** Firefox, Safari, mobile layouts.
- **Networks:** behind NAT or firewalls; TURN arrives in Phase 7.
