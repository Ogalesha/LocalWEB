#!/usr/bin/env python3
"""Publish fixed download URLs from a successful, trusted main build."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import tempfile
import uuid

VERSION = r"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)"
PACKAGES = (
    (rf"localweb_{VERSION}_amd64\.deb", "localweb-ubuntu-amd64.deb", "Ubuntu 26.04"),
    (rf"localweb-{VERSION}-\d+-x86_64\.pkg\.tar\.zst", "localweb-arch-x86_64.pkg.tar.zst", "Arch / Omarchy"),
    (rf"LocalWeb-{VERSION}\.pkg", "LocalWeb-macos-universal.pkg", "macOS 12+"),
    (rf"localweb-{VERSION}-windows-x86_64\.zip", "localweb-windows-x86_64.zip", "Windows 10/11"),
)
CURRENT_ASSETS = {alias for _, alias, _ in PACKAGES} | {"SHA256SUMS"}
REQUIRED_CHECKS = (".github/workflows/native-builds.yml", ".github/workflows/macos-smoke.yml")


class NoVerifiedBuild(ValueError):
    """The first complete main build has not succeeded yet."""


def gh(*args):
    result = subprocess.run(["gh", *args], text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stderr.strip()[-3000:])
    return result.stdout.strip()


def select_run(runs, repository):
    """Never publish PR, fork, failed, or non-main artifacts."""
    trusted = [run for run in runs if (
        run.get("event") == "push"
        and run.get("head_branch") == "main"
        and run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and run.get("path") == ".github/workflows/native-builds.yml"
        and (run.get("head_repository") or {}).get("full_name") == repository
        and type(run.get("id")) is int and run["id"] > 0
        and re.fullmatch(r"[0-9a-f]{40}", run.get("head_sha", ""))
    )]
    if not trusted:
        raise NoVerifiedBuild("No successful main build is available")
    return max(trusted, key=lambda run: run["id"])


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def release_checks_ready(runs, repository, sha, build_id, current_build=False):
    trusted = [run for run in runs if (
        run.get("head_sha") == sha
        and (run.get("head_repository") or {}).get("full_name") == repository
        and run.get("event") == "push"
        and (run.get("head_branch") == "main" or
             (current_build and run.get("id") == build_id and run.get("path") == REQUIRED_CHECKS[0]))
        and type(run.get("id")) is int and run["id"] > 0
    )]
    for workflow in REQUIRED_CHECKS:
        candidates = [run for run in trusted if run.get("path") == workflow]
        if not candidates:
            return False
        run = max(candidates, key=lambda item: item["id"])
        if workflow == REQUIRED_CHECKS[0] and run["id"] != build_id:
            raise ValueError("A newer native run exists for this commit; retain the previous downloads")
        if current_build and workflow == REQUIRED_CHECKS[0] and run["status"] == "in_progress":
            # This job depends on every native build/test job. Publication is
            # the final job in its own workflow, so that workflow is not done yet.
            continue
        if run["status"] != "completed":
            return False
        if run.get("conclusion") != "success":
            raise ValueError(f"Release blocked: {workflow} ended with {run.get('conclusion')}")
    return True


def wait_for_release_checks(repository, sha, build_id):
    current_build = (os.environ.get("GITHUB_WORKFLOW") == "Native packages and acceptance"
                     and os.environ.get("GITHUB_RUN_ID") == str(build_id))
    for attempt in range(21):
        runs = json.loads(gh("api", f"repos/{repository}/actions/runs?head_sha={sha}&per_page=100"))
        if release_checks_ready(runs["workflow_runs"], repository, sha, build_id, current_build):
            print(f"Required release checks passed for {sha}", flush=True)
            return
        if attempt < 20:
            print("Waiting for this commit's required release checks; previous downloads remain available", flush=True)
            time.sleep(30)
    raise ValueError("Release checks did not finish; retain the previous downloads")


def prepare(source, destination, repository, sha, run_id):
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or run_id <= 0:
        raise ValueError("Invalid build identity")
    files = [path for path in source.rglob("*") if path.is_file()]
    selected = []
    for pattern, alias, label in PACKAGES:
        matches = [(path, re.fullmatch(pattern, path.name)) for path in files]
        matches = [(path, match) for path, match in matches if match]
        if len(matches) != 1:
            raise ValueError(f"Expected one {label} installer, found {len(matches)}")
        path, match = matches[0]
        if path.is_symlink() or path.stat().st_size == 0:
            raise ValueError(f"Invalid installer: {path.name}")
        selected.append((path, alias, label, match[1]))
    versions = {version for _, _, _, version in selected}
    if len(versions) != 1:
        raise ValueError("Installer versions do not match")
    destination.mkdir(exist_ok=False)
    checksums = []
    links = []
    for path, alias, label, _ in selected:
        target = destination / alias
        shutil.copyfile(path, target)
        checksums.append(f"{digest(target)}  {alias}\n")
        links.append(f"- [{label}](https://github.com/{repository}/releases/download/rolling/{alias})")
    (destination / "SHA256SUMS").write_text("".join(sorted(checksums)))
    notes = (
        f"Latest verified build from `main`: **{versions.pop()}**.\n\n"
        f"Source: [{sha}](https://github.com/{repository}/commit/{sha}) · "
        f"[Passed native build](https://github.com/{repository}/actions/runs/{run_id})\n\n"
        + "\n".join(links)
        + "\n\nThese download URLs stay the same across updates. This is a rolling pre-release; "
          "versioned stable releases remain available separately. Packages are unsigned. "
          "Verify downloads with [SHA256SUMS]"
        f"(https://github.com/{repository}/releases/download/rolling/SHA256SUMS).\n"
    )
    return notes


def obsolete_asset(name):
    return name not in CURRENT_ASSETS and any(re.fullmatch(pattern, name) for pattern, _, _ in PACKAGES)


JOURNAL = "localweb-publication.json"


def release_state(repository):
    return json.loads(gh("api", f"repos/{repository}/releases/tags/rolling"))


def rename_asset(repository, asset_id, name):
    gh("api", "--method", "PATCH", f"repos/{repository}/releases/assets/{asset_id}", "-f", f"name={name}")


def set_source(repository, sha):
    gh("api", "--method", "PATCH", f"repos/{repository}/git/refs/tags/rolling", "-f", f"sha={sha}", "-F", "force=true")


def set_notes(repository, release_id, body):
    gh("api", "--method", "PATCH", f"repos/{repository}/releases/{release_id}", "-f", f"body={body}")


def verify_asset(repository, asset, expected, directory):
    # GitHub computes digests server-side. Older assets can lack that field;
    # download those instead of assuming an upload succeeded intact.
    if asset.get("digest"):
        if asset["digest"] != f"sha256:{expected}":
            raise ValueError(f"Uploaded checksum mismatch: {asset['name']}")
    else:
        with tempfile.TemporaryDirectory(dir=directory) as temp:
            gh("release", "download", "rolling", "--repo", repository, "--pattern", asset["name"], "--dir", temp)
            if digest(Path(temp) / asset["name"]) != expected:
                raise ValueError(f"Downloaded checksum mismatch: {asset['name']}")


def recover_publication(repository, release):
    pending = next((asset for asset in release["assets"] if asset["name"] == JOURNAL), None)
    if pending is None:
        return
    transaction = json.loads(gh("api", f"repos/{repository}/releases/assets/{pending['id']}", "-H", "Accept: application/octet-stream"))
    ids = {asset["id"] for asset in release["assets"]}
    if (not re.fullmatch(r"[0-9a-f]{40}", transaction.get("old_sha", ""))
            or not re.fullmatch(r"[0-9a-f]{40}", transaction.get("new_sha", ""))
            or {a["name"] for a in transaction["new_assets"]} != CURRENT_ASSETS
            or any(item["name"] not in CURRENT_ASSETS or item["id"] not in ids for item in transaction["old_assets"])
            or any(item["name"] not in CURRENT_ASSETS or item["id"] not in ids
                   or not re.fullmatch(r"candidate-[0-9a-f]{32}-" + re.escape(item["name"]), item["staged_name"])
                   for item in transaction["new_assets"])):
        raise ValueError("Invalid publication journal; retain the assets for manual recovery")
    current = {asset["id"]: asset for asset in release["assets"]}
    source = json.loads(gh("api", f"repos/{repository}/git/ref/tags/rolling"))["object"]["sha"]
    committed = (source == transaction["new_sha"] and release["body"] == transaction["new_body"]
                 and all(current[item["id"]]["name"] == item["name"] for item in transaction["new_assets"]))
    if not committed:
        # Move candidates out of the public namespace before restoring old IDs.
        # Re-running this recovery after another interruption is idempotent.
        for item in transaction["new_assets"]:
            if current[item["id"]]["name"] != item["staged_name"]:
                rename_asset(repository, item["id"], item["staged_name"])
        for item in transaction["old_assets"]:
            if current[item["id"]]["name"] != item["name"]:
                rename_asset(repository, item["id"], item["name"])
        set_source(repository, transaction["old_sha"])
        set_notes(repository, release["id"], transaction["old_body"])
    else:
        with tempfile.TemporaryDirectory() as temp:
            for item in transaction["new_assets"]:
                verify_asset(repository, current[item["id"]], item["sha256"], Path(temp))
    gh("release", "delete-asset", "rolling", JOURNAL, "--repo", repository, "--yes")
    return committed


def publish(destination, notes_path, repository, sha):
    result = subprocess.run(["gh", "release", "view", "rolling", "--repo", repository, "--json", "id"],
                            text=True, capture_output=True)
    if result.returncode:
        if "release not found" not in result.stderr.lower() and "404" not in result.stderr:
            raise RuntimeError(result.stderr)
        gh("release", "create", "rolling", "--repo", repository, "--prerelease", "--latest=false", "--target", sha,
           "--title", "LocalWeb — latest verified build", "--notes", "Preparing the first verified build.")
    recover_publication(repository, release_state(repository))
    release = release_state(repository)
    notes = notes_path.read_text()
    previous_run = re.search(r"/actions/runs/(\d+)", release.get("body") or "")
    next_run = re.search(r"/actions/runs/(\d+)", notes)
    if previous_run and (not next_run or int(next_run[1]) < int(previous_run[1])):
        raise ValueError("A newer build is already published; keep the current downloads")
    ref = json.loads(gh("api", f"repos/{repository}/git/ref/tags/rolling"))
    old_sha = ref["object"]["sha"]
    transaction = {"old_sha": old_sha, "old_body": release.get("body") or "",
                   "new_sha": sha, "new_body": notes,
                   "old_assets": [{"id": a["id"], "name": a["name"]} for a in release["assets"] if a["name"] in CURRENT_ASSETS],
                   "new_assets": []}
    with tempfile.TemporaryDirectory(dir=destination.parent) as temp:
        staging = Path(temp)
        token = uuid.uuid4().hex
        # Stage and verify the whole bundle before changing a public URL.
        for name in sorted(CURRENT_ASSETS):
            staged_name = f"candidate-{token}-{name}"
            path = staging / staged_name
            os.link(destination / name, path)
            gh("release", "upload", "rolling", str(path), "--repo", repository)
        assets = {a["name"]: a for a in release_state(repository)["assets"]}
        for name in sorted(CURRENT_ASSETS):
            staged_name = f"candidate-{token}-{name}"
            asset = assets[staged_name]
            expected = digest(destination / name)
            verify_asset(repository, asset, expected, staging)
            transaction["new_assets"].append({"id": asset["id"], "name": name, "staged_name": staged_name, "sha256": expected})
        journal = staging / JOURNAL
        journal.write_text(json.dumps(transaction))
        gh("release", "upload", "rolling", str(journal), "--repo", repository)
        try:
            # SHA256SUMS is promoted after every installer. Metadata is committed
            # last. The durable journal can recover even a terminated runner.
            order = sorted(CURRENT_ASSETS - {"SHA256SUMS"}) + ["SHA256SUMS"]
            old = {a["name"]: a for a in transaction["old_assets"]}
            new = {a["name"]: a for a in transaction["new_assets"]}
            for name in order:
                if name in old:
                    rename_asset(repository, old[name]["id"], f"previous-{old_sha[:12]}-{old[name]['id']}-{name}")
                rename_asset(repository, new[name]["id"], name)
            public = {a["name"]: a for a in release_state(repository)["assets"]}
            for name in order:
                verify_asset(repository, public[name], new[name]["sha256"], staging)
            set_source(repository, sha)
            set_notes(repository, release["id"], notes)
            # Read back metadata before marking the transaction committed.
            final = release_state(repository)
            final_sha = json.loads(gh("api", f"repos/{repository}/git/ref/tags/rolling"))["object"]["sha"]
            if final_sha != sha or final["body"] != notes:
                raise ValueError("Published source metadata did not match the verified bundle")
        except BaseException as error:
            try:
                if recover_publication(repository, release_state(repository)):
                    return  # The server committed before its response was lost.
            except (ValueError, RuntimeError, OSError) as recovery:
                raise RuntimeError(f"Publication failed: {error}; recovery is pending: {recovery}. Previous assets and the recovery journal were retained.") from error
            raise
        gh("release", "delete-asset", "rolling", JOURNAL, "--repo", repository, "--yes")
    # Retain the immediately previous bundle. Remove only older transaction
    # leftovers and legacy installers after the new bundle is fully committed.
    retained = {f"previous-{old_sha[:12]}-{a['id']}-{a['name']}" for a in transaction["old_assets"]}
    for asset in release_state(repository)["assets"]:
        name = asset["name"]
        temporary = re.fullmatch(r"(?:candidate-[0-9a-f]{32}|previous-[0-9a-f]{12}-\d+)-(.+)", name)
        if obsolete_asset(name) or (temporary and temporary[1] in CURRENT_ASSETS and name not in retained):
            gh("release", "delete-asset", "rolling", name, "--repo", repository, "--yes")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["select", "verify", "publish"])
    parser.add_argument("--source", type=Path, default=Path("release-files"))
    parser.add_argument("--destination", type=Path, default=Path("rolling-files"))
    args = parser.parse_args()
    repository = os.environ["GITHUB_REPOSITORY"]
    if args.command == "select":
        runs = json.loads(gh("api", f"repos/{repository}/actions/workflows/native-builds.yml/runs?branch=main&event=push&status=success&per_page=30"))
        try:
            run = select_run(runs["workflow_runs"], repository)
        except NoVerifiedBuild:
            print("::notice title=Downloads unchanged::No verified main build is available; keeping existing downloads.")
            return
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write(f"run_id={run['id']}\nsha={run['head_sha']}\n")
        print(f"Selected successful main build {run['id']} ({run['head_sha']})")
    else:
        sha = os.environ["BUILD_SHA"]
        build_id = int(os.environ["BUILD_RUN_ID"])
        if not re.fullmatch(r"[0-9a-f]{40}", sha) or build_id <= 0:
            raise ValueError("Invalid build identity")
        wait_for_release_checks(repository, sha, build_id)
        if args.command == "verify":
            return
        notes = prepare(args.source, args.destination, repository, sha, build_id)
        notes_path = args.destination.parent / "rolling-notes.md"
        notes_path.write_text(notes)
        publish(args.destination, notes_path, repository, sha)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, OSError) as error:
        message = str(error).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::error title=Download publication::{message}", flush=True)
        raise
