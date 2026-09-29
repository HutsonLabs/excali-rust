// Playwright config for the layered canvases (ex-503). Serves the wasm
// harness that scripts/web/canvas-layers.sh builds (tools/canvas-layers,
// default target/canvas-layers-harness) and the page in layers/page. Run it
// through that script.
//
// Each test opens its own browser context with the device pixel ratio it
// checks (deviceScaleFactor), so the project sets none.
import { join } from "node:path";

import { defineConfig } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.LAYERS_SUITE_PORT || 4176);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.CANVAS_LAYERS_HARNESS || join(REPO_ROOT, "target", "canvas-layers-harness");

export default defineConfig({
  testDir: "layers",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-layers" }]]
    : "list",
  outputDir: "test-results-layers",
  use: {
    baseURL,
    trace: "retain-on-failure",
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
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page layers/page --expect canvas_layers.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
