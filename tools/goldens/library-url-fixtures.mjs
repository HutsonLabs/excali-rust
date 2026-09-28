#!/usr/bin/env node
// Library import-from-URL fixtures for excali-core's `library_url` and
// `link` ports (ex-109): upstream's own validateLibraryUrl and
// parseLibraryTokensFromUrl (packages/excalidraw/data/library.ts:497-543,
// allow-list at library.ts:54-58), toValidURL and normalizeLink
// (packages/common/src/url.ts:5-37, with @braintree/sanitize-url 6.0.2 as
// upstream's yarn.lock resolves it), run from the pinned checkout under
// plain Node.
//
//   node tools/goldens/library-url-fixtures.mjs            write the fixture
//   node tools/goldens/library-url-fixtures.mjs --check    exit 1 if stale
//   node tools/goldens/library-url-fixtures.mjs --out DIR  write (or --check) DIR
//
// Writes crates/excali-core/tests/fixtures/library-url.json:
//
//   { "description", "upstream", "allowedLibraryUrls", "validate": [...],
//     "tokens": [...], "normalizeLink": [...], "toValidURL": [...],
//     "import": [...] }
//
// - validate cases: { id, url, allowList?, ok | error }. The call is
//   validateLibraryUrl(url) with upstream's default allow-list, or
//   validateLibraryUrl(url, allowList); `ok` is true when it returned,
//   `error` the message of what it threw (`errorType` its constructor name).
// - tokens cases: { id, href, result }: parseLibraryTokensFromUrl() with
//   window.location at `href` (its `search` and `hash` getters are the URL
//   standard's, the same as Node's URL); `result` is what it returned, null
//   or { libraryUrl, idToken }.
// - normalizeLink cases: { id, input, output }.
// - toValidURL cases: { id, input, origin, output }: with location.origin
//   `origin`.
// - import cases: { id, input, origin, url | error }: the first steps of
//   importLibraryFromURL (library.ts:726-731), each upstream's own function:
//   decodeURIComponent, toValidURL, then validateLibraryUrl with the
//   default allow-list; `url` is the URL it would fetch, `error` the message
//   of what threw.
//
// The functions are pure apart from `location`, so the output is byte-stable.
// Math.random throws while generating.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { loadUpstream, REPO_ROOT, verifyUpstream } from "./lib/upstream.mjs";

export const FIXTURES_DIR = join(REPO_ROOT, "crates", "excali-core", "tests", "fixtures");
export const FIXTURE = "library-url.json";
const ORIGIN = "https://excalidraw.com";

const ENTRY = `
export { ALLOWED_LIBRARY_URLS, validateLibraryUrl, parseLibraryTokensFromUrl } from "./packages/excalidraw/data/library";
export { toValidURL, normalizeLink } from "./packages/common/src/url";
`;

// library.ts imports browser and React code (file dialogs, image codecs, the
// scene exporter behind library item previews, jotai atoms) that the
// functions above never call; see library-fixtures.mjs.
const STUBS = [
  "react",
  "jotai",
  "jotai-scope",
  "pica",
  "image-blob-reduce",
  "pako",
  "browser-fs-access",
  "png-chunk-text",
  "png-chunks-extract",
  "png-chunks-encode",
  "packages/excalidraw/data/filesystem",
  "packages/excalidraw/data/image",
  "packages/excalidraw/data/encode",
  "packages/excalidraw/hooks/useLibraryItemSvg",
  "packages/excalidraw/scene",
  "packages/excalidraw/scene/export",
];
const SHIMS = {
  "packages/excalidraw/editor-jotai": "module.exports = { atom: (init) => ({ init }) };",
};

const usage = () => {
  process.stderr.write("usage: library-url-fixtures.mjs [--check] [--out DIR]\n");
  process.exit(2);
};

const parseArgs = (argv) => {
  const args = { check: false, out: FIXTURES_DIR };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--check") args.check = true;
    else if (argv[i] === "--out" && argv[i + 1]) args.out = resolve(argv[++i]);
    else usage();
  }
  return args;
};

