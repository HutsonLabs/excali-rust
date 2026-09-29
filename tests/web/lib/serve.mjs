// Static server for the web runtime suite (ex-307): serves the build of
// scripts/web/build.sh (excali_editor.js, excali_editor_bg.wasm, fonts/)
// and the test page in tests/web/page at "/". Every response is no-store,
// so each test's font requests reach the network log. The Canvas 2D
// fixture suite (ex-502, playwright.canvas2d.config.mjs) and the layered
// canvases suite (ex-503, playwright.layers.config.mjs) serve their wasm
// harnesses with their own pages the same way.
//
//   node lib/serve.mjs [--port N] [--root DIR] [--page DIR] [--expect FILE]
//
// Defaults: root dist, page tests/web/page, expect excali_editor.js (the
// file under root without which the server refuses to start); a relative
// --page is taken from tests/web.
import { createServer } from "node:http";
import { readFileSync, statSync } from "node:fs";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = resolve(HERE, "..", "..", "..");
export const PAGE_DIR = join(HERE, "..", "page");

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".wasm": "application/wasm",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
  ".txt": "text/plain; charset=utf-8",
  ".md": "text/plain; charset=utf-8",
};

const isFile = (p) => {
  try {
    return statSync(p).isFile();
  } catch {
    return false;
  }
};

export function createWebServer(rootDir, pageDir = PAGE_DIR) {
  const root = resolve(rootDir);
  const page = resolve(pageDir);
  return createServer((req, res) => {
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    } catch {
      res.writeHead(400).end();
      return;
    }
    const base = pathname === "/" || pathname === "/index.html" ? page : root;
    const target = resolve(base, `.${pathname === "/" ? "/index.html" : pathname}`);
    if (!target.startsWith(base + sep) || !isFile(target)) {
      res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
      res.end("not found");
      return;
    }
    const body = readFileSync(target);
    res.writeHead(200, {
      "content-type": TYPES[extname(target).toLowerCase()] || "application/octet-stream",
      "content-length": body.length,
      "cache-control": "no-store",
    });
    res.end(body);
  });
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  let port = 4174;
  let root = join(REPO_ROOT, "dist");
  let page = PAGE_DIR;
  let expected = "excali_editor.js";
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port") port = Number(args[++i]);
    else if (args[i] === "--root") root = resolve(args[++i]);
    else if (args[i] === "--page") page = resolve(HERE, "..", args[++i]);
    else if (args[i] === "--expect") expected = args[++i];
    else {
      process.stderr.write("usage: serve.mjs [--port N] [--root DIR] [--page DIR] [--expect FILE]\n");
      process.exit(2);
    }
  }
  if (!isFile(join(root, expected))) {
    const build =
      {
        "excali_editor.js": "scripts/web/build.sh",
        "canvas_layers.js": "scripts/web/canvas-layers.sh",
      }[expected] ?? "scripts/web/canvas2d-fixtures.sh";
    process.stderr.write(`serve: ${root}/${expected} missing; run ${build}\n`);
    process.exit(1);
  }
  createWebServer(root, page).listen(port, "127.0.0.1", () => {
    process.stdout.write(`serving ${root} on http://127.0.0.1:${port}/\n`);
  });
}
