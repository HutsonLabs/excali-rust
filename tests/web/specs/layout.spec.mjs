// The desktop layout of <excali-editor> (milestone M5, criterion D3 for
// desktop: site/content/plan/overview.md, site/content/design-system/
// layout.md, mockups 01 and 02), in Chromium against the release build, at
// the mockups' 1440 × 900 with the element filling the page.
//
// Upstream at the pin places the chrome so (packages/excalidraw/):
//
// - the layer UI sits above the canvases: `.layer-ui__wrapper` has
//   `z-index: var(--zIndex-layerUI)` (components/LayerUI.scss:7-17), 4,
//   over the interactive canvas's `--zIndex-interactiveCanvas`, 2
//   (css/styles.scss:4-15), so every control of the chrome, the footer's
//   too, takes the pointer at its centre;
// - `App-menu_top__left` is a `Stack.Col` of gap `menuTopGap`, 6 in the
//   full styles panel (components/LayerUI.tsx:178-192, 315-318), times
//   `--space-factor` 0.25rem (css/theme.scss:79): the styles panel's
//   island starts 1.5rem under the main-menu trigger, and is at most
//   `appState.height - 166` high (LayerUI.tsx:278-284);
// - the top row and the footer are inset by `--editor-container-padding`,
//   1rem (css/theme.scss:50; css/styles.scss `.App-menu_top`,
//   `.App-menu_bottom` bottom 1rem), the toolbar centred between them;
// - the footer's left holds ZoomActions then UndoRedoActions, its right
//   the help button (components/footer/Footer.tsx:33-92).
import { expect, test } from "@playwright/test";

const W = 1440;
const H = 900;
const PAD = 16; // --editor-container-padding
const MENU_TOP_GAP = 6 * 4; // menuTopGap × --space-factor

test.use({ viewport: { width: W, height: H } });

const mount = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.addStyleTag({ content: "#host { width: 100vw; height: 100vh; }" });
  await page.evaluate(() => {
    const ed = document.createElement("excali-editor");
    document.getElementById("host").appendChild(ed);
    window.ed = ed;
  });
  // a press on the canvas focuses the container (App.tsx:4178)
  await page.mouse.click(900, 700);
  return errors;
};

/** Draws a rectangle with R and a drag; it stays selected. */
const drawRectangle = async (page) => {
  await page.keyboard.press("r");
  await page.mouse.move(500, 300);
  await page.mouse.down();
  await page.mouse.move(700, 450, { steps: 5 });
  await page.mouse.up();
};

const box = (page, selector) =>
  page.evaluate((s) => {
    const el = document.querySelector(`excali-editor ${s}`);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
  }, selector);

/** The chrome's controls on the desktop layout with a rectangle selected. */
const CONTROLS = [
  ".main-menu-trigger",
  ".App-toolbar .ToolIcon",
  ".App-toolbar__extra-tools-trigger",
  ".default-sidebar-trigger",
  ".App-menu__left button",
  ".App-menu__left label",
  ".zoom-out-button",
  ".reset-zoom-button",
  ".zoom-in-button",
  ".undo-button-container button",
  ".redo-button-container button",
  ".help-icon",
];

test("desktop regions sit where upstream places them", async ({ page }) => {
  const errors = await mount(page);
  await drawRectangle(page);

  const menu = await box(page, ".main-menu-trigger");
  expect([menu.x, menu.y]).toEqual([PAD, PAD]);

  const panel = await box(page, ".App-menu__left");
  expect(panel.x).toBe(PAD);
  expect(panel.y).toBe(menu.bottom + MENU_TOP_GAP);
  expect(panel.height).toBeLessThanOrEqual(H - 166);

  const toolbar = await box(page, ".App-toolbar");
  expect(toolbar.y).toBe(PAD);
  expect(Math.abs(toolbar.x + toolbar.width / 2 - W / 2)).toBeLessThanOrEqual(0.5);

  const library = await box(page, ".default-sidebar-trigger");
  expect([library.right, library.y]).toEqual([W - PAD, PAD]);

  const zoom = await box(page, ".zoom-actions");
  const undoRedo = await box(page, ".undo-redo-buttons");
  const help = await box(page, ".help-icon");
  expect([zoom.x, zoom.bottom]).toEqual([PAD, H - PAD]);
  expect(undoRedo.x).toBeGreaterThan(zoom.right);
  expect(undoRedo.bottom).toBe(H - PAD);
  expect([help.right, help.bottom]).toEqual([W - PAD, H - PAD]);
  expect(errors).toEqual([]);
});

