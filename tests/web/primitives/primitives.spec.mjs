// excali-ui's UI primitives in Chromium (ex-516).
//
// The wasm harness tools/ui-primitives installs the primitives' stylesheet
// (upstream's theme.scss and component SCSS, compiled) and mounts a
// gallery of them in the page's `.excalidraw` #editor. The suite checks
// what the native tests cannot:
//
// - the sizes the design system gives them as Chromium lays them out
//   (theme.scss:46-49, 79, 148-149): 2rem tool buttons with 1rem icons,
//   Island padding and Stack gaps in 0.25rem units, 0.5rem island radii;
// - their behaviour: a tool toggle reports the press's pointer type (null
//   from the keyboard, IconButton.tsx:186-198), Button, RadioGroup, Range
//   and TextField call back with their values, the redaction eye reveals
//   and hides the value (TextField.tsx:82-110), the tooltip shows under
//   its item and hides on leave (Tooltip.tsx:20-60, 161-187), a dialog
//   focuses its first field, traps Tab and closes on Escape and on its
//   backdrop (Dialog.tsx:49-136, Modal.tsx), and a popover takes focus,
//   traps Tab, fits in the viewport and asks to close on a press outside
//   (Popover.tsx).
import { expect, test } from "@playwright/test";

const open = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  expect(await page.evaluate(() => window.harnessError || null), "the harness module loads").toBeNull();
  return errors;
};

const log = (page) => page.evaluate(() => window.harness.takeLog());

const box = (page, selector) =>
  page.evaluate((s) => {
    const r = document.querySelector(s).getBoundingClientRect();
    return { left: r.left, top: r.top, width: r.width, height: r.height, bottom: r.bottom, right: r.right };
  }, selector);

const computed = (page, selector, properties) =>
  page.evaluate(
    ([s, ps]) => {
      const cs = getComputedStyle(document.querySelector(s));
      return Object.fromEntries(ps.map((p) => [p, cs.getPropertyValue(p).trim()]));
    },
    [selector, properties],
  );

test("the design system's sizes", async ({ page }) => {
  const errors = await open(page);
  expect(
    await computed(page, "#editor", [
      "--default-button-size",
      "--default-icon-size",
      "--space-factor",
      "--border-radius-md",
      "--border-radius-lg",
    ]),
  ).toEqual({
    "--default-button-size": "2rem",
    "--default-icon-size": "1rem",
    "--space-factor": "0.25rem",
    "--border-radius-md": "0.375rem",
    "--border-radius-lg": "0.5rem",
  });
  // a tool button's icon box is 2rem, its svg 1rem
  const icon = await box(page, '[data-testid="toolbar-rectangle"] .ToolIcon__icon');
  expect([icon.width, icon.height]).toEqual([32, 32]);
  const svg = await box(page, '[data-testid="toolbar-rectangle"] .ToolIcon__icon svg');
  expect([svg.width, svg.height]).toEqual([16, 16]);
  const plain = await box(page, '[data-testid="plain-button"] .ToolIcon__icon');
  expect([plain.width, plain.height]).toEqual([32, 32]);
  // the outlined button is 2rem high
  expect((await box(page, ".gallery-button")).height).toBe(32);
  // Island: padding 2 × 0.25rem, radius 0.5rem
  expect(await computed(page, ".gallery-toolbar", ["padding-top", "padding-left", "border-top-left-radius"])).toEqual({
    "padding-top": "8px",
    "padding-left": "8px",
    "border-top-left-radius": "8px",
  });
  // Stack: gap 1 × 0.25rem between the row's buttons, 2 × 0.25rem in the column
  expect(await computed(page, ".gallery-row", ["column-gap", "display"])).toEqual({ "column-gap": "4px", display: "grid" });
  expect(await computed(page, ".gallery-controls", ["row-gap"])).toEqual({ "row-gap": "8px" });
  const [a, b] = await page.evaluate(() =>
    [...document.querySelectorAll(".gallery-row > button")].slice(0, 2).map((el) => {
      const r = el.getBoundingClientRect();
      return { left: r.left, right: r.right };
    }),
  );
  expect(b.left - a.right).toBe(4);
  expect(errors).toEqual([]);
});

