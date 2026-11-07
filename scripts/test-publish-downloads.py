#!/usr/bin/env python3
"""Regression checks for public download selection and publication."""

import importlib.util
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("publisher", Path(__file__).with_name("publish-downloads.py"))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)
REPOSITORY = "example/LocalWEB"
SHA = "a" * 40


class DownloadsTest(unittest.TestCase):
    def run_fixture(self, **changes):
        return dict(id=10, event="push", head_branch="main", status="completed", conclusion="success",
                    path=".github/workflows/native-builds.yml", head_sha=SHA,
                    head_repository={"full_name": REPOSITORY}) | changes

    def test_only_trusted_successful_main_runs_are_selected(self):
        good = self.run_fixture()
        for changes in [dict(event="pull_request"), dict(head_branch="feature"),
                        dict(status="in_progress"), dict(conclusion="failure"),
                        dict(head_repository={"full_name": "fork/LocalWEB"}),
                        dict(head_repository=None), dict(head_sha="bad"),
                        dict(path="unrelated.yml"), dict(id=True)]:
            with self.subTest(changes=changes):
                self.assertEqual(publisher.select_run([self.run_fixture(id=99) | changes, good], REPOSITORY), good)
                with self.assertRaises(ValueError):
                    publisher.select_run([self.run_fixture() | changes], REPOSITORY)
        latest = self.run_fixture(id=11)
        self.assertEqual(publisher.select_run([latest, good], REPOSITORY), latest)

    def test_bootstrap_retains_downloads_until_a_verified_build_exists(self):
        output = io.StringIO()
        with patch.object(publisher, "gh", return_value=json.dumps({"workflow_runs": []})) as gh, \
             patch.dict(publisher.os.environ, {"GITHUB_REPOSITORY": REPOSITORY}), \
             patch("sys.argv", ["publish-downloads.py", "select"]), contextlib.redirect_stdout(output):
            publisher.main()
        self.assertEqual(gh.call_count, 1)
        self.assertIn("Downloads unchanged", output.getvalue())

    def checks_fixture(self):
        return [self.run_fixture(), self.run_fixture(id=11, path=publisher.REQUIRED_CHECKS[1])]

    def test_every_required_workflow_must_pass_for_the_same_commit(self):
        runs = self.checks_fixture()
        self.assertTrue(publisher.release_checks_ready(runs, REPOSITORY, SHA, 10))
        for changes in [dict(conclusion="failure"), dict(conclusion="cancelled"), dict(conclusion="timed_out")]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                publisher.release_checks_ready([runs[0], runs[1] | changes], REPOSITORY, SHA, 10)
        for changes in [dict(status="in_progress", conclusion=None), dict(head_sha="b"*40),
                        dict(head_repository={"full_name": "fork/LocalWEB"})]:
            with self.subTest(changes=changes):
                self.assertFalse(publisher.release_checks_ready([runs[0], runs[1] | changes], REPOSITORY, SHA, 10))
        self.assertFalse(publisher.release_checks_ready(runs[:1], REPOSITORY, SHA, 10))

    def test_only_the_current_native_publication_can_be_in_progress(self):
        native, mac = self.checks_fixture()
        native |= dict(status="in_progress", conclusion=None)
        self.assertTrue(publisher.release_checks_ready([native, mac], REPOSITORY, SHA, 10, True))
        self.assertFalse(publisher.release_checks_ready([native, mac], REPOSITORY, SHA, 10, False))
        with self.assertRaises(ValueError):
            publisher.release_checks_ready([native | dict(id=12), mac], REPOSITORY, SHA, 10, True)

    def make_installers(self, root):
        names = ["localweb_0.1.3_amd64.deb", "localweb-0.1.3-1-x86_64.pkg.tar.zst",
                 "LocalWeb-0.1.3.pkg", "localweb-0.1.3-windows-x86_64.zip"]
        for index, name in enumerate(names):
            folder = root / str(index)
            folder.mkdir(parents=True)
            (folder / name).write_bytes(f"installer-{index}".encode())
        return list(root.rglob("*.*"))

    def test_fixed_urls_have_matching_checksums_and_build_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.make_installers(root / "input")
            notes = publisher.prepare(root / "input", root / "output", REPOSITORY, SHA, 123)
            self.assertEqual({p.name for p in (root / "output").iterdir()}, publisher.CURRENT_ASSETS)
            for line in (root / "output/SHA256SUMS").read_text().splitlines():
                checksum, name = line.split("  ")
                self.assertEqual(checksum, publisher.digest(root / "output" / name))
                self.assertIn(f"/rolling/{name}", notes)
            self.assertIn(SHA, notes)
            self.assertIn("/actions/runs/123", notes)

    def test_incomplete_duplicate_empty_and_mixed_installers_are_rejected(self):
        for mode in ("missing", "duplicate", "empty", "mixed"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                files = self.make_installers(root / "input")
                first = files[0]
                if mode == "missing":
                    first.unlink()
                elif mode == "duplicate":
                    (first.parent / ("duplicate")).mkdir()
                    (first.parent / "duplicate" / first.name).write_bytes(b"duplicate")
                elif mode == "empty":
                    first.write_bytes(b"")
                else:
                    first.rename(first.with_name(first.name.replace("0.1.3", "0.1.4")))
                with self.assertRaises(ValueError):
                    publisher.prepare(root / "input", root / "output", REPOSITORY, SHA, 123)
                self.assertFalse((root / "output").exists())

    def test_cleanup_preserves_current_and_unrelated_assets(self):
        for name in publisher.CURRENT_ASSETS | {"notes.txt", "another-app.zip"}:
            self.assertFalse(publisher.obsolete_asset(name), name)
        for name in ["localweb_0.1.2_amd64.deb", "LocalWeb-0.1.2.pkg",
                     "localweb-0.1.2-1-x86_64.pkg.tar.zst", "localweb-0.1.2-windows-x86_64.zip"]:
            self.assertTrue(publisher.obsolete_asset(name), name)

    def test_failed_upload_does_not_clean_existing_assets(self):
        with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), \
             patch.object(publisher, "gh", side_effect=RuntimeError("upload failed")) as gh:
            with self.assertRaises(RuntimeError):
                publisher.publish(Path("output"), Path("notes.md"), REPOSITORY, SHA)
            self.assertEqual(gh.call_count, 1)
            self.assertEqual(gh.call_args.args[:3], ("release", "upload", "rolling"))

    def test_publication_keeps_aligned_source_and_cleans_only_old_installers(self):
        calls = []
        def gh(*args):
            calls.append(args)
            if args[:2] == ("api", f"repos/{REPOSITORY}/git/ref/tags/rolling"):
                return json.dumps({"object": {"sha": SHA}})
            if args[:3] == ("release", "view", "rolling"):
                return json.dumps({"assets": [{"name": name} for name in
                    ["localweb_0.1.2_amd64.deb", "notes.txt", *publisher.CURRENT_ASSETS]]})
            return ""
        with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), \
             patch.object(publisher, "gh", side_effect=gh):
            publisher.publish(Path("output"), Path("notes.md"), REPOSITORY, SHA)
        deletes = [args for args in calls if args[:2] == ("release", "delete-asset")]
        self.assertEqual(deletes, [("release", "delete-asset", "rolling", "localweb_0.1.2_amd64.deb", "--yes")])
        self.assertEqual(calls[0][:2], ("release", "upload"))
        self.assertFalse(any("PATCH" in args for args in calls))


if __name__ == "__main__":
    unittest.main()
