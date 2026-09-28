"""Tests for scripts/site/smoke.py (post-deploy check of the Pages site)."""
import importlib.util
import pathlib
import unittest

HERE = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("smoke", HERE / "smoke.py")
smoke = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(smoke)


class SiteConfig(unittest.TestCase):
    def test_reads_base_url_and_title_from_repo_config(self):
        cfg = smoke.site_config(HERE.parent.parent / "site" / "config.toml")
        self.assertEqual(cfg["base_url"], "https://hutsonlabs.github.io/excali-rust")
        self.assertEqual(cfg["title"], "excali-rust")

    def test_home_url_has_trailing_slash(self):
        self.assertEqual(
            smoke.home_url("https://hutsonlabs.github.io/excali-rust"),
            "https://hutsonlabs.github.io/excali-rust/",
        )
        self.assertEqual(smoke.home_url("https://x.test/a/"), "https://x.test/a/")


class CheckHome(unittest.TestCase):
    def test_accepts_200_with_matching_title(self):
        self.assertEqual(
            smoke.check_home(200, "<html><head><title>excali-rust</title></head>", "excali-rust"),
            [],
        )

    def test_rejects_non_200(self):
        problems = smoke.check_home(404, "<title>excali-rust</title>", "excali-rust")
        self.assertEqual(len(problems), 1)
        self.assertIn("404", problems[0])

    def test_rejects_missing_or_wrong_title(self):
        self.assertTrue(smoke.check_home(200, "<html></html>", "excali-rust"))
        self.assertTrue(smoke.check_home(200, "<title>Site not found</title>", "excali-rust"))

    def test_title_match_tolerates_whitespace_and_case_of_tag(self):
        self.assertEqual(smoke.check_home(200, "<TITLE>\n excali-rust \n</TITLE>", "excali-rust"), [])


if __name__ == "__main__":
    unittest.main()
