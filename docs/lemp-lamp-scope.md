# LEMP/LAMP scope

Compared with [FlyEnv's features](https://flyenv.com/features) on 7 November
2026. LocalWeb keeps a focused PHP development workflow.

| Relevant capability | LocalWeb decision |
| --- | --- |
| Per-project PHP, matching CLI and server handler, Composer | Already supported. Fix version selection and process ownership before adding controls. |
| nginx / Apache, document roots, local domains and HTTPS | Already supported. Preserve this workflow and reject unrelated Host headers. |
| MySQL / MariaDB, isolated project credentials and data | Already supported. Secure administrator access and preserve data on restart. |
| Redis, development mail, workers and scheduled tasks | Already supported. Make lifecycle, environment and diagnostics consistent. |
| PHP configuration and extension management | A possible follow-up: selected-version extension inspection and explicit, validated overrides. A raw configuration editor needs clear ownership and persistence rules first. |
| Database export and restore | A possible follow-up: explicit destination, restricted backup files, compatible native dump tools and confirmation before replacing data. Avoid a one-click import that guesses another stack's database. |

[FlyEnv's PHP controls](https://flyenv.com/features/php) and
[database tools](https://flyenv.com/features/mysql) are useful references for
those two follow-ups. They are not implemented or advertised as available.
No new menus or services are needed for this audit. Reliable startup,
data preservation and safe command execution take priority.

Other language runtimes, AI services, containers, tunnels and unrelated
databases are outside this comparison. Existing optional tools can remain;
they do not justify expanding the core workflow.
