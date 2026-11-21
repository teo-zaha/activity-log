# Provider connections: several setups per provider

Status: implemented (af4643c, 2026-11-17; all six phases), then reworked by the merged plan's steps 1–7. This is the plan as written, kept for its reasons; [As built](#as-built-2026-11-21) at the end maps its names and rules to the code, and `docs/adr/0005-provider-connections.md`, AGENTS.md and the README describe what exists. Where this plan and the code differ, the code is right. Replaces the per-distro WSL map, the "secret list" idea for ICS links and the CLI's `--set`.

## Goal

A provider can be configured more than once: two GitLab accounts, two Jira sites, several ICS calendars, Git on this computer plus Git in each WSL distribution. Every setup has its own settings, startup check, status, filter and log lines, and the whole thing is one mechanism built into the provider system, not per-provider special cases.

Non-goals: Microsoft Graph calendar (separate later kind), syncing settings between machines, per-connection colours.

## Concepts

- **Kind**: what the code implements. Stays the existing `Source` enum (`git`, `gitlab`, `jira`, `outlook`, `system`, new `calendar`); one `ProviderAdapter` per kind, unchanged in spirit.
- **Connection**: one configured setup of a kind.
  - `id: ConnectionId`, a string `kind/name` such as `gitlab/default`, `gitlab/oss`, `git/wsl-nixos`. The name is `[a-z0-9-]`, set in the file or made from the label when the connection is added, and never changed afterwards, so cache files, filters and log lines stay stable when the label is renamed.
  - `label`: free text shown in the UI ("Work GitLab", "WSL · NixOS").
  - `location`: only for Git, `local` or `wsl:<distro>`; fixed when added.
  - `values`: the kind's config keys (`GITLAB_URL`, `GITLAB_TOKEN`, …) for this connection only.
- **Cardinality**, declared by the adapter (`fn instances(&self) -> Instances`):

  | Kind     | Instances | Implicit default when nothing is configured            |
  | -------- | --------- | ------------------------------------------------------ |
  | git      | Multiple  | `git/default`, plus on Windows `git/wsl-<distro>` each |
  | gitlab   | Multiple  | `gitlab/default` (unavailable until configured)        |
  | jira     | Multiple  | `jira/default` (unavailable until configured)          |
  | calendar | Multiple  | none; added in settings                                |
  | outlook  | Single    | `outlook/default`                                      |
  | system   | Single    | `system/default`                                       |

## Configuration: one file for every frontend

The CLI, the API server and the desktop app read and write the same file. There is no string encoding of connections in variable names.

- Location: the OS config directory, `activity-log/config.toml`
  - macOS `~/Library/Application Support/activity-log/config.toml`
  - Windows `%APPDATA%\activity-log\config.toml`
  - Linux `$XDG_CONFIG_HOME/activity-log/config.toml`
- Override: `--config <path>` on the CLI and the API server, or `ACTIVITY_LOG_CONFIG`.
- Format: one TOML table per connection, an array per kind. Field names are the field names of the provider's config struct (`url`, `token`, `dirs`), not the old upper-case keys.

  ```toml
  [[gitlab]]
  label = "Work GitLab"
  url = "https://gitlab.example.com"
  token = "glpat-…"
  user = "me"

  [[gitlab]]
  label = "OSS"
  url = "https://gitlab.com"
  token = "glpat-…"
  user = "me"

  [[git]]
  label = "This computer"
  dirs = ["~/Projects"]

  [[git]]
  label = "WSL · NixOS"
  wsl = "NixOS"
  dirs = ["~", "/srv/tools"]

  [[calendar]]
  label = "Team"
  ics_url = "https://outlook.office365.com/owa/calendar/…/calendar.ics"
  ```

