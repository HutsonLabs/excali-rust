"""Tests for scripts/site/snippets.py (ex-607): every code block on the
integration guide is a copy of a file the example hosts run."""
import importlib.util
import json
import pathlib
import re
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
_spec = importlib.util.spec_from_file_location("snippets", HERE / "snippets.py")
snippets = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(snippets)
_spec = importlib.util.spec_from_file_location(
    "wasm_size", ROOT / "scripts" / "gates" / "wasm_size.py"
)
wasm_size = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(wasm_size)

GUIDE = ROOT / "site" / "content" / "architecture" / "integration.md"


def page(text):
    return textwrap.dedent(text).lstrip("\n")


class Fixture(unittest.TestCase):
    """A temporary repository root with a few source files."""

    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self._dir.name)
        (self.root / "app").mkdir()
        (self.root / "app" / "conf.json").write_text(
            json.dumps(
                {
                    "app": {
                        "security": {
                            "csp": {
                                "default-src": "'self'",
                                "script-src": "'self' 'wasm-unsafe-eval'",
                                "connect-src": "'self' ipc: http://ipc.localhost",
                            }
                        }
                    },
                    "permissions": ["a:default"],
                },
                indent=2,
            )
        )
        (self.root / "app" / "main.js").write_text(
            page(
                """
                import init from "./x.js";

                const { invoke } = window.__TAURI__.core;
                try {
                  await init();
                } catch (e) {
                  throw e;
                }
                editor.addEventListener("library-fetch", (e) => {
                  e.preventDefault();
                });
                """
            )
        )

    def tearDown(self):
        self._dir.cleanup()

    def check(self, text):
        return snippets.check_text(page(text), self.root)


