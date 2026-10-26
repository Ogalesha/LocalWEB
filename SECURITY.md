# Security

The daemon, GUI, gateway, web servers, PHP, databases, and caches all run as the desktop user, never as root. The privileged helper is short-lived, accepts only a fixed set of operations, validates every input, and never runs a shell command.

The helper can install or remove the LocalWeb CA, synchronize the exact managed `/etc/hosts` block, register user-owned cleanup paths, and grant the low-port capability to the dedicated gateway binary. It rejects non-loopback host mappings, invalid domains, unexpected arguments, symlinked privileged files, and any certificate that is not the LocalWeb CA. Managed services run with `NoNewPrivileges=yes`, except the gateway unit: that unit must receive the gateway binary's file capability during `execve`, which the kernel would suppress under `NoNewPrivileges`. The gateway unit also skips `PrivateTmp=yes`, because its mount namespace would confine the capability to a user namespace where it cannot bind host ports. The gateway still runs as an unprivileged user process and holds only `CAP_NET_BIND_SERVICE`.

LocalWeb never stops or rewrites an unrelated listener. On Linux the gateway selects a dedicated IPv4 loopback address, normally `127.0.0.2:443`, while other applications keep `127.0.0.1:443`. The helper validates that managed host mappings remain inside `127.0.0.0/8`. A high port in 8443–8499 is used only if the dedicated low-port binding fails. LocalWeb does not inspect, signal or reconfigure another listener.

The CA private key is created with mode `0600`, stays in the user's data directory, and is never sent to the helper or written to logs. Only the public root certificate goes into the system trust store. Existing CA material is never silently replaced.

On Linux, the unprivileged CLI also imports that public root into the current user's NSS database for Brave and Chromium. It invokes `/usr/bin/certutil` with explicit arguments, verifies that the stored certificate bytes match the LocalWeb CA, and never sends the private key. Package removal deletes the LocalWeb nickname from every registered user's NSS database.

The daemon socket and configuration directories are user-only. Database credentials live in restricted per-project secret files and never appear in general configuration or logs. Managed databases and caches listen on loopback or Unix sockets only.

The private runtime uses native inputs, checksum-verified official PHP source archives, and verified Composer/Mailpit downloads. PHP 8.0 and 8.1 are end-of-life compatibility versions and receive no upstream security fixes; PHP 8.2 receives security fixes only until 31 Dec 2026. PHP 8.0 also requires the legacy OpenSSL 1.1.1 ABI, so its bundled compatibility library is not maintained upstream. Avoid these legacy combinations for security-sensitive work and use a currently supported PHP branch. Build checks verify hashes in `runtime-manifest.json`; runtime diagnostics do not repeat hash verification. Compare downloaded packages with their accompanying SHA-256 before installation.

No user-controlled project value is ever interpolated into a shell command. Native processes start from executable paths with argument arrays. Supervisor arguments are stored as an array, capped and validated as non-empty single-line values, and passed straight to `systemd-run`; `sh -c` is never involved. Relative supervisor executables must resolve inside the project directory.

Uninstall stops owned `localweb-*` services and executables below `/usr/lib/localweb`, then removes registered paths and the CA trust copy. Cleanup affects only LocalWeb-owned state and integration; source folders remain outside this boundary.
