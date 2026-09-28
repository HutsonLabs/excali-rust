// ex-006: every site page and every mockup loads without console errors at
// 1440x900, 1024x768 and 390x844.
//
// "Without console errors" is read strictly: a console message of type
// "error", an uncaught exception (pageerror), a request that fails at the
// network level, or any response with status >= 400 all fail the page. The
// last two matter because Chromium reports a missing stylesheet or icon
// sprite as a console error only some of the time (not for <use href>).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test, expect } from "@playwright/test";
import {
  VIEWPORTS,
  SITE_DIR,
  contentPages,
  mockupFiles,
  listedMockups,
  sitemapPaths,
  smokeTargets,
} from "../lib/pages.mjs";

const targets = smokeTargets();

test.describe("inventory", () => {
  test("the built sitemap lists exactly the content pages the suite visits", async ({ request }) => {
    const res = await request.get("/sitemap.xml");
    expect(res.status()).toBe(200);
    expect(sitemapPaths(await res.text())).toEqual(contentPages(join(SITE_DIR, "content")));
  });

  test("the mockups section lists exactly the mockup files on disk", () => {
    const md = readFileSync(join(SITE_DIR, "content", "mockups", "_index.md"), "utf8");
    expect([...listedMockups(md)].sort()).toEqual(mockupFiles(join(SITE_DIR, "static", "mockups")));
  });
});

for (const vp of VIEWPORTS) {
  test.describe(`${vp.name} ${vp.width}x${vp.height}`, () => {
    test.use({ viewport: { width: vp.width, height: vp.height } });

    for (const t of targets) {
      test(`${t.kind} ${t.path}`, async ({ page, baseURL }) => {
        const url = new URL(t.path, baseURL).href;
        const problems = [];
        page.on("console", (m) => {
          if (m.type() !== "error") return;
          // Chromium logs the document's own 404 as a resource error. That
          // one message is the expected outcome for the not-found target and
          // nothing else; any other error on the 404 page still fails.
          const own404 =
            t.expectStatus === 404 &&
            m.location().url === url &&
            m.text().startsWith("Failed to load resource: the server responded with a status of 404");
          if (!own404) problems.push(`console.error: ${m.text()} (${m.location().url})`);
        });
        page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));
        page.on("requestfailed", (r) => {
          problems.push(`request failed: ${r.url()} ${r.failure()?.errorText ?? ""}`);
        });
        page.on("response", (r) => {
          const expected404 = t.expectStatus === 404 && r.request().isNavigationRequest() && r.frame() === page.mainFrame();
          if (r.status() >= 400 && !expected404) problems.push(`HTTP ${r.status()}: ${r.url()}`);
        });

        const res = await page.goto(url, { waitUntil: "load" });
        expect(res, "navigation response").not.toBeNull();
        expect(res.status()).toBe(t.expectStatus);
        // Lazy iframes (the mockups index) and late subresources settle here.
        await page.waitForLoadState("networkidle");
        expect((await page.title()).trim(), "document title").not.toBe("");
        const box = await page.locator("body").boundingBox();
        expect(box && box.height > 0, "body renders with height").toBe(true);
        expect(problems).toEqual([]);
      });
    }
  });
}