test("a checked tool is styled as checked", async ({ page }) => {
  await open(page);
  const checked = await computed(page, '[data-testid="toolbar-rectangle"] .ToolIcon__icon', ["background-color"]);
  const unchecked = await computed(page, '[data-testid="toolbar-ellipse"] .ToolIcon__icon', ["background-color"]);
  expect(checked["background-color"]).not.toBe(unchecked["background-color"]);
  expect(await page.getAttribute('[data-testid="toolbar-rectangle"]', "aria-pressed")).toBe("true");
});

test("tool toggles report the pointer type, null from the keyboard", async ({ page }) => {
  await open(page);
  await page.click('[data-testid="toolbar-ellipse"]');
  expect(await log(page)).toEqual(["select:ellipse:mouse"]);
  await page.focus('[data-testid="toolbar-rectangle"]');
  // after the pointerup's animation frame the pointer type is forgotten
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  await page.keyboard.press("Enter");
  expect(await log(page)).toEqual(["select:rectangle:null"]);
  await page.click('[data-testid="plain-button"]');
  expect(await log(page)).toEqual(["click:plain"]);
});

test("button, radio group, range and text field call back", async ({ page }) => {
  await open(page);
  await page.click(".gallery-button");
  expect(await log(page)).toEqual(["button"]);
  expect(await page.getAttribute(".RadioGroup__choice.active", "title")).toBe("light");
  await page.click('input[aria-label="dark"]');
  expect(await log(page)).toEqual(["radio:dark"]);
  await page.fill('[data-testid="gallery-range"]', "30");
  expect(await log(page)).toEqual(["range:30"]);
  expect(await page.textContent(".value-bubble")).toBe("50");
  const input = ".gallery-text input";
  await page.fill(input, "abc");
  expect(await log(page)).toEqual(["text:abc"]);
  // clicking the field's frame focuses its input
  await page.click(".gallery-text .ExcTextField__label");
  expect(await page.evaluate(() => document.activeElement.matches(".gallery-text input"))).toBe(true);
});

test("the redaction eye reveals and hides the value", async ({ page }) => {
  await open(page);
  const input = ".gallery-text input";
  const eye = ".gallery-text .excalidraw-button";
  expect(await page.getAttribute(input, "class")).toBe("is-redacted");
  const closedPath = "M10.585 10.587a2 2 0 0 0 2.829 2.828";
  expect(await page.innerHTML(eye)).not.toContain(closedPath);
  await page.click(eye);
  expect(await page.getAttribute(input, "class")).toBe("");
  expect(await page.innerHTML(eye)).toContain(closedPath);
  await page.click(eye);
  expect(await page.getAttribute(input, "class")).toBe("is-redacted");
  expect(await page.innerHTML(eye)).not.toContain(closedPath);
});

test("the tooltip shows under its item and hides on leave", async ({ page }) => {
  await open(page);
  await page.hover(".gallery-tooltip-target");
  const tooltip = page.locator(".excalidraw-tooltip");
  await expect(tooltip).toHaveClass("excalidraw-tooltip excalidraw-tooltip--visible");
  await expect(tooltip).toHaveText("A tooltip");
  // what the port writes is updateTooltipPosition of the rects it reads,
  // exactly (the laid-out box then snaps to Chromium's 1/64px grid, so it
  // is compared with the written style, not the box)
  const item = await box(page, ".gallery-tooltip");
  const tip = await box(page, ".excalidraw-tooltip");
  const written = await tooltip.evaluate((el) => [el.style.top, el.style.left]);
  expect(written).toEqual([`${item.top + item.height + 5}px`, `${item.left + item.width / 2 - tip.width / 2}px`]);
  // and the box is there, to the layout grid
  expect(Math.abs(tip.top - (item.top + item.height + 5))).toBeLessThanOrEqual(1 / 64);
  expect(Math.abs(tip.left + tip.width / 2 - (item.left + item.width / 2))).toBeLessThanOrEqual(1 / 64);
  expect(await tooltip.evaluate((el) => [el.style.minWidth, el.style.maxWidth])).toEqual(["10ch", "15ch"]);
  await page.mouse.move(1000, 700);
  await expect(tooltip).toHaveClass("excalidraw-tooltip");
});

