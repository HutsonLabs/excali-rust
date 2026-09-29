// Playwright config for the text editor (ex-512). Serves the wasm harness
// that scripts/web/text-editing.sh builds (tools/text-editing, default
// target/text-editing-harness) and the page in text-editing/page. Run it
// through that script.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.TEXT_EDITING_SUITE_PORT || 4177);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.TEXT_EDITING_HARNESS || join(REPO_ROOT, "target", "text-editing-harness");

export default defineConfig({
  testDir: "text-editing",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-text-editing" }]]
    : "list",
  outputDir: "test-results-text-editing",
  use: {
    baseURL,
    trace: "retain-on-failure",
    viewport: { width: 1200, height: 900 },
  },
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        launchOptions: { args: ["--disable-gpu", "--force-color-profile=srgb"] },
      },
    },
  ],
  webServer: {
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page text-editing/page --expect text_editing.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
