// Browser E2E with real media: two participants through the whole meeting flow,
// plus a meeting created without an account.
// Needs the API (with ALLOW_GUEST_CREATE=true), web (BASE_URL) and LiveKit running,
// plus Google Chrome installed. Usage: npm run e2e -- [screenshot-dir]
import { mkdirSync } from "node:fs";
import { chromium } from "playwright-core";

const BASE = process.env.BASE_URL ?? "http://localhost:3000";
const OUT = process.argv[2] ?? "e2e/screenshots";
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({
  channel: "chrome",
  headless: true,
  args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"],
});
const errors = [];
const log = (...a) => console.log(...a);

async function newPage(label) {
  const ctx = await browser.newContext({ permissions: ["camera", "microphone"], viewport: { width: 1280, height: 800 } });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => errors.push(`${label}: ${e.message}`));
  return page;
}
const tiles = (page) => page.locator(".lk-participant-tile");

try {
  // ---------- Account host, password + lobby ----------
  const host = await newPage("host");
  await host.goto(`${BASE}/register`);
  await host.fill("#display_name", "Host Turtle");
  await host.fill("#email", `host-${Date.now()}@example.test`);
  await host.fill("#password", "s3cure-passphrase");
  await host.click("button[type=submit]");
  await host.getByText("Host Turtle").waitFor();
  await host.getByRole("button", { name: "New meeting" }).click();
  await host.waitForURL(/\/m\/[a-z]{3}-[a-z]{4}-[a-z]{3}$/);
  const code = host.url().split("/m/")[1];
  await host.getByRole("button", { name: "Join now" }).click();
  await tiles(host).first().waitFor({ timeout: 15000 });
  log("1. host registered, created", code, "and joined");

  await host.getByRole("button", { name: /^People/ }).click();
  await host.getByLabel("Lobby: approve people before they join").check();
  await host.getByText("Meeting settings saved").first().waitFor();
  await host.locator("#host-password").fill("open-sesame");
  await host.getByRole("button", { name: "Save" }).click();
  await host.getByText("Meeting password (set)").waitFor();
  log("2. host enabled lobby and set a password");

  const guest = await newPage("guest");
  await guest.goto(`${BASE}/m/${code}`);
  await guest.locator("#username").fill("Guest Tortoise");
  await guest.locator("#meeting-password").fill("wrong-password");
  await guest.getByRole("button", { name: "Join now" }).click();
  await guest.getByText("incorrect meeting password").waitFor();
  await guest.locator("#meeting-password").fill("open-sesame");
  await guest.getByRole("button", { name: "Join now" }).click();
  await guest.getByText("Waiting for the host to let you in").waitFor();
  log("3. wrong password rejected; right password -> lobby");

  const lobby = host.getByRole("region", { name: "Lobby requests" });
  await lobby.getByText("Guest Tortoise").waitFor({ timeout: 10000 });
  await lobby.getByRole("button", { name: "Admit" }).click();
  await tiles(guest).nth(1).waitFor({ timeout: 20000 });
  await tiles(host).nth(1).waitFor({ timeout: 15000 });
  await host.waitForFunction(
    () => [...document.querySelectorAll("video")].some((v) => v.videoWidth > 0 && !v.closest("[data-lk-local-participant='true']")),
    null,
    { timeout: 20000 },
  );
  log("4. host admitted guest from lobby; remote video flowing");

  // ---------- Raise hand + reactions ----------
  await guest.getByRole("button", { name: "Raise hand" }).click();
  const people = host.locator("[data-testid=people-list]");
  await people.getByLabel("Hand raised").waitFor({ timeout: 10000 });
  await guest.getByRole("button", { name: "Lower hand" }).waitFor();
  log("5. guest raised hand; host sees it");
  await guest.getByRole("button", { name: "Reactions" }).click();
  await guest.getByRole("button", { name: "React 👏" }).click();
  await host.getByText("👏").waitFor({ timeout: 10000 });
  log("6. reaction delivered");

  // ---------- Remote mute ----------
  await people.getByRole("button", { name: "Mute Guest Tortoise" }).click();
  await people.getByLabel("Muted").waitFor({ timeout: 10000 });
  log("7. host muted guest");
  await host.screenshot({ path: `${OUT}/host-panel.png` });

  // ---------- Chat ----------
  await guest.getByRole("button", { name: /Chat/ }).click();
  await guest.getByLabel("Message").fill("hello from the guest");
  await guest.getByRole("button", { name: "Send" }).click();
  await host.getByLabel("1 unread").waitFor({ timeout: 10000 });
  await host.getByRole("button", { name: /Chat/ }).click();
  await host.locator("[data-testid=chat-messages]").getByText("hello from the guest").waitFor();
  await host.getByLabel("Message").fill("welcome!");
  await host.getByRole("button", { name: "Send" }).click();
  await guest.locator("[data-testid=chat-messages]").getByText("welcome!").waitFor();
  log("8. chat both ways with unread badge");

  // ---------- Screen share ----------
  await host.getByRole("button", { name: /Share screen/ }).click();
  await guest.locator(".lk-focus-layout").waitFor({ timeout: 15000 });
  await host.getByRole("button", { name: /Stop screen share/ }).click();
  await guest.locator(".lk-focus-layout").waitFor({ state: "detached", timeout: 15000 });
  log("9. screen share shown in focus layout");

  // ---------- Remove, persistence, end ----------
  await host.getByRole("button", { name: /^People/ }).click();
  host.once("dialog", (d) => d.accept());
  await people.getByRole("button", { name: "Remove Guest Tortoise" }).click();
  await guest.getByText("You were removed from the meeting by the host").waitFor({ timeout: 15000 });
  if (await guest.getByRole("button", { name: "Rejoin" }).count()) throw new Error("removed guest offered rejoin");
  log("10. host removed guest");

  await host.reload();
  await host.getByRole("button", { name: "Join now" }).click();
  await host.getByRole("button", { name: /Chat/ }).click();
  await host.locator("[data-testid=chat-messages]").getByText("hello from the guest").waitFor();
  log("11. chat history persisted across rejoin");

  host.once("dialog", (d) => d.accept());
  await host.getByRole("button", { name: "End for all" }).click();
  await host.getByText("The host ended the meeting").waitFor({ timeout: 15000 });
  await host.goto(`${BASE}/m/${code}`);
  await host.getByText("This meeting has ended").waitFor();
  log("12. host ended meeting; link shows ended");

  // ---------- Meeting created without an account ----------
  const anon = await newPage("anon");
  await anon.goto(`${BASE}/`);
  await anon.getByRole("link", { name: "Sign in" }).waitFor(); // signed out
  await anon.getByRole("button", { name: "New meeting" }).click();
  await anon.waitForURL(/\/m\/[a-z]{3}-[a-z]{4}-[a-z]{3}$/);
  await anon.locator("#username").fill("Anon Creator");
  await anon.getByRole("button", { name: "Join now" }).click();
  await anon.getByRole("button", { name: "End for all" }).waitFor({ timeout: 15000 });
  log("13. visitor without account created a meeting and is its host");
} finally {
  await browser.close();
}

if (errors.length) {
  console.error("PAGE ERRORS:\n" + errors.join("\n"));
  process.exit(1);
}
log("PASS");
