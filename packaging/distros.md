# Native packaging

LocalWeb is distributed only through GitHub. OS archives and upstream
downloads are build inputs, not LocalWeb distribution channels.

| Platform | Package | Native integration | Validation |
| --- | --- | --- | --- |
| Ubuntu 26.04 amd64 | `.deb` | systemd / `update-ca-certificates` | Native build and acceptance job |
| Arch Linux amd64 | `.pkg.tar.zst` | systemd / `trust extract-compat` | Native build and acceptance job |
| Omarchy amd64 | Arch package | Uses the Arch base and integration | Covered by Arch CI; desktop acceptance remains separate |
| macOS universal (arm64, x86_64) | `.pkg` | pkgbuild / LaunchAgents / `security` trust store | Built universal and smoked on the macOS CI runner; no Apple hardware run yet |
| Windows amd64 | portable `.zip` | Per-user named pipe, `%LOCALAPPDATA%` state, scheduled-task daemon | Cross-built with cargo-xwin and exercised under Wine in CI; no real Windows run yet |

Native staging records the target OS and architecture and bundles private
shared libraries. `build-native-package.sh` produces the Arch package;
`build-fat-deb.sh` produces the Ubuntu package. Manual Arch builders can use
the PKGBUILD.

macOS binaries are lipo-linked into universal executables by CI, and
`packaging/macos/build-pkg.sh` bundles the CLI tools plus `/Applications/LocalWeb.app`.
The installer is unsigned: Gatekeeper warnings are expected until notarization
exists. `scripts/build-windows-zip.sh` packages the six cross-built executables
into `localweb-<version>-windows-x86_64.zip`.

Each supported distribution must finish runtime verification, installation,
acceptance and removal before its artifact is uploaded. Docker is used only
inside GitHub's Linux CI builders and is absent from the installed product.

Upgrades preserve state. Removal deletes owned services, settings, data,
certificates and host mappings while retaining source folders.
