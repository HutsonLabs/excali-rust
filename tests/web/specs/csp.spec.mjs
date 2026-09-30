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
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { createWebServer, PAGE_DIR, REPO_ROOT } from "../lib/serve.mjs";

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

// The runtime on another origin (a CDN), as the guide describes it (ex-803):
// that origin added to the directives the guide names, and the runtime's
// server sending Access-Control-Allow-Origin, since a module script, init()'s
// fetch of the wasm and web fonts are all CORS requests. Two more servers of
// the build on their own ports are that other origin, one with the header
// and one without. (Route handlers cannot stand in: Playwright's fulfilled
// responses skip the browser's CORS check.)
const DIRECTIVES = (() => {
  const m = /on another origin \(a CDN\), put that origin in ((?:`[a-z-]+`(?:, | and )?)+)/.exec(GUIDE);
  if (!m) throw new Error("integration.md does not say which directives take a CDN origin");
  return [...m[1].matchAll(/`([a-z-]+)`/g)].map((d) => d[1]);
})();

const cdnHeader = (origin) =>
  HEADER.split("; ")
    .map((d) => (DIRECTIVES.includes(d.split(" ")[0]) ? `${d} ${origin}` : d))
    .join("; ");

// Per case, the runtime's server and a server of the page (csp.html and
// csp-app.js pointed at the runtime's origin, with the policy for it). The
// page is served for real rather than through a route: Chromium treats a
// fulfilled page as public and refuses its requests to a loopback server.
const cdn = {};

const listen = async (server) => {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return `http://127.0.0.1:${server.address().port}`;
};

test.beforeAll(async () => {
  for (const cors of [true, false]) {
    const runtime = createWebServer(join(REPO_ROOT, "dist"));
    if (cors) {
      runtime.prependListener("request", (_req, res) => res.setHeader("access-control-allow-origin", "*"));
    }
    const origin = await listen(runtime);
    const dir = mkdtempSync(join(tmpdir(), "excali-csp-cdn-"));
    writeFileSync(
      join(dir, "csp.html"),
      readFileSync(join(PAGE_DIR, "csp.html"), "utf8").replace('href="/excali.css"', `href="${origin}/excali.css"`),
    );
    writeFileSync(
      join(dir, "csp-app.js"),
      APP_JS.toString().replace('"/excali_editor.js"', `"${origin}/excali_editor.js"`),
    );
    const pageServer = createWebServer(dir, dir);
    pageServer.prependListener("request", (_req, res) =>
      res.setHeader("content-security-policy", cdnHeader(origin)),
    );
    cdn[cors] = { origin, page: `${await listen(pageServer)}/csp.html`, servers: [runtime, pageServer], dir };
  }
});

test.afterAll(async () => {
  for (const { servers, dir } of Object.values(cdn)) {
    await Promise.all(servers.map((s) => new Promise((r) => s.close(r))));
    rmSync(dir, { recursive: true, force: true });
  }
});

async function openFromCdn(page, { page: url }) {
  const violations = [];
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.exposeFunction("reportViolation", (v) => violations.push(v));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) =>
      window.reportViolation(`${e.violatedDirective} ${e.blockedURI || "inline"}`),
    );
  });
  await page.goto(url);
  return { violations, errors };
}

test("from another origin that sends Access-Control-Allow-Origin, under the guide's policy for it, the editor works", async ({
  page,
}) => {
  const { origin } = cdn[true];
  expect(cdnHeader(origin)).toContain(`script-src 'self' 'wasm-unsafe-eval' ${origin}`);
  expect(cdnHeader(origin)).toContain(`font-src 'self' data: ${origin}`);
  const { violations, errors } = await openFromCdn(page, cdn[true]);
  await page.waitForFunction(() => window.editor?.querySelector("canvas"));
  const state = await page.evaluate(async (scene) => {
    await window.editor.load(scene);
    await document.fonts.ready;
    return window.editor.getState();
  }, SCENE);
  await page.waitForFunction(() => [...document.fonts].some((f) => f.status === "loaded"));
  expect(state.elementCount).toBe(3);
  // excali.css from the other origin applies
  expect(await page.evaluate(() => getComputedStyle(window.editor).position)).toBe("relative");
  expect(violations).toEqual([]);
  expect(errors).toEqual([]);
});

test("from another origin without Access-Control-Allow-Origin the module is refused", async ({ page }) => {
  const { origin } = cdn[false];
  const { violations, errors } = await openFromCdn(page, cdn[false]);
  await expect
    .poll(() => errors.some((e) => e.includes(`${origin}/excali_editor.js`) && e.includes("CORS")))
    .toBe(true);
  expect(await page.evaluate(() => window.editor)).toBeUndefined();
  expect(violations).toEqual([]);
});
