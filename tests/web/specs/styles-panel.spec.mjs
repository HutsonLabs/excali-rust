// The styles panel of <excali-editor> applying picks (ex-540), in Chromium
// against the release build: a colour picker's top pick, a swatch of its
// popup, a hotkey in the popup and the eye dropper each change the
// selection's colour through the action's perform (actionChangeStrokeColor,
// actionChangeBackgroundColor, packages/excalidraw/actions/
// actionProperties.tsx:358-551) with one history entry, as upstream's
// captureUpdate IMMEDIATELY records it; customising the top picks
// (ColorPicker.tsx:387-415) stores appState.colorTopPicks without one; a
// press outside the popup closes it (radix's onPointerDownOutside,
// ColorPicker.tsx:167-190, and App.handleCanvasPointerDown clearing
// openPopup, App.tsx:8772-8774); and the other controls run their
// performs (fill, stroke width, opacity, arrowheads).
import { expect, test } from "@playwright/test";

const SHORT = { timeout: 2_000 };
const MOD = "ControlOrMeta";

let seed = 1;
const element = (type, id, x, y, width = 100, height = 100, extra = {}) => ({
  id,
  type,
  x,
  y,
  width,
  height,
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "#a5d8ff",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 0,
  opacity: 100,
  groupIds: [],
  frameId: null,
  roundness: null,
  seed: seed++,
  version: 1,
  versionNonce: 1,
  isDeleted: false,
  boundElements: null,
  updated: 1,
  link: null,
  locked: false,
  ...extra,
});

const scene = (elements) =>
  JSON.stringify({
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements,
    appState: { viewBackgroundColor: "#ffffff" },
    files: {},
  });

const EMPTY = [850, 300];

const client = async (page, [x, y]) => {
  const box = await page.locator("excali-editor").boundingBox();
  return [box.x + x, box.y + y];
};

const click = async (page, at, options = {}) => {
  const [x, y] = await client(page, at);
  await page.mouse.click(x, y, options);
};

const mount = async (page, elements) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(async (text) => {
    const ed = document.createElement("excali-editor");
    document.getElementById("host").appendChild(ed);
    window.ed = ed;
    await ed.load(text);
  }, scene(elements));
  await click(page, EMPTY);
  return errors;
};

const byId = async (page) =>
  Object.fromEntries(
    (await page.evaluate(() => JSON.parse(window.ed.save()).elements)).map((e) => [e.id, e]),
  );

const state = (page) => page.evaluate(() => window.ed.getState());

const undo = (page) => page.keyboard.press(`${MOD}+KeyZ`);

const panel = (page) => page.locator("excali-editor .App-menu__left");

const strokeTrigger = (page) =>
  panel(page).locator('button.color-picker__button[aria-label="Stroke"]');

const popup = (page) => page.locator("excali-editor .color-picker-content");

/** Mounts a filled rectangle and selects it. */
const selectedRect = async (page) => {
  const errors = await mount(page, [element("rectangle", "r", 400, 100)]);
  await click(page, [450, 150]);
  await expect(panel(page)).toBeVisible(SHORT);
  return errors;
};

test("a top pick colours the selection with one history entry", async ({ page }) => {
  const errors = await selectedRect(page);
  await panel(page).locator('[data-testid="color-top-pick-#e03131"]').first().click();
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#e03131");
  await panel(page).locator('[data-testid="color-top-pick-#2f9e44"]').first().click();
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#2f9e44");
  await undo(page);
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#e03131");
  await undo(page);
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#1e1e1e");
  // the pick is the stroke default too (getColorTargetAppStateUpdates)
  expect(errors).toEqual([]);
});