// -- inputs ---------------------------------------------------------------------

const GH = "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries";

const validateCases = () => [
  // the default allow-list: excalidraw.com and its subdomains, any path
  { id: "excalidraw-root", url: "https://excalidraw.com" },
  { id: "excalidraw-path", url: "https://excalidraw.com/some/lib.excalidrawlib" },
  { id: "libraries-subdomain", url: "https://libraries.excalidraw.com/libraries/youritjang/stick-figures.excalidrawlib" },
  { id: "deep-subdomain", url: "https://a.b.c.excalidraw.com/x" },
  { id: "http-scheme", url: "http://excalidraw.com/x" },
  { id: "ftp-scheme", url: "ftp://excalidraw.com/x" },
  { id: "ws-scheme", url: "ws://excalidraw.com/x" },
  { id: "upper-case-host", url: "HTTPS://EXCALIDRAW.COM/X" },
  { id: "port", url: "https://excalidraw.com:8443/x" },
  { id: "credentials", url: "https://user:pass@excalidraw.com/x" },
  { id: "query-and-hash", url: "https://excalidraw.com/x?y=1#z" },
  { id: "fullwidth-host-mapped", url: "https://\uff45\uff58\uff43\uff41\uff4c\uff49\uff44\uff52\uff41\uff57.com/x" },
  { id: "suffix-without-dot", url: "https://notexcalidraw.com/x" },
  { id: "host-as-prefix", url: "https://excalidraw.com.evil.com/x" },
  { id: "trailing-dot-host", url: "https://excalidraw.com./x" },
  { id: "credentials-trick", url: "https://excalidraw.com@evil.com/x" },
  { id: "host-in-path", url: "https://evil.com/excalidraw.com" },
  { id: "host-in-query", url: "https://evil.com/?excalidraw.com" },
  { id: "host-in-hash", url: "https://evil.com/#excalidraw.com" },
  { id: "idn-lookalike", url: "https://excalidr\u00e4w.com/x" },
  { id: "ipv4", url: "https://127.0.0.1/x" },
  { id: "ipv6", url: "https://[::1]/x" },
  // the dot of the allow-list entry is a regular expression's `.`
  { id: "dot-is-any-character", url: "https://excalidraw-com/x" },
  { id: "dot-is-any-character-subdomain", url: "https://evil.excalidrawxcom/x" },
  { id: "github-dots-any-character", url: "https://rawxgithubusercontent-com/excalidraw/excalidraw-libraries/x" },
  // raw.githubusercontent.com: the path is a prefix, on segment boundaries
  { id: "github-library", url: `${GH}/main/libraries/youritjang/stick-figures.excalidrawlib` },
  { id: "github-prefix-exact", url: GH },
  { id: "github-prefix-slash", url: `${GH}/` },
  { id: "github-prefix-double-slash", url: `${GH}//x` },
  { id: "github-subdomain", url: "https://x.raw.githubusercontent.com/excalidraw/excalidraw-libraries/x" },
  { id: "github-backslash", url: "https://raw.githubusercontent.com/excalidraw\\excalidraw-libraries/x" },
  { id: "github-prefix-longer-segment", url: `${GH}-evil/x` },
  { id: "github-other-repo", url: "https://raw.githubusercontent.com/excalidraw/excalidraw/x" },
  { id: "github-other-owner", url: "https://raw.githubusercontent.com/evil/excalidraw-libraries/x" },
  { id: "github-path-case", url: "https://raw.githubusercontent.com/Excalidraw/excalidraw-libraries/x" },
  { id: "github-dot-segments", url: `${GH}/../../evil/x` },
  { id: "github-encoded-dot-segments", url: `${GH}/%2e%2E/%2e./evil/x` },
  { id: "github-dot-segments-stay", url: `${GH}/a/../x` },
  { id: "github-encoded-slash", url: "https://raw.githubusercontent.com/excalidraw%2Fexcalidraw-libraries/x" },
  { id: "github-root", url: "https://raw.githubusercontent.com/" },
  { id: "github-host-other", url: "https://githubusercontent.com/excalidraw/excalidraw-libraries/x" },
  // non-hierarchical and invalid URLs
  { id: "about-blank", url: "about:blank" },
  { id: "javascript", url: "javascript:alert(1)//excalidraw.com" },
  { id: "data", url: "data:text/plain,excalidraw.com" },
  { id: "file", url: "file:///excalidraw.com/x" },
  { id: "mailto", url: "mailto:a@excalidraw.com" },
  { id: "custom-scheme-host", url: "web+lib://excalidraw.com/x" },
  { id: "not-a-url", url: "excalidraw.com/x" },
  { id: "empty", url: "" },
  { id: "invalid-port", url: "https://excalidraw.com:99999/x" },
  { id: "whitespace-around", url: "  https://excalidraw.com/x \n" },
  { id: "tab-in-host", url: "https://excali\tdraw.com/x" },

  // a caller's own allow-list
  { id: "list-host", url: "https://example.com/a", allowList: ["example.com"] },
  { id: "list-host-subdomain", url: "https://cdn.example.com/a", allowList: ["example.com"] },
  { id: "list-host-default-not-included", url: "https://excalidraw.com/a", allowList: ["example.com"] },
  { id: "list-https-prefix-stripped", url: "https://example.com/libs/a", allowList: ["https://example.com/libs/"] },
  { id: "list-http-prefix-stripped", url: "https://example.com/libs", allowList: ["http://example.com/libs"] },
  { id: "list-path-boundary", url: "https://example.com/libsx", allowList: ["https://example.com/libs/"] },
  { id: "list-path-trailing-slashes", url: "https://example.com/libs//a", allowList: ["example.com/libs///"] },
  { id: "list-path-query-ignored", url: "https://example.com/libs/a", allowList: ["example.com/libs?x=1#y"] },
  { id: "list-upper-case-scheme-kept", url: "https://example.com/a", allowList: ["HTTPS://example.com"] },
  { id: "list-other-scheme-kept", url: "https://example.com/a", allowList: ["ftp://example.com"] },
  { id: "list-empty", url: "https://example.com/a", allowList: [] },
  { id: "list-empty-entry", url: "https://example.com/a", allowList: [""] },
  { id: "list-bad-entry-first-throws", url: "https://example.com/a", allowList: ["exa mple.com", "example.com"] },
  { id: "list-good-entry-first-returns", url: "https://example.com/a", allowList: ["example.com", "exa mple.com"] },
  { id: "list-second-entry", url: "https://other.org/a", allowList: ["example.com", "other.org"] },
  { id: "list-bad-library-url", url: "nope", allowList: ["example.com"] },
  { id: "list-port-ignored", url: "https://example.com:1/a", allowList: ["example.com:8080"] },
  { id: "list-upper-case-host", url: "https://example.com/a", allowList: ["EXAMPLE.com"] },
  { id: "list-ipv4", url: "https://10.0.0.1/a", allowList: ["10.0.0.1"] },
  { id: "list-ipv4-shorthand", url: "https://10.0.0.1/a", allowList: ["0xa.1"] },
  { id: "list-ipv6-class", url: "https://10.0.0.1/a", allowList: ["[::1]"] },
  { id: "list-ipv6-itself", url: "https://[::1]/a", allowList: ["[::1]"] },
  { id: "list-idn-entry", url: "https://xn--exmple-cua.com/a", allowList: ["ex\u00e4mple.com"] },
  // the entry's host and path are regular expression sources
  { id: "regexp-plus-in-path", url: "https://example.com/aaab/x", allowList: ["example.com/a+b"] },
  { id: "regexp-plus-in-path-literal", url: "https://example.com/a+b/x", allowList: ["example.com/a+b"] },
  { id: "regexp-star-in-path", url: "https://example.com/anything", allowList: ["example.com/*"] },
  { id: "regexp-alternation-in-path", url: "https://example.com/zzz/b", allowList: ["example.com/a|b"] },
  { id: "regexp-alternation-in-path-other", url: "https://example.com/zzz", allowList: ["example.com/a|b"] },
  // `^` is in the URL Standard's path percent-encode set: `%5E` in both
  { id: "caret-in-library-path-encoded", url: "https://example.com/^/a", allowList: ["example.com/%5E"] },
  { id: "caret-in-entry-path-encoded", url: "https://example.com/%5E/a", allowList: ["example.com/^"] },
  { id: "regexp-caret-in-path",url: "https://example.com/x", allowList: ["example.com/^x"] },
  { id: "regexp-dollar-in-path", url: "https://example.com/", allowList: ["example.com/$"] },
  { id: "regexp-dollar-in-path-longer", url: "https://example.com/a", allowList: ["example.com/$"] },
  { id: "regexp-class-in-path", url: "https://example.com/b/x", allowList: ["example.com/[a-c]"] },
  { id: "regexp-negated-class-in-path", url: "https://example.com/d", allowList: ["example.com/[^a-c]"] },
  { id: "regexp-empty-class-in-path", url: "https://example.com/a", allowList: ["example.com/[]"] },
  { id: "regexp-any-class-in-path", url: "https://example.com/a", allowList: ["example.com/[^]"] },
  { id: "regexp-group-in-path", url: "https://example.com/ababx/y", allowList: ["example.com/(ab)+x"] },
  { id: "regexp-closing-bracket-literal", url: "https://example.com/a]", allowList: ["example.com/a]"] },
  { id: "regexp-star-in-host", url: "https://anything.com/a", allowList: ["*.com"] },
  { id: "regexp-plus-in-host", url: "https://aaab.com/a", allowList: ["a+b.com"] },
  { id: "regexp-group-in-host", url: "https://abab.com/a", allowList: ["(ab)+.com"] },
  { id: "regexp-braces-in-host", url: "https://aa.com/a", allowList: ["a{2}.com"] },
  { id: "regexp-braces-in-host-literal", url: "https://a{2}.com/a", allowList: ["a{2}.com"] },
  { id: "regexp-brace-literal-in-host", url: "https://a{.com/a", allowList: ["a{.com"] },
  { id: "regexp-brace-range-in-host", url: "https://aaa.com/a", allowList: ["a{2,}.com"] },
  { id: "regexp-brace-bounded-in-host", url: "https://aaaa.com/a", allowList: ["xa{1,3}.com"] },
  { id: "regexp-dollar-in-host", url: "https://a.com/a", allowList: ["a$.com"] },
  // invalid regular expressions throw, but only once reached
  { id: "regexp-unterminated-group-host", url: "https://example.com/a", allowList: ["exa(mple.com"] },
  { id: "regexp-unmatched-paren-host", url: "https://example.com/a", allowList: ["exa)mple.com"] },
  { id: "regexp-nothing-to-repeat-host", url: "https://example.com/a", allowList: ["+example.com"] },
  { id: "regexp-braces-nothing-to-repeat-host", url: "https://example.com/a", allowList: ["{2}example.com"] },
  { id: "regexp-braces-out-of-order-host", url: "https://example.com/a", allowList: ["a{3,2}.com"] },
  { id: "regexp-unmatched-paren-path", url: "https://example.com/a", allowList: ["example.com/a)b"] },
  { id: "regexp-unmatched-paren-path-host-fails", url: "https://other.com/a", allowList: ["example.com/a)b"] },
  { id: "regexp-unterminated-class-path", url: "https://example.com/a", allowList: ["example.com/a[b"] },
  { id: "regexp-range-out-of-order-path", url: "https://example.com/a", allowList: ["example.com/[z-a]"] },
  { id: "regexp-double-quantifier-path", url: "https://example.com/a", allowList: ["example.com/a**"] },
  { id: "regexp-quantified-anchor-path", url: "https://example.com/a", allowList: ["example.com/^*"] },
  { id: "regexp-invalid-after-allowed-entry", url: "https://example.com/a", allowList: ["example.com", "exa(mple.com"] },
];

