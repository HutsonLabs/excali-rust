// The performance budgets (ex-710), in Chromium against the release
// scripts/web/build.sh builds (dist/). Each test measures one budget of
// site/content/plan/phases.md (## Budgets) and writes its numbers to
// PERF_RESULTS (default test-results-perf/perf.json);
// scripts/gates/perf_budget.py compares them with the table, so the page
// is the single source of the limits. Here the tests only check that the
// measurement measured something: the 1,000 elements loaded, the view
// moved on every frame and the static canvas was drawn again.
//
// Pan: the view scrolls by a wheel event per frame (the handler repaints
// synchronously, EditorCore's wheel listener then after_event) over the
// 1,000-element scene of perf/scene.mjs, every element in the viewport. A
// frame's time runs from dispatching the wheel event to the static canvas
// read back with one getImageData, which makes Chromium execute the frame's
// deferred drawing; the budget holds the 95th percentile of the measured
// frames after a warm-up, the median of three rounds, each in a fresh page.
// First paint is the median of five fresh loads after one load that is not
// measured.
//
// Calibration: before each first paint load and each pan round the
// calibration workload of perf/page/calibrate.html runs in a fresh page of
// the same browser, and each measurement records the median of its own
// calibrations (firstPaintCalibrationMs, panCalibrationMs);
// scripts/gates/perf_budget.py scales the measurement by the phases page's
// reference calibration over that median, so a slower or busier runner
// gets proportionally more milliseconds.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

import { expect, test } from "@playwright/test";

import { COUNT, sceneJson } from "./scene.mjs";

const RESULTS = resolve(process.env.PERF_RESULTS || join("test-results-perf", "perf.json"));
const WARMUP = 20;
const FRAMES = 240;
const FIRST_PAINT_RUNS = 5;
// a first load, not measured: the browser's own first-launch work (disk
// cache, font and code caches) is not the editor's
const FIRST_PAINT_WARMUP = 1;
const PAN_ROUNDS = 3;
// the calibration's wasm hash (calibrate.html) over this many iterations
const CALIBRATION_ITERATIONS = 10_000_000;

/** Merges `entry` into the results file. */
const record = (entry) => {
  let results = {};
  try {
    results = JSON.parse(readFileSync(RESULTS, "utf8"));
  } catch {
    // first test of the run
  }
  mkdirSync(dirname(RESULTS), { recursive: true });
  writeFileSync(RESULTS, `${JSON.stringify({ ...results, ...entry }, null, 2)}\n`);
};

const percentile = (values, p) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)];
};

/** Opens the page with the scene and waits for its first paint. */
const open = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.addInitScript((scene) => {
    window.PERF_SCENE = scene;
  }, sceneJson());
  await page.goto("/");
  await page.waitForFunction(() => window.firstPaint !== undefined, null, { timeout: 30_000 });
  return errors;
};

/** The integer hash of calibrate.html's wasm module, in JS. */
const calibrationHash = (n) => {
  let h = 0;
  for (let i = 0; i < n; i++) h = ((Math.imul(h, 1103515245) + 12345) | 0) ^ i;
  return h;
};

/** Runs the calibration workload in a fresh page: its record. */
const calibrate = async (browser) => {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/calibrate.html");
  await page.waitForFunction(() => window.calibration !== undefined, null, { timeout: 30_000 });
  const calibration = await page.evaluate(() => window.calibration);
  await page.close();
  expect(errors).toEqual([]);
  expect(calibration.wasm).toBe(calibrationHash(CALIBRATION_ITERATIONS));
  expect(calibration.ms).toBeGreaterThan(0);
  return calibration;
};

const round2 = (v) => Number(v.toFixed(2));

