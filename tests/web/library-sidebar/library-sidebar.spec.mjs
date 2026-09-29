// excali-ui's library sidebar in Chromium (ex-526).
//
// The page tools/library-sidebar serves mounts excali_ui::library_sidebar's
// default sidebar (and LayerUI's library trigger) in an editor container
// with the sidebar's stylesheets, runs each event through
// excali_ui::library_sidebar::update and renders again. The checks: the
// docked sidebar's size and place (RIGHT_SIDEBAR_WIDTH, 302 px, less the
// island's outer margin, flush right: Sidebar.scss), the tab triggers,
// search, Shift-click selection, insertion, drag data, header menu,
// docking, Escape and the trigger, as upstream's handlers behave
// (crates/excali-ui/tests/fixtures/library-sidebar.json holds the same
// events against upstream's React tree; this suite holds the DOM handlers
// that report them).
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const fixture = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates/excali-ui/tests/fixtures/library-sidebar.json"), "utf8"),
);

const ITEMS = fixture.items.filter((i) => !i.id.startsWith("m"));

const start = async (page, config = {}) => {
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady || window.harnessError);
  expect(await page.evaluate(() => window.harnessError ?? null)).toBeNull();
  await page.evaluate((c) => window.sidebar.start(c), {
    items: ITEMS,
    pending: [],
    tab: "library",
    docked: false,
    canFitSidebar: true,
    ...config,
  });
};

const take = (page) => page.evaluate(() => window.sidebar.take());
const snapshot = (page) => page.evaluate(() => window.sidebar.snapshot());
const units = (page) => page.locator(".library-unit");

