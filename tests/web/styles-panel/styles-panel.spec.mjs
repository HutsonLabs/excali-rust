// excali-ui's full styles panel in Chromium (ex-519).
//
// For every case of crates/excali-ui/tests/fixtures/styles-panel.json
// (upstream's getShapeActionPredicates, showSelectedShapeActions,
// SelectedShapeActions and LayerUI's section and island at the pinned
// commit, tools/goldens/styles-panel.mjs), the page tools/styles-panel
// serves mounts excali_ui::styles_panel with web-sys, and the DOM
// Chromium holds is read back into the fixture's form and compared with
// upstream's tree: the section and island around the panel, the panel's
// fieldsets, legends and rows, and a <button data-action> where upstream
// calls renderAction. The island's inline style is checked as Chromium
// parsed it.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const fixture = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates/excali-ui/tests/fixtures/styles-panel.json"), "utf8"),
);

/** The case's app state: `{ ref }` replaced by the scene's element, at
 * the generator's 1440×900 (tools/goldens/styles-panel.mjs). */
const appState = (c) => {
  const scene = fixture.scenes[c.scene];
  const byId = new Map(scene.map((e) => [e.id, e]));
  const state = { width: 1440, height: 900 };
  for (const [k, v] of Object.entries(c.appState)) {
    state[k] = v && typeof v === "object" && "ref" in v ? byId.get(v.ref) : v;
  }
  return state;
};

/** Upstream's tree for the case inside the 900 px wrapper. */
const expected = (c) => {
  const wrapper = fixture.wrapper.find((w) => w.height === 900 && !w.zenModeEnabled);
  const fill = (node) =>
    node.action === "<panel>"
      ? c.tree[0]
      : typeof node === "string" || node.action
        ? node
        : { ...node, ...(node.children ? { children: node.children.map(fill) } : {}) };
  return fill(wrapper.tree[0]);
};

const ready = async (page) => {
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady || window.harnessError);
  expect(await page.evaluate(() => window.harnessError ?? null)).toBeNull();
};

/** The mounted DOM in the fixture's form; styles are compared separately. */
const read = (page, scene, state, rtl, containerId) =>
  page.evaluate(
    ([scene, state, rtl, containerId]) => {
      const walk = (node) => {
        if (node.nodeType === Node.TEXT_NODE) return node.data;
        if (node.tagName === "BUTTON" && node.dataset.action) return { action: node.dataset.action };
        const out = { tag: node.tagName.toLowerCase() };
        if (node.className) out.class = node.className;
        const attrs = [...node.attributes].filter((a) => a.name !== "class" && a.name !== "style");
        if (attrs.length) out.attrs = Object.fromEntries(attrs.map((a) => [a.name, a.value]));
        const children = [...node.childNodes].map(walk);
        if (children.length) out.children = children;
        return out;
      };
      const root = window.panel.mount(scene, state, rtl, containerId);
      const island = root.querySelector(".Island");
      return {
        tree: walk(root),
        style: {
          padding: island.style.getPropertyValue("--padding"),
          maxHeight: island.style.maxHeight,
          computedMaxHeight: getComputedStyle(island).maxHeight,
        },
      };
    },
    [scene, state, rtl, containerId],
  );

const withoutStyle = (node) =>
  typeof node === "string" || node.action
    ? node
    : Object.fromEntries(
        Object.entries(node)
          .filter(([k]) => k !== "style")
          .map(([k, v]) => [k, k === "children" ? v.map(withoutStyle) : v]),
      );

test("every case mounts upstream's tree", async ({ page }) => {
  await ready(page);
  expect(fixture.cases.length).toBeGreaterThanOrEqual(280);
  for (const c of fixture.cases) {
    const got = await read(page, fixture.scenes[c.scene], appState(c), c.rtl, fixture.containerId);
    expect(got.tree, c.id).toEqual(withoutStyle(expected(c)));
    expect(got.style, c.id).toEqual({ padding: "2", maxHeight: "734px", computedMaxHeight: "734px" });
  }
});

test("showSelectedShapeActions holds for every case", async ({ page }) => {
  await ready(page);
  for (const c of fixture.cases) {
    const show = await page.evaluate(([s, a]) => window.panel.show(s, a), [fixture.scenes[c.scene], appState(c)]);
    expect(show, c.id).toBe(c.show);
  }
});

test("the island follows the app height and zen mode", async ({ page }) => {
  await ready(page);
  const scene = fixture.scenes.main;
  for (const w of fixture.wrapper) {
    const state = { width: 1440, height: w.height, zenModeEnabled: w.zenModeEnabled, selectedElementIds: { r1: true } };
    const got = await read(page, scene, state, false, fixture.containerId);
    const section = w.tree[0];
    expect(got.tree.class).toBe(section.class);
    expect(got.style.maxHeight).toBe(section.children[1].style.maxHeight);
  }
});
