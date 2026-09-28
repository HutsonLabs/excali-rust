// Unit tests for the page inventory the smoke suite iterates over (ex-006).
// Run with `node --test unit/` from tests/site.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  VIEWPORTS,
  contentUrlPath,
  contentPages,
  mockupFiles,
  listedMockups,
  sitemapPaths,
  smokeTargets,
  SITE_DIR,
} from "../lib/pages.mjs";

test("viewports are exactly the three sizes in the acceptance criteria", () => {
  assert.deepEqual(
    VIEWPORTS.map((v) => [v.name, v.width, v.height]),
    [
      ["desktop", 1440, 900],
      ["tablet", 1024, 768],
      ["phone", 390, 844],
    ],
  );
});

test("content paths map to Zola URLs", () => {
  assert.equal(contentUrlPath("_index.md"), "/");
  assert.equal(contentUrlPath("plan/_index.md"), "/plan/");
  assert.equal(contentUrlPath("plan/agent-workflow.md"), "/plan/agent-workflow/");
  assert.equal(contentUrlPath("a/b/_index.md"), "/a/b/");
  assert.equal(contentUrlPath("a/b/c.md"), "/a/b/c/");
  assert.equal(contentUrlPath("design-system/tokens.md"), "/design-system/tokens/");
});

test("contentPages walks a content tree, sorted, ignoring non-markdown", () => {
  const root = mkdtempSync(join(tmpdir(), "ex006-content-"));
  mkdirSync(join(root, "plan"));
  writeFileSync(join(root, "_index.md"), "+++\n+++\n");
  writeFileSync(join(root, "plan", "_index.md"), "+++\n+++\n");
  writeFileSync(join(root, "plan", "risks.md"), "+++\n+++\n");
  writeFileSync(join(root, "plan", "diagram.svg"), "<svg/>");
  assert.deepEqual(contentPages(root), ["/", "/plan/", "/plan/risks/"]);
});

test("contentPages refuses front matter that would move or hide a page", () => {
  for (const line of ['path = "elsewhere"', 'slug = "x"', "draft = true", "render = false"]) {
    const root = mkdtempSync(join(tmpdir(), "ex006-content-"));
    writeFileSync(join(root, "_index.md"), "+++\n+++\n");
    writeFileSync(join(root, "p.md"), `+++\ntitle = "P"\n${line}\n+++\nbody\n`);
    assert.throws(() => contentPages(root), /front matter/, line);
  }
});

test("front matter keys only count inside the +++ block", () => {
  const root = mkdtempSync(join(tmpdir(), "ex006-content-"));
  writeFileSync(join(root, "_index.md"), '+++\ntitle = "Home"\n+++\n\n```toml\npath = "fine in a code sample"\n```\n');
  assert.deepEqual(contentPages(root), ["/"]);
});

test("mockupFiles lists the html files only, sorted", () => {
  const dir = mkdtempSync(join(tmpdir(), "ex006-mock-"));
  for (const f of ["02-b.html", "01-a.html", "shell.css", "icons.svg"]) writeFileSync(join(dir, f), "");
  assert.deepEqual(mockupFiles(dir), ["01-a.html", "02-b.html"]);
});

test("listedMockups reads the file entries from the mockups section front matter", () => {
  const md = [
    "+++",
    'title = "Mockups"',
    "[extra]",
    "mockups = [",
    ' { file = "01-a.html", title = "A", summary = "x" },',
    ' { file = "02-b.html", title = "B", summary = "y" },',
    "]",
    "+++",
    'Body text mentioning { file = "ignored.html" }.',
  ].join("\n");
  assert.deepEqual(listedMockups(md), ["01-a.html", "02-b.html"]);
});

test("sitemapPaths returns sorted pathnames relative to the base url", () => {
  const xml = `<?xml version="1.0"?><urlset>
    <url><loc>http://127.0.0.1:4173/plan/</loc></url>
    <url><loc>http://127.0.0.1:4173/</loc></url>
    <url>
      <loc>http://127.0.0.1:4173/plan/risks/</loc>
    </url></urlset>`;
  assert.deepEqual(sitemapPaths(xml), ["/", "/plan/", "/plan/risks/"]);
});

test("smokeTargets covers every content page, every mockup and the 404 page in this repo", () => {
  const targets = smokeTargets();
  const paths = targets.map((t) => t.path);
  assert.equal(new Set(paths).size, paths.length, "no duplicates");
  for (const p of contentPages(join(SITE_DIR, "content"))) assert.ok(paths.includes(p), p);
  for (const f of mockupFiles(join(SITE_DIR, "static", "mockups"))) assert.ok(paths.includes(`/mockups/${f}`), f);
  // The 404 page is reached the way visitors reach it: an unknown URL.
  const missing = targets.filter((t) => t.kind === "not-found");
  assert.equal(missing.length, 1);
  assert.equal(missing[0].expectStatus, 404);
  for (const t of targets.filter((t) => t.kind !== "not-found")) assert.equal(t.expectStatus, 200, t.path);
  // 36 content pages and 7 mockups when this suite was written; the inventory
  // can grow, but an empty or broken walk must never pass silently.
  assert.ok(targets.filter((t) => t.kind === "mockup").length >= 7);
  assert.ok(targets.filter((t) => t.kind === "page").length >= 36);
});
