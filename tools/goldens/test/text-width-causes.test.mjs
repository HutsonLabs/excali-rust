// scripts/fixtures/text_width_causes.mjs (ex-g303): the page loads a face
// for every family a known deviation is set in, and --check holds another
// machine's Chrome to the committed numbers within the 0.001 px the Rust
// tests (crates/excali-text/tests/text_width_corpus.rs) hold the port to,
// and everything else exactly.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { REPO_ROOT } from "./helpers.mjs";

import {
  differences,
  familiesOf,
  OUT,
  TOLERANCE,
  texts,
} from "../../../scripts/fixtures/text_width_causes.mjs";

const committed = () => JSON.parse(readFileSync(OUT, "utf8"));

test("the page loads every family a known deviation is set in", () => {
  // Virgil, Cascadia, Excalifont, Lilita One, Comic Shanns: the gated
  // families (GATED_FAMILIES) with a listed text. Nunito has none.
  assert.deepEqual(familiesOf(texts()), [1, 3, 5, 7, 8]);
});

test("the known deviations are the report's, in its order", () => {
  const report = JSON.parse(
    readFileSync(join(REPO_ROOT, "crates", "excali-text", "tests", "fixtures", "text-width-corpus-report.json"), "utf8"),
  );
  const list = texts();
  assert.equal(list.length, report.known_deviations.length);
  assert.deepEqual(
    list.map((t) => t.id),
    report.known_deviations.map((k) => k.id),
  );
  assert.deepEqual(
    committed().texts.map((t) => [t.id, t.cause]),
    list.map((t) => [t.id, t.cause]),
  );
});

test("check tolerates 0.001 px and nothing else", () => {
  assert.equal(TOLERANCE, 0.001);
  const base = committed();
  assert.deepEqual(differences(base, structuredClone(base)), []);

  const browser = structuredClone(base);
  browser.browser = "Chromium 0 (elsewhere)";
  assert.deepEqual(differences(base, browser), []);

  const near = structuredClone(base);
  near.texts[0].refreshed += 0.0009;
  near.texts[1].ink_box_62228e0b -= 0.0009;
  assert.deepEqual(differences(base, near), []);

  const far = structuredClone(base);
  far.texts[0].refreshed += 0.0011;
  assert.equal(differences(base, far).length, 1);
  assert.match(differences(base, far)[0], /texts\.0\.refreshed/);

  const cause = structuredClone(base);
  cause.texts[0].cause = "kept_width";
  assert.equal(differences(base, cause).length, 1);

  const pin = structuredClone(base);
  pin.upstream = "0000000";
  assert.equal(differences(base, pin).length, 1);

  const fewer = structuredClone(base);
  fewer.texts.pop();
  assert.ok(differences(base, fewer).length > 0);

  const missing = structuredClone(base);
  delete missing.texts[0].restored;
  assert.ok(differences(base, missing).length > 0);
});
