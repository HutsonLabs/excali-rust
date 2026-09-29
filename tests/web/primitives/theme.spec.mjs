// excali-ui's theme tokens in Chromium (ex-532).
//
// The harness installs the stylesheet (upstream's theme.scss compiled, with
// the primitives' SCSS) and mounts the gallery in the page's `.excalidraw`
// #editor. The suite checks, against tests/fixtures/theme-tokens.json
// (tools/goldens/theme-tokens.mjs, theme.scss at the pin):
//
// - every light token computes on `.excalidraw` as theme.scss declares it,
//   and every dark one once the container has `theme--dark` (App.tsx:4434-4437
//   toggles it), the container's inline `--right-sidebar-width` (App.tsx:2453)
//   included;
// - a host page's own `.excalidraw` / `.excalidraw.theme--dark` rules
//   override `--color-primary*` (upstream's theming guidance), even though
//   the host's stylesheet precedes the harness's in the page: the stylesheet
//   goes first in the head, so equal specificity lets the host's win; the
//   primitives follow (the active RadioGroup choice's background).
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import { REPO_ROOT } from "../lib/serve.mjs";

const FIXTURE = JSON.parse(
  readFileSync(join(REPO_ROOT, "crates", "excali-ui", "tests", "fixtures", "theme-tokens.json"), "utf8"),
);

// the effective declarations: later ones (dark over light) win
const effective = (...sets) => Object.fromEntries(sets.flat());
const LIGHT = effective(FIXTURE.light, Object.entries(FIXTURE.container));
const DARK = effective(FIXTURE.light, FIXTURE.dark, Object.entries(FIXTURE.container));

const PRIMARY = [
  "--color-primary",
  "--color-primary-darker",
  "--color-primary-darkest",
  "--color-primary-light",
  "--color-primary-light-darker",
  "--color-primary-hover",
];

const open = async (page) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("/");
  await page.waitForFunction(() => window.harnessReady === true || window.harnessError);
  expect(await page.evaluate(() => window.harnessError || null), "the harness module loads").toBeNull();
  return errors;
};

const computed = (page, selector, properties) =>
  page.evaluate(
    ([s, ps]) => {
      const cs = getComputedStyle(document.querySelector(s));
      return Object.fromEntries(ps.map((p) => [p, cs.getPropertyValue(p).trim()]));
    },
    [selector, properties],
  );

const setTheme = (page, dark) =>
  page.evaluate((d) => window.harness.setTheme(document.getElementById("editor"), d), dark);

const squash = (v) => v.replace(/\s+/g, " ").trim();

const expectTokens = async (page, want) => {
  const names = Object.keys(want);
  const got = await computed(page, "#editor", names);
  let literal = 0;
  for (const name of names) {
    expect(got[name], `${name} is set`).not.toBe("");
    // var() references compute to what they name; the rest as declared
    if (!want[name].includes("var(")) {
      expect(squash(got[name]), name).toBe(squash(want[name]));
      literal++;
    }
  }
  return literal;
};

test("every theme token computes on .excalidraw, light and dark", async ({ page }) => {
  const errors = await open(page);
  expect(await page.getAttribute("#editor", "class")).toBe("excalidraw");
  expect(await expectTokens(page, LIGHT)).toBeGreaterThan(80);
  await setTheme(page, true);
  expect(await page.getAttribute("#editor", "class")).toBe("excalidraw theme--dark");
  expect(await expectTokens(page, DARK)).toBeGreaterThan(80);
  // a var() token follows what it names into the dark theme
  expect(await computed(page, "#editor", ["--color-promo"])).toEqual({ "--color-promo": "#a8a5ff" });
  await setTheme(page, false);
  expect(await page.getAttribute("#editor", "class")).toBe("excalidraw");
  await expectTokens(page, LIGHT);
  expect(errors).toEqual([]);
});

test("the stylesheet goes first in the head", async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => document.head.firstElementChild.getAttribute("data-excali-ui"))).toBe(
    "primitives",
  );
  expect(await page.evaluate(() => document.querySelectorAll("style[data-excali-ui]").length)).toBe(1);
});

test("a host page overrides --color-primary* on .excalidraw and .excalidraw.theme--dark", async ({ page }) => {
  const errors = await open(page);
  const active = ".RadioGroup__choice.active";
  const background = async () => (await computed(page, active, ["background-color"]))["background-color"];
  expect(await background()).toBe("rgb(105, 101, 219)"); // #6965db
  await page.evaluate(() => document.getElementById("host-theme").setAttribute("media", "all"));
  expect(await computed(page, "#editor", PRIMARY)).toEqual({
    "--color-primary": "#0b7a3e",
    "--color-primary-darker": "#096a36",
    "--color-primary-darkest": "#07592d",
    "--color-primary-light": "#d3f9e0",
    "--color-primary-light-darker": "#b8f2cc",
    "--color-primary-hover": "#0a6e38",
  });
  expect(await background()).toBe("rgb(11, 122, 62)");
  // tokens the host leaves alone keep upstream's values
  expect(await computed(page, "#editor", ["--color-selection", "--color-brand-hover"])).toEqual({
    "--color-selection": "#6965db",
    "--color-brand-hover": "#5753d0",
  });
  await setTheme(page, true);
  expect(await computed(page, "#editor", PRIMARY)).toEqual({
    "--color-primary": "#ffb000",
    "--color-primary-darker": "#ffbb26",
    "--color-primary-darkest": "#ffc64d",
    "--color-primary-light": "#5c4300",
    "--color-primary-light-darker": "#4d3800",
    "--color-primary-hover": "#ffc233",
  });
  expect(await background()).toBe("rgb(255, 176, 0)");
  expect(await computed(page, "#editor", ["--color-selection"])).toEqual({ "--color-selection": "#b4b0ff" });
  // the host's styles off again: upstream's dark primary
  await page.evaluate(() => document.getElementById("host-theme").setAttribute("media", "not all"));
  expect(await background()).toBe("rgb(168, 165, 255)"); // #a8a5ff
  expect(errors).toEqual([]);
});
