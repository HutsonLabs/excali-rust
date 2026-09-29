// Playwright config for the UI primitives (ex-516). Serves the wasm
// harness that scripts/web/ui-primitives.sh builds (tools/ui-primitives,
// default target/ui-primitives-harness) and the page in primitives/page.
// Run it through that script. The viewport is 1024 x 768 at device pixel
// ratio 1, under the 1921px query that enlarges the buttons.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.PRIMITIVES_SUITE_PORT || 4177);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.UI_PRIMITIVES_HARNESS || join(REPO_ROOT, "target", "ui-primitives-harness");

export default defineConfig({
  testDir: "primitives",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-primitives" }]]
    : "list",
  outputDir: "test-results-primitives",
  use: {
    baseURL,
    trace: "retain-on-failure",
    viewport: { width: 1024, height: 768 },
    deviceScaleFactor: 1,
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
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page primitives/page --expect ui_primitives.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
