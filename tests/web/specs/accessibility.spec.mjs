// Accessibility of <excali-editor> (ex-709), in Chromium, against the
// release scripts/web/build.sh builds (dist/):
//
// - axe-core (@axe-core/playwright) finds no WCAG 2.0/2.1 A or AA violation
//   in the editor's chrome: empty (the welcome screen) and with a scene, in
//   the light and dark themes, and with the main menu, the help dialog, the
//   command palette and the library sidebar open;
// - focus order: Tab walks the controls in upstream's LayerUI order (the
//   welcome screen's centre, then App-menu_top's left section with the main
//   menu, the shapes toolbar, the top-right section with the library
//   trigger, then the footer; components/LayerUI.tsx:301-440 and :648-660),
//   skips controls that are not shown (the exit-zen-mode button outside zen
//   mode, footer/Footer.tsx and LayerUI.scss), and Tab on the focused
//   container itself stays there (App.tsx:5642-5650, the convert cycle);
// - RTL: in a `dir="rtl"` document an icon created with `mirror: true`
//   (icons.tsx:22-47, the `rtl-mirror` class) is flipped by styles.scss's
//   `:root[dir="rtl"] .rtl-mirror { transform: scaleX(-1) }` (:679-683),
//   and not in a left-to-right one.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const SCENE = readFileSync(
  join(REPO_ROOT, "crates", "excali-wasm", "tests", "fixtures", "bound.excalidraw"),
  "utf8",
);

const SHORT = { timeout: 2_000 };
const MOD = "ControlOrMeta";
const WCAG = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

/** Opens the page and mounts an editor, with `scene` loaded when given. */
const mount = async (page, { scene = null, theme = "light", dir = null } = {}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(
    async ({ scene, theme, dir }) => {
      if (dir) document.documentElement.setAttribute("dir", dir);
      const ed = document.createElement("excali-editor");
      ed.setAttribute("theme", theme);
      ed.setAttribute("ui", "full");
      document.getElementById("host").appendChild(ed);
      window.ed = ed;
      if (scene) {
        await ed.load(scene);
        // the scene's appState carries its own theme
        ed.setAttribute("theme", theme === "light" ? "dark" : "light");
        ed.setAttribute("theme", theme);
      }
    },
    { scene, theme, dir },
  );
  return errors;
};

/** axe's WCAG A/AA violations on the page: rule, targets and why. */
const violations = async (page) => {
  // a menu or dialog fading in has not its final colours yet
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getTiming().iterations !== Infinity)
        .map((a) => a.finished),
    ),
  );
  const result = await new AxeBuilder({ page }).withTags(WCAG).analyze();
  return result.violations.map(
    (v) =>
      `${v.id} (${v.impact}): ` +
      v.nodes.map((n) => `${n.target.join(" ")}: ${n.failureSummary}`).join("; "),
  );
};

/** The chrome states axe checks, each opened on a mounted editor. */
const STATES = {
  "the empty editor with the welcome screen": {
    scene: null,
    open: async (page) => {
      await expect(page.locator("excali-editor .welcome-screen-center")).toBeVisible(SHORT);
    },
  },
  "a scene": { open: async () => {} },
  "the main menu": {
    open: async (page) => {
      await page.locator("excali-editor .main-menu-trigger").click(SHORT);
      await expect(page.locator("excali-editor .main-menu")).toBeVisible(SHORT);
    },
  },
  "the help dialog": {
    open: async (page) => {
      await page.locator("excali-editor .excalidraw-container").focus();
      await page.keyboard.press("Shift+Slash");
      await expect(page.locator("body > .excalidraw-modal-container .HelpDialog")).toBeVisible(
        SHORT,
      );
    },
  },
  "the command palette": {
    open: async (page) => {
      await page.locator("excali-editor .excalidraw-container").focus();
      await page.keyboard.press(`${MOD}+Slash`);
      await expect(
        page.locator("body > .excalidraw-modal-container .command-palette-dialog"),
      ).toBeVisible(SHORT);
    },
  },
  "the library sidebar": {
    open: async (page) => {
      await page.locator("excali-editor .default-sidebar-trigger").click(SHORT);
      await expect(page.locator("excali-editor .default-sidebar")).toBeVisible(SHORT);
    },
  },
};

test.describe("axe finds no WCAG A/AA violation", () => {
  for (const theme of ["light", "dark"]) {
    for (const [name, { scene = SCENE, open }] of Object.entries(STATES)) {
      test(`${name}, ${theme}`, async ({ page }) => {
        const errors = await mount(page, { scene, theme });
        await open(page);
        expect(await violations(page)).toEqual([]);
        expect(errors).toEqual([]);
      });
    }
  }
});

/** The focused element: its section of the layer UI and a readable name. */
const focused = (page) =>
  page.evaluate(() => {
    const a = document.activeElement;
    const sections = [
      [".welcome-screen-center", "welcome"],
      [".excali-editor__top-left", "menu"],
      [".excali-editor__top", "toolbar"],
      [".excali-editor__top-right", "top-right"],
      [".layer-ui__wrapper__footer", "footer"],
    ];
    const section = sections.find(([sel]) => a.closest(sel))?.[1] ?? a.tagName.toLowerCase();
    const name =
      a.getAttribute("aria-label") ||
      a.getAttribute("data-testid") ||
      a.getAttribute("title") ||
      a.textContent.trim();
    return { section, name, visible: a.checkVisibility({ visibilityProperty: true }) };
  });

/** Tabs from the focused element `n` times; each stop. */
const tabs = async (page, n, key = "Tab") => {
  const stops = [];
  for (let i = 0; i < n; i++) {
    await page.keyboard.press(key);
    stops.push(await focused(page));
  }
  return stops;
};