test("a swatch of the popup and a hotkey pick, one entry each", async ({ page }) => {
  const errors = await selectedRect(page);
  await strokeTrigger(page).click();
  await expect(popup(page)).toBeVisible(SHORT);
  const swatch = popup(page).locator('[data-testid="color-blue"]');
  const hex = (await swatch.getAttribute("title")).match(/#[0-9a-f]{6}/)[0];
  await swatch.click();
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe(hex);
  // the popup stays open on a pick; a hotkey picks the swatch it labels
  await expect(popup(page)).toBeVisible(SHORT);
  const green = popup(page).locator('[data-testid="color-green"]');
  const key = (await green.getAttribute("aria-label")).split(" — ").pop();
  const greenHex = (await green.getAttribute("title")).match(/#[0-9a-f]{6}/)[0];
  await page.keyboard.press(key.toLowerCase());
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe(greenHex);
  await undo(page);
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe(hex);
  expect(errors).toEqual([]);
});

test("the hex input picks a typed colour", async ({ page }) => {
  const errors = await selectedRect(page);
  await strokeTrigger(page).click();
  const input = popup(page).locator("input.color-picker-input");
  await input.click();
  await input.fill("ff00aa");
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#ff00aa");
  await expect(input).toBeFocused();
  expect(errors).toEqual([]);
});

test("the eye dropper picks a canvas colour with one entry", async ({ page }) => {
  const errors = await mount(page, [
    element("rectangle", "r", 400, 100),
    element("rectangle", "s", 100, 400, 100, 100, { backgroundColor: "#ffc9c9" }),
  ]);
  await click(page, [450, 150]);
  await strokeTrigger(page).click();
  await popup(page).locator(".excalidraw-eye-dropper-trigger").click();
  await expect(page.locator("excali-editor .excalidraw-eye-dropper-backdrop")).toBeVisible(SHORT);
  await click(page, [150, 450]);
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#ffc9c9");
  await expect(page.locator("excali-editor .excalidraw-eye-dropper-backdrop")).toHaveCount(0);
  await undo(page);
  await expect.poll(async () => (await byId(page)).r.strokeColor).toBe("#1e1e1e");
  expect(errors).toEqual([]);
});

test("dragging a swatch to the strip customises the top picks", async ({ page }) => {
  const errors = await selectedRect(page);
  await strokeTrigger(page).click();
  const swatch = popup(page).locator('[data-testid="color-violet"]');
  const hex = (await swatch.getAttribute("title")).match(/#[0-9a-f]{6}/)[0];
  const from = await swatch.boundingBox();
  const to = await panel(page).locator('[data-testid="color-top-pick-#e03131"]').first().boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  // a drag starts past 10 px and 100 ms (topPicksDnD.tsx:19-22)
  await page.mouse.move(from.x + from.width / 2 + 12, from.y + from.height / 2, { steps: 3 });
  await page.waitForTimeout(150);
  await page.mouse.move(from.x + from.width / 2 + 16, from.y + from.height / 2, { steps: 2 });
  await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 12 });
  await page.waitForTimeout(50);
  await page.mouse.up();
  await expect(panel(page).locator(`[data-testid="color-top-pick-${hex}"]`).first()).toBeVisible(SHORT);
  // app state only: the element and the history are as they were
  expect((await byId(page)).r.strokeColor).toBe("#1e1e1e");
  // an undo takes back the selection, not the picks
  await undo(page);
  await click(page, [450, 150]);
  await expect(panel(page).locator(`[data-testid="color-top-pick-${hex}"]`).first()).toBeVisible();
  expect((await byId(page)).r.strokeColor).toBe("#1e1e1e");
  expect(errors).toEqual([]);
});

test("a press outside the popup closes it", async ({ page }) => {
  const errors = await selectedRect(page);
  await strokeTrigger(page).click();
  await expect(popup(page)).toBeVisible(SHORT);
  // on the panel, outside the popup
  await panel(page).locator("h3").first().click();
  await expect(popup(page)).toHaveCount(0, SHORT);
  await strokeTrigger(page).click();
  await expect(popup(page)).toBeVisible(SHORT);
  // on the canvas
  await click(page, [450, 150]);
  await expect(popup(page)).toHaveCount(0, SHORT);
  expect((await state(page)).selectionCount).toBe(1);
  expect(errors).toEqual([]);
});

test("the property controls run their performs", async ({ page }) => {
  const errors = await selectedRect(page);
  await panel(page).locator('[data-testid="fill-cross-hatch"]').click();
  await expect.poll(async () => (await byId(page)).r.fillStyle).toBe("cross-hatch");
  // RadioSelection's inputs are hidden under their labels
  await panel(page).locator('[data-testid="strokeWidth-bold"]').locator("..").click();
  await expect.poll(async () => (await byId(page)).r.strokeWidth).toBe(4);
  await panel(page).locator('input[type="radio"][name="strokeStyle"]').nth(1).locator("..").click();
  await expect.poll(async () => (await byId(page)).r.strokeStyle).toBe("dashed");
  const opacity = panel(page).locator('[data-testid="opacity"]');
  await opacity.fill("50");
  await expect.poll(async () => (await byId(page)).r.opacity).toBe(50);
  await undo(page);
  await expect.poll(async () => (await byId(page)).r.opacity).toBe(100);
  expect(errors).toEqual([]);
});

const text = (id, x, y, value) =>
  element("text", id, x, y, 50, 25, {
    backgroundColor: "transparent",
    text: value,
    originalText: value,
    fontSize: 20,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    autoResize: true,
    lineHeight: 1.25,
  });

test("the font family picker's picks and its list", async ({ page }) => {
  const errors = await mount(page, [text("t", 400, 100, "hello")]);
  await click(page, [420, 110]);
  await expect(panel(page)).toBeVisible(SHORT);
  // the top pick of the code font (FONT_FAMILY["Comic Shanns"])
  await panel(page).locator('[data-testid="font-family-code"]').click();
  await expect.poll(async () => (await byId(page)).t.fontFamily).toBe(8);
  // the list: a pick closes it
  await panel(page).locator('[data-testid="font-family-show-fonts"]').click();
  const list = page.locator("excali-editor .FontPicker__container, excali-editor [role=dialog] .dropdown-menu");
  await expect(list.first()).toBeVisible(SHORT);
  await page.locator("excali-editor .dropdown-menu-item", { hasText: "Lilita One" }).click();
  await expect.poll(async () => (await byId(page)).t.fontFamily).toBe(7);
  await undo(page);
  await expect.poll(async () => (await byId(page)).t.fontFamily).toBe(8);
  expect(errors).toEqual([]);
});

test("the arrowheads' icon picker picks the end arrowhead", async ({ page }) => {
  const errors = await mount(page, [
    element("arrow", "a", 400, 100, 200, 0, {
      backgroundColor: "transparent",
      points: [
        [0, 0],
        [200, 0],
      ],
      startBinding: null,
      endBinding: null,
      startArrowhead: null,
      endArrowhead: "arrow",
      elbowed: false,
    }),
  ]);
  await click(page, [500, 100]);
  await expect(panel(page)).toBeVisible(SHORT);
  await panel(page).locator('button[aria-label="arrowhead_end"]').click();
  await page.locator('excali-editor .picker-option[aria-label="Triangle"]').click();
  await expect.poll(async () => (await byId(page)).a.endArrowhead).toBe("triangle");
  // a click pick keeps it open (IconPicker.tsx:206-208); a press outside closes it
  await expect(page.locator('excali-editor .picker-option[aria-label="Triangle"]')).toHaveClass(/active/);
  await panel(page).locator("legend").first().click();
  await expect(page.locator("excali-editor .picker-option")).toHaveCount(0, SHORT);
  expect(errors).toEqual([]);
});

test("align and layers act on the selection", async ({ page }) => {
  const errors = await mount(page, [
    element("rectangle", "r", 400, 100),
    element("rectangle", "s", 300, 300),
  ]);
  await page.locator("excali-editor .excalidraw-container").focus();
  await page.keyboard.press(`${MOD}+KeyA`);
  await expect(panel(page)).toBeVisible(SHORT);
  await panel(page).locator('button[aria-label="Align left"]').click();
  await expect.poll(async () => (await byId(page)).r.x).toBe(300);
  // the layers: the first element brought to the front
  await click(page, EMPTY);
  await click(page, [350, 150]);
  await panel(page).locator('button.zIndexButton[title^="Bring to front"]').click();
  await expect
    .poll(async () => (await page.evaluate(() => JSON.parse(window.ed.save()).elements)).map((e) => e.id))
    .toEqual(["s", "r"]);
  expect(errors).toEqual([]);
});
