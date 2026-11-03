# Security review

Review of the whole repo on 2026-11-03 (code read end to end; the two highest findings
were reproduced against the code paths). Status reflects this branch.

| # | Severity | Finding | Status |
|---|---|---|---|
| 1 | High | Any participant could publish on the chat topic and show a message under another name (e.g. the host) | **Fixed:** clients drop chat/lobby packets sent by participants; only server broadcasts are shown |
| 2 | Medium-high | Lobby, denied and removed participants could read chat history with their participant token | **Fixed:** history requires the latest row to be admitted, not removed, meeting live (same check as sending) |
| 3 | Medium | Unbounded Argon2 work from logins/sign-ups with random emails could starve the server | **Fixed:** hashing bounded to one per core; Traefik limits `/api/auth` to 10/min per IP |
| 4 | Medium | `/embed/*` frameable by any site by default | **Fixed:** default is `'self'`; operators list integrator origins |
| 5 | Medium | A removed participant could reconnect with a saved join token (15 min) | **Fixed:** join tokens live 60 s; LiveKit refreshes them for connected clients |
| 6 | Low-medium | Compose defaulted to non-Secure cookies | **Fixed:** defaults to `COOKIE_SECURE=true` |
| 7 | Low | `change-me` secrets in `.env.example` could ship | **Fixed:** API refuses to start in production with a weak LiveKit secret |
| 8 | Low | `GET /api/meetings/{code}` is public and unthrottled (title, host id, flags) | Open: add per-IP limit and trim fields for non-participants |
| 9 | Low | Webhook URLs are not SSRF-filtered | Open, by design: only the operator CLI sets them. Needed before any self-service registration |
| 10 | Low | Failed-login emails in `audit_logs`; full webhook URL in logs | Open |

Also open: a removed **guest** can rejoin under a new guest identity unless the meeting
has a lobby or password (accounts and embed identities are blocked). That's the
Jitsi-style trade-off for link-only meetings.

Checked and sound: token kinds can't be swapped (join vs participant vs embed), host
authority comes from the database not the token, integration meetings are owned per API
key, all SQL uses binds, sessions are hashed 256-bit tokens in HttpOnly SameSite cookies,
both webhook directions are signed, no HTML injection paths in the web UI, and secrets
aren't logged.