const tokenCases = () => [
  { id: "hash", href: "https://excalidraw.com/#addLibrary=https%3A%2F%2Flibraries.excalidraw.com%2Fa.excalidrawlib&token=abc123" },
  { id: "hash-no-token", href: "https://excalidraw.com/#addLibrary=https%3A%2F%2Fexample.com%2Fa" },
  { id: "hash-token-first", href: "https://excalidraw.com/#token=t&addLibrary=u" },
  { id: "hash-empty-token", href: "https://excalidraw.com/#token=&addLibrary=u" },
  { id: "hash-token-without-value", href: "https://excalidraw.com/#addLibrary=u&token" },
  { id: "hash-repeated-keys-first-wins", href: "https://excalidraw.com/#addLibrary=a&addLibrary=b&token=1&token=2" },
  { id: "hash-plus-and-percent", href: "https://excalidraw.com/#addLibrary=a+b%20c%2Bd&token=x+y" },
  { id: "hash-bad-percent", href: "https://excalidraw.com/#addLibrary=%zz%4&token=%" },
  { id: "hash-invalid-utf8", href: "https://excalidraw.com/#addLibrary=%FF%C3" },
  { id: "hash-non-ascii", href: "https://excalidraw.com/#addLibrary=\u00e9\u{1f600}&token=\u00fc" },
  { id: "hash-value-with-equals", href: "https://excalidraw.com/#addLibrary=a=b=c" },
  { id: "hash-key-case", href: "https://excalidraw.com/#addlibrary=u" },
  { id: "hash-key-only", href: "https://excalidraw.com/#addLibrary" },
  { id: "hash-empty-value", href: "https://excalidraw.com/#addLibrary=&token=t" },
  { id: "hash-question-mark", href: "https://excalidraw.com/#?addLibrary=u&token=t" },
  { id: "hash-double-hash", href: "https://excalidraw.com/##addLibrary=u" },
  { id: "hash-ampersands", href: "https://excalidraw.com/#&&addLibrary=u&&token=t&" },
  { id: "hash-other-keys", href: "https://excalidraw.com/#room=abc,def&addLibrary=u" },
  { id: "hash-spaces-encoded", href: "https://excalidraw.com/#addLibrary=a b" },
  { id: "hash-empty", href: "https://excalidraw.com/#" },
  { id: "no-hash-no-query", href: "https://excalidraw.com/" },
  { id: "token-only", href: "https://excalidraw.com/#token=t" },
  { id: "query-legacy", href: "https://excalidraw.com/?addLibrary=https%3A%2F%2Fexample.com%2Fa" },
  { id: "query-legacy-token-in-query-ignored", href: "https://excalidraw.com/?addLibrary=u&token=t" },
  { id: "query-legacy-token-in-hash", href: "https://excalidraw.com/?addLibrary=u#token=t" },
  { id: "query-and-hash-hash-wins", href: "https://excalidraw.com/?addLibrary=q#addLibrary=h&token=t" },
  { id: "query-with-empty-hash-value", href: "https://excalidraw.com/?addLibrary=q#addLibrary=&token=t" },
  { id: "query-empty-value", href: "https://excalidraw.com/?addLibrary=#token=t" },
  { id: "query-plus", href: "https://excalidraw.com/?addLibrary=a+b" },
  { id: "query-other-keys", href: "https://excalidraw.com/?x=1&addLibrary=u&y=2" },
  { id: "query-empty", href: "https://excalidraw.com/?" },
  { id: "query-non-ascii", href: "https://excalidraw.com/?addLibrary=\u00e9" },
  { id: "path-not-read", href: "https://excalidraw.com/addLibrary=u" },
];