test("docked, the sidebar takes the 302 px column at the right edge", async ({ page }) => {
  await start(page, { docked: true });
  const sidebar = page.locator(".sidebar");
  await expect(sidebar).toHaveClass(/sidebar--docked/);
  const box = await sidebar.boundingBox();
  const column = await page.evaluate(() =>
    parseFloat(getComputedStyle(document.querySelector(".excalidraw")).getPropertyValue("--right-sidebar-width")),
  );
  expect(column).toBe(302);
  // calc(var(--right-sidebar-width) - var(--space-factor) * 2): 302 - 8
  expect(box.width).toBe(294);
  expect(box.x + box.width).toBe(1440);
  expect(box.y).toBe(0);
  expect(box.height).toBe(900);
  // docked, no shadow; LayerUI makes room and hides the trigger
  expect(await sidebar.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");
  await expect(page.locator(".excalidraw")).toHaveAttribute("data-docked-and-fits", "true");
  await expect(page.locator(".sidebar-trigger__label-element")).toHaveCount(0);
});

test("undocked, the sidebar floats with its shadow, the same width", async ({ page }) => {
  await start(page);
  const sidebar = page.locator(".sidebar");
  await expect(sidebar).not.toHaveClass(/sidebar--docked/);
  expect((await sidebar.boundingBox()).width).toBe(294);
  expect(await sidebar.evaluate((el) => getComputedStyle(el).boxShadow)).not.toBe("none");
  await expect(page.locator(".excalidraw")).toHaveAttribute("data-docked-and-fits", "false");
});

test("sections: personal then Excalidraw library, units 55 px", async ({ page }) => {
  await start(page);
  await expect(page.locator(".library-menu-items-container__header")).toHaveText([
    "Personal Library",
    "Excalidraw Library",
  ]);
  await expect(units(page)).toHaveCount(6);
  const box = await units(page).first().boundingBox();
  expect([box.width, box.height]).toEqual([55, 55]);
  await expect(page.locator(".library-menu-browse-button")).toHaveAttribute(
    "href",
    "https://libraries.excalidraw.com?target=_blank&referrer=http://localhost/&useHash=true&token=harness&theme=light&version=2",
  );
});

test("the tab triggers switch tabs on a press", async ({ page }) => {
  await start(page);
  await page.locator(".sidebar-tab-trigger").first().click();
  expect(await take(page)).toEqual([{ setAppState: { openSidebar: { name: "default", tab: "search" } } }]);
  await expect(page.locator("[data-testid=search]")).not.toHaveAttribute("hidden", "");
  await expect(page.locator(".search-menu-slot")).toHaveCount(1);
  // the search tab forces docking: no dock button
  await expect(page.locator(".sidebar__dock")).toHaveCount(0);
  await expect(page.locator(".sidebar")).toHaveClass(/sidebar--docked/);
});

test("the dock button docks and undocks", async ({ page }) => {
  await start(page);
  await page.locator(".sidebar__dock").click();
  expect(await take(page)).toEqual([
    { trackEvent: ["sidebar", "toggleDock (dock)", "(desktop)"] },
    { setAppState: { defaultSidebarDockedPreference: true } },
  ]);
  await expect(page.locator(".sidebar__dock")).toHaveClass(/selected/);
  await page.locator(".sidebar__dock").click();
  await expect(page.locator(".sidebar")).not.toHaveClass(/sidebar--docked/);
});

test("a click inserts the item, Shift-clicks select a range, a click in it inserts them", async ({ page }) => {
  await start(page);
  await units(page).nth(0).locator(".library-unit__dragger").click();
  expect(await take(page)).toEqual([{ insert: ["u1"] }, { focusContainer: true }]);
  await units(page).nth(1).locator(".library-unit__dragger").click({ modifiers: ["Shift"] });
  await expect(page.locator(".library-actions-counter")).toHaveText("1");
  await units(page).nth(4).locator(".library-unit__dragger").click({ modifiers: ["Shift"] });
  expect((await snapshot(page)).selected).toEqual(["u2", "u3", "u4", "p1"]);
  await expect(page.locator(".library-actions-counter")).toHaveText("4");
  await expect(page.locator(".library-unit--selected")).toHaveCount(4);
  await units(page).nth(2).locator(".library-unit__dragger").click();
  expect(await take(page)).toEqual([{ insert: ["u2", "u3", "u4", "p1"] }, { focusContainer: true }]);
});

test("hovering shows the checkbox, which toggles the item", async ({ page }) => {
  await start(page);
  await units(page).nth(1).hover();
  await expect(units(page).nth(1)).toHaveClass(/library-unit--hover/);
  await units(page).nth(1).locator(".library-unit__checkbox").click();
  expect((await snapshot(page)).selected).toEqual(["u2"]);
  await expect(units(page).nth(1).locator(".Checkbox")).toHaveClass(/is-checked/);
});

test("dragging an item carries its id, a selection's ids with it", async ({ page }) => {
  await start(page);
  const drag = (i) =>
    units(page)
      .nth(i)
      .locator(".library-unit__dragger")
      .evaluate((el) => {
        const dataTransfer = new DataTransfer();
        el.dispatchEvent(new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer }));
        return dataTransfer.getData("application/vnd.excalidrawlib.ids+json");
      });
  expect(JSON.parse(await drag(1))).toEqual({ itemIds: ["u2"] });
  await units(page).nth(0).locator(".library-unit__dragger").click({ modifiers: ["Shift"] });
  await expect(page.locator(".library-actions-counter")).toHaveText("1");
  await units(page).nth(4).locator(".library-unit__dragger").click({ modifiers: ["Shift"] });
  // the harness renders after the handler returns
  await expect(page.locator(".library-actions-counter")).toHaveText("5");
  expect(JSON.parse(await drag(1))).toEqual({ itemIds: ["u1", "u2", "u3", "u4", "p1"] });
});

test("the pending selection is added to the library", async ({ page }) => {
  await start(page, { pending: [fixture.canvas[0]] });
  const pending = units(page).first();
  await expect(pending.locator(".library-unit__adder")).toHaveCount(1);
  await pending.locator(".library-unit__dragger").click();
  const effects = await take(page);
  expect(effects[0]).toEqual({ trackEvent: ["element", "addToLibrary", "ui"] });
  expect(effects[1]).toEqual({
    setAppState: { selectedElementIds: {}, selectedGroupIds: {}, activeEmbeddable: null },
  });
  expect(effects[2].setLibrary).toEqual(["id0", "u1", "u2", "u3", "u4", "p1", "p2"]);
  await expect(units(page)).toHaveCount(7);
});