test("every control of the chrome takes the pointer at its centre", async ({ page }) => {
  const errors = await mount(page);
  await drawRectangle(page);
  const covered = await page.evaluate((selectors) => {
    const out = [];
    for (const s of selectors) {
      const all = [...document.querySelectorAll(`excali-editor ${s}`)];
      if (all.length === 0) out.push(`${s}: none rendered`);
      for (const el of all) {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        if (!el.contains(top) && !top?.contains(el)) {
          out.push(`${s} (${el.getAttribute("aria-label") ?? el.textContent}): under ${top?.tagName}.${top?.className}`);
        }
      }
    }
    return out;
  }, CONTROLS);
  expect(covered).toEqual([]);
  expect(errors).toEqual([]);
});

test("the footer's buttons work with the mouse", async ({ page }) => {
  const errors = await mount(page);
  await drawRectangle(page);
  const clickCentre = async (selector) => {
    const b = await box(page, selector);
    await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
  };
  const live = () =>
    page.evaluate(() => JSON.parse(window.ed.save()).elements.filter((e) => !e.isDeleted).length);

  await clickCentre(".zoom-in-button");
  await expect(page.locator("excali-editor .reset-zoom-button")).toHaveText("110%");
  await clickCentre(".reset-zoom-button");
  await expect(page.locator("excali-editor .reset-zoom-button")).toHaveText("100%");

  expect(await live()).toBe(1);
  await clickCentre(".undo-button-container button");
  expect(await live()).toBe(0);
  await clickCentre(".redo-button-container button");
  expect(await live()).toBe(1);
  expect(errors).toEqual([]);
});

// Mockup 02: the dark theme. Upstream draws the static canvas with
// `theme: this.state.theme` and `renderGrid: isGridModeEnabled(this)`
// (App.tsx:2675-2690): the background, the grid and every element colour go
// through applyDarkModeFilter (common/src/colors.ts), and with grid mode off
// by default (appState.ts:76) there is no grid.
test("the dark theme draws the canvas through the dark filter, without a grid", async ({ page }) => {
  const errors = await mount(page);
  await page.evaluate(() => window.ed.setAttribute("theme", "dark"));
  await drawRectangle(page);
  await page.mouse.click(1000, 700); // deselect: no handles over the stroke
  const px = await page.evaluate(() => {
    const canvas = document.querySelector("excali-editor canvas.static");
    const ctx = canvas.getContext("2d");
    const r = canvas.width / canvas.getBoundingClientRect().width;
    const at = (x, y) => Array.from(ctx.getImageData(Math.round(x * r), Math.round(y * r), 1, 1).data);
    // the brightest pixel across the rectangle's left edge (x 500)
    let edge = [0, 0, 0, 0];
    for (let x = 494; x <= 506; x++) {
      const p = at(x, 375);
      if (p[0] > edge[0]) edge = p;
    }
    return { background: at(300, 610), gridLine: at(300, 620), edge };
  });
  // applyDarkModeFilter("#ffffff") is #121212
  expect(px.background).toEqual([18, 18, 18, 255]);
  expect(px.gridLine).toEqual(px.background);
  // #1e1e1e through the filter is light on the dark background
  expect(px.edge[0]).toBeGreaterThan(160);
  expect(errors).toEqual([]);
});
