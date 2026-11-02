import { chromium } from "playwright-core";
// Two-participant browser E2E: register, create, join via invite link, real media, leave.
// Needs the API, web (BASE_URL) and LiveKit running, plus Google Chrome installed.
// Usage: npm run e2e -- [screenshot-dir]
import { mkdirSync } from "node:fs";
const BASE = process.env.BASE_URL ?? "http://localhost:3000";
const OUT = process.argv[2] ?? "e2e/screenshots";
mkdirSync(OUT, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome", headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"],
});
const ctx = () => browser.newContext({ permissions: ["camera", "microphone"], viewport: { width: 1280, height: 800 } });
const log = (...a) => console.log(...a);
const errors = [];

const host = await (await ctx()).newPage();
host.on("pageerror", (e) => errors.push("host: " + e.message));
await host.goto(`${BASE}/register`);
await host.fill("#display_name", "Host Turtle");
await host.fill("#email", `host-${Date.now()}@example.test`);
await host.fill("#password", "s3cure-passphrase");
await host.click("button[type=submit]");
await host.getByText("Host Turtle").waitFor();
log("1. registered + signed in");

await host.getByRole("button", { name: "New meeting" }).click();
await host.waitForURL(/\/m\/[a-z]{3}-[a-z]{4}-[a-z]{3}$/);
const code = host.url().split("/m/")[1];
log("2. meeting created:", code);
await host.getByRole("button", { name: "Join now" }).waitFor();
await host.waitForTimeout(1500);
await host.screenshot({ path: `${OUT}/prejoin.png` });
await host.getByRole("button", { name: "Join now" }).click();
await host.locator(".lk-participant-tile").first().waitFor({ timeout: 15000 });
log("3. host in call");

const guest = await (await ctx()).newPage();
guest.on("pageerror", (e) => errors.push("guest: " + e.message));
await guest.goto(`${BASE}/`);
await guest.fill("input[aria-label='Meeting code or link']", `${BASE}/m/${code}`);
await guest.getByRole("button", { name: "Join", exact: true }).click();
await guest.waitForURL(`**/m/${code}`);
await guest.locator("#username").fill("Guest Tortoise");
await guest.getByRole("button", { name: "Join now" }).click();
await guest.locator(".lk-participant-tile").nth(1).waitFor({ timeout: 15000 });
await host.locator(".lk-participant-tile").nth(1).waitFor({ timeout: 15000 });
log("4. guest joined via invite link; host sees", await host.locator(".lk-participant-tile").count(), "tiles");

// Wait until the host actually receives remote video frames (real media path, not just signaling).
await host.waitForFunction(() =>
  [...document.querySelectorAll("video")].some((v) => v.videoWidth > 0 && v.readyState >= 2 && !v.closest("[data-lk-local-participant='true']")),
  null, { timeout: 20000 });
log("5. host receives remote video frames");
await host.waitForTimeout(1500);
await host.screenshot({ path: `${OUT}/host.png` });

await guest.locator(".lk-disconnect-button").click();
await guest.getByText("You left the meeting").waitFor();
log("6. guest left -> sees left screen");
await host.waitForFunction(() => document.querySelectorAll(".lk-participant-tile").length === 1, null, { timeout: 15000 });
log("7. host grid back to 1 tile");
console.log("CODE=" + code);
await browser.close();
if (errors.length) {
  console.error("PAGE ERRORS:\n" + errors.join("\n"));
  process.exit(1);
}
log("PASS");
