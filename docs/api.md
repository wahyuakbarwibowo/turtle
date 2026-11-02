# API reference

Base path `/api`. Requests and responses are JSON. Errors look like this:

```json
{ "error": { "code": "invalid_password", "message": "incorrect meeting password" } }
```

Error codes: `validation_error` (422), `invalid_credentials` / `unauthorized` /
`password_required` / `invalid_password` (401), `forbidden` (403), `not_found` (404),
`conflict` (409), `gone` (410), `rate_limited` (429), `internal_error` (500).

**Auth column:**
- **cookie**: session cookie from login.
- **cookie?**: works signed in or as a guest.
- **pt**: `Authorization: Bearer <participant_token>` from the join response, scoped to that meeting.
- **public**: no auth.
- **LiveKit**: signed by LiveKit.

## Accounts

| Method | Path | Auth | Notes |
|---|---|---|---|
| GET | `/config` | public | `{ allow_guest_create }` |
| POST | `/auth/register` | public | `{ email, password (8-128), display_name }`, sets cookie, 201. Limit: 5/h per email |
| POST | `/auth/login` | public | `{ email, password }`, sets cookie. Limit: 10/15 min per email |
| POST | `/auth/logout` | cookie | Revokes the session, 204 |
| GET | `/auth/me` | cookie | Current user |

## Meetings

| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/meetings` | cookie? | `{ title, password?, lobby_enabled? }`, 201. Guests only if `ALLOW_GUEST_CREATE`; they get a one-time `host_secret` |
| GET | `/meetings` | cookie | Meetings you hosted or joined (latest 50) |
| GET | `/meetings/{code}` | public | Title and state, including `has_password` and `lobby_enabled` |
| POST | `/meetings/{code}/join` | cookie? | `{ display_name?, password?, host_secret? }`, see below |

`join` returns one of two shapes:

```jsonc
// Admitted: connect to LiveKit with `token`
{ "status": "admitted", "token": "…", "participant_token": "…", "livekit_url": "wss://…",
  "identity": "u-<uuid>|g-<uuid>", "role": "host|participant", "meeting": { … } }

// Lobby on: poll /lobby/me with participant_token
{ "status": "waiting", "identity": "g-<uuid>", "participant_token": "…" }
```

Hosts (account owner, or the holder of `host_secret`) skip the password and the lobby.
Signed-in users who were already admitted once skip the lobby on rejoin.

## In-meeting (participant token)

| Method | Path | Who | Notes |
|---|---|---|---|
| GET | `/meetings/{code}/lobby/me` | waiting participant | Returns `admitted` (with tokens), `waiting`, or 403 if denied. Also a heartbeat |
| GET | `/meetings/{code}/lobby` | host | Waiting requests seen in the last 30 s |
| POST | `/meetings/{code}/lobby/{identity}/admit` · `/deny` | host | 204 |
| PATCH | `/meetings/{code}/settings` | host | `{ lobby_enabled?, password? }`. `password: null` clears it |
| POST | `/meetings/{code}/participants/{identity}/mute` | host | Server-side mic mute; only the participant can unmute |
| POST | `/meetings/{code}/mute-all` | host | Mutes everyone except the caller |
| POST | `/meetings/{code}/participants/{identity}/remove` | host | Disconnects them and blocks rejoin. Hosts can't be removed |
| POST | `/meetings/{code}/end` | host | Disconnects everyone and closes the meeting |
| POST | `/meetings/{code}/hand` | admitted | `{ raised }`, sets the `hand` attribute |
| GET | `/meetings/{code}/chat` | admitted | Latest 200 messages, oldest first |
| POST | `/meetings/{code}/chat` | admitted | `{ body (1-4000) }`, stored, then broadcast on `turtlemeet.chat`. Limit: 20/10 s |

## LiveKit data topics (client side)

| Topic | Sender | Payload |
|---|---|---|
| `turtlemeet.chat` | server | Chat message JSON (same shape as the history items) |
| `turtlemeet.lobby` | server | `{}`: hosts should refresh the lobby list |
| `turtlemeet.reaction` | clients (lossy) | One whitelisted emoji |

## Webhook

| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/livekit/webhook` | LiveKit | Signature and body hash verified. Records `left_at` on `participant_left`, `participant_connection_aborted` and `room_finished` |
