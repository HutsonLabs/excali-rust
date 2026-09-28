// Playwright config for the web runtime suite (ex-307). Serves the output of
// scripts/web/build.sh (run it first; CI does) and the test page.
import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.WEB_SUITE_PORT || 4174);
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
    command: `node lib/serve.mjs --port ${port}`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
