// <excali-editor> from plain JS (ex-530), in Chromium, against the release
// scripts/web/build.sh builds (dist/): the page loads the module exactly as
// the term.hut integration page documents (tests/web/page/editor.html), and
// each test drives the documented API: load, save, export, importLibrary,
// getState and the change, save-request, open-link and library-fetch
// events.
//
// The undo tests use crates/excali-wasm/tests/fixtures/bound.excalidraw,
// saved with a stale layout: the label of "box" off-centre and the end of
// the arrow "link" away from "b". After a drag and an undo the history puts
// the elements back as recorded, then lays out the bound text and the bound
// arrows again (redrawTextBoundingBox, updateBoundElements): the label comes
// back centred in its container and the arrow routed to "b". The native
// test crates/excali-wasm/tests/editor.rs runs the same gestures.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const SCENE = readFileSync(
  join(REPO_ROOT, "crates", "excali-wasm", "tests", "fixtures", "bound.excalidraw"),
  "utf8",
);

const LIBRARY = JSON.stringify({
  type: "excalidrawlib",
  version: 2,
  source: "https://excalidraw.com",
  libraryItems: [
    {
      id: "item-1",
      status: "published",
      created: 1,
      name: "one",
      elements: [
        {
          id: "r1", type: "rectangle", x: 0, y: 0, width: 10, height: 10, angle: 0,
          strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
          strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [],
          frameId: null, index: "a0", roundness: null, seed: 1, version: 1, versionNonce: 1,
          isDeleted: false, boundElements: null, updated: 1, link: null, locked: false,
        },
      ],
    },
  ],
});

const LIBRARY_URL = "https://libraries.excalidraw.com/libraries/x/one.excalidrawlib";

/** Opens the page and mounts an editor with the fixture scene loaded. */
const mount = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(async (scene) => {
    const ed = document.createElement("excali-editor");
    ed.setAttribute("theme", "dark");
    ed.setAttribute("ui", "full");
    window.events = [];
    for (const type of ["change", "save-request", "open-link"]) {
      ed.addEventListener(type, (e) => window.events.push({ type, detail: e.detail }));
    }
    document.getElementById("host").appendChild(ed);
    window.ed = ed;
    await ed.load(scene);
  }, SCENE);
  return errors;
};

/** The scene's elements by id, as save() writes them. */
const elements = (page) =>
  page.evaluate(() =>
    Object.fromEntries(JSON.parse(window.ed.save()).elements.map((e) => [e.id, e])),
  );

/** Client coordinates of a scene point (scroll 0, zoom 1). */
const client = async (page, [x, y]) => {
  const box = await page.locator("excali-editor").boundingBox();
  return [box.x + x, box.y + y];
};

const drag = async (page, from, to) => {
  const [x0, y0] = await client(page, from);
  const [x1, y1] = await client(page, to);
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x1, y1, { steps: 4 });
  await page.mouse.up();
};

const center = (e) => [e.x + e.width / 2, e.y + e.height / 2];
const arrow = (e) => ({ x: e.x, y: e.y, points: e.points });

test("mounts from plain JS with the documented attributes and state", async ({ page }) => {
  const errors = await mount(page);
  const dom = await page.evaluate(() => {
    const container = window.ed.querySelector(".excalidraw.excalidraw-container");
    return {
      container: !!container,
      dark: container.classList.contains("theme--dark"),
      canvases: [...container.querySelectorAll("canvas")].map((c) => c.className),
      toolbar: !!container.querySelector('[data-testid="toolbar-rectangle"]'),
      stylesheet: [...document.styleSheets].some((s) => (s.href || "").endsWith("/excali.css")),
    };
  });
  expect(dom).toEqual({
    container: true,
    dark: true,
    canvases: ["excalidraw__canvas static", "excalidraw__canvas interactive"],
    toolbar: true,
    stylesheet: true,
  });
  expect(await page.evaluate(() => window.ed.getState())).toEqual({
    dirty: false,
    elementCount: 6,
    zoom: 1,
    selectionCount: 0,
    activeTool: "selection",
  });
  expect(await page.evaluate(() => window.events)).toEqual([
    { type: "change", detail: { dirty: false } },
  ]);
  // the theme attribute follows
  await page.evaluate(() => window.ed.setAttribute("theme", "light"));
  expect(
    await page.evaluate(() => window.ed.querySelector(".excalidraw").classList.contains("theme--dark")),
  ).toBe(false);
  expect(errors).toEqual([]);
});

