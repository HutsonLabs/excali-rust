// Minimal static file server for the built site, behaving like GitHub Pages
// where the smoke suite can observe it: directory URLs serve index.html, a
// directory without a trailing slash redirects, unknown paths get 404.html
// with status 404.
//
//   node lib/serve.mjs [--build] [--port N] [--root DIR]
//
// --build first runs scripts/site/zola.sh build with base_url set to this
// server, into tests/site/.site (git-ignored), and serves that.
import { createServer } from "node:http";
import { spawnSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
  ".xml": "application/xml",
  ".txt": "text/plain; charset=utf-8",
  ".wasm": "application/wasm",
};

function stat(p) {
  try {
    return statSync(p);
  } catch {
    return null;
  }
}

export function createStaticServer(rootDir) {
  const root = resolve(rootDir);
  const send = (res, status, file, headers = {}) => {
    const body = readFileSync(file);
    res.writeHead(status, {
      "content-type": TYPES[extname(file).toLowerCase()] || "application/octet-stream",
      "content-length": body.length,
      "cache-control": "no-store",
      ...headers,
    });
    res.end(body);
  };
  const notFound = (res) => {
    const page = join(root, "404.html");
    if (stat(page)?.isFile()) return send(res, 404, page);
    res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    res.end("not found");
  };

  return createServer((req, res) => {
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    } catch {
      return notFound(res);
    }
    const target = resolve(root, `.${pathname}`);
    if (target !== root && !target.startsWith(root + sep)) return notFound(res);
    const st = stat(target);
    if (st?.isDirectory()) {
      if (!pathname.endsWith("/")) {
        res.writeHead(301, { location: `${pathname}/` });
        return res.end();
      }
      const index = join(target, "index.html");
      return stat(index)?.isFile() ? send(res, 200, index) : notFound(res);
    }
    if (st?.isFile()) return send(res, 200, target);
    return notFound(res);
  });
}

function parseArgs(argv) {
  const args = { build: false, port: 4173, root: join(HERE, "..", ".site") };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--build") args.build = true;
    else if (a === "--port") args.port = Number(argv[++i]);
    else if (a === "--root") args.root = resolve(argv[++i]);
    else throw new Error(`unknown argument ${a}`);
  }
  return args;
}

function build(outDir, baseUrl) {
  const zola = join(HERE, "..", "..", "..", "scripts", "site", "zola.sh");
  const r = spawnSync(zola, ["build", "--base-url", baseUrl, "--output-dir", outDir, "--force"], {
    stdio: "inherit",
  });
  if (r.status !== 0) {
    console.error(`zola build failed (exit ${r.status ?? r.signal})`);
    process.exit(1);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = parseArgs(process.argv.slice(2));
  const baseUrl = `http://127.0.0.1:${args.port}`;
  if (args.build) build(args.root, baseUrl);
  createStaticServer(args.root).listen(args.port, "127.0.0.1", () => {
    console.log(`serving ${args.root} at ${baseUrl}/`);
  });
}
