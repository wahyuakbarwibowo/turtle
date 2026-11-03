# Integrations: server API + embed SDK

Lets any platform (queue system, LMS, CRM, telehealth app…) create meetings for its own
users and show the call inside its UI, the way Jitsi's External API is used.

```
your backend ──API key──▶ TurtleMeet API   create meeting, mint embed link per user
your frontend ─iframe──▶ /embed/<code>#t=…  via turtlemeet-embed.js (postMessage)
TurtleMeet ──signed POST──▶ your webhook   participant_joined / _left, meeting_ended
```

## 1. Create an API key (operator)

```bash
server integration create <name> [--webhook-url https://your.app/hooks/turtlemeet]
server integration list
server integration revoke <name>
```

`create` prints `api_key` (`tmk_…`) and, with a webhook, `webhook_secret` (`whsec_…`)
**once**. Only a hash of the key is stored. The command uses the same env as the server
(`DATABASE_URL`, …). `<name>` is 2–32 chars of `a-z 0-9 -`.

## 2. Server API (your backend only)

`Authorization: Bearer tmk_…`. Never ship the key to browsers.

| Method | Path | Body | Returns |
|---|---|---|---|
| POST | `/api/integrations/meetings` | `{title, external_ref?}` | 201 meeting (`code`, …) |
| POST | `/api/integrations/meetings/{code}/tokens` | `{external_id, display_name, role: "host"\|"participant", ttl?}` | `{embed_url, embed_token, identity, expires_in}` |
| GET | `/api/integrations/meetings/{code}` | | `{meeting, participants[]}` |
| POST | `/api/integrations/meetings/{code}/end` | | 204, disconnects everyone |

- `external_ref` (≤200 bytes): your id for the call (e.g. ticket id), echoed in webhooks.
- `external_id`: your user id, `[A-Za-z0-9_-]{1,64}`. Identity becomes
  `x-<integration>-<external_id>`; the same user opening a second tab kicks the first.
- `ttl`: seconds the link stays valid to **start** a session, 30–3600 (default 300).
  Mint links right before showing the call.
- A meeting belongs to the integration that created it; others get 404.
- Integration meetings can **only** be joined with embed links. `/m/<code>` and the
  plain join API return 403, so a leaked code lets nobody in. No lobby or password: you
  decide who gets a link.

## 3. Embed (your frontend)

```html
<div id="call" style="height:600px"></div>
<script src="https://meet.example.com/sdk/turtlemeet-embed.js"></script>
<script>
  const meet = new TurtleMeetEmbed(document.getElementById("call"), {
    url: embedUrlFromYourBackend,
    audio: true, video: true,   // start with mic / camera on
  });
  meet.on("joined", (e) => console.log("in call as", e.identity, e.role));
  meet.on("readyToClose", () => meet.dispose());
  // meet.execute("toggleAudio" | "toggleVideo" | "toggleShareScreen" | "hangup")
</script>
```

Events (`meet.on(type, fn)`, payload includes `type`):

| Event | Payload |
|---|---|
| `ready` | iframe loaded |
| `joined` | `identity, role, meeting` |
| `participantJoined` / `participantLeft` | `identity, name` |
| `audioMuteStatusChanged` / `videoMuteStatusChanged` | `muted` |
| `left` | `reason` (LiveKit `DisconnectReason`) |
| `readyToClose` | after leaving, being removed, or the meeting ending |
| `error` | `message` |

Security: the iframe only trusts messages from its direct parent and answers only to the
origin that completed the `hello` handshake. Restrict who may frame `/embed/*` with
`EMBED_ALLOWED_ORIGINS` on the web app (space-separated origins; unset means only the
TurtleMeet site itself, so set it to your integrators' origins). All other
pages send `frame-ancestors 'self'`. The token is in the URL fragment, so it never
reaches server logs, and the page removes it from history once read.

Without the SDK: use an `<iframe src="{embed_url}" allow="camera; microphone; display-capture; autoplay">`.

## 4. Webhooks

`POST` JSON to your `webhook_url`:

```json
{ "id": "uuid", "type": "participant_joined", "created_at": "…",
  "meeting": { "code": "abc-defg-hij", "external_ref": "ticket-7" },
  "participant": { "identity": "x-qnext-42", "external_id": "42", "name": "Budi", "role": "participant" } }
```

Types: `participant_joined`, `participant_left`, `meeting_ended` (no `participant`).
`external_id` is null for people who didn't join through your links.

Verify every request:

```
X-TurtleMeet-Timestamp: 1700000000
X-TurtleMeet-Signature: v1=<hex HMAC-SHA256(webhook_secret, "<timestamp>.<raw body>")>
```

```js
const want = "v1=" + crypto.createHmac("sha256", secret).update(`${ts}.${rawBody}`).digest("hex");
crypto.timingSafeEqual(Buffer.from(want), Buffer.from(signatureHeader)); // and |now - ts| < 300
```

Delivery: 10s timeout, retried after 1s, 5s and 25s on non-2xx. Dedupe on `id`;
`meeting_ended` can arrive twice (your end call and LiveKit's room close). Retries are
in-process, so events in flight are lost if the server restarts.

## Migrating from Jitsi's External API

| Jitsi | TurtleMeet |
|---|---|
| Self-signed JWT with `moderator` | `POST …/tokens` with `role: "host"` |
| `new JitsiMeetExternalAPI(domain, {roomName, jwt})` | `new TurtleMeetEmbed(el, {url: embed_url})` |
| `executeCommand('hangup' / 'toggleAudio' / 'toggleVideo' / 'toggleShareScreen')` | `execute(…)` with the same names |
| `videoConferenceJoined` / `videoConferenceLeft` | `joined` / `left` |
| `participantJoined` / `participantLeft` / `readyToClose` | same names |
| Prosody single-session module | built in (same `external_id` replaces the old session) |
