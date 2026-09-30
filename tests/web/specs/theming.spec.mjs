// A host that themes <excali-editor> itself (ex-807), in Chromium against
// the release build.
//
// - A `theme` attribute makes the theme the host's, as upstream's `theme`
//   prop without `onThemeChange`: `UIOptions.canvasActions.toggleTheme`
//   stays null (packages/excalidraw/index.tsx:142-147), so the main menu
//   has no theme item (components/main-menu/DefaultItems.tsx:256), the help
//   dialog no toggle-theme row (components/HelpDialog.tsx:310) and
//   Alt+Shift+D does nothing (actions/manager.tsx:97-104).
// - `--excali-canvas-background` (not upstream) paints the on-screen canvas
//   behind a scene on the default background, unfiltered in either theme,
//   read again at every paint; exports keep the scene's own background.
import { expect, test } from "@playwright/test";

const SHORT = { timeout: 5_000 };

test.use({ viewport: { width: 1000, height: 700 } });

/** Mounts the element with `attrs` and `style` set before it connects. */
const mount = async (page, { attrs = {}, style = "" } = {}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(
    ({ attrs, style }) => {
      const ed = document.createElement("excali-editor");
      for (const [k, v] of Object.entries(attrs)) ed.setAttribute(k, v);
      if (style) ed.setAttribute("style", style);
      ed.style.display = "block";
      ed.style.width = "100%";
      ed.style.height = "100%";
      document.getElementById("host").appendChild(ed);
      window.ed = ed;
    },
    { attrs, style },
  );
  await page.waitForFunction(() => document.querySelector("excali-editor canvas.static"));
  return errors;
};

/** The static canvas's pixel at CSS (x, y) of the canvas. */
const pixel = (page, x = 900, y = 80) =>
  page.evaluate(
    ([x, y]) => {
      const canvas = document.querySelector("excali-editor canvas.static");
      const r = canvas.width / canvas.getBoundingClientRect().width;
      const ctx = canvas.getContext("2d");
      return Array.from(ctx.getImageData(Math.round(x * r), Math.round(y * r), 1, 1).data);
    },
    [x, y],
  );

const isDark = (page) =>
  page.evaluate(() => document.querySelector("excali-editor .excalidraw").classList.contains("theme--dark"));

const openMenu = async (page) => {
  await page.locator('excali-editor [data-testid="main-menu-trigger"]').click(SHORT);
  await expect(page.locator("excali-editor .dropdown-menu")).toBeVisible(SHORT);
};

const focusCanvas = (page) => page.mouse.click(600, 400);

const helpHasThemeRow = async (page) => {
  await page.keyboard.press("Shift+Slash");
  const dialog = page.locator("body > .excalidraw-modal-container .HelpDialog");
  await expect(dialog).toBeVisible(SHORT);
  const text = await dialog.innerText();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0, SHORT);
  // labels.toggleTheme (locales/en.json:139)
  return text.includes("Toggle light/dark theme");
};

test("without a theme attribute the element keeps its theme toggle", async ({ page }) => {
  const errors = await mount(page);
  await openMenu(page);
  await expect(page.locator('excali-editor [data-testid="toggle-dark-mode"]')).toHaveCount(1);
  await page.keyboard.press("Escape");
  await focusCanvas(page);
  expect(await helpHasThemeRow(page)).toBe(true);
  await focusCanvas(page);
  await page.keyboard.press("Alt+Shift+D");
  expect(await isDark(page)).toBe(true);
  expect(errors).toEqual([]);
});

