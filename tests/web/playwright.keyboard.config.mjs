// Playwright config for the keyboard handling (ex-515). Serves the wasm
// harness that scripts/web/keyboard.sh builds (tools/keyboard, default
// target/keyboard-harness) and the page in keyboard/page. Run it through
// that script.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.KEYBOARD_SUITE_PORT || 4177);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.KEYBOARD_HARNESS || join(REPO_ROOT, "target", "keyboard-harness");

export default defineConfig({
  testDir: "keyboard",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-keyboard" }]]
    : "list",
  outputDir: "test-results-keyboard",
  use: {
    baseURL,
    trace: "retain-on-failure",
    viewport: { width: 1000, height: 700 },
  },
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        launchOptions: { args: ["--disable-gpu"] },
      },
    },
  ],
  webServer: {
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page keyboard/page --expect keyboard.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
