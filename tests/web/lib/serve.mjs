// Static server for the web runtime suite (ex-307): serves the build of
// scripts/web/build.sh (excali_editor.js, excali_editor_bg.wasm, fonts/)
// and the test page in tests/web/page at "/". Every response is no-store,
// so each test's font requests reach the network log.
//
//   node lib/serve.mjs [--port N] [--root DIR]     default root: dist
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

export function createWebServer(rootDir) {
  const root = resolve(rootDir);
  return createServer((req, res) => {
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    } catch {
      res.writeHead(400).end();
      return;
    }
    const base = pathname === "/" || pathname === "/index.html" ? PAGE_DIR : root;
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
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port") port = Number(args[++i]);
    else if (args[i] === "--root") root = resolve(args[++i]);
    else {
      process.stderr.write("usage: serve.mjs [--port N] [--root DIR]\n");
      process.exit(2);
    }
  }
  if (!isFile(join(root, "excali_editor.js"))) {
    process.stderr.write(`serve: ${root}/excali_editor.js missing; run scripts/web/build.sh\n`);
    process.exit(1);
  }
  createWebServer(root).listen(port, "127.0.0.1", () => {
    process.stdout.write(`serving ${root} on http://127.0.0.1:${port}/\n`);
  });
}
