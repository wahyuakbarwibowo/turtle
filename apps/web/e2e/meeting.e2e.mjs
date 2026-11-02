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

// --- Chat: guest sends, host gets a live unread badge, then sees it in the panel.
await guest.getByRole("button", { name: /Chat/ }).click();
await guest.getByLabel("Message").fill("hello from the guest");
await guest.getByRole("button", { name: "Send" }).click();
await guest.locator("[data-testid=chat-messages]").getByText("hello from the guest").waitFor();
await host.getByLabel("1 unread").waitFor({ timeout: 10000 });
await host.getByRole("button", { name: /Chat/ }).click();
await host.locator("[data-testid=chat-messages]").getByText("hello from the guest").waitFor();
log("6. chat delivered live (unread badge + panel)");

// History is persisted: a fresh load of the panel still shows it.
await host.getByLabel("Message").fill("welcome!");
await host.getByRole("button", { name: "Send" }).click();
await guest.locator("[data-testid=chat-messages]").getByText("welcome!").waitFor();
log("7. host reply delivered to guest");

// --- Screen share switches the stage to a focus layout.
await host.getByRole("button", { name: /Share screen/ }).click();
await guest.locator(".lk-focus-layout").waitFor({ timeout: 15000 });
log("8. guest sees host screen share in focus layout");
await host.getByRole("button", { name: /Stop screen share/ }).click();
await guest.locator(".lk-focus-layout").waitFor({ state: "detached", timeout: 15000 });

// --- People panel + host removes guest.
await host.getByRole("button", { name: "2" }).click();
const people = host.locator("[data-testid=people-list]");
await people.getByText("Host", { exact: true }).waitFor();
await people.getByText("Guest Tortoise").waitFor();
await host.screenshot({ path: `${OUT}/host-panel.png` });
host.once("dialog", (d) => d.accept());
await people.getByRole("button", { name: "Remove" }).click();
await guest.getByText("You were removed from the meeting by the host").waitFor({ timeout: 15000 });
log("9. host removed guest -> guest sees removed screen (no rejoin button)");
if (await guest.getByRole("button", { name: "Rejoin" }).count()) throw new Error("removed guest offered rejoin");

// --- Persisted history survives a reload of the meeting.
await host.reload();
await host.getByRole("button", { name: "Join now" }).click();
await host.getByRole("button", { name: /Chat/ }).click();
await host.locator("[data-testid=chat-messages]").getByText("hello from the guest").waitFor();
log("10. chat history persisted across rejoin");

// --- Host ends the meeting for everyone.
host.once("dialog", (d) => d.accept());
await host.getByRole("button", { name: "End for all" }).click();
await host.getByText("The host ended the meeting").waitFor({ timeout: 15000 });
await host.goto(`${BASE}/m/${code}`);
await host.getByText("This meeting has ended").waitFor();
log("11. host ended meeting; link now shows ended");
console.log("CODE=" + code);
await browser.close();
if (errors.length) {
  console.error("PAGE ERRORS:\n" + errors.join("\n"));
  process.exit(1);
}
log("PASS");