- Optional `name` per table sets the id (`gitlab/oss`); otherwise it is the slugged label, with `default` for the first connection of a kind. The id never changes when the label is edited.
- Which connections exist, per kind, first match wins:
  1. The file has the kind (`[[gitlab]]`, or `gitlab = []` for none): exactly those connections.
  2. The environment has the kind's old keys (`GITLAB_URL`, `GITLAB_TOKEN`, …, also from `.env`): one default connection from them. Existing `.env` setups keep working; there is no way to describe a second connection in the environment.
  3. The implicit defaults from the table above.
- Removing an implicit connection (a WSL distribution) writes the remaining ones to the file, so nothing comes back on its own.
- Secrets live in the file like they live in `settings.json` today. The file is written with mode `0600` on Unix. Moving secrets to the OS keychain is a later step.
- The desktop and the CLI write with `toml_edit`, so comments and order in a hand-edited file survive.
- Migration: on first start, the desktop turns its `settings.json` (`providerConfig`) into `config.toml`, one connection per kind, with a `WSL_GIT_DIRS` map becoming one `[[git]]` per distribution. `settings.json` is left in place as a backup and no longer read.

The removed settings: `WSL_GIT_DIRS`, `WSL_DISTROS`, the `Map` config shape and its DTOs and editor (none were released), and the CLI's `--set KEY=VALUE`.

## Core changes

### Model (`core/src/model/`)

- `common/connection_id.rs`: `ConnectionId(String)` with `kind()`, `slug()`, parsing and slugging.
- `common/connection_info.rs`, replacing the role of `ProviderInfo`: `{ id, source, label, location?, status, usable, supportedOs, reason?, checkedAt }`. `ProvidersResponse` lists connections plus `kinds: [{ source, instances, supportedOs }]`.
- Every entry type (`GitEntry`, `GitLabEntry`, `JiraEntry`, `OutlookEntry`, `SystemEntry`, new `CalendarEntry`) gains `connection: String`. Adapters leave it empty; the runtime stamps it after collection (`ActivityEntry::set_connection`), so no collector has to know about connections.
- `CollectedActivitySummary` becomes `{ counts: [{ connection, source, count }], mergedCount }` with `count_for(connection)`.

### Config (`core/src/provider/provider_config/`)

- `ConnectionSpec { id, label, location, values: Arc<dyn ConfigSource> }` and a `ConnectionSource` trait (`fn connections(&self) -> Vec<ConnectionSpec>`).
- Each config key gets its file field name (the struct field in the `provider!` config declaration); `ProviderConfig`, typed `from_config`, validation and fingerprints stay as they are and run per connection.
- New `core/src/settings/`: `ConfigFile` (load, save with `toml_edit`, path resolution), `FileConnections` (the resolution rules above, as a `ConnectionSource`), add/rename/remove/set operations shared by the desktop and the CLI, and the `settings.json` migration.
- Remove `ProviderConfigShape::Map`, `ConfigMap`, `parse_config_map`, `encode_config_map`, `clean_config_map`.

### Runtime (`core/src/provider/provider_runtime.rs`, `core/src/runtime/`, `core/src/lib.rs`)

- One `ProviderRuntime` per connection; the adapter `Arc` is shared. `inspect_provider_runtimes_with(&dyn ConnectionSource)`.
- `ProviderContext` gains `connection: ConnectionSpec` (id, label, location) and `shared: Option<Arc<dyn Any + Send + Sync>>`, one value per kind and runtime build, created by `ProviderAdapter::shared_state()`.
- New optional hook `ProviderAdapter::finish_kind(&self, shared, entries) -> entries`, run once per kind after all its connections collected. Kinds that use it must not use the day cache (asserted in debug builds).
- Diagnostics use the connection id as category (`[gitlab/oss]`); the startup "not configured" message names the label.
- `CollectionRuntime` and `collect_activity_*` take a `ConnectionSource` instead of a `ConfigSource`.

### Cache (`core/src/cache/`)

- File name `{connection-id}-{fingerprint}.json` (`/` replaced by `_`), `FILE_VERSION = 3`, so old files are ignored and removed by the existing cleanup.

### Git (`core/src/provider/git/`, `core/src/collect/git/`)

