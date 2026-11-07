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

VERSION = r"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)"
PACKAGES = (
    (rf"localweb_{VERSION}_amd64\.deb", "localweb-ubuntu-amd64.deb", "Ubuntu 26.04"),
    (rf"localweb-{VERSION}-\d+-x86_64\.pkg\.tar\.zst", "localweb-arch-x86_64.pkg.tar.zst", "Arch / Omarchy"),
    (rf"LocalWeb-{VERSION}\.pkg", "LocalWeb-macos-universal.pkg", "macOS 12+"),
    (rf"localweb-{VERSION}-windows-x86_64\.zip", "localweb-windows-x86_64.zip", "Windows 10/11"),
)
CURRENT_ASSETS = {alias for _, alias, _ in PACKAGES} | {"SHA256SUMS"}


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True).strip()


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
        raise ValueError("No successful main build is available")
    return max(trusted, key=lambda run: run["id"])


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


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


def publish(destination, notes_path, repository, sha):
    result = subprocess.run(["gh", "release", "view", "rolling", "--json", "id"],
                            text=True, capture_output=True)
    if result.returncode:
        if "release not found" not in result.stderr.lower() and "404" not in result.stderr:
            raise RuntimeError(result.stderr)
        gh("release", "create", "rolling", "--prerelease", "--latest=false", "--target", sha,
           "--title", "LocalWeb — latest verified build", "--notes-file", str(notes_path))
    assets = [str(destination / name) for name in sorted(CURRENT_ASSETS)]
    gh("release", "upload", "rolling", *assets, "--clobber")
    # Keep the rolling source archives aligned with the installers, too.
    gh("api", "--method", "PATCH", f"repos/{repository}/git/refs/tags/rolling",
       "-f", f"sha={sha}", "-F", "force=true")
    gh("release", "edit", "rolling", "--prerelease", "--latest=false",
       "--title", "LocalWeb — latest verified build", "--notes-file", str(notes_path))
    release = json.loads(gh("release", "view", "rolling", "--json", "assets"))
    # Remove old versioned installers only after all replacements are uploaded.
    for asset in release["assets"]:
        if obsolete_asset(asset["name"]):
            gh("release", "delete-asset", "rolling", asset["name"], "--yes")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["select", "publish"])
    parser.add_argument("--source", type=Path, default=Path("release-files"))
    parser.add_argument("--destination", type=Path, default=Path("rolling-files"))
    args = parser.parse_args()
    repository = os.environ["GITHUB_REPOSITORY"]
    if args.command == "select":
        runs = json.loads(gh("api", f"repos/{repository}/actions/workflows/native-builds.yml/runs?branch=main&event=push&status=success&per_page=30"))
        run = select_run(runs["workflow_runs"], repository)
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write(f"run_id={run['id']}\nsha={run['head_sha']}\n")
        print(f"Selected successful main build {run['id']} ({run['head_sha']})")
    else:
        sha = os.environ["BUILD_SHA"]
        notes = prepare(args.source, args.destination, repository, sha, int(os.environ["BUILD_RUN_ID"]))
        notes_path = args.destination.parent / "rolling-notes.md"
        notes_path.write_text(notes)
        publish(args.destination, notes_path, repository, sha)


if __name__ == "__main__":
    main()