/** Consecutive duplicates removed. */
const runs = (xs) => xs.filter((x, i) => i === 0 || x !== xs[i - 1]);

test.describe("focus order", () => {
  test("Tab walks the layer UI in upstream's order, visible controls only", async ({ page }) => {
    await mount(page);
    // the first control of the layer UI: the welcome screen's first item
    await page.locator("excali-editor .welcome-screen-menu-item").first().focus();
    const stops = [await focused(page)];
    for (;;) {
      await page.keyboard.press("Tab");
      const stop = await focused(page);
      if (!["welcome", "menu", "toolbar", "top-right", "footer"].includes(stop.section)) break;
      stops.push(stop);
      expect(stops.length).toBeLessThan(80);
    }
    expect(runs(stops.map((s) => s.section))).toEqual([
      "welcome",
      "menu",
      "toolbar",
      "top-right",
      "footer",
    ]);
    expect(stops.filter((s) => !s.visible)).toEqual([]);
    expect(stops.filter((s) => !s.name)).toEqual([]);
    const names = stops.map((s) => s.name);
    // App-menu_top__left (LayerUI.tsx:315-320), then the toolbar's lock
    // first (Toolbar.tsx), the library trigger, the footer's zoom and help
    expect(names[names.indexOf("Menu") + 1]).toBe("Keep selected tool active after drawing");
    expect(names).toContain("Library");
    expect(names.slice(-4)).toEqual(["Zoom out", "Reset zoom", "Zoom in", "Help"]);
    expect(names).not.toContain("Exit zen mode");

    // Shift+Tab, from past the last stop, walks the same stops back
    const back = await tabs(page, stops.length, "Shift+Tab");
    expect(back.map((s) => s.name)).toEqual([...names].reverse());
  });

  test("Tab on the focused container stays on it (the convert cycle takes the key)", async ({
    page,
  }) => {
    await mount(page, { scene: SCENE });
    const container = page.locator("excali-editor .excalidraw-container");
    await container.focus();
    for (const stop of await tabs(page, 3)) {
      expect(stop.section).toBe("div");
    }
    await expect(container).toBeFocused();
  });
});

test.describe("the command palette's list", () => {
  test("is a tab stop after the search field, and the keys still pick commands", async ({
    page,
  }) => {
    await mount(page, { scene: SCENE });
    await page.locator("excali-editor .excalidraw-container").focus();
    await page.keyboard.press(`${MOD}+Slash`);
    const palette = page.locator("body > .excalidraw-modal-container .command-palette-dialog");
    await expect(palette.locator("input")).toBeFocused(SHORT);
    await page.keyboard.type("ellipse");
    await page.keyboard.press("Tab");
    await expect(palette.locator(".commands")).toBeFocused(SHORT);
    // the palette reads its keys on the window (App.tsx's capture listener)
    await expect(palette.locator(".command-item.item-selected .name")).toHaveText("Ellipse", SHORT);
    await page.keyboard.press("Enter");
    await expect(palette).toHaveCount(0, SHORT);
    expect(await page.evaluate(() => window.ed.getState().activeTool)).toBe("ellipse");
  });
});

/** The computed transform of each mirrored icon in the chrome. */
const mirroredTransforms = (page) =>
  page.evaluate(() =>
    // rendered ones: the styles panel keeps a hidden ungroup button
    // (actionGroup.tsx's `hidden`), which has no box to transform
    [...document.querySelectorAll(".excalidraw svg.rtl-mirror")]
      .filter((svg) => svg.getClientRects().length > 0)
      .map(
      (svg) => getComputedStyle(svg).transform,
    ),
  );

test.describe("RTL mirroring", () => {
  /** Selects every element and opens the command palette on "group". */
  const paletteWithGroup = async (page) => {
    await page.locator("excali-editor .excalidraw-container").focus();
    await page.keyboard.press(`${MOD}+KeyA`);
    await page.keyboard.press(`${MOD}+Slash`);
    const palette = page.locator("body > .excalidraw-modal-container .command-palette-dialog");
    await expect(palette).toBeVisible(SHORT);
    await page.keyboard.type("group");
    // actionGroup's icon is GroupIcon, created with mirror: true
    await expect(palette.locator(".command-item svg.rtl-mirror").first()).toBeVisible(SHORT);
  };

  test("mirrored icons flip in a right-to-left document", async ({ page }) => {
    const errors = await mount(page, { scene: SCENE, dir: "rtl" });
    await paletteWithGroup(page);
    const transforms = await mirroredTransforms(page);
    expect(transforms.length).toBeGreaterThan(0);
    for (const t of transforms) expect(t).toBe("matrix(-1, 0, 0, 1, 0, 0)");
    expect(errors).toEqual([]);
  });

  test("and not in a left-to-right one", async ({ page }) => {
    await mount(page, { scene: SCENE, dir: "ltr" });
    await paletteWithGroup(page);
    const transforms = await mirroredTransforms(page);
    expect(transforms.length).toBeGreaterThan(0);
    for (const t of transforms) expect(t).toBe("none");
  });

  test("only icons with mirror: true carry the class", async ({ page }) => {
    await mount(page, { scene: SCENE, dir: "rtl" });
    // the footer's help icon is HelpIcon, not questionCircle: not mirrored
    const help = page.locator("excali-editor .help-icon svg");
    await expect(help).toBeVisible(SHORT);
    await expect(help).not.toHaveClass(/rtl-mirror/);
    expect(await help.evaluate((svg) => getComputedStyle(svg).transform)).toBe("none");
  });
});
