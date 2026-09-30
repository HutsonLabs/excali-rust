// Playwright config for the performance budgets (ex-710). Serves the output
// of scripts/web/build.sh (run it first; CI does) and the page in
// perf/page. The spec measures; scripts/gates/perf_budget.py holds the
// measurements to the budgets on site/content/plan/phases.md. Run it
// through scripts/web/perf.sh.
import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.PERF_SUITE_PORT || 4190);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "perf",
  // one test at a time: a second browser on the runner would share its CPU
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report-perf" }]]
    : "list",
  outputDir: "test-results-perf",
  use: {
    baseURL,
    trace: "retain-on-failure",
    viewport: { width: 1000, height: 700 },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1000, height: 700 } } }],
  webServer: {
    command: `node lib/serve.mjs --port ${port} --page perf/page`,
    url: `${baseURL}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