const normalizeLinkCases = () =>
  [
    "https://example.com",
    "  https://example.com/a  ",
    "",
    "   ",
    "\u00a0\ufeff\u2028",
    'https://example.com/"quoted"',
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    "  javascript:alert(1)",
    "java\tscript:alert(1)",
    "java\nscript:alert(1)",
    "ja\u0000vascript:alert(1)",
    "ja\u200bvascript:alert(1)",
    "ja\u0085vascript:alert(1)",
    "\ufeffjavascript:alert(1)",
    "&#106;avascript:alert(1)",
    "&#106avascript:alert(1)",
    "&#0x6A;avascript:alert(1)",
    "&#0X6a;avascript:alert(1)",
    "&#x6A;avascript:alert(1)",
    "&#0o152;avascript:alert(1)",
    "&#0b1101010;avascript:alert(1)",
    "&#1.06e2;avascript:alert(1)",
    "&#106e0;avascript:alert(1)",
    "&#65642;avascript:alert(1)",
    "&#00106;avascript:alert(1)",
    "&#Infinity;avascript:alert(1)",
    "&#_;x",
    "&#9;javascript:alert(1)",
    "&#55357;&#56832;",
    "https://example.com/&#55357;&#56832;",
    "https://example.com/&#123456789012345678901234567890;",
    "https://example.com/&#65601e0;",
    "https://example.com/&#0x10041;",
    "https://example.com/&#;",
    "&newline;javascript:alert(1)",
    "&NewLine;javascript:alert(1)",
    "&tab;javascript:alert(1)",
    "&colon;",
    "javascript&colon;alert(1)",
    "javascript&#58;alert(1)",
    "  &#32;javascript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "vbscript:msgbox(1)",
    "DATA:x",
    "!!javascript:alert(1)",
    "xjavascript:alert(1)",
    "_javascript:alert(1)",
    "https://example.com/?next=javascript:alert(1)",
    "x\u2029javascript:alert(1)",
    "https://example.com\u2028javascript:alert(1)",
    "a:b\u2028c:d",
    "\u2028",
    "::",
    ":javascript",
    "/relative/path",
    "./relative",
    "../relative",
    ".javascript:alert(1)",
    "/javascript:alert(1)",
    "mailto:a@example.com",
    "example.com",
    "\u00e9t\u00e9:x",
  ].map((input, i) => ({ id: `normalize-${String(i).padStart(2, "0")}`, input }));

