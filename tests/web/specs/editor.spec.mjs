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

// ex-525: the context menu (App.openContextMenu, ContextMenu.tsx): the
// canvas menu over nothing, the element menu over an element (selecting
// it), a row closing the menu and running its action, a press outside
// closing it. The rows are upstream's (tests/context_menu.rs in excali-ui
// holds them to upstream's own component).
test("right-click opens the canvas or the element context menu", async ({ page }) => {
  const errors = await mount(page);
  const menu = page.locator("excali-editor .context-menu");
  const rows = () => menu.locator("li").evaluateAll((lis) => lis.map((li) => li.dataset.testid));

  const [cx, cy] = await client(page, [900, 100]);
  await page.mouse.click(cx, cy, { button: "right" });
  await expect(menu).toBeVisible();
  // actionCopyAsPng's predicate is probablySupportsClipboardBlob, which
  // Chromium has (App.tsx:13856-13873)
  expect(await rows(page)).toEqual([
    "paste",
    "copyAsPng",
    "selectAll",
    "gridMode",
    "objectsSnapMode",
    "arrowBinding",
    "midpointSnapping",
    "zenMode",
    "viewMode",
    "stats",
  ]);
  // a press outside the popover (which fits itself left of the pointer
  // here) closes it
  const [ox, oy] = await client(page, [900, 650]);
  await page.mouse.click(ox, oy);
  await expect(menu).toHaveCount(0);

  const a = center((await elements(page)).a);
  const [ax, ay] = await client(page, a);
  await page.mouse.click(ax, ay, { button: "right" });
  await expect(menu).toBeVisible();
  const elementRows = await rows(page);
  expect(elementRows.slice(0, 3)).toEqual(["cut", "copy", "paste"]);
  expect(elementRows).toContain("bringToFront");
  expect(elementRows.at(-1)).toBe("deleteSelectedElements");
  await expect(menu.locator(".context-menu-item.dangerous")).toHaveText(/Delete/);
  expect(await page.evaluate(() => window.ed.getState().selectionCount)).toBe(1);

  await menu.locator('[data-testid="deleteSelectedElements"] button').click();
  await expect(menu).toHaveCount(0);
  expect((await elements(page)).a.isDeleted).toBe(true);
  expect(errors).toEqual([]);
});

// ex-535: the convert element type popup (ConvertElementTypePopup.tsx,
// App.tsx:5636-5672). Tab with a shape selected opens it under the shape,
// each Tab after converts to the next type; a click on a type converts to
// it and focuses the panel, so Shift+Tab keeps cycling from there; Escape
// closes it. excali-ui's tests/convert_popup.rs holds the panel to
// upstream's own component.
test("Tab opens the convert popup, a type converts and takes the focus", async ({ page }) => {
  const errors = await mount(page);
  const type = async () => (await elements(page)).linked.type;
  const popup = page.locator("excali-editor .ConvertElementTypePopup");

  const [lx, ly] = await client(page, [600, 530]);
  await page.mouse.click(lx, ly);
  expect(await page.evaluate(() => window.ed.getState().selectionCount)).toBe(1);
  await expect(popup).toHaveCount(0);

  await page.keyboard.press("Tab");
  await expect(popup).toBeVisible();
  await expect(popup.locator('[data-testid="toolbar-rectangle"]')).toHaveAttribute("aria-pressed", "true");
  expect(await type()).toBe("rectangle");

  await page.keyboard.press("Tab");
  expect(await type()).toBe("diamond");
  await expect(popup.locator('[data-testid="toolbar-diamond"]')).toHaveAttribute("aria-pressed", "true");

  await popup.locator('[data-testid="toolbar-ellipse"]').click();
  expect(await type()).toBe("ellipse");
  const focused = () =>
    page.evaluate(() => document.activeElement?.classList.contains("ConvertElementTypePopup") ?? false);
  expect(await focused()).toBe(true);

  await page.keyboard.press("Shift+Tab");
  expect(await type()).toBe("diamond");
  expect(await focused()).toBe(true);

  await page.keyboard.press("Escape");
  await expect(popup).toHaveCount(0);
  expect(errors).toEqual([]);
});

// ex-542: the shortcut table's clipboard and file rows. Shift+Alt+C is
// actionCopyAsPng (actionClipboard.tsx:193-250): the selection's PNG export
// on the system clipboard (copyBlobToClipboardAsPng). Ctrl/Cmd+O
// (actionLoadScene) and Ctrl/Cmd+Shift+S (actionSaveFileToDisk,
// actionExport.tsx:329-430) open upstream's file dialogs, which belong to the
// host: the element asks with the cancelable open-request and save-as-request
// events, and falls back to the browser's file input and a download when no
// host answers.
/** A click on empty canvas: the keys go to the element's container. */
const focusCanvas = async (page) => {
  const [x, y] = await client(page, [900, 100]);
  await page.mouse.click(x, y);
};

