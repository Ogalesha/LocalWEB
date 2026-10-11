# Troubleshooting

Run `localweb doctor` first. Inspect services with `systemctl --user status localwebd.service` and `systemctl --user list-units 'localweb-*'`.

Logs under `~/.local/state/localweb/logs/` cover nginx, Apache, PHP-FPM, MariaDB, MySQL, Redis and cron. PostgreSQL, workers and Mailpit also report through `journalctl --user -u 'localweb-*'`. Custom XDG paths change those locations.

If HTTPS shows as untrusted, run `localweb ca status` followed by `localweb ca install`. If a project hostname does not resolve, run `localweb hosts sync`.

If the gateway cannot bind HTTPS port 443, check `getcap /usr/lib/localweb/localweb-gateway`. Reinstalling the package reapplies the capability when `setcap` is available.

Missing PHP requires matching CLI/FPM; PostgreSQL needs its complete runtime and PHP's `pdo_pgsql`. Accepted versions do not download binaries. See `docs/runtime-management.md`.

Removal wipes LocalWeb-owned state. Export databases first; source directories are kept.