class Blocks(Fixture):
    def test_reads_the_marker_language_and_code_of_each_fence(self):
        blocks = snippets.blocks(
            page(
                """
                Intro.

                <!-- snippet: app/main.js -->
                ```js
                await init();
                ```

                ```sh
                ls
                ```
                """
            )
        )
        self.assertEqual(
            [(b.line, b.marker, b.lang, b.code) for b in blocks],
            [(4, "app/main.js", "js", "await init();\n"), (8, None, "sh", "ls\n")],
        )

    def test_a_block_without_a_marker_is_unverified(self):
        errors = self.check(
            """
            ```js
            await init();
            ```
            """
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("line 1", errors[0])
        self.assertIn("no snippet marker", errors[0])

    def test_a_marker_not_followed_by_a_fence_is_an_error(self):
        errors = self.check("<!-- snippet: app/main.js -->\nText.\n")
        self.assertEqual(len(errors), 1)
        self.assertIn("not followed by a code block", errors[0])

    def test_a_missing_file_is_an_error(self):
        errors = self.check(
            """
            <!-- snippet: app/nope.js -->
            ```js
            x
            ```
            """
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("app/nope.js", errors[0])


class Excerpts(Fixture):
    def test_contiguous_lines_match_whatever_their_indentation(self):
        self.assertEqual(
            self.check(
                """
                <!-- snippet: app/main.js -->
                ```js
                editor.addEventListener("library-fetch", (e) => {
                    e.preventDefault();
                });
                ```
                """
            ),
            [],
        )

    def test_blank_lines_are_ignored(self):
        self.assertEqual(
            self.check(
                """
                <!-- snippet: app/main.js -->
                ```js
                import init from "./x.js";
                const { invoke } = window.__TAURI__.core;
                ```
                """
            ),
            [],
        )

    def test_an_edited_line_fails(self):
        errors = self.check(
            """
            <!-- snippet: app/main.js -->
            ```js
            editor.addEventListener("library-fetch", (e) => {
              e.stopPropagation();
            });
            ```
            """
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("app/main.js", errors[0])
        self.assertIn("e.stopPropagation();", errors[0])

    def test_an_elision_line_splits_chunks_that_must_come_in_order(self):
        ok = """
            <!-- snippet: app/main.js -->
            ```js
            import init from "./x.js";
            // …
            await init();
            ```
            """
        self.assertEqual(self.check(ok), [])
        reversed_ = """
            <!-- snippet: app/main.js -->
            ```js
            await init();
            // …
            import init from "./x.js";
            ```
            """
        self.assertEqual(len(self.check(reversed_)), 1)

    def test_lines_that_are_not_adjacent_in_the_file_fail_without_an_elision(self):
        errors = self.check(
            """
            <!-- snippet: app/main.js -->
            ```js
            import init from "./x.js";
            await init();
            ```
            """
        )
        self.assertEqual(len(errors), 1)


class JsonMembers(Fixture):
    def test_a_member_matches_the_value_at_the_pointer(self):
        self.assertEqual(
            self.check(
                """
                <!-- snippet: app/conf.json#/app/security -->
                ```json
                "security": {
                  "csp": {
                    "connect-src": "'self' ipc: http://ipc.localhost",
                    "script-src": "'self' 'wasm-unsafe-eval'",
                    "default-src": "'self'"
                  }
                }
                ```
                """
            ),
            [],
        )

    def test_a_different_value_fails(self):
        errors = self.check(
            """
            <!-- snippet: app/conf.json#/app/security/csp/script-src -->
            ```json
            "script-src": "'self'"
            ```
            """
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("/app/security/csp/script-src", errors[0])

    def test_a_whole_file(self):
        (self.root / "app" / "cap.json").write_text('{"a": [1, 2]}\n')
        good = '<!-- snippet: app/cap.json -->\n```json\n{ "a": [1, 2] }\n```\n'
        bad = '<!-- snippet: app/cap.json -->\n```json\n{ "a": [2, 1] }\n```\n'
        self.assertEqual(snippets.check_text(good, self.root), [])
        self.assertEqual(len(snippets.check_text(bad, self.root)), 1)

    def test_a_missing_pointer_fails(self):
        errors = self.check(
            """
            <!-- snippet: app/conf.json#/app/nope -->
            ```json
            "nope": 1
            ```
            """
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("/app/nope", errors[0])


class WebCsp(Fixture):
    def test_the_browser_header_is_the_tauri_csp_without_the_ipc_sources(self):
        csp = {
            "default-src": "'self'",
            "script-src": "'self' 'wasm-unsafe-eval'",
            "connect-src": "'self' ipc: http://ipc.localhost",
        }
        self.assertEqual(
            snippets.web_csp(csp),
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'",
        )

    def test_a_web_csp_block_is_checked_against_the_file_it_names(self):
        good = """
            <!-- snippet: web-csp app/conf.json#/app/security/csp -->
            ```text
            Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'
            ```
            """
        self.assertEqual(self.check(good), [])
        bad = """
            <!-- snippet: web-csp app/conf.json#/app/security/csp -->
            ```text
            Content-Security-Policy: default-src 'self'; script-src 'self'; connect-src 'self'
            ```
            """
        errors = self.check(bad)
        self.assertEqual(len(errors), 1)
        self.assertIn("wasm-unsafe-eval", errors[0])


class TheGuide(unittest.TestCase):
    """The published page itself."""

    def test_every_block_on_the_guide_matches_the_example_hosts(self):
        self.assertEqual(snippets.check_page(GUIDE, ROOT), [])

    def test_the_guide_covers_what_the_task_asks_for(self):
        markers = {b.marker for b in snippets.blocks(GUIDE.read_text())}
        for want in [
            # CSP: Tauri and a plain web server
            "examples/tauri-app/src-tauri/tauri.conf.json#/app/security",
            "web-csp examples/tauri-app/src-tauri/tauri.conf.json#/app/security/csp",
            # capabilities
            "examples/tauri-app/src-tauri/capabilities/excali.json",
            "crates/tauri-plugin-excali/capabilities/excali.json",
            # module loading without a bundler
            "examples/tauri-app/ui/index.html",
            "examples/tauri-app/ui/app.js",
            "tests/web/page/csp.html",
            "tests/web/page/csp-app.js",
            # the Rust side
            "examples/tauri-app/src-tauri/src/lib.rs",
            "examples/tauri-app/src-tauri/Cargo.toml",
        ]:
            self.assertIn(want, markers)

    def test_the_browser_page_is_served_with_the_guides_header(self):
        # tests/web/specs/csp.spec.mjs reads the header from the guide.
        spec = (ROOT / "tests" / "web" / "specs" / "csp.spec.mjs").read_text()
        self.assertIn("site/content/architecture/integration.md", spec)
        self.assertIn("web-csp", spec)

    # Divergences found walking the guide in a fresh clone (ex-803,
    # site/content/architecture/integration-walk.md).

    def test_every_element_a_script_looks_up_is_in_the_hosts_html(self):
        # Each host's script finds its container by id; a reader copying the
        # blocks of one host must get that element from its HTML block.
        hosts = {}
        for b in snippets.blocks(GUIDE.read_text()):
            if b.lang not in ("html", "js"):
                continue
            host = pathlib.PurePosixPath(b.marker.split("#")[0]).parent
            hosts.setdefault(host, {"html": "", "js": ""})[b.lang] += b.code
        self.assertEqual(len(hosts), 2)
        for host, code in hosts.items():
            ids = set(re.findall(r'getElementById\("([^"]+)"\)', code["js"]))
            self.assertTrue(ids, host)
            for id_ in ids:
                self.assertIn(f'id="{id_}"', code["html"], f"{host}: #{id_}")

    def test_the_wasm_size_quoted_is_the_budget_the_build_enforces(self):
        text = GUIDE.read_text()
        m = re.search(r"`excali_editor_bg\.wasm` \| The module \(at most ([^)]*?) gzipped", text)
        self.assertIsNotNone(m, "the guide's wasm row does not quote the budget")
        self.assertEqual(
            wasm_size.parse_size(m.group(1)), wasm_size.budgets()["excali_editor_bg.wasm"]
        )

    def test_a_host_outside_this_repository_bundles_the_runtimes_fonts(self):
        # The example's resource path exists only inside this repository;
        # the guide gives the path of the runtime's own fonts/ as well.
        text = GUIDE.read_text()
        self.assertTrue('"../ui/excali/fonts/": "fonts/"' in text, "no resource path for fonts/")
        build = (ROOT / "scripts" / "web" / "build.sh").read_text()
        self.assertIn('cp -R "$root/crates/excali-text/assets/fonts" "$out/fonts"', build)

    def test_a_cross_origin_runtime_needs_cors(self):
        text = GUIDE.read_text()
        self.assertTrue("Access-Control-Allow-Origin" in text, "no CORS for another origin")
        spec = (ROOT / "tests" / "web" / "specs" / "csp.spec.mjs").read_text()
        self.assertIn("Access-Control-Allow-Origin", spec)

    def test_the_walk_is_linked_from_the_guide(self):
        walk = ROOT / "site" / "content" / "architecture" / "integration-walk.md"
        self.assertTrue(walk.is_file())
        self.assertTrue("(../integration-walk/)" in GUIDE.read_text(), "no link to the walk")


if __name__ == "__main__":
    unittest.main()