test.describe("clipboard and file shortcuts", () => {
  test.use({ permissions: ["clipboard-read", "clipboard-write"] });

  test("Shift+Alt+C copies the selection as PNG", async ({ page }) => {
    const errors = await mount(page);
    // the left edge of "a" (60, 300, 100 × 100)
    const [x, y] = await client(page, [60, 350]);
    await page.mouse.click(x, y);
    expect(await page.evaluate(() => window.ed.getState().selectionCount)).toBe(1);

    await page.keyboard.press("Shift+Alt+C");
    // 100 × 100 and DEFAULT_EXPORT_PADDING on each side, at exportScale 1
    await expect
      .poll(() =>
        page.evaluate(async () => {
          const items = await navigator.clipboard.read();
          const item = items.find((i) => i.types.includes("image/png"));
          if (!item) return null;
          const bitmap = await createImageBitmap(await item.getType("image/png"));
          return [bitmap.width, bitmap.height];
        }),
      )
      .toEqual([120, 120]);
    expect(errors).toEqual([]);
  });

  test("Ctrl/Cmd+O and Ctrl/Cmd+Shift+S reach the host", async ({ page }) => {
    const errors = await mount(page);
    await focusCanvas(page);
    await page.evaluate(() => {
      window.requests = [];
      for (const type of ["open-request", "save-as-request"]) {
        window.ed.addEventListener(type, (e) => {
          e.preventDefault();
          window.requests.push({ type, detail: e.detail });
        });
      }
    });
    await page.keyboard.press("ControlOrMeta+o");
    await page.keyboard.press("ControlOrMeta+Shift+s");
    const requests = await page.evaluate(() => window.requests);
    expect(requests.map((r) => r.type)).toEqual(["open-request", "save-as-request"]);
    expect(requests[0].detail).toEqual({});
    // app.getName(): `${t("labels.untitled")}-${getDateTime()}`
    expect(requests[1].detail.name).toMatch(/^Untitled-\d{4}-\d{2}-\d{2}-\d{4}$/);
    expect(errors).toEqual([]);
  });

  test("with no host, the browser's file dialogs", async ({ page }) => {
    const errors = await mount(page);
    const saved = await page.evaluate(() => window.ed.save());
    await focusCanvas(page);

    const [download] = await Promise.all([
      page.waitForEvent("download"),
      page.keyboard.press("ControlOrMeta+Shift+s"),
    ]);
    expect(download.suggestedFilename()).toMatch(
      /^Untitled-\d{4}-\d{2}-\d{2}-\d{4}\.excalidraw$/,
    );
    const text = readFileSync(await download.path(), "utf8");
    expect(JSON.parse(text)).toEqual(JSON.parse(saved));

    const scene = JSON.parse(saved);
    scene.elements = scene.elements.filter((e) => e.id === "a");
    const [chooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.keyboard.press("ControlOrMeta+o"),
    ]);
    await chooser.setFiles({
      name: "one.excalidraw",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(scene)),
    });
    await expect
      .poll(() => page.evaluate(() => window.ed.getState().elementCount))
      .toBe(1);
    expect(errors).toEqual([]);
  });
});

// ex-543: the shortcut table's Ctrl/Cmd+K row. actionLink
// (actionLink.tsx:20-42) opens upstream's link editor (Hyperlink.tsx) above
// the selected element; Enter submits normalizeLink(input) || null as one
// history entry and shows the link; the link icon (hyperlink/helpers.ts)
// then opens it; Remove clears it.
test("Ctrl/Cmd+K edits the selected element's link", async ({ page }) => {
  const errors = await mount(page);
  const popup = page.locator("excali-editor .excalidraw-hyperlinkContainer");
  const input = popup.locator("input.excalidraw-hyperlinkContainer-input");
  const link = async () => (await elements(page)).box.link;

  // "box" (60, 60, 200 × 100): the popup is centred 85 px above its top
  const [bx, by] = await client(page, [160, 110]);
  await page.mouse.click(bx, by);
  await page.keyboard.press("ControlOrMeta+k");
  await expect(popup).toBeVisible();
  await expect(popup).toHaveAttribute("style", /top: -25px/);
  await expect(popup).toHaveAttribute("style", /left: -30px/);
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute("placeholder", "Type or paste your link here");

  await page.keyboard.type("  https://excalidraw.com  ");
  expect(await link()).toBe(null);
  await page.keyboard.press("Enter");
  expect(await link()).toBe("https://excalidraw.com");
  await expect(input).toHaveCount(0);
  await expect(popup.locator("a.excalidraw-hyperlinkContainer-link")).toHaveText("https://excalidraw.com");

  // one history entry, the keys back on the editor
  await page.keyboard.press("ControlOrMeta+z");
  expect(await link()).toBe(null);
  await page.keyboard.press("ControlOrMeta+Shift+z");
  expect(await link()).toBe("https://excalidraw.com");

  // deselected, the link icon (getLinkHandleFromCoords: 262, 46, 12 × 12)
  // is drawn and opens the link
  const [ex, ey] = await client(page, [900, 100]);
  await page.mouse.click(ex, ey);
  await expect(popup).toHaveCount(0);
  await page.evaluate(() => (window.events = []));
  const [ix, iy] = await client(page, [268, 52]);
  await page.mouse.click(ix, iy);
  expect(await page.evaluate(() => window.events.at(-1))).toEqual({
    type: "open-link",
    detail: { href: "https://excalidraw.com" },
  });

  // the editor again: the link selected, Ctrl+K kept in the input, Remove
  await page.mouse.click(bx, by);
  await page.keyboard.press("ControlOrMeta+k");
  await expect(input).toBeFocused();
  await expect(input).toHaveValue("https://excalidraw.com");
  expect(await input.evaluate((i) => [i.selectionStart, i.selectionEnd])).toEqual([0, 22]);
  await page.keyboard.press("ControlOrMeta+k");
  await expect(input).toBeFocused();
  await popup.locator(".excalidraw-hyperlinkContainer--remove").click();
  expect(await link()).toBe(null);
  await expect(popup).toHaveCount(0);
  expect(errors).toEqual([]);
});