const toValidUrlCases = () => {
  const inputs = [
    "https://example.com/a",
    "  https://example.com/a  ",
    "http://localhost:3000/x",
    "/relative/path",
    "/",
    "//evil.com/x",
    "./relative",
    "relative",
    "example.com",
    "",
    "   ",
    "javascript:alert(1)",
    "&#106;avascript:alert(1)",
    "data:text/plain,hi",
    "about:blank",
    "mailto:a@example.com",
    "web+lib:whatever",
    "https://",
    "https://exa mple.com",
    "https://example.com/\u00e9",
    "https://example.com/&#55357;&#56832;",
    'https://example.com/"q"',
    "https://example.com:99999",
    "x:",
    "\u00e9:x",
    "&#0;/x",
  ];
  const origins = [ORIGIN, "http://localhost:3000"];
  const cases = [];
  inputs.forEach((input, i) => {
    for (const origin of origins) {
      if (origin !== ORIGIN && !input.trimStart().startsWith("/")) continue;
      cases.push({ id: `to-valid-url-${String(i).padStart(2, "0")}${origin === ORIGIN ? "" : "-localhost"}`, input, origin });
    }
  });
  return cases;
};

const importCases = () => [
  { id: "encoded-library", input: "https%3A%2F%2Flibraries.excalidraw.com%2Flibraries%2Fa.excalidrawlib" },
  { id: "plain-library", input: "https://libraries.excalidraw.com/libraries/a.excalidrawlib" },
  { id: "github-library", input: `${encodeURIComponent(GH)}%2Fmain%2Flibraries%2Fa.excalidrawlib` },
  { id: "double-encoded", input: "https%253A%252F%252Fexcalidraw.com%252Fa" },
  { id: "encoded-other-host", input: "https%3A%2F%2Fevil.com%2Fa" },
  { id: "relative-path-becomes-origin", input: "%2Flibraries%2Fa.excalidrawlib" },
  { id: "relative-path-other-origin", input: "/a.excalidrawlib", origin: "https://app.example.org" },
  { id: "protocol-relative-stays-on-origin", input: "//evil.com/a" },
  { id: "javascript-blank", input: "javascript%3Aalert(1)" },
  { id: "entity-javascript-blank", input: "%26%23106%3Bavascript%3Aalert(1)" },
  { id: "not-a-url-blank", input: "excalidraw.com%2Fa" },
  { id: "empty-blank", input: "" },
  { id: "whitespace-trimmed", input: "%20%20https%3A%2F%2Fexcalidraw.com%2Fa%0A" },
  { id: "malformed-percent", input: "https%3A%2F%2Fexcalidraw.com%2F%" },
  { id: "malformed-percent-digits", input: "https://excalidraw.com/%zz" },
  { id: "malformed-utf8-lone-continuation", input: "https://excalidraw.com/%80" },
  { id: "malformed-utf8-truncated", input: "https://excalidraw.com/%E2%82" },
  { id: "malformed-utf8-overlong", input: "https://excalidraw.com/%C0%AF" },
  { id: "malformed-utf8-surrogate", input: "https://excalidraw.com/%ED%A0%80" },
  { id: "malformed-utf8-too-large", input: "https://excalidraw.com/%F4%90%80%80" },
  { id: "malformed-utf8-five-bytes", input: "https://excalidraw.com/%F8%88%80%80%80" },
  { id: "utf8-decoded", input: "https://excalidraw.com/%C3%A9%F0%9F%98%80%E2%82%AC%7F" },
  { id: "utf8-max", input: "https://excalidraw.com/%F4%8F%BF%BF" },
  { id: "reserved-decoded", input: "https://excalidraw.com%2Fa%3Fb%3Dc%23d" },
  { id: "decoded-host-trick", input: "https://evil.com%23.excalidraw.com/a" },
  { id: "decoded-credentials-trick", input: "https://excalidraw.com%40evil.com/a" },
  { id: "lower-case-hex", input: "https%3a%2f%2fexcalidraw.com%2fa" },
];

