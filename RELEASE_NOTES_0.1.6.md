Reaper 0.1.6 (build 457) — macOS split release.

**Install:** download the **DMG for your Mac** below. Ignore GitHub's automatic "Source code (zip)" and "Source code (tar.gz)" links — those archives are empty placeholders and are not distributable builds.

| Mac | Download |
|-----|----------|
| Apple Silicon (M1/M2/M3/M4) | `reaper-0.1.6-macos-arm64.dmg` |
| Intel (2015–2020 MacBook Pro, iMac, etc.) | `reaper-0.1.6-macos-x86_64.dmg` |

Drag Reaper.app to Applications, then launch.

### What's new

- **MySQL / MariaDB in Database viewer:** Connect with `mysql://` / `mariadb://`, browse schema, run queries, and execute `.sql` files. Discovers Compose MySQL services and `DATABASE_URL`.
- **SSL for Postgres and MySQL:** CA, client certificate, and client private key (DBeaver-style) via the Database panel; maps to `psql` / `mysql --ssl-*`.
- **SSH bastion tunnel:** Jump-host local port forward (`ssh -N -L`) with bastion host/user/key, optional remote host/port, and auto local port. Works with SSL on top when the remote DB needs TLS.

### Also in 0.1.5

- Bedrock agent tab, live Bedrock model catalog, Converse API, faster AI quick fixes.

**Tip:** opening the .dmg repeatedly mounts a new Finder volume each time — eject old Reaper drives or run `scripts/eject-reaper-dmgs.sh`.
