// The library sidebar's header menu in <excali-editor> (ex-537): Open merges
// a picked .excalidrawlib, Save to... saves the selection or the whole
// library as library.excalidrawlib, Reset library and Remove ask with
// upstream's ConfirmDialog, and Rename or publish opens PublishLibrary,
// whose submission goes to the host as library-publish
// (packages/excalidraw/components/LibraryMenuHeaderContent.tsx,
// ConfirmDialog.tsx, PublishLibrary.tsx; the dialogs' DOM and handlers are
// held to upstream's by crates/excali-ui/tests/library_sidebar.rs).
import { expect, test } from "@playwright/test";

const SHORT = { timeout: 5_000 };

const rect = (id, x) => ({
  id, type: "rectangle", x, y: 0, width: 10, height: 10, angle: 0,
  strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
  strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [],
  frameId: null, index: "a0", roundness: null, seed: 1, version: 1, versionNonce: 1,
  isDeleted: false, boundElements: null, updated: 1, link: null, locked: false,
});

const library = (items) =>
  JSON.stringify({
    type: "excalidrawlib",
    version: 2,
    source: "https://excalidraw.com",
    libraryItems: items.map(([id, name], i) => ({
      id,
      status: "unpublished",
      created: i + 1,
      ...(name ? { name } : {}),
      elements: [rect(`${id}-r`, i * 20)],
    })),
  });

const TWO = library([["item-1", "one"], ["item-2", ""]]);

/** Mounts an editor with a library of `text` and opens the sidebar. */
const mount = async (page, text = TWO) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/editor.html");
  await page.waitForFunction(() => window.editorReady === true);
  await page.evaluate(async (text) => {
    localStorage.clear();
    const ed = document.createElement("excali-editor");
    document.getElementById("host").appendChild(ed);
    window.ed = ed;
    await ed.importLibrary(text, { merge: false });
  }, text);
  await page.locator("excali-editor .default-sidebar-trigger").click(SHORT);
  await expect(page.locator("excali-editor .default-sidebar")).toBeVisible(SHORT);
  return errors;
};

const items = async (page) =>
  JSON.parse(await page.evaluate(() => window.ed.exportLibrary())).libraryItems;

const menu = async (page, label) => {
  await page.locator("excali-editor .library-menu-dropdown-container .dropdown-menu-button").click(SHORT);
  await page.locator("excali-editor .library-menu [role=menuitem]", { hasText: label }).click(SHORT);
};

const select = (page, index) =>
  page.locator("excali-editor .library-unit__dragger").nth(index).click({ modifiers: ["Shift"] });

const dialog = (page, cls) => page.locator(`body > .excalidraw-modal-container .Modal.${cls}`);

test("Save to... saves the library, or the selected items, as library.excalidrawlib", async ({ page }) => {
  const errors = await mount(page);
  const save = async () => {
    const download = page.waitForEvent("download");
    await menu(page, "Save to...");
    const file = await download;
    expect(file.suggestedFilename()).toBe("library.excalidrawlib");
    const text = await (await file.createReadStream()).toArray();
    return JSON.parse(Buffer.concat(text).toString("utf8"));
  };
  const all = await save();
  expect([all.type, all.version]).toEqual(["excalidrawlib", 2]);
  expect(all.libraryItems.map((i) => i.id)).toEqual(["item-1", "item-2"]);
  await select(page, 1);
  expect((await save()).libraryItems.map((i) => i.id)).toEqual(["item-2"]);
  expect(errors).toEqual([]);
});

test("Open merges a picked library file; a file that is no library leaves it", async ({ page }) => {
  const errors = await mount(page, library([["item-1", "one"]]));
  const open = async (text) => {
    const chooser = page.waitForEvent("filechooser");
    await menu(page, "Open");
    await (await chooser).setFiles({ name: "x.excalidrawlib", mimeType: "application/json", buffer: Buffer.from(text) });
  };
  await open(library([["item-9", "nine"]]));
  await expect.poll(async () => (await items(page)).map((i) => i.id), SHORT).toEqual(["item-9", "item-1"]);
  await expect(page.locator("excali-editor .library-unit")).toHaveCount(2, SHORT);
  await open("not a library");
  await page.waitForTimeout(200);
  expect((await items(page)).length).toBe(2);
  expect(errors).toEqual([]);
});

test("Reset library asks first; Cancel keeps the library, Confirm clears it", async ({ page }) => {
  const errors = await mount(page);
  await menu(page, "Reset library");
  const confirm = dialog(page, "confirm-dialog");
  await expect(confirm.locator(".Dialog__title")).toHaveText("Reset library", SHORT);
  await expect(confirm.locator("p")).toHaveText("This will clear your library. Are you sure?");
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toHaveCount(0, SHORT);
  expect((await items(page)).length).toBe(2);
  // the container has the focus back
  expect(await page.evaluate(() => document.activeElement?.classList.contains("excalidraw-container"))).toBe(true);
  // Escape is the dialog's close request
  await menu(page, "Reset library");
  await expect(confirm).toBeVisible(SHORT);
  await page.keyboard.press("Escape");
  await expect(confirm).toHaveCount(0, SHORT);
  expect((await items(page)).length).toBe(2);
  await menu(page, "Reset library");
  await confirm.getByRole("button", { name: "Confirm" }).click();
  await expect(confirm).toHaveCount(0, SHORT);
  expect(await items(page)).toEqual([]);
  expect(errors).toEqual([]);
});

