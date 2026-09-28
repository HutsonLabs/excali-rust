// Playwright config for the Canvas 2D backend's fixture check (ex-502).
// Serves the wasm harness that scripts/web/canvas2d-fixtures.sh builds
// (tools/canvas2d-fixtures, default target/canvas2d-harness, with the
// reader scripts/fixtures/raster_references.js beside it) and the page in
// canvas2d/page. Run it through that script, which also runs the
// excali-raster comparison against the PNGs this suite keeps.
//
// Chromium draws the canvas in software in sRGB, as the raster references'
// headless Chrome does (scripts/fixtures/raster-references.sh).
import { join } from "node:path";

import { defineConfig, devices } from "@playwright/test";

import { REPO_ROOT } from "./lib/serve.mjs";

const port = Number(process.env.CANVAS2D_SUITE_PORT || 4175);
const baseURL = `http://127.0.0.1:${port}`;
const harness = process.env.CANVAS2D_HARNESS || join(REPO_ROOT, "target", "canvas2d-harness");

export default defineConfig({
  testDir: "canvas2d",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-canvas2d" }]]
    : "list",
  outputDir: "test-results-canvas2d",
  use: {
    baseURL,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: { args: ["--disable-gpu", "--force-color-profile=srgb"] },
      },
    },
  ],
  webServer: {
    command: `node lib/serve.mjs --port ${port} --root "${harness}" --page canvas2d/page --expect excali_canvas2d_fixtures.js`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