test("search filters by name; Escape clears it, then closes an undocked sidebar", async ({ page }) => {
  await start(page);
  const input = page.locator(".library-menu-items-container__search input");
  await expect(input).toBeFocused();
  await input.pressSequentially("caf");
  await expect(input).toHaveValue("caf");
  await expect(page.locator(".library-unit__name")).toHaveText(["Café Diamond", "Frame Cafe"]);
  await input.press("Escape");
  await expect(input).toHaveValue("");
  await expect(units(page)).toHaveCount(6);
  expect(await take(page)).toEqual([]);
  await input.press("Escape");
  expect(await take(page)).toEqual([{ setAppState: { openSidebar: null } }, { focusContainer: true }]);
  await expect(page.locator(".sidebar")).toHaveCount(0);
});

test("no matches offers to clear the search", async ({ page }) => {
  await start(page);
  const input = page.locator(".library-menu-items-container__search input");
  await input.pressSequentially("zzz");
  await expect(page.locator(".library-menu-items__no-items__hint")).toHaveText("No matching items found...");
  await page.getByText("Clear search").click();
  await expect(input).toHaveValue("");
});

test("Escape on the canvas closes an undocked sidebar, not a docked one", async ({ page }) => {
  // the search field has focus: move it to the page
  const escapeOnBody = async () => {
    await page.evaluate(() => document.activeElement?.blur());
    await page.keyboard.press("Escape");
  };
  await start(page, { docked: true });
  await escapeOnBody();
  await expect(page.locator(".sidebar")).toHaveCount(1);
  expect(await take(page)).toEqual([]);
  await start(page);
  await escapeOnBody();
  await expect(page.locator(".sidebar")).toHaveCount(0);
  expect(await take(page)).toEqual([{ setAppState: { openSidebar: null } }]);
});

test("the checkbox sits in the unit's corner, 16 px", async ({ page }) => {
  await start(page);
  await units(page).nth(1).hover();
  const unit = await units(page).nth(1).boundingBox();
  const box = await units(page).nth(1).locator(".Checkbox-box").boundingBox();
  expect([box.width, box.height]).toEqual([16, 16]);
  expect(box.x + box.width).toBeLessThanOrEqual(unit.x + unit.width);
  expect(box.y).toBeGreaterThanOrEqual(unit.y);
});

test("a press outside closes an undocked sidebar; the trigger opens it again", async ({ page }) => {
  await start(page);
  await page.mouse.click(200, 400);
  await expect(page.locator(".sidebar")).toHaveCount(0);
  const trigger = page.locator(".sidebar-trigger__label-element");
  await expect(trigger).toHaveAttribute("aria-pressed", "false");
  await trigger.click();
  expect(await take(page)).toEqual([
    { setAppState: { openSidebar: null } },
    { setAppState: { openSidebar: { name: "default", tab: "library" }, openMenu: null, openPopup: null } },
    { trackEvent: ["sidebar", "default (open)", "button (desktop)"] },
  ]);
  await expect(page.locator(".sidebar")).toHaveCount(1);
});

test("the header menu: Open, Save to..., Reset library; with a selection Save, Publish, Remove", async ({ page }) => {
  await start(page);
  await page.locator(".dropdown-menu-button").click();
  const items = page.locator("[role=menuitem]");
  await expect(items).toHaveText(["Open", "Save to...", "Reset library"]);
  // placed below the trigger, inside the viewport
  const menu = await page.locator(".library-menu").boundingBox();
  const trigger = await page.locator(".dropdown-menu-button").boundingBox();
  expect(menu.y).toBeGreaterThan(trigger.y);
  expect(menu.x + menu.width).toBeLessThanOrEqual(1440);
  await page.locator("[data-testid=lib-dropdown--export]").click();
  expect(await take(page)).toEqual([{ exportLibrary: ["u1", "u2", "u3", "u4", "p1", "p2"] }]);
  await expect(page.locator(".library-menu")).toHaveCount(0);
  await units(page).nth(5).locator(".library-unit__dragger").click({ modifiers: ["Shift"] });
  await expect(page.locator(".library-actions-counter")).toHaveText("1");
  await page.locator(".dropdown-menu-button").click();
  await expect(items).toHaveText(["Save to...", "Rename or publish", "Remove"]);
  await page.keyboard.press("Escape");
  await expect(page.locator(".library-menu")).toHaveCount(0);
});
