#!/usr/bin/env python3
"""Regression checks for public download selection and publication."""

import importlib.util
import contextlib
import io
import json
import hashlib
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

    def publication_fixture(self, root):
        self.make_installers(root / "input")
        notes = publisher.prepare(root / "input", root / "output", REPOSITORY, SHA, 123)
        (root / "notes.md").write_text(notes)
        return FakeRelease(), root / "output", root / "notes.md"

    def test_promotion_preserves_previous_bundle_and_unrelated_assets(self):
        with tempfile.TemporaryDirectory() as temp:
            remote, output, notes = self.publication_fixture(Path(temp))
            with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh):
                publisher.publish(output, notes, REPOSITORY, SHA)
            remote.assert_bundle(self, output, SHA, notes.read_text())
            self.assertNotIn(publisher.JOURNAL, remote.by_name())
            self.assertEqual(sum(name.startswith("previous-") for name in remote.by_name()), 5)
            self.assertIn("notes.txt", remote.by_name())

    def test_failed_staging_never_changes_public_downloads(self):
        with tempfile.TemporaryDirectory() as temp:
            remote, output, notes = self.publication_fixture(Path(temp))
            remote.fail = ("upload", 3, "after")
            with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh), self.assertRaises(RuntimeError):
                publisher.publish(output, notes, REPOSITORY, SHA)
            remote.assert_previous(self)
            self.assertNotIn(publisher.JOURNAL, remote.by_name())

    def test_every_partial_rename_and_source_failure_is_recovered(self):
        for event, count, timing in [("rename", n, when) for n in range(1, 11) for when in ("before", "after")] + [("source", 1, when) for when in ("before", "after")] + [("notes", 1, "before")]:
            with self.subTest(event=event, count=count, timing=timing), tempfile.TemporaryDirectory() as temp:
                remote, output, notes = self.publication_fixture(Path(temp))
                remote.fail = (event, count, timing)
                with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh), self.assertRaises(RuntimeError):
                    publisher.publish(output, notes, REPOSITORY, SHA)
                remote.assert_previous(self)
                self.assertNotIn(publisher.JOURNAL, remote.by_name())

    def test_durable_journal_recovers_after_a_runner_is_lost(self):
        with tempfile.TemporaryDirectory() as temp:
            remote, output, notes = self.publication_fixture(Path(temp))
            recover = publisher.recover_publication
            def interrupted_recovery(*args):
                if remote.failed:
                    raise RuntimeError("runner lost")
                return recover(*args)
            remote.fail = ("rename", 4, "after")
            with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh), patch.object(publisher, "recover_publication", side_effect=interrupted_recovery), self.assertRaises(RuntimeError):
                publisher.publish(output, notes, REPOSITORY, SHA)
            self.assertIn(publisher.JOURNAL, remote.by_name())
            with patch.object(publisher, "gh", side_effect=remote.gh):
                publisher.recover_publication(REPOSITORY, publisher.release_state(REPOSITORY))
            remote.assert_previous(self)

    def test_lost_response_after_metadata_commit_preserves_verified_publication(self):
        with tempfile.TemporaryDirectory() as temp:
            remote, output, notes = self.publication_fixture(Path(temp))
            remote.fail = ("notes", 1, "after")
            with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh):
                publisher.publish(output, notes, REPOSITORY, SHA)
            remote.assert_bundle(self, output, SHA, notes.read_text())
            self.assertNotIn(publisher.JOURNAL, remote.by_name())

    def test_missing_server_digest_downloads_without_overwriting_local_input(self):
        with tempfile.TemporaryDirectory() as temp:
            remote, output, notes = self.publication_fixture(Path(temp))
            remote.include_digest = False
            before = {p.name: p.read_bytes() for p in output.iterdir()}
            with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh):
                publisher.publish(output, notes, REPOSITORY, SHA)
            self.assertEqual(before, {p.name: p.read_bytes() for p in output.iterdir()})
            remote.assert_bundle(self, output, SHA, notes.read_text())

    def test_bad_staged_digest_and_older_build_cannot_replace_downloads(self):
        for mode in ("digest", "older"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                remote, output, notes = self.publication_fixture(Path(temp))
                if mode == "digest":
                    remote.corrupt_digest = True
                else:
                    remote.body = "Source /actions/runs/999"
                    remote.old_body = remote.body
                with patch.object(publisher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), patch.object(publisher, "gh", side_effect=remote.gh), self.assertRaises(ValueError):
                    publisher.publish(output, notes, REPOSITORY, SHA)
                remote.assert_previous(self)


class FakeRelease:
    """Stateful GitHub fixture: failures can happen after a remote mutation."""
    def __init__(self):
        self.assets = {}
        self.sequence = 0
        self.sha = "b" * 40
        self.body = self.old_body = "Previous verified build /actions/runs/9"
        self.fail = None
        self.failed = False
        self.events = {}
        self.include_digest = True
        self.corrupt_digest = False
        for name in sorted(publisher.CURRENT_ASSETS | {"notes.txt"}):
            self.add(name, f"previous {name}".encode())

    def add(self, name, data):
        if name in self.by_name():
            raise RuntimeError("asset already exists")
        self.sequence += 1
        self.assets[self.sequence] = {"id": self.sequence, "name": name, "data": data}

    def by_name(self):
        return {asset["name"]: asset for asset in self.assets.values()}

    def event(self, name, action):
        count = self.events.get(name, 0) + 1
        self.events[name] = count
        for timing in ("before", "after"):
            if self.fail == (name, count, timing) and not self.failed:
                self.failed = True
                raise RuntimeError(f"injected {name} {timing}")
            if timing == "before":
                action()

    def gh(self, *args):
        if args[:2] == ("release", "upload"):
            path = Path(args[3])
            self.event("upload", lambda: self.add(path.name, path.read_bytes()))
        elif args[:2] == ("release", "delete-asset"):
            asset = self.by_name()[args[3]]
            del self.assets[asset["id"]]
        elif args[:2] == ("release", "download"):
            name = args[args.index("--pattern") + 1]
            path = Path(args[args.index("--dir") + 1]) / name
            path.write_bytes(self.by_name()[name]["data"])
        elif args[0] == "api":
            endpoint = next(arg for arg in args if arg.startswith("repos/"))
            if "--method" not in args:
                if endpoint.endswith("releases/tags/rolling"):
                    assets = []
                    for asset in self.assets.values():
                        item = {key: asset[key] for key in ("id", "name")}
                        if self.include_digest:
                            item["digest"] = "sha256:" + ("0" * 64 if self.corrupt_digest and asset["name"].startswith("candidate-") else hashlib.sha256(asset["data"]).hexdigest())
                        assets.append(item)
                    return json.dumps({"id": 100, "body": self.body, "assets": assets})
                if endpoint.endswith("git/ref/tags/rolling"):
                    return json.dumps({"object": {"sha": self.sha}})
                if "/releases/assets/" in endpoint:
                    return self.assets[int(endpoint.rsplit("/", 1)[1])]["data"].decode()
                raise AssertionError(args)
            field = args[args.index("-f") + 1]
            key, value = field.split("=", 1)
            if key == "name":
                asset = self.assets[int(endpoint.rsplit("/", 1)[1])]
                def rename():
                    existing = self.by_name().get(value)
                    if existing and existing["id"] != asset["id"]:
                        raise RuntimeError("duplicate public alias")
                    asset["name"] = value
                self.event("rename", rename)
            elif key == "sha":
                self.event("source", lambda: setattr(self, "sha", value))
            elif key == "body":
                self.event("notes", lambda: setattr(self, "body", value))
            else:
                raise AssertionError(args)
        else:
            raise AssertionError(args)
        return ""

    def assert_previous(self, test):
        test.assertEqual(self.sha, "b" * 40)
        test.assertEqual(self.body, self.old_body)
        for name in publisher.CURRENT_ASSETS:
            test.assertEqual(self.by_name()[name]["data"], f"previous {name}".encode())

    def assert_bundle(self, test, output, sha, body):
        test.assertEqual(self.sha, sha)
        test.assertEqual(self.body, body)
        for name in publisher.CURRENT_ASSETS:
            test.assertEqual(self.by_name()[name]["data"], (output / name).read_bytes())


if __name__ == "__main__":
    unittest.main()
