// Unit tests for the static file server the smoke suite runs against.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createStaticServer } from "../lib/serve.mjs";

async function withServer(root, fn) {
  const server = createStaticServer(root);
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const base = `http://127.0.0.1:${server.address().port}`;
  try {
    await fn(base);
  } finally {
    await new Promise((r) => server.close(r));
  }
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "ex006-serve-"));
  mkdirSync(join(root, "plan"));
  writeFileSync(join(root, "index.html"), "<title>home</title>");
  writeFileSync(join(root, "plan", "index.html"), "<title>plan</title>");
  writeFileSync(join(root, "style.css"), "body{}");
  writeFileSync(join(root, "icons.svg"), "<svg/>");
  writeFileSync(join(root, "404.html"), "<title>404</title>");
  return root;
}

test("serves index.html for directory urls with an html content type", async () => {
  await withServer(fixture(), async (base) => {
    const r = await fetch(`${base}/plan/`);
    assert.equal(r.status, 200);
    assert.match(r.headers.get("content-type"), /^text\/html/);
    assert.equal(await r.text(), "<title>plan</title>");
  });
});

test("content types for css and svg", async () => {
  await withServer(fixture(), async (base) => {
    assert.match((await fetch(`${base}/style.css`)).headers.get("content-type"), /^text\/css/);
    assert.match((await fetch(`${base}/icons.svg`)).headers.get("content-type"), /^image\/svg\+xml/);
  });
});

test("redirects a directory without trailing slash, like GitHub Pages", async () => {
  await withServer(fixture(), async (base) => {
    const r = await fetch(`${base}/plan`, { redirect: "manual" });
    assert.equal(r.status, 301);
    assert.equal(r.headers.get("location"), "/plan/");
  });
});

test("missing files are 404 with the site's 404 page", async () => {
  await withServer(fixture(), async (base) => {
    const r = await fetch(`${base}/nope/`);
    assert.equal(r.status, 404);
    assert.equal(await r.text(), "<title>404</title>");
  });
});

test("path traversal cannot escape the root", async () => {
  await withServer(fixture(), async (base) => {
    const r = await fetch(`${base}/..%2f..%2fetc%2fpasswd`);
    assert.equal(r.status, 404);
    const r2 = await fetch(`${base}/%2e%2e/%2e%2e/etc/passwd`);
    assert.equal(r2.status, 404);
  });
});
