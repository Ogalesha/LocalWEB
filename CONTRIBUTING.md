# Contributing

Keep changes inside the native-process architecture. Docker, container runtimes, VMs, global PHP switching, public database listeners, and arbitrary privileged commands are out of scope.

Every change must pass formatting, Clippy with warnings denied, the test suite, and a release build. When you touch configuration, protocol, paths, hostnames, certificates, hosts-file handling, command construction, or routing, add a test covering it. If something is intentionally unsupported, say so in the docs instead of leaving it implied.

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`, with an optional scope (`feat(daemon): ...`). One logical change per commit, imperative present tense.
- Branches: `main` is always releasable. Work happens on short-lived `feature/<topic>` or `fix/<topic>` branches, merged through a pull request, then deleted.
- Versions follow [SemVer](https://semver.org/). Every user-facing change goes under `## [Unreleased]` in `CHANGELOG.md` (Keep a Changelog); the release moves it under a new version section.
- Tags are annotated SemVer (`git tag -a v0.1.0 -m "LocalWeb 0.1.0"`) and mark GitHub Releases. The workspace version in `Cargo.toml` and the tag always match.