test("load rejects with a one-sentence reason", async ({ page }) => {
  await mount(page);
  const reasons = await page.evaluate(async () => {
    const out = [];
    for (const text of ["not json", '{"type":"excalidrawlib","libraryItems":[]}']) {
      try {
        await window.ed.load(text);
        out.push(null);
      } catch (e) {
        out.push(e.message);
      }
    }
    return out;
  });
  expect(reasons[0]).toMatch(/^The text is not valid JSON \(.*\)\.$/);
  expect(reasons[1]).toBe("The JSON is not an Excalidraw scene.");
  expect(await page.evaluate(() => window.ed.getState().elementCount)).toBe(6);
});

test("save writes 2-space JSON in upstream's key order", async ({ page }) => {
  await mount(page);
  const text = await page.evaluate(() => window.ed.save());
  const origin = new URL(page.url()).origin;
  expect(text.startsWith(`{\n  "type": "excalidraw",\n  "version": 2,\n  "source": "${origin}",\n  "elements": [`)).toBe(true);
  const saved = JSON.parse(text);
  expect(saved.elements.map((e) => e.id)).toEqual(["box", "label", "a", "b", "link", "linked"]);
  expect(saved.appState).toMatchObject({ gridSize: 20, viewBackgroundColor: "#ffffff" });
  // what save wrote loads back to the same text
  expect(await page.evaluate(async (t) => (await window.ed.load(t), window.ed.save()), text)).toBe(text);
});

test("export gives a PNG Blob and SVG text, the scene embedded when asked", async ({ page }) => {
  await mount(page);
  const out = await page.evaluate(async () => {
    const png = await window.ed.export("png", { scale: 2, background: true, embedScene: true });
    const bytes = new Uint8Array(await png.arrayBuffer());
    const bitmap = await createImageBitmap(png);
    const plain = await window.ed.export("png", { scale: 1 });
    const plainBytes = new Uint8Array(await plain.arrayBuffer());
    const svg = window.ed.export("svg", { embedScene: true });
    const latin1 = (b) => Array.from(b, (c) => String.fromCharCode(c)).join("");
    return {
      type: png.type,
      signature: Array.from(bytes.slice(0, 8)),
      size: [bitmap.width, bitmap.height],
      embedded: latin1(bytes).includes("tEXtapplication/vnd.excalidraw+json"),
      plainEmbedded: latin1(plainBytes).includes("tEXtapplication/vnd.excalidraw+json"),
      svgType: typeof svg,
      svg: svg.includes("<svg") && svg.includes("payload-type:application/vnd.excalidraw+json"),
      fonts: svg.includes(`${location.origin}/fonts/`),
    };
  });
  expect(out).toEqual({
    type: "image/png",
    signature: [137, 80, 78, 71, 13, 10, 26, 10],
    // the scene's bounds (60..700 × 60..580) with 10 px padding, at scale 2
    size: [1320, 1080],
    embedded: true,
    plainEmbedded: false,
    svgType: "string",
    svg: true,
    fonts: true,
  });
});

test("importLibrary takes text, and URLs through library-fetch", async ({ page }) => {
  await mount(page);
  const out = await page.evaluate(
    async ({ library, url }) => {
      const ed = window.ed;
      const fromText = await ed.importLibrary(library, { merge: true });
      const asked = [];
      ed.addEventListener("library-fetch", (e) => {
        asked.push(e.detail.url);
        e.preventDefault();
        setTimeout(() => e.detail.respond(library), 10);
      });
      // a libraries.excalidraw.com link, resolved as parseLibraryTokensFromUrl does
      const link = `https://libraries.excalidraw.com/?target=_excalidraw#addLibrary=${encodeURIComponent(url)}&token=t`;
      const fromUrl = await ed.importLibrary(link, { merge: true });
      let refused = null;
      try {
        await ed.importLibrary("https://example.com/x.excalidrawlib");
      } catch (e) {
        refused = e.message;
      }
      const lib = JSON.parse(ed.exportLibrary());
      return { fromText, fromUrl, asked, refused, items: lib.libraryItems.map((i) => i.id) };
    },
    { library: LIBRARY, url: LIBRARY_URL },
  );
  expect(out).toEqual({
    fromText: 1,
    // the same item merged again is not added twice (mergeLibraryItems)
    fromUrl: 1,
    asked: [LIBRARY_URL],
    refused: 'Invalid or disallowed library URL: "https://example.com/x.excalidrawlib".',
    items: ["item-1"],
  });
});

