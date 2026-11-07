# English product copy

LocalWeb speaks to developers in short, direct sentences. Name the action,
state its outcome, and give a next step when something fails. Use ordinary
language without jokes, filler or promises the application cannot verify.

## Terms

| Meaning | Use |
| --- | --- |
| User's configured application | Project |
| Existing project directory | Source folder |
| Project DNS name | Local domain |
| PHP, Node.js and related tools | Runtime |
| Long-running project process | Background worker; Worker in compact summaries |
| Cron entry | Scheduled task |
| Development email | Test email; Mailpit for the service name |
| Trusting the development CA | Trust certificate; HTTPS certificate trusted |
| Active service state | Running / Stopped / Unavailable |
| First launch without configuration | Setup required |
| Configured feature | Enabled / Disabled |

Keep technical identifiers such as `--supervisor`, `hostname`, systemd unit
names, protocol keys and the certificate's actual subject unchanged.

## Actions and consequences

- **Add project** registers an existing source folder.
- **Edit** opens settings; **Save changes** applies them. A running project restarts.
- **Start**, **Stop** and **Restart** refer to the selected project.
- **Remove project** stops it and removes LocalWeb settings and its local domain.
  Source files and databases are kept.
- **Stop LocalWeb** stops LocalWeb and all running projects.
- Closing the window keeps projects running. **Quit desktop app** closes only the desktop app.
- **Purge data** permanently deletes this account's LocalWeb data. Never describe this as project removal.
- App autostart means login startup. Project autostart means starting with LocalWeb.

## Messages

Use sentence case. Short labels and success messages need no final period.
Use full sentences for explanations and confirmations. Explain the result
before technical detail. Keep explicit recovery commands when they help.
Do not call a project ready just because LocalWeb answered, or call a
certificate installed when the relevant state is whether it is trusted.
Missing setup is a separate state from an unreachable service; its message
and action both lead to **Set up LocalWeb**.

Shared English outcomes and terms live in
[`localweb-core/src/copy.rs`](../crates/localweb-core/src/copy.rs), reused by
the desktop, CLI and MCP. Browser error pages use the same project/domain
terminology. Localized desktop strings retain their language-specific wording.
