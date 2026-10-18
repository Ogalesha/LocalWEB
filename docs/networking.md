# Networking

Every project owns a fixed loopback web port in `18000-18999`, rendered as `listen 127.0.0.1:<port>` in nginx and as `Listen`/`VirtualHost 127.0.0.1:<port>` in Apache. Start prints `http://127.0.0.1:<port>`; plain HTTP therefore works even when another local stack holds ports 80 and 443. Both web servers forward PHP to per-project PHP-FPM sockets.

When the HTTPS gateway can bind `127.0.0.1:443`, it also serves every project on its configured hostname over TLS. If port 443 is occupied, LocalWeb logs the conflict and keeps the direct ports instead of failing. On Linux only the gateway executable holds `CAP_NET_BIND_SERVICE`. It chooses certificates by SNI, validates the Host header and forwards standard proxy headers (`X-Forwarded-Host`, `X-Forwarded-Proto`, `X-Forwarded-For`). LocalWeb adds no HSTS header; applications may return their own. Both `.dev` and `.test` domains are served only through the gateway.

Shared database engines stay on their internal loopback ports (MariaDB `33071`, MySQL `33072`, PostgreSQL `33073`). Each project with a network database gets a fixed proxy port in `19000-19999`; the daemon forwards it to the shared engine, and the project environment exposes it as `DB_HOST=127.0.0.1` with `DB_PORT=<proxy port>`. Redis and Mailpit keep their own loopback ports or private sockets.

The privileged helper writes exact project hostnames into one atomic `/etc/hosts` block. It never configures a `.dev` wildcard: every hostname in the block maps to `127.0.0.1` and nothing else.