test("a theme attribute makes the theme the host's: no toggle, until it is removed", async ({ page }) => {
  const errors = await mount(page, { attrs: { theme: "dark" } });
  expect(await isDark(page)).toBe(true);
  await openMenu(page);
  await expect(page.locator('excali-editor [data-testid="toggle-dark-mode"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  await focusCanvas(page);
  expect(await helpHasThemeRow(page)).toBe(false);
  await focusCanvas(page);
  await page.keyboard.press("Alt+Shift+D");
  expect(await isDark(page)).toBe(true);

  // removed: the element's own toggle is back (and the theme light)
  await page.evaluate(() => window.ed.removeAttribute("theme"));
  expect(await isDark(page)).toBe(false);
  await openMenu(page);
  await expect(page.locator('excali-editor [data-testid="toggle-dark-mode"]')).toHaveCount(1);
  await page.keyboard.press("Escape");
  await focusCanvas(page);
  await page.keyboard.press("Alt+Shift+D");
  expect(await isDark(page)).toBe(true);
  expect(errors).toEqual([]);
});

test("--excali-canvas-background paints the canvas unfiltered and follows the host's tokens", async ({ page }) => {
  // a token the host resolves through var(), on the element
  const errors = await mount(page, {
    attrs: { theme: "dark" },
    style: "--host-bg: #123456; --excali-canvas-background: var(--host-bg)",
  });
  // applyDarkModeFilter would have turned it into something else
  expect(await pixel(page)).toEqual([0x12, 0x34, 0x56, 255]);

  // the host swaps its tokens, then sets the theme again (the same value)
  await page.evaluate(() => {
    window.ed.style.setProperty("--host-bg", "hsl(30, 100%, 50%)");
    window.ed.setAttribute("theme", "dark");
  });
  expect(await pixel(page)).toEqual([255, 128, 0, 255]);

  // in the light theme as well
  await page.evaluate(() => {
    window.ed.style.setProperty("--host-bg", "rgb(10, 20, 30)");
    window.ed.setAttribute("theme", "light");
  });
  expect(await pixel(page)).toEqual([10, 20, 30, 255]);

  // inherited from an ancestor, the element's own declaration gone
  await page.evaluate(() => {
    window.ed.style.removeProperty("--excali-canvas-background");
    document.getElementById("host").style.setProperty("--excali-canvas-background", "#fedcba");
    window.ed.setAttribute("theme", "light");
  });
  expect(await pixel(page)).toEqual([0xfe, 0xdc, 0xba, 255]);

  // a value the canvas rejects, then none: upstream's white
  await page.evaluate(() => {
    document.getElementById("host").style.setProperty("--excali-canvas-background", "not-a-colour");
    window.ed.setAttribute("theme", "light");
  });
  expect(await pixel(page)).toEqual([255, 255, 255, 255]);
  await page.evaluate(() => {
    document.getElementById("host").style.removeProperty("--excali-canvas-background");
    window.ed.setAttribute("theme", "dark");
  });
  // applyDarkModeFilter("#ffffff") is #121212
  expect(await pixel(page)).toEqual([18, 18, 18, 255]);
  expect(errors).toEqual([]);
});

test("a scene with a background of its own keeps it", async ({ page }) => {
  const errors = await mount(page, {
    attrs: { theme: "light" },
    style: "--excali-canvas-background: #123456",
  });
  expect(await pixel(page)).toEqual([0x12, 0x34, 0x56, 255]);
  await page.evaluate(() =>
    window.ed.load(
      JSON.stringify({
        type: "excalidraw",
        version: 2,
        source: "https://excalidraw.com",
        elements: [],
        appState: { viewBackgroundColor: "#ffc9c9" },
        files: {},
      }),
    ),
  );
  await page.evaluate(() => window.ed.setAttribute("theme", "light"));
  expect(await pixel(page)).toEqual([0xff, 0xc9, 0xc9, 255]);
  expect(errors).toEqual([]);
});

test("exports keep the scene's background", async ({ page }) => {
  const scene = JSON.stringify({
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements: [
      {
        id: "a", type: "rectangle", x: 0, y: 0, width: 100, height: 60, angle: 0,
        strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
        strokeWidth: 2, strokeStyle: "solid", roughness: 0, opacity: 100, groupIds: [],
        frameId: null, index: null, roundness: null, seed: 1, version: 1, versionNonce: 0,
        isDeleted: false, boundElements: null, updated: 1, link: null, locked: false,
      },
    ],
    appState: { viewBackgroundColor: "#ffffff" },
    files: {},
  });
  const exports = async () =>
    page.evaluate(async () => {
      const png = await window.ed.export("png", { scale: 1, background: true });
      const bitmap = await createImageBitmap(png);
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const ctx = canvas.getContext("2d");
      ctx.drawImage(bitmap, 0, 0);
      return {
        corner: Array.from(ctx.getImageData(1, 1, 1, 1).data),
        png: Array.from(new Uint8Array(await png.arrayBuffer())).join(","),
        svg: window.ed.export("svg", {}),
      };
    });
  await mount(page, { attrs: { theme: "dark" } });
  await page.evaluate((s) => window.ed.load(s), scene);
  const plain = await exports();
  await page.evaluate(() => {
    window.ed.style.setProperty("--excali-canvas-background", "#123456");
    window.ed.setAttribute("theme", "dark");
  });
  expect(await pixel(page, 900, 600)).toEqual([0x12, 0x34, 0x56, 255]);
  const hosted = await exports();
  expect(hosted.corner).toEqual([255, 255, 255, 255]);
  expect(hosted.png).toBe(plain.png);
  expect(hosted.svg).toBe(plain.svg);
  expect(hosted.svg).not.toContain("#123456");
});

// The host stylesheet of the integration guide's Theming section
// (site/content/architecture/integration.md, checked by
// scripts/site/snippets.py): the tokens on `.excalidraw`, one class more
// specific than excali.css's `.excalidraw.theme--dark`, and the canvas
// colour per palette.
const HOST_CSS = `
excali-editor .excalidraw.excalidraw {
  --island-bg-color: #181825;
  --color-primary: #cba6f7;
  --color-selection: #cba6f7;
}
excali-editor {
  --excali-canvas-background: #1e1e2e;
}
:root[data-palette="latte"] excali-editor {
  --excali-canvas-background: #eff1f5;
}
`;

test("a host stylesheet themes the chrome and the canvas, and a palette switch repaints", async ({ page }) => {
  const errors = await mount(page, { attrs: { theme: "dark" } });
  await page.addStyleTag({ content: HOST_CSS });
  await page.evaluate(() => window.ed.setAttribute("theme", "dark"));
  const token = (name) =>
    page.evaluate(
      (name) => getComputedStyle(document.querySelector("excali-editor .excalidraw")).getPropertyValue(name).trim(),
      name,
    );
  expect(await token("--island-bg-color")).toBe("#181825");
  expect(await token("--color-primary")).toBe("#cba6f7");
  expect(await pixel(page)).toEqual([0x1e, 0x1e, 0x2e, 255]);
  // the main menu's island takes the host's token
  await openMenu(page);
  const island = await page.evaluate(
    () => getComputedStyle(document.querySelector("excali-editor .dropdown-menu .dropdown-menu-container")).backgroundColor,
  );
  expect(island).toBe("rgb(24, 24, 37)");
  await page.keyboard.press("Escape");

  // the host switches its palette, then sets the theme
  await page.evaluate(() => {
    document.documentElement.dataset.palette = "latte";
    window.ed.setAttribute("theme", "light");
  });
  expect(await pixel(page)).toEqual([0xef, 0xf1, 0xf5, 255]);
  expect(await token("--island-bg-color")).toBe("#181825");
  expect(errors).toEqual([]);
});
