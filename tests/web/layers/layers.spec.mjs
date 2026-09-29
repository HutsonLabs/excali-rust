// excali-ui's layered canvases in Chromium (ex-503).
//
// The wasm harness tools/canvas-layers mounts excali_ui::layers::CanvasLayers
// in the page's #editor and paints its layers. At each device pixel ratio
// (Playwright's deviceScaleFactor, which is window.devicePixelRatio):
//
// - every canvas's backing store is its CSS size × devicePixelRatio in whole
//   pixels (StaticCanvas.tsx:38-41, NewElementCanvas.tsx:50-58,
//   InteractiveCanvas.tsx:199-210), its CSS box is the CSS size, and the
//   canvases are stacked as App.tsx mounts them (static in its wrapper, the
//   new-element canvas only while there is a preview, the interactive one
//   on top) with upstream's classes and stylesheet rules;
// - the rules the port computes backing sizes with agree with Chromium's
//   HTMLCanvasElement for the static canvas's property assignment and the
//   other canvases' attribute, on ordinary and edge values;
// - the scene is drawn at device-pixel scale at the scroll snapped to whole
//   device pixels (snapScrollToDevicePixels): scrolls that snap alike paint
//   identical pixels, and a scroll of one device pixel moves the picture by
//   exactly one pixel;
// - each layer's bootstrap clears what it drew before (unless an opaque hex
//   background repaints every pixel).
import { expect, test } from "@playwright/test";

const RATIOS = [1, 1.25, 1.5, 2, 2.625, 3];
const WIDTH = 801;
const HEIGHT = 601;