/** The calibrations' median and each run, for the record. */
const calibrationRecord = (prefix, runs) => ({
  [`${prefix}CalibrationMs`]: round2(percentile(runs.map((c) => c.ms), 50)),
  [`${prefix}CalibrationRunsMs`]: runs.map((c) => round2(c.ms)),
  [`${prefix}CalibrationPartsMs`]: Object.fromEntries(
    Object.keys(runs[0].parts).map((k) => [k, round2(percentile(runs.map((c) => c.parts[k]), 50))]),
  ),
});

test("the calibration workload does the same work on every run", async ({ browser }) => {
  const a = await calibrate(browser);
  const b = await calibrate(browser);
  // the same hash (checked against JS in calibrate()) and the same pixels
  expect(b.wasm).toBe(a.wasm);
  expect(b.digest).toBe(a.digest);
  expect(a.digest).not.toBe(0);
  for (const part of ["wasmMs", "canvasMs", "jsMs"]) expect(a.parts[part]).toBeGreaterThan(0);
});

test("the scene holds 1,000 elements, all in the viewport", async ({ page }) => {
  const errors = await open(page);
  const saved = await page.evaluate(() => JSON.parse(window.ed.save()).elements);
  expect(saved.filter((e) => !e.isDeleted)).toHaveLength(COUNT);
  expect(COUNT).toBe(1000);
  for (const e of saved) {
    expect(e.x).toBeGreaterThanOrEqual(0);
    expect(e.y).toBeGreaterThanOrEqual(0);
    expect(e.x + e.width).toBeLessThanOrEqual(1000);
    expect(e.y + e.height).toBeLessThanOrEqual(700);
  }
  expect(errors).toEqual([]);
});

test("the first paint draws the scene once", async ({ page }) => {
  // every drawImage on a canvas until the first frame is presented: the
  // load's frame blits each element's bitmap once, and nothing repaints
  // the unchanged scene before the frame (upstream re-renders on a
  // ResizeObserver callback only when the size changed)
  await page.addInitScript(() => {
    window.drawImages = 0;
    const draw = CanvasRenderingContext2D.prototype.drawImage;
    CanvasRenderingContext2D.prototype.drawImage = function (...args) {
      if (!window.presented && this.canvas.classList.contains("static")) {
        window.drawImages++;
      }
      return draw.apply(this, args);
    };
  });
  const errors = await open(page);
  expect(await page.evaluate(() => window.drawImages)).toBe(COUNT);
  expect(errors).toEqual([]);
});

test("first paint after module load", async ({ browser }) => {
  // the median of fresh loads, each in a new page, so one slow start on a
  // shared runner does not decide the budget
  const runs = [];
  const calibrations = [];
  for (let i = 0; i < FIRST_PAINT_WARMUP; i++) {
    await calibrate(browser);
    const page = await browser.newPage();
    await open(page);
    await page.close();
  }
  for (let i = 0; i < FIRST_PAINT_RUNS; i++) {
    calibrations.push(await calibrate(browser));
    const page = await browser.newPage();
    const errors = await open(page);
    const first = await page.evaluate(() => window.firstPaint);
    // the static canvas holds the scene: not all background
    const inked = await page.evaluate(() => {
      const canvas = window.ed.querySelector("canvas.static");
      const { data } = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height);
      let n = 0;
      for (let k = 0; k < data.length; k += 4) if (data[k] < 200) n++;
      return n;
    });
    expect(inked).toBeGreaterThan(10_000);
    expect(first.ms).toBeGreaterThan(0);
    expect(errors).toEqual([]);
    runs.push(first);
    await page.close();
  }
  const ms = runs.map((r) => r.ms);
  record({
    firstPaintMs: Number(percentile(ms, 50).toFixed(2)),
    firstPaintRunsMs: ms.map((v) => Number(v.toFixed(2))),
    ...calibrationRecord("firstPaint", calibrations),
    fontsLoadedMs: Number(percentile(runs.map((r) => r.fontsMs), 50).toFixed(2)),
    // the median of each phase: module init, mount, load(), the static
    // canvas's raster and presenting the frame
    firstPaintPhasesMs: Object.fromEntries(
      Object.keys(runs[0].phases).map((k) => [
        k,
        Number(percentile(runs.map((r) => r.phases[k]), 50).toFixed(2)),
      ]),
    ),
  });
});

