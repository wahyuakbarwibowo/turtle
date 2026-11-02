// Embed E2E: an integrator's backend (this script) uses an API key to create a meeting and
// mint embed links; two "integrator pages" on another origin embed the call with the SDK.
// Checks SDK events and commands, the closed /m/ route, and HMAC-signed webhooks.
//
// Needs `make dev` running and TURTLEMEET_API_KEY + TURTLEMEET_WEBHOOK_SECRET from:
//   server integration create e2e-<n> --webhook-url http://127.0.0.1:4100/hook
// Usage: npm run e2e:embed
import { createHmac } from "node:crypto";
import { createServer } from "node:http";
import { chromium } from "playwright-core";

const BASE = process.env.BASE_URL ?? "http://localhost:3000";
const API = process.env.API_BASE ?? "http://127.0.0.1:8080";
const KEY = process.env.TURTLEMEET_API_KEY;
const SECRET = process.env.TURTLEMEET_WEBHOOK_SECRET;
if (!KEY || !SECRET) throw new Error("set TURTLEMEET_API_KEY and TURTLEMEET_WEBHOOK_SECRET");
const log = (...a) => console.log(...a);
const assert = (c, m) => {
  if (!c) throw new Error(`assertion failed: ${m}`);
};

// ---- Integrator: webhook receiver (:4100) and a page on a different origin (:4200) ----
const hooks = [];
const hookServer = createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    const ts = req.headers["x-turtlemeet-timestamp"];
    const want = "v1=" + createHmac("sha256", SECRET).update(`${ts}.${body}`).digest("hex");
    hooks.push({ valid: want === req.headers["x-turtlemeet-signature"], event: JSON.parse(body) });
    res.end();
  });
}).listen(4100, "127.0.0.1");

const page = `<!doctype html><html><body style="margin:0">
<div id="call" style="width:100vw;height:100vh"></div>
<script src="${BASE}/sdk/turtlemeet-embed.js"></script>
<script>
  window.events = [];
  const url = decodeURIComponent(location.hash.slice(1));
  window.meet = new TurtleMeetEmbed(document.getElementById("call"), { url });
  for (const t of ["ready","joined","left","participantJoined","participantLeft",
                   "audioMuteStatusChanged","videoMuteStatusChanged","readyToClose","error"])
    meet.on(t, (e) => window.events.push(e));
</script></body></html>`;
const pageServer = createServer((_, res) => res.end(page)).listen(4200, "127.0.0.1");
const PARENT = "http://127.0.0.1:4200/";

async function api(path, body) {
  const res = await fetch(`${API}/api/integrations${path}`, {
    method: "POST",
    headers: { authorization: `Bearer ${KEY}`, "content-type": "application/json" },
    body: body && JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${path}: ${res.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

const browser = await chromium.launch({
  channel: "chrome",
  headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"],
});
const errors = [];
async function integratorPage(label, embedUrl) {
  const ctx = await browser.newContext({ permissions: ["camera", "microphone"], viewport: { width: 1100, height: 750 } });
  const p = await ctx.newPage();
  p.on("pageerror", (e) => errors.push(`${label}: ${e.message}`));
  await p.goto(PARENT + "#" + encodeURIComponent(embedUrl));
  return p;
}
const events = (p) => p.evaluate(() => window.events);
async function waitEvent(p, pred, what) {
  for (let i = 0; i < 60; i++) {
    const found = (await events(p)).find(pred);
    if (found) return found;
    await p.waitForTimeout(250);
  }
  throw new Error(`timed out waiting for ${what}; got ${JSON.stringify(await events(p))}`);
}
async function waitHook(pred, what) {
  for (let i = 0; i < 60; i++) {
    const h = hooks.find((h) => pred(h.event));
    if (h) return h;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`no webhook ${what}; got ${JSON.stringify(hooks.map((h) => h.event.type))}`);
}

try {
  const meeting = await api("/meetings", { title: "Embedded consult", external_ref: "ticket-e2e" });
  const code = meeting.code;
  const officer = await api(`/meetings/${code}/tokens`, { external_id: "officer-1", display_name: "Officer", role: "host" });
  const applicant = await api(`/meetings/${code}/tokens`, {
    external_id: "applicant-1",
    display_name: "Applicant",
    role: "participant",
  });
  log("1. integrator created", code, "and minted two embed links");

  const host = await integratorPage("officer", officer.embed_url);
  const joined = await waitEvent(host, (e) => e.type === "joined", "joined");
  assert(joined.role === "host", "officer is host");
  const guest = await integratorPage("applicant", applicant.embed_url);
  await waitEvent(guest, (e) => e.type === "joined", "applicant joined");
  await waitEvent(host, (e) => e.type === "participantJoined" && e.name === "Applicant", "participantJoined");
  log("2. both joined across origins; officer saw participantJoined");

  const frame = guest.frameLocator("iframe");
  await frame.locator(".lk-participant-tile").nth(1).waitFor({ timeout: 15000 });
  assert(!(await frame.getByLabel("Copy invite link").count()), "no invite button in embeds");

  await guest.evaluate(() => meet.execute("toggleAudio"));
  await waitEvent(guest, (e) => e.type === "audioMuteStatusChanged" && e.muted === true, "muted");
  log("3. execute(toggleAudio) muted the applicant");

  const plain = await fetch(`${API}/api/meetings/${code}/join`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ display_name: "Intruder" }),
  });
  assert(plain.status === 403, `plain join is closed (${plain.status})`);
  log("4. joining by meeting code alone is rejected");

  const hj = await waitHook(
    (e) => e.type === "participant_joined" && e.participant?.external_id === "applicant-1",
    "participant_joined",
  );
  assert(hj.valid, "participant_joined signature");
  assert(hj.event.meeting.external_ref === "ticket-e2e", "external_ref echoed");
  log("5. signed participant_joined webhook with external_id + external_ref");

  await guest.evaluate(() => meet.execute("hangup"));
  await waitEvent(guest, (e) => e.type === "readyToClose", "readyToClose");
  await waitEvent(host, (e) => e.type === "participantLeft", "participantLeft");
  const hl = await waitHook((e) => e.type === "participant_left", "participant_left");
  assert(hl.valid, "participant_left signature");
  log("6. hangup → readyToClose, participantLeft, signed participant_left webhook");

  await api(`/meetings/${code}/end`);
  await waitEvent(host, (e) => e.type === "readyToClose", "host readyToClose after end");
  const he = await waitHook((e) => e.type === "meeting_ended", "meeting_ended");
  assert(he.valid, "meeting_ended signature");
  log("7. integrator ended the meeting → host readyToClose + meeting_ended webhook");

  assert(errors.length === 0, `page errors: ${errors.join("; ")}`);
  log("EMBED E2E PASSED");
} finally {
  await browser.close();
  hookServer.close();
  pageServer.close();
}
