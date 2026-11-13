# Switchover from Bibliotek

liotek replaced Bibliotek on port 5678 on 2026-11-13 at 04:26 (CET). Bibliotek was stopped
at 04:25 and disabled in launchd; its binary, plist, database, config, and Tigris key were
left untouched.

## Rollback

```bash
launchctl bootout gui/$(id -u)/com.gwuah.liotek
launchctl enable gui/$(id -u)/com.gwuah.bibliotek && launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.gwuah.bibliotek.plist
```

Bibliotek comes back exactly as it was at 04:25 on 2026-11-13. Anything written in liotek
after that would have to be redone in Bibliotek.

## Kept for rollback and audit

- `~/bibliotek-archive/bibliotek-production-20261113T025701Z.zip`: Bibliotek's binary,
  config, plist, logs, source, and a consistent copy of its database. It was taken before
  the metadata cleanup reached production, so for a rollback `~/.bibliotek/` as it stands
  is the current state.
- `~/.liotek/rehearsal/`: the rehearsal and final snapshots and every import report,
  including `final-migration-report.json`.

## After the retention period (from 2026-12-13)

Each step only with the owner's OK:

- revoke Bibliotek's old Tigris key;
- delete the `liotek-preview` bucket and its keys;
- remove Bibliotek's plist and binary, and archive its repo;
- decide what to do with Bibliotek's Turso database.