// -- running upstream -----------------------------------------------------------

/** Runs fn with window.location and location at `href`. */
const at = (href, fn) => {
  const location = new URL(href);
  const saved = { window: globalThis.window, location: globalThis.location };
  globalThis.window = { ...(globalThis.window ?? {}), location };
  globalThis.location = location;
  try {
    return fn();
  } finally {
    globalThis.window = saved.window;
    globalThis.location = saved.location;
  }
};

const outcome = (fn) => {
  try {
    return { value: fn() };
  } catch (error) {
    return { error: error.message, errorType: error.constructor.name };
  }
};

const runValidate = (up, c) => {
  const r = outcome(() =>
    c.allowList === undefined ? up.validateLibraryUrl(c.url) : up.validateLibraryUrl(c.url, c.allowList),
  );
  const out = { id: c.id, url: c.url };
  if (c.allowList !== undefined) out.allowList = c.allowList;
  if (r.error === undefined) {
    if (r.value !== true) throw new Error(`${c.id}: validateLibraryUrl returned ${r.value}`);
    out.ok = true;
  } else {
    out.error = r.error;
    out.errorType = r.errorType;
  }
  return out;
};

const runTokens = (up, c) => ({
  id: c.id,
  href: c.href,
  search: new URL(c.href).search,
  hash: new URL(c.href).hash,
  result: at(c.href, () => up.parseLibraryTokensFromUrl()),
});

