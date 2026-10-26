# Troubleshooting

Run `localweb doctor` first. Inspect services with `systemctl --user status localwebd.service` and `systemctl --user list-units 'localweb-*'`.

Logs under `~/.local/state/localweb/logs/` cover nginx, Apache, PHP-FPM, MariaDB, MySQL, Redis and cron. PostgreSQL, workers and Mailpit also report through `journalctl --user -u 'localweb-*'`. Custom XDG paths change those locations.

If HTTPS shows as untrusted, run `localweb ca status` followed by `localweb ca install`. Status checks both Ubuntu's trust store and the current user's Brave/Chromium NSS database. Fully restart an already-running browser after the first installation. If a project hostname does not resolve, run `localweb hosts sync`.

If another application owns `127.0.0.1:443`, LocalWeb leaves it untouched and uses a dedicated address such as `127.0.0.2:443`; `https://PROJECT.dev` remains portless. If the browser reaches the other application's certificate, run `localweb hosts sync` and verify the selected address with `localweb status` or `localweb doctor`. A URL with a port in `8443-8499` appears only when the dedicated low-port binding also fails. Check `getcap /usr/lib/localweb/localweb-gateway`; reinstalling the package reapplies the capability when `setcap` is available.

Missing PHP requires matching CLI/FPM; PostgreSQL needs its complete runtime and PHP's `pdo_pgsql`. Accepted versions do not download binaries. See `docs/runtime-management.md`.

Removal wipes LocalWeb-owned state. Export databases first; source directories are kept.