/** One pan round over the scene in a fresh page. */
const panRound = async (browser) => {
  const page = await browser.newPage();
  const errors = await open(page);
  const run = await page.evaluate(
    async ({ warmup, frames }) => {
      const ed = window.ed;
      const target = ed.querySelector("canvas.interactive");
      const canvas = ed.querySelector("canvas.static");
      const ctx = canvas.getContext("2d");
      const rect = target.getBoundingClientRect();
      const frame = () => new Promise((r) => requestAnimationFrame(r));
      // a digest of the canvas's middle, read outside the timed span
      const probe = () => {
        const { data } = ctx.getImageData(canvas.width / 2 - 100, canvas.height / 2 - 100, 200, 200);
        let h = 0;
        for (let k = 0; k < data.length; k++) h = (Math.imul(h, 31) + data[k]) | 0;
        return h;
      };
      const times = [];
      const handler = [];
      const probes = [probe()];
      const stamps = [];
      for (let i = 0; i < warmup + frames; i++) {
        stamps.push(await frame());
        // a slow circle: the view moves every frame and stays near the scene
        const a = (i / 30) * Math.PI;
        const t = performance.now();
        const handled = () => performance.now() - t;
        target.dispatchEvent(
          new WheelEvent("wheel", {
            deltaX: Math.round(6 * Math.cos(a)),
            deltaY: Math.round(6 * Math.sin(a)) || 1,
            clientX: rect.left + rect.width / 2,
            clientY: rect.top + rect.height / 2,
            bubbles: true,
            cancelable: true,
          }),
        );
        const h = handled();
        ctx.getImageData(0, 0, 1, 1);
        const dt = performance.now() - t;
        if (i >= warmup) {
          times.push(dt);
          handler.push(h);
        }
        probes.push(probe());
      }
      const intervals = stamps.slice(warmup + 1).map((s, k) => s - stamps[warmup + k]);
      return { times, handler, probes, intervals };
    },
    { warmup: WARMUP, frames: FRAMES },
  );
  await page.close();
  expect(run.times).toHaveLength(FRAMES);
  // the view moved on every frame: the static canvas differs from the frame before
  for (let i = 1; i < run.probes.length; i++) expect(run.probes[i]).not.toBe(run.probes[i - 1]);
  expect(errors).toEqual([]);
  const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;
  return {
    p50: percentile(run.times, 50),
    p95: percentile(run.times, 95),
    max: Math.max(...run.times),
    fps: 1000 / mean(run.intervals),
    // the wheel handler alone (wasm and canvas calls), before the raster
    handlerP50: percentile(run.handler, 50),
    handlerP95: percentile(run.handler, 95),
  };
};

test("pan at 1,000 elements", async ({ browser }) => {
  // an over-budget build still finishes, so the gate can report by how much
  test.setTimeout(300_000);
  const rounds = [];
  const calibrations = [];
  for (let r = 0; r < PAN_ROUNDS; r++) {
    calibrations.push(await calibrate(browser));
    rounds.push(await panRound(browser));
  }
  const median = (k) => round2(percentile(rounds.map((round) => round[k]), 50));
  record({
    panFrames: FRAMES,
    panRounds: PAN_ROUNDS,
    panFrameP50Ms: median("p50"),
    panFrameP95Ms: median("p95"),
    panFrameP95RunsMs: rounds.map((round) => round2(round.p95)),
    panFrameMaxMs: round2(Math.max(...rounds.map((round) => round.max))),
    panFps: Number(percentile(rounds.map((round) => round.fps), 50).toFixed(1)),
    panHandlerP50Ms: median("handlerP50"),
    panHandlerP95Ms: median("handlerP95"),
    ...calibrationRecord("pan", calibrations),
  });
});