const open = async (browser, deviceScaleFactor) => {
  const context = await browser.newContext({ deviceScaleFactor, viewport: { width: 1000, height: 800 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  expect(await page.evaluate(() => window.harnessError || null), "the harness module loads").toBeNull();
  await page.evaluate(
    ([w, h]) => {
      window.layers = new window.harness.Layers(document.getElementById("editor"));
      window.scale = window.layers.resize(w, h);
    },
    [WIDTH, HEIGHT],
  );
  return { context, page, errors };
};

/** Every canvas and the wrapper as the page sees them. */
const describe = (page) =>
  page.evaluate(() => {
    const editor = document.getElementById("editor");
    const box = (el) => {
      const r = el.getBoundingClientRect();
      return [r.width, r.height];
    };
    const canvas = (c) => {
      const cs = getComputedStyle(c);
      return {
        className: c.className,
        width: c.width,
        height: c.height,
        styleWidth: c.style.width,
        styleHeight: c.style.height,
        opacity: c.style.opacity,
        box: box(c),
        position: cs.position,
        zIndex: cs.zIndex,
        pointerEvents: cs.pointerEvents,
        touchAction: cs.touchAction,
        imageRendering: cs.imageRendering,
        text: c.textContent,
      };
    };
    return {
      children: [...editor.children].map((c) => `${c.tagName.toLowerCase()}.${[...c.classList].join(".")}`),
      wrapperChildren: [...editor.children[0].children].map((c) => `${c.tagName.toLowerCase()}.${[...c.classList].join(".")}`),
      wrapperPointerEvents: getComputedStyle(editor.children[0]).pointerEvents,
      canvases: Object.fromEntries(
        ["static", "new-element", "interactive"].map((name) => {
          const c = window.layers.canvas(name);
          return [name, c ? canvas(c) : null];
        }),
      ),
      devicePixelRatio: window.devicePixelRatio,
      scale: window.scale,
    };
  });

for (const dpr of RATIOS) {
  test(`at devicePixelRatio ${dpr}: each canvas is ${WIDTH}x${HEIGHT} CSS px backed by CSS size × dpr, stacked in upstream's order`, async ({
    browser,
  }) => {
    const { context, page, errors } = await open(browser, dpr);
    const backing = [Math.trunc(WIDTH * dpr), Math.trunc(HEIGHT * dpr)];

    let d = await describe(page);
    expect(d.devicePixelRatio).toBe(dpr);
    expect(d.scale).toBe(dpr);
    // no preview: no new-element canvas (App.tsx:2692)
    expect(d.children).toEqual(["div.excalidraw__canvas-wrapper", "canvas.excalidraw__canvas.interactive"]);
    expect(d.wrapperChildren).toEqual(["canvas.excalidraw__canvas.static"]);
    expect(d.canvases["new-element"]).toBeNull();

    await page.evaluate(() => window.layers.showNewElement(0.5));
    d = await describe(page);
    expect(d.children).toEqual([
      "div.excalidraw__canvas-wrapper",
      "canvas.excalidraw__canvas",
      "canvas.excalidraw__canvas.interactive",
    ]);
    for (const name of ["static", "new-element", "interactive"]) {
      const c = d.canvases[name];
      expect([c.width, c.height], `${name} backing store`).toEqual(backing);
      expect([c.styleWidth, c.styleHeight], `${name} CSS size`).toEqual([`${WIDTH}px`, `${HEIGHT}px`]);
      expect(c.box, `${name} box`).toEqual([WIDTH, HEIGHT]);
      expect(c.position, `${name} position`).toBe("absolute");
      expect(c.touchAction).toBe("none");
      expect(c.imageRendering).toBe("pixelated");
    }
    // styles.scss:105-132: canvases at --zIndex-canvas, the interactive one
    // above at --zIndex-interactiveCanvas; the static canvas and its
    // wrapper let pointer events through
    expect(d.canvases.static.zIndex).toBe("1");
    expect(d.canvases["new-element"].zIndex).toBe("1");
    expect(d.canvases.interactive.zIndex).toBe("2");
    expect(d.canvases.static.pointerEvents).toBe("none");
    expect(d.wrapperPointerEvents).toBe("none");
    expect(d.canvases.interactive.pointerEvents).toBe("auto");
    // TOOL_DRAG_PREVIEW_OPACITY on a tool dragged out of the toolbar
    expect(d.canvases["new-element"].opacity).toBe("0.5");
    // the interactive canvas's fallback content (t("labels.drawingCanvas"))
    expect(d.canvases.interactive.text).toBe("Drawing canvas");

    // the preview ends: the canvas goes; a new one has no opacity
    await page.evaluate(() => window.layers.hideNewElement());
    expect((await describe(page)).canvases["new-element"]).toBeNull();
    await page.evaluate(() => window.layers.showNewElement(undefined));
    d = await describe(page);
    expect(d.canvases["new-element"].opacity).toBe("");
    expect([d.canvases["new-element"].width, d.canvases["new-element"].height]).toEqual(backing);

    // a resize resizes every canvas
    await page.evaluate(() => window.layers.resize(400.5, 300.25));
    d = await describe(page);
    for (const name of ["static", "new-element", "interactive"]) {
      const c = d.canvases[name];
      expect([c.width, c.height], name).toEqual([Math.trunc(400.5 * dpr), Math.trunc(300.25 * dpr)]);
      expect([c.styleWidth, c.styleHeight], name).toEqual(["400.5px", "300.25px"]);
    }

    // unmounting removes the canvases
    await page.evaluate(() => window.layers.unmount());
    expect(await page.evaluate(() => document.getElementById("editor").children.length)).toBe(0);
    expect(errors).toEqual([]);
    await context.close();
  });
}

test("the port's backing size rules are Chromium's HTMLCanvasElement's", async ({ browser }) => {
  const { context, page, errors } = await open(browser, 1);
  const values = [
    0, 0.4, 1, 800, 1001.25, 1001.999, -0, -0.5, -1, -1001.25, 1e-7, 5e-7, 1e21, 1.5e22, 2147483647, 2147483648,
    3e9, 4294967295, 4294967296, 4294967301, 1e300, Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY,
  ];
  const rows = await page.evaluate((xs) => {
    const { canvasDimensionFromProperty, canvasDimensionFromAttribute } = window.harness;
    return xs.map((x) => {
      const byProperty = document.createElement("canvas");
      byProperty.width = x;
      byProperty.height = x;
      const byAttribute = document.createElement("canvas");
      byAttribute.setAttribute("width", String(x));
      byAttribute.setAttribute("height", String(x));
      return {
        x: String(x),
        chromium: [byProperty.width, byProperty.height, byAttribute.width, byAttribute.height],
        port: [
          canvasDimensionFromProperty(x, 300),
          canvasDimensionFromProperty(x, 150),
          canvasDimensionFromAttribute(x, 300),
          canvasDimensionFromAttribute(x, 150),
        ],
      };
    });
  }, values);
  for (const r of rows) expect(r.port, `canvas width/height ${r.x}`).toEqual(r.chromium);
  expect(errors).toEqual([]);
  await context.close();
});

// A red square with no stroke, at (10, 10) and 20 wide, and an ellipse whose
// anti-aliased edge shows any sub-pixel shift.
const SQUARE = {
  type: "rectangle",
  id: "square",
  x: 10,
  y: 10,
  width: 20,
  height: 20,
  angle: 0,
  strokeColor: "transparent",
  backgroundColor: "#ff0000",
  fillStyle: "solid",
  strokeWidth: 1,
  strokeStyle: "solid",
  roughness: 0,
  opacity: 100,
  seed: 1,
  groupIds: [],
  frameId: null,
  roundness: null,
  boundElements: null,
  locked: false,
  link: null,
  version: 1,
  versionNonce: 1,
  isDeleted: false,
  updated: 1,
};
const ELLIPSE = { ...SQUARE, type: "ellipse", id: "ellipse", x: 60.37, y: 20.61, width: 90.5, height: 50.25, strokeColor: "#1e1e1e", strokeWidth: 2, backgroundColor: "#a5d8ff", roughness: 1, seed: 7 };
const SCENE = JSON.stringify([SQUARE, ELLIPSE]);

/** Page-side pixel helpers (layers/page/index.html). */
const pixel = (page, layer, x, y) => page.evaluate(([l, px, py]) => window.pixel(l, px, py), [layer, x, y]);
const snapshot = (page, layer) => page.evaluate((l) => window.snapshot(l), layer);
const sameAs = (page, layer, id) => page.evaluate(([l, i]) => window.sameAs(l, i), [layer, id]);
const blank = (page, layer) => page.evaluate((l) => window.blank(l), layer);

for (const dpr of [1, 1.25, 2]) {
  test(`at devicePixelRatio ${dpr}: the static canvas draws at device-pixel scale at the snapped scroll`, async ({ browser }) => {
    const { context, page, errors } = await open(browser, dpr);
    const width = Math.trunc(WIDTH * dpr);
    const paint = (sx, sy, zoom, bg = "#ffffff", grid = false) =>
      page.evaluate(([s, x, y, z, b, g]) => window.layers.paintStatic(s, x, y, z, b ?? undefined, g), [SCENE, sx, sy, zoom, bg, grid]);

    // the square covers device pixels [10·dpr, 30·dpr) at zoom 1, scroll 0
    await paint(0, 0, 1);
    const inside = Math.ceil(10 * dpr) + 1;
    const lastInside = Math.floor(30 * dpr) - 2;
    const beyond = Math.floor(30 * dpr) + 1;
    expect(await pixel(page, "static", inside, inside)).toEqual([255, 0, 0, 255]);
    expect(await pixel(page, "static", lastInside, lastInside)).toEqual([255, 0, 0, 255]);
    expect(await pixel(page, "static", beyond, beyond)).toEqual([255, 255, 255, 255]);
    expect(await pixel(page, "static", Math.floor(10 * dpr) - 2, inside)).toEqual([255, 255, 255, 255]);

    for (const zoom of [1, 1.5, 0.73]) {
      const d = zoom * dpr;
      const snapped = (s) => Math.round(s * d) / d;
      // a fractional scroll paints what its snapped scroll paints
      await paint(3.3, -7.77, zoom);
      const fractional = await snapshot(page, "static");
      await paint(snapped(3.3), snapped(-7.77), zoom);
      expect(await sameAs(page, "static", fractional), `zoom ${zoom}: snapped`).toBeNull();
      // a tenth of a device pixel further (away from the rounding
      // boundary) lands on the same pixels
      const tenth = Math.round(3.3 * d + 0.1) === Math.round(3.3 * d) ? 0.1 : -0.1;
      await paint(3.3 + tenth / d, -7.77, zoom);
      expect(await sameAs(page, "static", fractional), `zoom ${zoom}: a tenth further`).toBeNull();
      // one device pixel further moves the picture by exactly one pixel
      await paint(snapped(3.3) + 1 / d, snapped(-7.77), zoom);
      const moved = await page.evaluate(([i]) => window.shiftedFrom("static", i, 1), [fractional]);
      expect(moved.mismatch, `zoom ${zoom}: moved by one device pixel`).toBeNull();
      expect(moved.compared).toBe((width - 1) * Math.trunc(HEIGHT * dpr));
    }

    // a translucent background is cleared before it is filled again, so
    // repainting does not accumulate it; transparent leaves nothing
    await paint(0, 0, 1, "#ff000080");
    await paint(0, 0, 1, "#ff000080");
    expect((await pixel(page, "static", 1, 1))[3]).toBe(128);
    await paint(0, 0, 1, "transparent");
    expect(await pixel(page, "static", 1, 1)).toEqual([0, 0, 0, 0]);
    expect(await pixel(page, "static", inside, inside)).toEqual([255, 0, 0, 255]);
    await paint(0, 0, 1, null);
    expect(await pixel(page, "static", 1, 1)).toEqual([0, 0, 0, 0]);
    // an opaque hex background repaints every pixel without a clear
    await paint(0, 0, 1, "#abc");
    expect(await pixel(page, "static", 1, 1)).toEqual([0xaa, 0xbb, 0xcc, 255]);
    expect(errors).toEqual([]);
    await context.close();
  });

  test(`at devicePixelRatio ${dpr}: the new-element and interactive canvases clear and draw at the snapped scroll`, async ({ browser }) => {
    const { context, page, errors } = await open(browser, dpr);
    await page.evaluate(() => window.layers.showNewElement(undefined));
    const paint = (element, sx, sy, zoom) =>
      page.evaluate(([e, x, y, z]) => window.layers.paintNewElement(e ?? undefined, x, y, z), [element, sx, sy, zoom]);
    const ellipse = JSON.stringify(ELLIPSE);
    const zoom = 1.5;
    const d = zoom * dpr;
    await paint(ellipse, 3.3, -7.77, zoom);
    expect(await blank(page, "new-element")).toBe(false);
    const fractional = await snapshot(page, "new-element");
    await paint(ellipse, Math.round(3.3 * d) / d, Math.round(-7.77 * d) / d, zoom);
    expect(await sameAs(page, "new-element", fractional)).toBeNull();
    // a square at zoom 1 covers device pixels [10·dpr, 30·dpr), on a clear
    // canvas (no background on this layer)
    await paint(JSON.stringify(SQUARE), 0, 0, 1);
    const inside = Math.ceil(10 * dpr) + 1;
    expect(await pixel(page, "new-element", inside, inside)).toEqual([255, 0, 0, 255]);
    expect(await pixel(page, "new-element", 1, 1)).toEqual([0, 0, 0, 0]);
    // nothing to draw clears it
    await paint(null, 0, 0, 1);
    expect(await blank(page, "new-element")).toBe(true);
    // an invisibly small element draws nothing either
    await paint(JSON.stringify(SQUARE), 0, 0, 1);
    await paint(JSON.stringify({ ...SQUARE, width: 0, height: 0 }), 0, 0, 1);
    expect(await blank(page, "new-element")).toBe(true);

    // the interactive canvas's bootstrap clears what was on it
    await page.evaluate(() => {
      const c = window.layers.canvas("interactive");
      const ctx = c.getContext("2d");
      ctx.fillStyle = "#00ff00";
      ctx.fillRect(0, 0, c.width, c.height);
      window.layers.paintInteractive();
    });
    expect(await blank(page, "interactive")).toBe(true);
    expect(errors).toEqual([]);
    await context.close();
  });
}
