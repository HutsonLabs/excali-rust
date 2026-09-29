// Playwright config for the library sidebar (ex-526). Serves the wasm
// harness that scripts/web/library-sidebar.sh builds (tools/library-sidebar, default
// target/library-sidebar-harness) and the page in library-sidebar/page. Run it through
// that script.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.LIBRARY_SIDEBAR_SUITE_PORT || 4187);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.LIBRARY_SIDEBAR_HARNESS || join(REPO_ROOT, "target", "library-sidebar-harness");

export default defineConfig({
  testDir: "library-sidebar",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-library-sidebar" }]]
    : "list",
  outputDir: "test-results-library-sidebar",
  use: {
    baseURL,
    trace: "retain-on-failure",
    viewport: { width: 1440, height: 900 },
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
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page library-sidebar/page --expect library_sidebar.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
