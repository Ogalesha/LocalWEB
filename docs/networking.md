# Networking

The gateway serves TLS on `127.0.0.1:443` only; LocalWeb does not listen for project traffic on port 80. On Linux only its executable holds `CAP_NET_BIND_SERVICE`. It chooses certificates by SNI, validates the Host header and forwards standard proxy headers (`X-Forwarded-Host`, `X-Forwarded-Proto`, `X-Forwarded-For`). LocalWeb adds no HSTS header; applications may return their own. Both `.dev` and `.test` project domains are HTTPS-only.

nginx listens on `127.0.0.1:18080` and Apache on `127.0.0.1:18081`. Both forward PHP to per-project PHP-FPM sockets. Databases, caches and Mailpit use loopback ports or private sockets.

The privileged helper writes exact project hostnames into one atomic `/etc/hosts` block. It never configures a `.dev` wildcard: every hostname in the block maps to `127.0.0.1` and nothing else.
