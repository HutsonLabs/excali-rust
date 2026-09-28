#!/usr/bin/env python3
"""Post-deploy smoke check for the GitHub Pages site (ex-005).

Fetches the home page at base_url from site/config.toml and requires HTTP 200
with a <title> equal to the configured site title. Retries for a while because
Pages can take a short time to serve a fresh deployment.

usage: smoke.py [--config site/config.toml] [--url URL] [--attempts N] [--delay S]
"""
import argparse
import pathlib
import re
import sys
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
TITLE_RE = re.compile(r"<title[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)


KEY_RE = re.compile(r'^(\w+)\s*=\s*"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$')


def site_config(path):
    """Top-level string keys of a Zola config (before the first [table]).

    Parsed by hand so the script runs on any python3 (tomllib is 3.11+).
    """
    cfg = {}
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("["):
            break
        m = KEY_RE.match(line)
        if m:
            cfg[m.group(1)] = m.group(2)
    return cfg


def home_url(base_url):
    return base_url if base_url.endswith("/") else base_url + "/"


def check_home(status, html, title):
    """Return a list of problems (empty means the home page is being served)."""
    if status != 200:
        return [f"expected HTTP 200, got {status}"]
    m = TITLE_RE.search(html)
    if not m:
        return ["no <title> in response"]
    got = m.group(1).strip()
    if got != title:
        return [f"title {got!r} != {title!r}"]
    return []


def fetch(url):
    req = urllib.request.Request(url, headers={"Cache-Control": "no-cache"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, ""
    except urllib.error.URLError as e:
        return f"unreachable ({e.reason})", ""


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--config", default=str(ROOT / "site" / "config.toml"))
    ap.add_argument("--url", help="override base_url from the config")
    ap.add_argument("--attempts", type=int, default=10)
    ap.add_argument("--delay", type=float, default=15.0)
    args = ap.parse_args(argv)

    cfg = site_config(args.config)
    url = home_url(args.url or cfg["base_url"])
    for attempt in range(1, args.attempts + 1):
        status, html = fetch(url)
        problems = check_home(status, html, cfg["title"])
        if not problems:
            print(f"pages smoke OK: {url} serves {cfg['title']!r}")
            return 0
        print(f"attempt {attempt}/{args.attempts}: {url}: {'; '.join(problems)}", file=sys.stderr)
        if attempt < args.attempts:
            time.sleep(args.delay)
    print(f"pages smoke FAILED: {url}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
