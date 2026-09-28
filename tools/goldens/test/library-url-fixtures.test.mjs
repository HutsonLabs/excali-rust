// The library import-from-URL fixtures of excali-core's `library_url` and
// `link` tests are upstream's output: tools/goldens/library-url-fixtures.mjs
// regenerates them from the pinned checkout, byte-stable across runs, and
// --check fails when the committed file differs.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";

import { REPO_ROOT, TOOL_DIR } from "./helpers.mjs";

const GENERATOR = join(TOOL_DIR, "library-url-fixtures.mjs");
const NAME = "library-url.json";
const COMMITTED = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures", NAME);

const scratch = mkdtempSync(join(tmpdir(), "library-url-fixtures-test-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const run = (args, env = {}) =>
  spawnSync(process.execPath, [GENERATOR, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });

test("two runs are byte-identical and equal to the committed fixture", () => {
  const outs = [join(scratch, "a"), join(scratch, "b")];
  for (const out of outs) {
    const r = run(["--out", out]);
    assert.equal(r.status, 0, r.stderr);
  }
  const first = readFileSync(join(outs[0], NAME));
  assert.ok(first.equals(readFileSync(join(outs[1], NAME))), "differs between runs");
  assert.ok(
    first.equals(readFileSync(COMMITTED)),
    `${NAME} is stale: run node tools/goldens/library-url-fixtures.mjs`,
  );
});

test("output is upstream's allow-list, token parser and link sanitizer", () => {
  const out = join(scratch, "c");
  assert.equal(run(["--out", out]).status, 0);
  const fixture = JSON.parse(readFileSync(join(out, NAME), "utf8"));
  // library.ts:54-58
  assert.deepEqual(fixture.allowedLibraryUrls, [
    "excalidraw.com",
    "raw.githubusercontent.com/excalidraw/excalidraw-libraries",
  ]);
  const validate = new Map(fixture.validate.map((c) => [c.id, c]));
  assert.equal(validate.get("libraries-subdomain").ok, true);
  assert.equal(validate.get("github-library").ok, true);
  assert.equal(
    validate.get("suffix-without-dot").error,
    'Invalid or disallowed library URL: "https://notexcalidraw.com/x"',
  );
  assert.equal(validate.get("github-prefix-longer-segment").errorType, "Error");
  assert.equal(validate.get("not-a-url").errorType, "TypeError");
  // the allow-list entry is a regular expression source
  assert.equal(validate.get("dot-is-any-character").ok, true);
  assert.match(validate.get("regexp-unterminated-group-host").error, /^Invalid regular expression: .*Unterminated group$/);
  // where the url crate parses differently from new URL
  assert.equal(validate.get("file-github-backslash-after-host").errorType, "Error");
  assert.equal(validate.get("file-github").ok, true);
  assert.equal(validate.get("list-file-backslash-after-host").errorType, "Error");
  assert.equal(
    validate.get("xn-empty-label-https").error,
    'Invalid or disallowed library URL: "https://xn--/excalidraw/excalidraw-libraries/x"',
  );
  assert.equal(validate.get("xn-empty-label-subdomain").ok, true);
  assert.equal(validate.get("credentials-no-host").errorType, "TypeError");

  const tokens = new Map(fixture.tokens.map((c) => [c.id, c.result]));
  assert.deepEqual(tokens.get("hash"), {
    libraryUrl: "https://libraries.excalidraw.com/a.excalidrawlib",
    idToken: "abc123",
  });
  assert.deepEqual(tokens.get("query-legacy-token-in-hash"), { libraryUrl: "u", idToken: "t" });
  assert.equal(tokens.get("token-only"), null);

  const normalized = new Map(fixture.normalizeLink.map((c) => [c.input, c.output]));
  assert.equal(normalized.get("javascript:alert(1)"), "about:blank");
  assert.equal(normalized.get('https://example.com/"quoted"'), "https://example.com/&quot;quoted&quot;");
  const valid = new Map(fixture.toValidURL.map((c) => [c.id, c.output]));
  assert.equal(valid.get("to-valid-url-03"), "https://excalidraw.com/relative/path");
  assert.equal(valid.get("to-valid-url-07"), "about:blank");

  const imports = new Map(fixture.import.map((c) => [c.id, c]));
  assert.equal(imports.get("encoded-library").url, "https://libraries.excalidraw.com/libraries/a.excalidrawlib");
  assert.equal(imports.get("malformed-percent").errorType, "URIError");
  assert.equal(imports.get("credentials-no-host-plain").error, 'Invalid or disallowed library URL: "about:blank"');
});

test("--check reports a stale fixture", () => {
  const copy = join(scratch, "stale");
  assert.equal(run(["--out", copy]).status, 0);
  const file = join(copy, NAME);
  writeFileSync(file, readFileSync(file, "utf8").replace('"idToken": "abc123"', '"idToken": "abc124"'));
  const r = run(["--check", "--out", copy]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /library-url\.json/);
  assert.equal(run(["--check"]).status, 0);
});

test("refuses to run against anything but a clean checkout at the pin", () => {
  const empty = mkdtempSync(join(scratch, "no-upstream-"));
  const r = run(["--out", join(scratch, "never")], { UPSTREAM_DIR: empty });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /upstream checkout is not clean at the pin/);
});