- Config keys: `GIT_DIRS` only. The location comes from the connection, not from a key.
- `collect`: `local` runs the existing local collector; `wsl:<distro>` runs the existing WSL script for that distro with the connection's folders (`WslTarget { distro, dirs }` stays as it is now).
- Startup check per connection: local opens a repository; WSL lists repositories in that distro. One broken distro no longer affects the others.
- Email matching across connections:
  - `collect` returns commits by every author and records the emails it found (repo `user.email`, global email, distro global email) in the kind's shared `GitIdentities`.
  - `finish_kind` keeps commits by any recorded email, ignoring case, and logs which emails were used. Repositories without an email of their own are matched by the others, as today.
- Implicit defaults on Windows: `git/default` and one `git/wsl-<distro>` per user distribution from the registry (Docker, Rancher, Podman skipped), each searching `~`.

### Calendar (`core/src/provider/calendar/`, `core/src/collect/calendar/`, new kind)

- Config: `ICS_URL` (Password: write-only in settings, never logged). Accepts `https://`, `webcal://` (read as `https://`) and an absolute local `.ics` path or `file://` URL; `http://` is rejected.
- Fetch with the shared reqwest client; errors use `reqwest::Error::without_url()` so the link never reaches logs.
- Parse with `calcard` (`default-features = false`): it resolves VTIMEZONE and Windows zone names, RRULE, RECURRENCE-ID overrides and EXDATE. Each recurring series (grouped by UID) is expanded on its own with a cap of 10,000 occurrences, because `expand_dates` shares one limit across all series.
- Times without a zone are read in the machine's local zone (`iana-time-zone`), the documented exception to "instants only". All-day events and `STATUS:CANCELLED` are skipped.
- `CalendarEntry { source: "calendar", connection, timestamp, title, location?, organizer?, durationMinutes }`, kept when the event overlaps the range like Outlook.
- No day cache: a feed is one request whatever the range.

## Desktop (`desktop/src/`)

- `provider_config/`: reads and writes through `core::settings` instead of `tauri-plugin-store`; runs the `settings.json` migration once.
- Commands:
  - `get_provider_config` → `{ kinds: [{ source, instances, locations? }], connections: [{ id, source, label, location?, removable, fields }] }`. `locations` lists the installed WSL distributions for Git on Windows.
  - `save_provider_config { connection, label?, values }`.
  - `add_connection { source, label, location? }` and `remove_connection { connection }`.
  - Each returns the new snapshot and rebuilds the runtime, as saving does now.
- Remove the map handling added for `WSL_GIT_DIRS`.

## API server

- Reads the same `config.toml` (`--config`, `ACTIVITY_LOG_CONFIG`). Settings stay read-only over HTTP.
- Rebuilds its runtime when the file's modification time changes, checked before each collection, so edits from the CLI or the desktop apply without a restart.
- `GET /api/providers` returns the new `ProvidersResponse`; `POST /api/collect` is unchanged.

## CLI

Collecting stays as it is (`activity-log --range week`). Connections are managed with subcommands instead of strings:

```text
activity-log sources                          list connections with status
activity-log sources add gitlab               asks for label, url, token (hidden), user
activity-log sources add git --wsl NixOS      a Git location inside WSL
activity-log sources set gitlab/oss url https://gitlab.com
activity-log sources set gitlab/oss token     asks for the value, hidden
activity-log sources set git/default dirs ~/Projects ~/work
activity-log sources rename gitlab/oss "Open source"
activity-log sources remove gitlab/oss
activity-log config path                      prints the config file path
activity-log config edit                      opens it in $EDITOR
```

- A connection can also be named by its name alone (`oss`) when that is unique.
- Values are checked like in the desktop (URL, number, list) before the file is written.
- Markdown export prefixes entries with the connection label when a kind has more than one connection.

## Dashboard (`dashboard/src/app/`)