test("a URL the host does not fetch rejects the import", async ({ page }) => {
  await mount(page);
  const reason = await page.evaluate(async (url) => {
    try {
      await window.ed.importLibrary(url);
      return null;
    } catch (e) {
      return e.message;
    }
  }, LIBRARY_URL);
  expect(reason).toBe(`The host did not handle library-fetch for ${LIBRARY_URL}.`);
});

test("change, save-request and open-link events", async ({ page }) => {
  await mount(page);
  await page.evaluate(() => (window.events = []));
  await drag(page, [160, 110], [200, 110]);
  expect(await page.evaluate(() => window.events.at(-1))).toEqual({
    type: "change",
    detail: { dirty: true },
  });
  expect(await page.evaluate(() => window.ed.getState())).toMatchObject({
    dirty: true,
    selectionCount: 1,
  });

  await page.keyboard.press("ControlOrMeta+s");
  expect(await page.evaluate(() => window.events.at(-1))).toEqual({
    type: "save-request",
    detail: {},
  });

  // the link icon of "linked" (600, 480, 100 × 100): getLinkHandleFromCoords
  // at zoom 1 puts it at (702, 462), 12 × 12
  const [x, y] = await client(page, [708, 468]);
  await page.mouse.click(x, y);
  expect(await page.evaluate(() => window.events.at(-1))).toEqual({
    type: "open-link",
    detail: { href: "https://example.com/docs" },
  });
});

test("undoing a container move re-centres its label", async ({ page }) => {
  const errors = await mount(page);
  const before = await elements(page);
  expect([before.label.x, before.label.y]).toEqual([70, 70]);
  expect(center(before.label)).not.toEqual(center(before.box));

  await drag(page, [160, 110], [200, 110]);
  const moved = await elements(page);
  expect(moved.box.x).toBe(100);
  // the drag moves the label with its container, stale offset and all
  expect([moved.label.x, moved.label.y]).toEqual([110, 70]);

  await page.keyboard.press("ControlOrMeta+z");
  const undone = await elements(page);
  expect([undone.box.x, undone.box.y]).toEqual([60, 60]);
  expect(undone.label.x).not.toBe(70);
  expect(center(undone.label)[0]).toBeCloseTo(center(undone.box)[0], 9);
  expect(center(undone.label)[1]).toBeCloseTo(center(undone.box)[1], 9);
  expect(undone.label.text).toBe("label");

  await page.keyboard.press("ControlOrMeta+Shift+z");
  const redone = await elements(page);
  expect(redone.box.x).toBe(100);
  expect(center(redone.label)[0]).toBeCloseTo(center(redone.box)[0], 9);
  expect(errors).toEqual([]);
});

test("undoing a bound rectangle move re-routes its arrow", async ({ page }) => {
  const errors = await mount(page);
  const stale = arrow((await elements(page)).link);
  expect(stale.points).toEqual([[0, 0], [180, 40]]);

  await drag(page, [450, 350], [490, 350]);
  const afterMove = await elements(page);
  expect(afterMove.b.x).toBe(440);
  const moved = arrow(afterMove.link);
  expect(moved).not.toEqual(stale);

  await drag(page, [490, 350], [450, 350]);
  const back = await elements(page);
  expect(back.b.x).toBe(400);
  // the arrow laid out against b where the file has it
  const laidOut = arrow(back.link);
  expect(laidOut).not.toEqual(stale);

  await page.keyboard.press("ControlOrMeta+z");
  let now = await elements(page);
  expect(now.b.x).toBe(440);
  expect(arrow(now.link)).toEqual(moved);

  await page.keyboard.press("ControlOrMeta+z");
  now = await elements(page);
  expect(now.b.x).toBe(400);
  // the history restored the stale arrow, then updateBoundElements routed it
  // to b again
  expect(arrow(now.link)).toEqual(laidOut);
  expect(errors).toEqual([]);
});
