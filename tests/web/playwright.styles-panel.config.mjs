// Playwright config for the full styles panel (ex-519). Serves the wasm
// harness that scripts/web/styles-panel.sh builds (tools/styles-panel, default
// target/styles-panel-harness) and the page in styles-panel/page. Run it through
// that script.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.STYLES_PANEL_SUITE_PORT || 4181);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.STYLES_PANEL_HARNESS || join(REPO_ROOT, "target", "styles-panel-harness");

export default defineConfig({
  testDir: "styles-panel",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-styles-panel" }]]
    : "list",
  outputDir: "test-results-styles-panel",
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
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page styles-panel/page --expect styles_panel.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
