// excali-ui's full and compact styles panels in Chromium (ex-519, ex-701).
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
// parsed it. The compact panel (CompactShapeActions, ex-701) is read the
// same way, an icon as the icons.tsx export whose markup it is and an open
// popover as the fixture's `{ tag: "popover" }` (radix's content, without
// its popper wrapper and arrow); its triggers are clicked open and closed,
// and the form factor rules are held to upstream's grid.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const fixture = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates/excali-ui/tests/fixtures/styles-panel.json"), "utf8"),
);
const icons = JSON.parse(readFileSync(join(REPO_ROOT, "crates/excali-ui/tests/fixtures/icons.json"), "utf8")).icons;

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

/** Upstream's compact tree for the case inside the 900 px compact wrapper,
 * the style as CSS names (what the DOM reads back). */
const expectedCompact = (c) => {
  const wrapper = fixture.compactWrapper.find((w) => w.height === 900 && !w.zenModeEnabled);
  const css = (k) => (k.startsWith("--") ? k : k.replace(/[A-Z]/g, (m) => `-${m.toLowerCase()}`));
  const fill = (node) => {
    if (node.action === "<panel>") return fill(c.compact[0]);
    if (typeof node === "string" || node.action || node.icon) return node;
    const out = { ...node };
    if (node.style) out.style = Object.fromEntries(Object.entries(node.style).map(([k, v]) => [css(k), String(v)]));
    if (node.children) out.children = node.children.map(fill);
    return out;
  };
  return fill(wrapper.tree[0]);
};

/** Mounts the compact panel and reads it back in the fixture's form. */
const readCompact = (page, scene, state, rtl, containerId) =>
  page.evaluate(
    ([scene, state, rtl, containerId, icons]) => {
      const markup = new Map();
      for (const icon of icons) {
        if (icon.kind !== "static") continue;
        const t = document.createElement("template");
        t.innerHTML = icon.markup;
        const html = t.content.firstElementChild?.outerHTML;
        if (html && !markup.has(html)) markup.set(html, icon.name);
      }
      const style = (node) =>
        Object.fromEntries([...node.style].map((k) => [k, node.style.getPropertyValue(k)]));
      const walk = (node) => {
        if (node.nodeType === Node.TEXT_NODE) return node.data;
        if (node.tagName === "BUTTON" && node.dataset.action) {
          return "cycle" in node.dataset ? { action: node.dataset.action, data: { cycle: true } } : { action: node.dataset.action };
        }
        if (node.localName === "svg") return { icon: markup.get(node.outerHTML) ?? node.outerHTML };
        if (node.hasAttribute("data-radix-popper-content-wrapper")) {
          const content = node.firstElementChild;
          return {
            tag: "popover",
            class: content.className,
            attrs: { side: content.dataset.side, align: content.dataset.align, sideOffset: "20", alignOffset: "-16" },
            style: { "z-index": content.style.zIndex },
            // the island; the arrow is radix's
            children: [...content.children].filter((c) => c.localName !== "span").map(walk),
          };
        }
        const out = { tag: node.tagName.toLowerCase() };
        if (node.className) out.class = node.className;
        const radix = ["aria-haspopup", "aria-expanded", "data-state"];
        const attrs = [...node.attributes].filter((a) => a.name !== "class" && a.name !== "style" && !radix.includes(a.name));
        if (attrs.length) out.attrs = Object.fromEntries(attrs.map((a) => [a.name, a.value]));
        if (node.style.length) out.style = style(node);
        const children = [...node.childNodes].map(walk);
        if (children.length) out.children = children;
        return out;
      };
      const root = window.panel.mountCompact(scene, state, rtl, containerId);
      return walk(root);
    },
    [scene, state, rtl, containerId, icons],
  );

test("every case mounts upstream's compact tree", async ({ page }) => {
  await ready(page);
  for (const c of fixture.cases) {
    const got = await readCompact(page, fixture.scenes[c.scene], appState(c), c.rtl, fixture.containerId);
    expect(got, c.id).toEqual(expectedCompact(c));
  }
});

test("a trigger opens its popover beside it, a second click or Escape closes it", async ({ page }) => {
  await ready(page);
  const c = fixture.cases.find((x) => x.id === "select-r1-e1-d1");
  await page.evaluate(([s, a, id]) => window.panel.mountCompact(s, a, false, id), [fixture.scenes[c.scene], appState(c), fixture.containerId]);
  const popovers = page.locator("[data-radix-popper-content-wrapper]");
  await expect(popovers).toHaveCount(0);
  for (const title of ["Stroke", "Actions"]) {
    const trigger = page.locator(`button[title="${title}"]`);
    await trigger.click();
    await expect(popovers).toHaveCount(1);
    const opened = page.locator(`button[title="${title}"]`);
    await expect(opened).toHaveClass(/\bactive\b/);
    await expect(opened).toHaveAttribute("aria-expanded", "true");
    // side right, sideOffset 20: the content starts 20 px right of the trigger
    const t = await opened.boundingBox();
    const p = await page.locator("[data-radix-popper-content-wrapper] > div").boundingBox();
    expect(Math.round(p.x - (t.x + t.width))).toBe(20);
    await opened.click();
    await expect(popovers).toHaveCount(0);
    await expect(page.locator(`button[title="${title}"]`)).not.toHaveClass(/\bactive\b/);
  }
  await page.locator('button[title="Stroke"]').click();
  await page.locator("[data-radix-popper-content-wrapper] > div").press("Escape");
  await expect(popovers).toHaveCount(0);
});

test("the form factor and the styles panel mode follow upstream", async ({ page }) => {
  await ready(page);
  const got = await page.evaluate(
    (sizes) => sizes.map((s) => window.panel.formFactor(s.width, s.height)),
    fixture.formFactor.sizes,
  );
  expect(got).toEqual(fixture.formFactor.sizes.map((s) => s.formFactor));
  // a tablet (768×1024 either way) gets the compact panel whatever the
  // desktop UI mode; a desktop its stored mode; a phone the mobile one
  const modes = await page.evaluate(() => [
    window.panel.mode(768, 1024, "full"),
    window.panel.mode(1024, 768, "full"),
    window.panel.mode(1440, 900, "full"),
    window.panel.mode(1440, 900, "compact"),
    window.panel.mode(390, 844, "full"),
  ]);
  expect(modes).toEqual(["compact", "compact", "full", "compact", "mobile"]);
});
