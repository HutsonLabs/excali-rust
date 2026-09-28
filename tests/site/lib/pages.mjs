// Inventory of what the site smoke suite visits (ex-006): every Zola content
// page, every static mockup, and the 404 page, at the three review viewports.
//
// The list is derived from the source tree (site/content, site/static/mockups)
// so tests can be generated before the site is built; the suite then checks
// the built sitemap against it, so a page Zola publishes cannot be skipped.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const SITE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "site");

// Acceptance of ex-006: desktop, small laptop/tablet landscape, phone.
export const VIEWPORTS = Object.freeze([
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet", width: 1024, height: 768 },
  { name: "phone", width: 390, height: 844 },
]);

// A URL that is never a page, to reach the 404 page as visitors do.
export const MISSING_PATH = "/ex-006-no-such-page/";

/** URL path Zola gives a content file (relative, `/`-separated). */
export function contentUrlPath(rel) {
  const parts = rel.split("/");
  const file = parts.pop();
  if (file !== "_index.md") parts.push(file.replace(/\.md$/, ""));
  return parts.length ? `/${parts.join("/")}/` : "/";
}

/** Top-level lines of the TOML front matter (before any [table]). */
function frontMatterTopLevel(text) {
  const lines = text.split(/\r?\n/);
  if (lines[0]?.trim() !== "+++") return [];
  const out = [];
  for (const line of lines.slice(1)) {
    if (line.trim() === "+++") break;
    if (/^\s*\[/.test(line)) break;
    out.push(line);
  }
  return out;
}

// Keys that change where (or whether) Zola publishes a page. The mapping in
// contentUrlPath does not model them, so fail loudly rather than guess.
const UNSUPPORTED = [
  /^\s*path\s*=/,
  /^\s*slug\s*=/,
  /^\s*draft\s*=\s*true\b/,
  /^\s*render\s*=\s*false\b/,
  /^\s*redirect_to\s*=/,
];

function walk(dir) {
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...walk(p));
    else if (e.isFile()) out.push(p);
  }
  return out;
}

/** Sorted URL paths of every page and section under a Zola content dir. */
export function contentPages(contentDir) {
  const paths = [];
  for (const file of walk(contentDir)) {
    if (!file.endsWith(".md")) continue;
    const rel = relative(contentDir, file).split(sep).join("/");
    for (const line of frontMatterTopLevel(readFileSync(file, "utf8"))) {
      if (UNSUPPORTED.some((re) => re.test(line))) {
        throw new Error(`${rel}: front matter "${line.trim()}" is not modelled by tests/site/lib/pages.mjs`);
      }
    }
    paths.push(contentUrlPath(rel));
  }
  return paths.sort();
}

/** Sorted mockup HTML file names in site/static/mockups. */
export function mockupFiles(dir) {
  return readdirSync(dir)
    .filter((f) => f.endsWith(".html"))
    .sort();
}

/** `file = "..."` entries of the mockups section front matter, in order. */
export function listedMockups(markdown) {
  const lines = markdown.split(/\r?\n/);
  const end = lines.indexOf("+++", 1);
  const fm = (end > 0 ? lines.slice(1, end) : []).join("\n");
  return [...fm.matchAll(/\bfile\s*=\s*"([^"]+)"/g)].map((m) => m[1]);
}

/** Sorted URL pathnames of the <loc> entries of a sitemap. */
export function sitemapPaths(xml) {
  return [...xml.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)].map((m) => new URL(m[1]).pathname).sort();
}

/** Everything the smoke suite loads, with the status each must return. */
export function smokeTargets(siteDir = SITE_DIR) {
  return [
    ...contentPages(join(siteDir, "content")).map((path) => ({ kind: "page", path, expectStatus: 200 })),
    ...mockupFiles(join(siteDir, "static", "mockups")).map((f) => ({
      kind: "mockup",
      path: `/mockups/${f}`,
      expectStatus: 200,
    })),
    { kind: "not-found", path: MISSING_PATH, expectStatus: 404 },
  ];
}
