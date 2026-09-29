// The integration guide's Content-Security-Policy for a browser host
// (ex-607), in Chromium. The header is read from the guide itself
// (site/content/architecture/integration.md, the block marked
// `<!-- snippet: web-csp ... -->`; scripts/site/snippets.py holds that block
// to the example Tauri app's CSP), and tests/web/page/csp.html, which has no
// inline script or style, is served with it: the editor mounts, opens a
// scene with text, loads its fonts, saves and exports with no violation.
// Taking out 'wasm-unsafe-eval', style-src's 'unsafe-inline' or img-src's
// data:, or adding a nonce to style-src (what Tauri does for a <style> in
// the page), breaks it the way the guide says.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { PAGE_DIR, REPO_ROOT } from "../lib/serve.mjs";

const GUIDE = readFileSync(
  join(REPO_ROOT, "site", "content", "architecture", "integration.md"),
  "utf8",
);
const SCENE = readFileSync(
  join(REPO_ROOT, "examples", "tauri-app", "smoke", "open.excalidraw"),
  "utf8",
);
const APP_JS = readFileSync(join(PAGE_DIR, "csp-app.js"));

const HEADER = (() => {
  const m = /<!-- snippet: web-csp [^>]*-->\s*```text\nContent-Security-Policy: (.+)\n```/.exec(GUIDE);
  if (!m) throw new Error("integration.md has no web-csp block");
  return m[1];
})();

// The header with one source taken out of one directive.
const without = (directive, source) =>
  HEADER.split("; ")
    .map((d) => {
      const [name, ...sources] = d.split(" ");
      return name === directive ? [name, ...sources.filter((s) => s !== source)].join(" ") : d;
    })
    .join("; ");

async function open(page, csp) {
  const violations = [];
  await page.exposeFunction("reportViolation", (v) => violations.push(v));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) =>
      window.reportViolation(`${e.violatedDirective} ${e.blockedURI || "inline"}`),
    );
  });
  await page.route("**/csp.html", async (route) => {
    const response = await route.fetch();
    await route.fulfill({
      response,
      headers: { ...response.headers(), "content-security-policy": csp },
    });
  });
  await page.route("**/csp-app.js", (route) =>
    route.fulfill({ body: APP_JS, contentType: "text/javascript; charset=utf-8" }),
  );
  await page.goto("/csp.html");
  return violations;
}

test("the guide's header is a policy: scripts from the origin and wasm only", () => {
  expect(HEADER).toContain("script-src 'self' 'wasm-unsafe-eval'");
  expect(HEADER).not.toContain("'unsafe-eval'");
  expect(HEADER).not.toMatch(/script-src[^;]*'unsafe-inline'/);
  expect(HEADER).not.toContain("ipc");
});

test("under the guide's header the editor opens, renders text, saves and exports with no violation", async ({
  page,
}) => {
  const violations = await open(page, HEADER);
  await page.waitForFunction(() => window.editor?.querySelector("canvas"));
  const result = await page.evaluate(async (scene) => {
    const ed = window.editor;
    await ed.load(scene);
    await document.fonts.ready;
    const svg = ed.export("svg", { embedScene: true });
    const png = await ed.export("png", { scale: 1, background: true });
    return {
      state: ed.getState(),
      saved: JSON.parse(ed.save()).elements.length,
      svg: svg.includes("<svg"),
      png: png.type === "image/png" && png.size > 0,
    };
  }, SCENE);
  // The text element's font face, fetched under font-src.
  await page.waitForFunction(() => [...document.fonts].some((f) => f.status === "loaded"));
  expect(result).toMatchObject({ state: { elementCount: 3 }, saved: 3, svg: true, png: true });
  expect(violations).toEqual([]);
});

test("without 'wasm-unsafe-eval' the module does not compile", async ({ page }) => {
  const violations = await open(page, without("script-src", "'wasm-unsafe-eval'"));
  await page.waitForFunction(() => window.initError !== undefined);
  expect(await page.evaluate(() => window.initError)).toMatch(/WebAssembly|wasm-unsafe-eval/);
  expect(violations.some((v) => v.startsWith("script-src"))).toBe(true);
  expect(await page.evaluate(() => window.editor)).toBeUndefined();
});

test("without style-src 'unsafe-inline' the editor's own styles are refused", async ({ page }) => {
  const violations = await open(page, without("style-src", "'unsafe-inline'"));
  await page.waitForFunction(() => window.editor?.querySelector("canvas"));
  expect(violations.some((v) => v.startsWith("style-src"))).toBe(true);
});

test("a nonce in style-src turns 'unsafe-inline' off and refuses the editor's styles", async ({
  page,
}) => {
  const csp = HEADER.replace("style-src 'self' 'unsafe-inline'", "style-src 'self' 'unsafe-inline' 'nonce-excali'");
  expect(csp).not.toBe(HEADER);
  const violations = await open(page, csp);
  await page.waitForFunction(() => window.editor?.querySelector("canvas"));
  expect(violations.some((v) => v.startsWith("style-src"))).toBe(true);
});

test("without img-src data: the canvas's built-in images are refused", async ({ page }) => {
  const violations = await open(page, without("img-src", "data:"));
  await page.waitForFunction(() => window.editor?.querySelector("canvas"));
  await expect.poll(() => violations.some((v) => v.startsWith("img-src data"))).toBe(true);
});
