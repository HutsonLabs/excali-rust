// Playwright config for the site smoke suite (ex-006).
//
// The web server step builds site/ with the pinned Zola (scripts/site/zola.sh)
// into a scratch directory with base_url pointed at the local server, then
// serves it. A fresh build every run: a stale server would test stale pages.
import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.SITE_SMOKE_PORT || 4173);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "specs",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL,
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `node lib/serve.mjs --build --port ${port}`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
