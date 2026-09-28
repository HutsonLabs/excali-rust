// excali-canvas2d against the fixture display lists (ex-502).
//
// Every crates/excali-raster/tests/fixtures/display-lists/<name>.json is
// parsed by the Rust fixture loader (tests/support/display_lists.rs, the
// parse excali-raster's own fixture test renders) and painted on a real
// CanvasRenderingContext2D by excali_canvas2d::paint_scaled through
// WebCanvas, in the wasm harness tools/canvas2d-fixtures. In the same page
// scripts/fixtures/raster_references.js, the independent reading that drew
// excali-raster's Chrome references, draws the fixture on a second canvas.
//
// Each test:
//
// - requires the backend's canvas to be within the fixture's tolerance of
//   the independent drawing (at most `pixels` pixels differ by more than
//   `channel` levels in a premultiplied channel, as excali_raster::diff
//   counts), and to differ from an empty canvas by more than it, so no
//   fixture passes by drawing nothing;
// - keeps the backend's canvas as <CANVAS2D_OUT>/<name>.png, beside a
//   manifest.json in the form of the raster references' (this Chromium's
//   user agent and each fixture's SHA-256).
//
// scripts/web/canvas2d-fixtures.sh then runs excali-raster's fixture test
// with EXCALI_RASTER_REFERENCES=<CANVAS2D_OUT>: the tiny-skia backend must
// render every list within the same tolerance of what excali-canvas2d
// painted, which is the acceptance of ex-502.
import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const LISTS = join(REPO_ROOT, "crates", "excali-raster", "tests", "fixtures", "display-lists");
const OUT = process.env.CANVAS2D_OUT || join(REPO_ROOT, "target", "canvas2d-fixtures");

const names = readdirSync(LISTS)
  .filter((f) => f.endsWith(".json"))
  .map((f) => f.slice(0, -".json".length))
  .sort();

const open = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  const failed = await page.evaluate(() => window.harnessError || null);
  expect(failed, "the harness module loads").toBeNull();
  return errors;
};

test("there are fixture display lists", () => {
  expect(names.length).toBeGreaterThan(0);
});

test.beforeAll(async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const page = await browser.newPage();
  const userAgent = await page.evaluate(() => navigator.userAgent);
  await page.close();
  const fixtures = {};
  for (const name of names) {
    fixtures[name] = createHash("sha256")
      .update(readFileSync(join(LISTS, `${name}.json`)))
      .digest("hex");
  }
  // Every worker writes the same file.
  writeFileSync(join(OUT, "manifest.json"), `${JSON.stringify({ userAgent, fixtures }, null, 1)}\n`);
});

test("the harness rejects a list the loader cannot read, naming the problem", async ({ page }) => {
  await open(page);
  const error = await page.evaluate(() =>
    window.drawFixture(JSON.stringify({ width: 4, height: 4, items: [{ type: "text" }] })).then(
      () => null,
      (e) => String(e.message || e),
    ),
  );
  expect(error).toContain('unknown item type "text"');
});

test("built-in images are loaded by WebCanvas itself", async ({ page }) => {
  await open(page);
  const ids = await page.evaluate(() => window.builtinImageIds());
  expect(ids.sort()).toEqual([
    "excalidraw:element-link",
    "excalidraw:external-link",
    "excalidraw:image-error-placeholder",
    "excalidraw:image-placeholder",
  ]);
});

for (const name of names) {
  test(`${name}: excali-canvas2d paints within the fixture's tolerance`, async ({ page }) => {
    const errors = await open(page);
    const text = readFileSync(join(LISTS, `${name}.json`), "utf8");
    const fixture = JSON.parse(text);
    const r = await page.evaluate((t) => window.drawFixture(t), text);
    expect([r.width, r.height]).toEqual([fixture.width, fixture.height]);
    const prefix = "data:image/png;base64,";
    expect(r.png.startsWith(prefix)).toBe(true);
    writeFileSync(join(OUT, `${name}.png`), Buffer.from(r.png.slice(prefix.length), "base64"));

    const { channel, pixels } = fixture.tolerance;
    const report =
      `${name}: ${r.diff.over} of ${r.width * r.height} pixels differ from the independent drawing ` +
      `by more than ${channel} (allowed ${pixels}); max channel difference ${r.diff.max}` +
      (r.diff.worst ? ` at (${r.diff.worst.x}, ${r.diff.worst.y}): canvas2d ` +
        `${JSON.stringify(r.diff.worst.actual)}, reference ${JSON.stringify(r.diff.worst.expected)}` : "");
    console.log(`${report}; over 1/4/8/16/32: ${r.diff.overs.join("/")}`);
    expect(r.diff.over, report).toBeLessThanOrEqual(pixels);
    expect(r.blank.over, `${name}: an empty canvas passes the tolerance`).toBeGreaterThan(pixels);
    expect(errors).toEqual([]);
  });
}