test("Remove asks to delete the selected items, then removes them", async ({ page }) => {
  const errors = await mount(page);
  await select(page, 0);
  await menu(page, "Remove");
  const confirm = dialog(page, "confirm-dialog");
  await expect(confirm.locator(".Dialog__title")).toHaveText("Remove selected items from library", SHORT);
  await expect(confirm.locator("p")).toHaveText("Delete 1 item(s) from library?");
  // a click on the backdrop is the dialog's close request: nothing removed
  await confirm.locator(".Modal__background").click({ position: { x: 5, y: 5 } });
  await expect(confirm).toHaveCount(0, SHORT);
  expect((await items(page)).length).toBe(2);
  await menu(page, "Remove");
  await confirm.getByRole("button", { name: "Confirm" }).click();
  await expect(confirm).toHaveCount(0, SHORT);
  expect((await items(page)).map((i) => i.id)).toEqual(["item-2"]);
  await expect(page.locator("excali-editor .library-actions-counter")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("Rename or publish: names are required, saved on exit, and the host takes the submission", async ({ page }) => {
  const errors = await mount(page);
  await page.evaluate(() => {
    window.published = [];
    window.ed.addEventListener("library-publish", (e) => {
      e.preventDefault();
      window.published.push({ library: JSON.parse(e.detail.library), fields: e.detail.fields });
      e.detail.respond("https://libraries.test/pr/1");
    });
  });
  await select(page, 0);
  await select(page, 1);
  await menu(page, "Rename or publish");
  const publish = dialog(page, "publish-library");
  await expect(publish.locator(".Dialog__title")).toHaveText("Publish library", SHORT);
  await expect(publish.locator(".single-library-item")).toHaveCount(2);
  await expect(publish.locator(".single-library-item__svg svg")).toHaveCount(2);
  await publish.locator("input[name=name]").fill("Shapes");
  await publish.locator("textarea[name=description]").fill("Two shapes");
  await publish.locator("input[name=authorName]").fill("Ada");
  // the second item has no name
  await publish.getByRole("button", { name: "Submit" }).click();
  await expect(publish.locator(".single-library-item .error").nth(1)).toHaveText("Required", SHORT);
  expect(await page.evaluate(() => window.published.length)).toBe(0);
  // Save name(s) and exit stores the fields; a name typed after a failed
  // submit stays in the dialog (upstream's items are copies by then)
  await publish.locator(".single-library-item input").nth(0).fill("first");
  await publish.getByRole("button", { name: "Save name(s) and exit" }).click();
  await expect(publish).toHaveCount(0, SHORT);
  expect((await items(page)).map((i) => i.name ?? null)).toEqual(["one", null]);
  expect(JSON.parse(await page.evaluate(() => localStorage.getItem("publish-library-data")))).toEqual({
    authorName: "Ada",
    githubHandle: "",
    name: "Shapes",
    description: "Two shapes",
    twitterHandle: "",
    website: "",
  });
  // open again: the fields come back; name the second item and submit
  await menu(page, "Rename or publish");
  await expect(publish.locator("input[name=authorName]")).toHaveValue("Ada", SHORT);
  await publish.locator(".single-library-item input").nth(1).fill("two");
  await publish.getByRole("button", { name: "Submit" }).click();
  const success = dialog(page, "publish-library-success");
  await expect(success.locator(".Dialog__title")).toHaveText("Library submitted", SHORT);
  await expect(success.locator("p")).toHaveText(
    "Thank you Ada. Your library has been submitted for review. You can track the status here",
  );
  await expect(success.locator("p a")).toHaveAttribute("href", "https://libraries.test/pr/1");
  const [sent] = await page.evaluate(() => window.published);
  expect(sent.library.libraryItems.map((i) => [i.id, i.name])).toEqual([
    ["item-1", "one"],
    ["item-2", "two"],
  ]);
  expect(sent.fields).toEqual([
    ["title", "Shapes"],
    ["authorName", "Ada"],
    ["githubHandle", ""],
    ["name", "Shapes"],
    ["description", "Two shapes"],
    ["twitterHandle", ""],
    ["website", ""],
  ]);
  // the submitted items are marked published; the saved fields are gone
  expect((await items(page)).map((i) => i.status)).toEqual(["published", "published"]);
  expect(await page.evaluate(() => localStorage.getItem("publish-library-data"))).toBeNull();
  await success.locator(".publish-library-success-close").click();
  await expect(success).toHaveCount(0, SHORT);
  expect(errors).toEqual([]);
});

test("a submission no host takes is alerted", async ({ page }) => {
  const errors = await mount(page, library([["item-1", "one"]]));
  const alerts = [];
  page.on("dialog", (d) => {
    alerts.push(d.message());
    d.accept();
  });
  await select(page, 0);
  await menu(page, "Rename or publish");
  const publish = dialog(page, "publish-library");
  await publish.locator("input[name=name]").fill("Shapes");
  await publish.locator("textarea[name=description]").fill("Two shapes");
  await publish.locator("input[name=authorName]").fill("Ada");
  await publish.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => alerts, SHORT).toEqual(["Error: The host did not handle library-publish."]);
  await expect(publish).toBeVisible();
  expect(errors).toEqual([]);
});
