// The web runtime loads only the font ranges a scene uses (ex-307).
//
// For every scene of excali-text's font-assets goldens (upstream's own
// inputs, tools/goldens/font-assets.mjs), in Chromium:
//
// - registerFonts("/fonts/") adds every non-local face of the manifest to
//   document.fonts and fetches nothing;
// - loadSceneFonts(scene) makes the browser fetch exactly the files
//   sceneFontFiles(scene) names (the Rust selection in
//   excali_text::font_assets), each once, and no other font file;
// - the faces it reports loaded, and the faces document.fonts marks
//   "loaded", are those files.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const golden = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", "font-assets.json"), "utf8"),
);
const manifest = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates", "excali-text", "assets", "fonts", "manifest.json"), "utf8"),
);
const registeredFaces = manifest.families.filter((f) => !f.local).flatMap((f) => f.faces);

const base = (i, type, isDeleted) => ({
  id: `e${i}`,
  type,
  x: 0,
  y: 40 * i,
  width: 100,
  height: 25,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  index: null,
  roundness: null,
  seed: i + 1,
  version: 1,
  versionNonce: 0,
  isDeleted,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
});

/** A .excalidraw document holding the fixture scene's elements. */
const sceneJson = (scene) =>
  JSON.stringify({
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements: scene.elements.map((e, i) =>
      e.type === "text"
        ? {
            ...base(i, "text", e.isDeleted),
            text: e.originalText,
            fontSize: 20,
            fontFamily: e.fontFamily,
            textAlign: "left",
            verticalAlign: "top",
            containerId: null,
            originalText: e.originalText,
            autoResize: true,
            lineHeight: 1.25,
          }
        : base(i, e.type, e.isDeleted),
    ),
    appState: {},
    files: {},
  });

const fontPath = (url) => {
  const { pathname } = new URL(url);
  return pathname.startsWith("/fonts/") ? decodeURIComponent(pathname.slice("/fonts/".length)) : null;
};

const open = async (page) => {
  const fetched = [];
  page.on("request", (req) => {
    const file = fontPath(req.url());
    if (file !== null) fetched.push(file);
  });
  await page.goto("/");
  await page.waitForFunction(() => window.excaliReady === true);
  return fetched;
};

test("registering the manifest fetches no font file", async ({ page }) => {
  const fetched = await open(page);
  const registered = await page.evaluate(() => window.excali.registerFonts("/fonts/"));
  expect(registered).toBe(registeredFaces.length);
  const faces = await page.evaluate(() =>
    [...document.fonts].map((f) => ({ family: f.family, status: f.status, unicodeRange: f.unicodeRange })),
  );
  expect(faces).toHaveLength(registeredFaces.length);
  expect(faces.every((f) => f.status === "unloaded")).toBe(true);
  // Registering twice adds nothing.
  await page.evaluate(() => window.excali.registerFonts("/fonts/"));
  expect(await page.evaluate(() => document.fonts.size)).toBe(registeredFaces.length);
  await page.waitForTimeout(250);
  expect(fetched).toEqual([]);
});

test("registering from another base URL replaces the faces and loads from there", async ({ page }) => {
  const requested = [];
  page.on("request", (req) => requested.push(new URL(req.url()).pathname));
  await page.goto("/");
  await page.waitForFunction(() => window.excaliReady === true);
  await page.evaluate(() => window.excali.registerFonts("/nowhere/"));
  const n = await page.evaluate(() => window.excali.registerFonts("/fonts"));
  expect(n).toBe(registeredFaces.length);
  expect(await page.evaluate(() => document.fonts.size)).toBe(registeredFaces.length);
  const scene = golden.scenes.find((s) => s.name === "excalifont-latin");
  const loaded = await page.evaluate((text) => window.excali.loadSceneFonts(text), sceneJson(scene));
  expect(loaded).toEqual(["Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"]);
  expect(requested.filter((p) => p.endsWith(".woff2") || p.endsWith(".ttf"))).toEqual([
    "/fonts/Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2",
  ]);
});

test("loadSceneFonts rejects before registerFonts", async ({ page }) => {
  await open(page);
  const error = await page.evaluate(async () => {
    try {
      await window.excali.loadSceneFonts('{"type":"excalidraw","version":2,"elements":[]}');
      return null;
    } catch (e) {
      return String(e.message);
    }
  });
  expect(error).toContain("registerFonts");
});

for (const scene of golden.scenes) {
  test(`scene ${scene.name}: only the ranges its text uses are fetched`, async ({ page }) => {
    const fetched = await open(page);
    await page.evaluate(() => window.excali.registerFonts("/fonts/"));
    const json = sceneJson(scene);
    const { plan, loaded, statuses } = await page.evaluate(async (text) => {
      const plan = window.excali.sceneFontFiles(text);
      const loaded = await window.excali.loadSceneFonts(text);
      const statuses = [...document.fonts]
        .filter((f) => f.status !== "unloaded")
        .map((f) => ({ family: f.family, status: f.status, unicodeRange: f.unicodeRange }));
      return { plan, loaded, statuses };
    }, json);
    const sorted = (xs) => [...xs].sort();
    expect(sorted(fetched)).toEqual(sorted(plan));
    expect(new Set(fetched).size).toBe(fetched.length);
    expect(sorted(loaded)).toEqual(sorted(plan));
    expect(statuses.every((s) => s.status === "loaded")).toBe(true);
    expect(statuses).toHaveLength(plan.length);
    for (const file of plan) {
      expect(registeredFaces.some((f) => f.file === file)).toBe(true);
    }
  });
}