test("a dialog focuses its first field, traps Tab and closes on Escape", async ({ page }) => {
  const errors = await open(page);
  await page.focus(".gallery-button");
  await page.evaluate(() => window.harness.openDialog("Help", "small", false));
  const container = page.locator("body > .excalidraw.excalidraw-modal-container");
  await expect(container).toHaveCount(1);
  expect(await page.getAttribute(".Modal", "class")).toBe("Modal Dialog");
  expect(await page.textContent("#gallery-dialog-title")).toBe("Help");
  // the content is 550px wide at most
  expect((await box(page, ".Modal__content")).width).toBeLessThanOrEqual(550);
  // autofocus: the second focusable (after a close button, of which a
  // desktop dialog has none)
  await expect(page.locator(".dialog-button[data-i='1']")).toBeFocused();
  await page.focus(".dialog-button[data-i='2']");
  await page.keyboard.press("Tab");
  await expect(page.locator(".dialog-button[data-i='0']")).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.locator(".dialog-button[data-i='2']")).toBeFocused();
  await page.keyboard.press("Escape");
  expect(await log(page)).toEqual(["dialog-close"]);
  await expect(container).toHaveCount(0);
  // focus returns to where it was
  await expect(page.locator(".gallery-button")).toBeFocused();
  expect(errors).toEqual([]);
});

test("a phone dialog is full screen with a close button; the backdrop closes it", async ({ page }) => {
  await open(page);
  await page.evaluate(() => window.harness.openDialog("Help", "regular", true));
  expect(await page.getAttribute(".Modal", "class")).toBe("Modal Dialog Dialog--fullscreen");
  expect(await page.getAttribute("body > .excalidraw-modal-container", "class")).toBe(
    "excalidraw excalidraw-modal-container excalidraw--mobile",
  );
  // focusables: the close button, then the buttons; autofocus takes the second
  await expect(page.locator(".dialog-button[data-i='0']")).toBeFocused();
  await page.click(".Dialog__close");
  expect(await log(page)).toEqual(["dialog-close"]);
  await page.evaluate(() => window.harness.openDialog("Help", "regular", false));
  await page.locator(".Modal__background").click({ position: { x: 5, y: 5 }, force: true });
  expect(await log(page)).toEqual(["dialog-close"]);
});

test("a popover takes focus, traps Tab, fits in the viewport and closes on a press outside", async ({ page }) => {
  const errors = await open(page);
  await page.evaluate(() => window.harness.openPopover(document.body, 700, 900, 300, 200, true));
  const popover = page.locator(".gallery-popover");
  await expect(popover).toBeFocused();
  // fitInViewport: 1024 × 768, 10px inside: left 1024 - 10 - 300, top 768 - 10 - 200
  expect(await popover.evaluate((el) => [el.style.left, el.style.top])).toEqual(["714px", "558px"]);
  await page.keyboard.press("Tab");
  await expect(page.locator(".popover-button[data-i='0']")).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.locator(".popover-button[data-i='2']")).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.locator(".popover-button[data-i='0']")).toBeFocused();
  await page.click(".popover-button[data-i='1']");
  expect(await log(page)).toEqual([]);
  await page.mouse.click(5, 5);
  expect(await log(page)).toEqual(["popover-close"]);
  expect(errors).toEqual([]);
});