const runNormalize = (up, c) => ({ id: c.id, input: c.input, output: up.normalizeLink(c.input) });

const runToValidUrl = (up, c) => ({
  id: c.id,
  input: c.input,
  origin: c.origin,
  output: at(`${c.origin}/page`, () => up.toValidURL(c.input)),
});

const runImport = (up, c) => {
  const origin = c.origin ?? ORIGIN;
  const r = outcome(() =>
    at(`${origin}/`, () => {
      // library.ts:726-731
      let libraryUrl = c.input;
      libraryUrl = decodeURIComponent(libraryUrl);
      libraryUrl = up.toValidURL(libraryUrl);
      up.validateLibraryUrl(libraryUrl);
      return libraryUrl;
    }),
  );
  const out = { id: c.id, input: c.input, origin };
  if (r.error === undefined) out.url = r.value;
  else {
    out.error = r.error;
    out.errorType = r.errorType;
  }
  return out;
};

const unique = (cases) => {
  const ids = new Set();
  for (const c of cases) {
    if (ids.has(c.id)) throw new Error(`duplicate case id ${c.id}`);
    ids.add(c.id);
  }
  return cases;
};

const buildFixture = (up, commit) => {
  const fixture = {
    description:
      "validateLibraryUrl and parseLibraryTokensFromUrl (packages/excalidraw/data/library.ts:54-58, 497-543), " +
      "normalizeLink and toValidURL (packages/common/src/url.ts:5-37, @braintree/sanitize-url 6.0.2), " +
      "and the first steps of importLibraryFromURL (library.ts:726-731). " +
      "Generated by tools/goldens/library-url-fixtures.mjs.",
    upstream: commit,
    // module-private in library.ts, exported for the generator (`expose`)
    allowedLibraryUrls: up.ALLOWED_LIBRARY_URLS,
    validate: unique(validateCases()).map((c) => runValidate(up, c)),
    tokens: unique(tokenCases()).map((c) => runTokens(up, c)),
    normalizeLink: unique(normalizeLinkCases()).map((c) => runNormalize(up, c)),
    toValidURL: unique(toValidUrlCases()).map((c) => runToValidUrl(up, c)),
    import: unique(importCases()).map((c) => runImport(up, c)),
  };
  // Every non-ASCII code unit as a \u escape: the same JSON value, and the
  // file stays free of invisible code points the attribution gate rejects.
  const text = JSON.stringify(fixture, null, 2).replace(
    /[\u0080-\uffff]/g,
    (ch) => `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `${text}\n`;
};

const deterministic = (fn) => {
  const random = Math.random;
  Math.random = () => {
    throw new Error("Math.random called while generating library URL fixtures");
  };
  try {
    return fn();
  } finally {
    Math.random = random;
  }
};

const main = async () => {
  const args = parseArgs(process.argv.slice(2));
  let upstream;
  try {
    upstream = verifyUpstream();
  } catch (error) {
    process.stderr.write(`library-url-fixtures: ${error.message}\n`);
    process.exit(1);
  }
  globalThis.devicePixelRatio = 1;
  globalThis.window ??= {};
  const up = await loadUpstream(upstream, {
    entry: ENTRY,
    stubs: STUBS,
    shims: SHIMS,
    expose: { "packages/excalidraw/data/library": ["ALLOWED_LIBRARY_URLS"] },
    define: { "import.meta.env.MODE": '"test"' },
  });
  const text = deterministic(() => buildFixture(up, upstream.commit));
  const path = join(args.out, FIXTURE);
  const where = relative(process.cwd(), path) || path;

  if (args.check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== text) {
      process.stderr.write(`stale: ${where}\n`);
      process.stderr.write("library URL fixtures are out of date: run node tools/goldens/library-url-fixtures.mjs\n");
      process.exit(1);
    }
    process.stdout.write(`library URL fixtures up to date: ${where}\n`);
    return;
  }

  mkdirSync(args.out, { recursive: true });
  writeFileSync(path, text);
  process.stdout.write(`wrote ${where} from upstream ${upstream.commit.slice(0, 7)}\n`);
};

await main();