- Types: everything that keys on `Source` for providers switches to `ConnectionId`; entries keep `source` for picking the UI adapter.
- Sidebar: one filter per connection, grouped under the kind when it has more than one; `activeSources` becomes `activeConnections`.
- Settings dialog: the nav lists connections grouped by kind, with "Add GitLab account", "Add Jira site", "Add calendar", "Add Git location" (Windows: "This computer" or a WSL distribution from `locations`). The source pane is scoped to one connection, gets a label field and "Remove", and keeps the existing field editor (single values and lists). `SettingsSelection` becomes `connection:${id}`.
- Entry adapters: `connection` and `connectionLabel` on `EntryUiAdapter`; the badge and detail panel show the label when the kind has more than one connection; search includes the label.
- Calendar kind: `providers/adapters/calendar.ts`, icon, colour token in `_dashboard-shared.scss`. Day and week views pick blocks by `calendarDurationMinutes !== null` instead of `source === 'outlook'`.
- Remove the map field editor from the source pane.
- Fixtures and the mock backend move to connections.

## Phases

Each phase ends green on `nix develop -c pnpm verify` and `nix develop .#windows -c pnpm win:check`, and is one commit (or a few).

0. **Commit what is open**: the Outlook crash fix (`GetFirst`/`GetNext`), the removed Git log line, and the WSL folder work (its collector part is reused; its `Map` setting is removed in phase 3).
1. **Core connections and the config file**: `ConnectionId`, `ConnectionSpec`, `ConnectionSource`, `core::settings` (`config.toml` load and save, resolution rules, environment fallback), runtime per connection, `connection` stamped on entries, summary per connection, cache naming. GitLab and Jira become multi-instance. The API server reads the file and reloads on change. Tests: resolution order, field names, `gitlab = []`, comments surviving a save, cache file names, two GitLab connections collecting independently.
2. **Shared kind state and Git locations**: `shared_state`, `finish_kind`, Git connections by location, identities across connections, implicit WSL defaults. Tests: identity pooling across a local and a WSL connection, one failing distro leaving the others available, implicit defaults from a fake registry list.
3. **Desktop and CLI on the file**: desktop commands through `core::settings`, the `settings.json` migration, add/remove/rename, snapshot with kinds and locations; the CLI `sources` and `config` subcommands, `--set` removed; remove the `Map` shape and its DTOs. Tests: migration of a real `settings.json` (including `WSL_GIT_DIRS`), removing an implicit connection, secrets never in the snapshot, CLI commands against a temp config file.
4. **Dashboard**: settings nav and pane per connection with add/remove/rename, sidebar per connection, badges, fixtures. Tests: add and remove flows, label shown only with several connections, filters per connection.
5. **Calendar kind**: provider, collector, entry, dashboard adapter. Tests: Outlook-style feed with Windows zone names, a weekly series with an override and an EXDATE, an unbounded daily series, floating times, all-day and cancelled events skipped, URL never in an error message.
6. **Docs**: README (sources, `config.toml` with examples, CLI `sources` commands, environment fallback), `.env.example`, ADR `docs/adr/0005-provider-connections.md`.

## Risks

- **Contract churn**: `ProviderInfo`, summary and every entry change shape. Only this repo consumes them; generated DTOs and the dashboard move in the same phases.
- **Settings migration**: a bad migration loses tokens. Mitigated by leaving `settings.json` untouched and by migration tests on real files.
- **Secrets in a plain file**: same exposure as `settings.json` today; mode `0600` on Unix, keychain later.
- **Windows-only paths**: WSL connections, registry defaults and Outlook still need a run on real Windows after phases 2, 3 and 5.
- **Email pooling**: `finish_kind` sees only what collected in this run, as today; a connection that failed contributes no emails, so its absence can drop commits elsewhere. Logged.

## As built (2026-11-21)

What differs from the plan above, by section. Paths are in the repository; "step n" is a step of the merged review plan that reworked the first build.

