Reaper 0.1.6 (build 469) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.6-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.6-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **MariaDB password + SSL over SSH:** Pass `--password=` (MariaDB ignores `MYSQL_PWD` and was disabling cert verify as “passwordless”). Tunnel uses TCP; hostname verify softened on loopback while CA still applies; clearer handshake / bastion remote-host errors. Build **469**.
- **Database over SSH + SSL:** Postgres tunnels keep the real hostname for TLS (`hostaddr=127.0.0.1`); MySQL softens verify-* over the loopback forward. Opening the Database panel no longer live-probes (avoids 120s hangs); Test/Connect still probe with connect timeouts. Clearer SSH auth errors (strips OpenSSH post-quantum noise). Build **468**.
- **Import remotes & folders as dropdowns:** Recent clone URLs and local paths are saved and shown in the Import dialog; also seeded from existing repos. UI build **467**.
- **New / Import dialogs refreshed:** Clearer From URL / From folder choices, plain-language labels, and Create/Clone & open actions. UI build **466**.
- **Database connection picker stays populated:** Schema responses include the saved connection list so the dropdown does not clear after Connect. UI build **467**.
- **Welcome Recent repos:** Last opened repo appears first.
- **Database viewer tree starts collapsed:** Schema/database nodes are listed closed; expand folders and tables as needed (filter still expands matches). UI build **463**.
- **Agent panel × closes the panel:** The close button on the agent toolbar now hides the agent (right/bottom dock) or switches away when docked left. UI build **462**.
- **MariaDB SSL fix:** Database viewer no longer passes MySQL-only `--ssl-mode` to MariaDB clients (fixes `unknown variable 'ssl-mode=…'`). Test and Connect both run a real `SELECT 1` probe. UI build **461**.
- **Named Database connections:** Save multiple DB profiles per repo, switch from a dropdown, and Test without saving. Passwords are masked in the URL (`***`) with a separate password field (blank keeps the stored secret). UI build **459**.
- **Fix caret / Space after `:` and `()`:** Typing colon or parentheses no longer stalls the caret (completion type-through). Space works again after `)` and after YAML/Java `:` — inline ghost no longer swallows the key (UI build **458**).
- **MySQL / MariaDB in Database viewer:** Connect with `mysql://` / `mariadb://`, browse schema, run queries, and execute `.sql` files. Discovers Compose MySQL services and `DATABASE_URL`.
- **SSL for Postgres and MySQL:** CA, client certificate, and client private key (DBeaver-style) via the Database panel; maps to `psql` / `mysql --ssl-*`.
- **SSH bastion tunnel:** Jump-host local port forward (`ssh -N -L`) with bastion host/user/key, optional remote host/port, and auto local port. Works with SSL on top when the remote DB needs TLS.

### Also in 0.1.5

- Bedrock agent tab, live Bedrock model catalog, Converse API, faster AI quick fixes.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