- **Concepts**: a connection is `Connection` (`core/src/provider/connection/`), not `ConnectionSpec`; its id is `ConnectionId { source, name }`, an object on the wire, not a `kind/name` string (the dashboard and the CLI spell it `kind/name`). `ProviderInfo` is kept and gained `connection`, `label`, `wslDistro`, `problem` and `recheckable`; there is no `ConnectionInfo`. A kind declares `instances()`, and work across a kind's connections is `collection_state()` and `finish_kind()` (`core/src/provider/provider_adapter.rs`), not `shared_state()`; `finish_kind()` also runs provisionally (`complete: false`) while connections of its kind are pending.
- **Which connections exist**: only what `config.toml` lists. The environment fallback and the implicit defaults (`git/default`, the WSL distributions, `outlook/default`, `system/default`) were dropped before release: a kind the file does not mention has no connections, and sources exist only once added. Without an OS config folder and without `ACTIVITY_LOG_CONFIG` or `--config` there are no sources and every save fails with `settings::NO_LOCATION`.
- **The file**: a repeated name is numbered (`work-2`) with a warning; each table gets a reserved, non-secret `instance` id on the first edit (user decision, 2026-11-19); a table that cannot be read is skipped with a warning. Every writer goes through `settings::edit_config` (`core/src/settings/write/`, step 1): a lock, a re-read, the writer's `Precondition`, and an owner-only publish through `core/src/platform/private_file/` (more than mode `0600`: a location that cannot keep the file private is refused before anything is written). `config edit` gives the editor a private copy and saves it only if the file did not change meanwhile.
- **Migration**: the `settings.json` migration was removed with the 1.0.0 decision (`docs/open-decisions-design.md`, "Version 1.0.0"); there was no earlier release to migrate from.
- **Runtime**: one `ProviderRuntime` per connection under one `Host` per process (`core/src/runtime/host/`, step 5), not `CollectionRuntime`. Entries carry `connection` as a `ConnectionName`: unstamped in a collector's entry and stamped with a `ConnectionStamp` by `ProviderRuntime::collect`, so no entry is stored or sent without one. `CollectedActivitySummary` is `{ connections: [{ connection, count }], mergedCount }` with `entry_count_for`.
- **Cache**: one file per connection and verified principal, `{source}-{name}-{scope:016x}.json`, file version 6 (`ProviderRuntime::cache_scope`: the safe fingerprint plus the check's `RemoteIdentity`, steps 2 and 3); without a verified identity the connection is collected uncached.
- **Calendar**: `CalendarEntry` and Outlook entries flatten one `MeetingBody` (duration, busy status, meeting weight, description, join link); the day and week views tell meetings by `meetingWeight`, not by `calendarDurationMinutes`. The collector is split into feed, expansion and entry modules (`core/src/collect/calendar/`, d2aa3fd).
- **Desktop**: the snapshot and its edits live in core (`core/src/settings/view/`: `build_snapshot`, `normalize`, `prepare_save`, `apply_save`, `apply_add`); `desktop/src/provider_config/` keeps the file's path and the `SnapshotRegistry` of recent snapshots. Commands: `get_provider_config`, `save_provider_config` and `remove_connection` with `basedOn` (a stale one is a `conflict`), `add_connection` (answers the `created` id), `check_provider_config` (a save's issues without saving). Every answer that installs carries an `InstallReceipt`, and every failure is a typed `BackendError`.
- **API server**: it reloads the file when its content id changes (`Host::reload_if_changed`, before the state routes) and watches it every 2 s (`watch_config`), not by modification time.
- **CLI**: `sources`, `sources add|set|unset|rename|remove`, `config path`, `config edit` (`cli/src/sources/`, `cli/src/main.rs`); values are checked by core's `normalize`, as the desktop checks them.
- **Dashboard**: `SettingsSelection` is `<source>/<name>` (a `ConnectionKey`), not `connection:${id}`; the sidebar's source chips and unavailable sources are one per connection (`components/source-list/`); the settings pane is one per connection with its health and Check again (`components/settings-dialog/source-pane/`); per-source UI adapters live in `providers/`.
