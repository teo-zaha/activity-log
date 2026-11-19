# AGENTS.md

This file defines the working rules and architecture expectations for agents editing this repository.

## Overview

`activity-log` is split into a Rust backend workspace plus an Angular frontend:

- `core/`: shared Rust library with models, collectors, cache, merge logic, markdown export, and TypeScript generation
- `api/`: Rocket HTTP API
- `cli/`: Rust CLI package containing the user-facing collection binary and the TypeScript generator binary
- `dashboard/`: Angular application that consumes generated backend DTOs
- `desktop/`: Tauri desktop shell that wraps the Angular app and talks to Rust commands

The backend contract is owned by Rust. The frontend consumes generated DTOs and should not redefine backend models by hand.

## Project Architecture

### Rust workspace

- `core/src/model/`: shared models grouped by domain/source
- `core/src/collect/`: one collector per source
- `core/src/lib.rs`: orchestration entry point used by both `api` and `cli`; stamps `connection` on entries and numbers ids a source repeats (`id#2`), so `(source, connection, id)` names one entry
- `core/src/aggregator/`: builds an `ActivityLog` from per-connection results: folds GitLab pushes into their git commits (by `GitLabEntry.pushedCommit`), sorts, counts, and carries the previous log's entries for connections still pending (`carry_over`, `carried_for`)
- `core/src/runtime/`: `CollectionRuntime`, the collection state shared by `api` and `desktop`: the running or last log, startup checks, `start_collection` (a running collection of the same range goes on, another range replaces it) and `take_over` after a settings change. A new run shows what the log on screen had for each connection until that connection is in again. `recheck` runs one connection's startup check again and then collects its kind again for the range on screen (`Scope::Kind`; the other kinds keep their entries, a running collection starts over, a failed one runs whole). A connection whose last collection failed is listed `degraded` with the error as its reason
- `core/src/provider/`: one adapter per source kind (`provider!` declarations: supported OS, config keys, cache policy, startup check, collect), the per-connection `ProviderRuntime` and the typed startup problem (`ProviderInfo.problem`)
- `core/src/provider/{jira,gitlab}/api/`: `JiraApi` and `GitLabApi`, the only way the Jira and GitLab checks and collectors reach their servers (`remote_api/` holds the shared client). Paths are built from segments and query values through `query_pairs_mut`, never with `format!`; credentials go only to the configured origin (a redirect elsewhere stops, a URL with a user name or password leaves the connection not configured); every failure goes through `remote_error/` (`ApiError`, `remote_message`: the server's own message only, secrets redacted, addresses without user info or query, control characters escaped, at most 300 characters; error bodies are read only in part); a refusal (401/403) is decided by its status, a body that breaks off is a transfer failure, never an unreadable answer. The startup check parses its identity request (Jira `/myself`, GitLab `/user`) into `IdentityEvidence`, which `ProviderRuntime::identity()` keeps; for GitLab that is the token's owner, not the configured user whose events are collected
- `core/src/platform/`: OS integrations shared by collectors (Windows COM/event log, macOS AppleScript); `platform/private_file/` writes files only the current user can open (settings, the cache): a verified owner-only staging folder beside the target, a file created private inside it, and publication over the target through folder handles. A location that cannot keep files private (FAT, exFAT, network shares, a folder other accounts can write to) is `Unprotectable` before anything is written. Its OS modules are chosen with a compile-time `imp` alias
- `core/src/export/`: renders an `ActivityLog` as JSON or Markdown (`render_export`) for the CLI, the desktop save dialog and `POST /api/export`; days and times in the zone the caller passes
- `core/src/ts.rs`: generated TypeScript contract source
- `core/src/utils/`: date utilities, markdown formatting, merge logic, `stable_id` (FNV-1a) for entries whose source has no id
- `core/src/cache/`: per-day disk cache for remote API providers. A provider opts in with `cache: CachePolicy::api()` in its `provider!` declaration; the runtime then fetches only the runs of missing or stale days. One file per connection and verified principal (`ProviderRuntime::cache_scope`: the safe fingerprint, URLs as their endpoint, plus the check's `RemoteIdentity`); without a verified identity, or with a query or fragment in a URL, the connection is collected uncached (a URL with a user name or password leaves the connection not configured, so it is not collected at all). A folder that fails `Folder::check_private` is neither read nor written. Each run of stale days is written as soon as it is fetched: without a lock during the fetch, then under `{file}.lock` re-read, merge only its days (newer `fetched_at` wins, ties keep the stored day) and publish an owner-only file through `platform::private_file`; a file that does not decode or is of a later version is left alone (a later version's file is reported by each collection that finds it). The folder is checked again once the lock is held. Cache I/O runs on `spawn_blocking`. Files live in the OS cache dir (`ACTIVITY_LOG_CACHE_DIR` overrides it), never the working directory
- `core/src/diagnostics/`: in-memory diagnostic log (ring buffer) shown in the settings Logs pane; use `diagnostics::info/warn/error` instead of `eprintln!` so messages are visible in the desktop app
- `core/src/provider/connection/`: a provider kind can be configured more than once; each setup is a connection (`ConnectionId { source, name }`) with its own values, runtime, status, cache file and log scope. An adapter declares `instances()` (one or several connections) and optionally `collection_state()`/`finish_kind()` for work across its connections (Git matches emails found in all of them). Collectors stay unaware of connections: the runtime stamps `connection` on entries and scopes log lines. Each connection runs its own check → collect pipeline (`collect_progressively` in `core/src/lib.rs`), and the log is rebuilt each time one settles, so `finish_kind()` also runs provisionally (`complete: false`) while connections of its kind are pending and must then return a subset of its complete result
- `core/src/settings/`: `config.toml`, the connections file shared by the CLI, the API server and the desktop app (`[[gitlab]]` tables, field names without the kind prefix). It is the only list of sources: a kind it does not mention has no connections, and environment variables never configure a source. Connection names are compared as slugs; a repeated name is numbered (`work-2`) in file order with a warning, and the next edit writes it. Read it with `load_connections`/`ConfigFile::logged_connections`, which send its warnings to the Logs pane; a table it cannot read is skipped with a warning, and edits address tables by their place in the file. Every writer goes through `settings::edit_config`: under the `config.toml.lock` lock (5 s wait, then "busy") it reads the file again, checks the writer's `Precondition` (an `EditBase` taken before any question; account fields and the table's reserved `instance` id tell a changed or re-added connection apart), applies the change, gives each table it wrote an `instance` if it has none, checks the result reads, and publishes it through `private_file` unless the bytes are unchanged. Readers never lock or write. `config edit` (`settings::edit_externally`) gives the editor a copy in a private session folder and publishes it only if the file did not change meanwhile. Parse errors name the line and column, never the file's text. Without an OS config folder and no `ACTIVITY_LOG_CONFIG`/`--config` there are no sources and every save fails with `settings::NO_LOCATION`; nothing falls back to a temporary folder

### API

- `api/src/lib.rs`: Rocket routes, state, startup collection
- `api/src/reload/`: rebuilds the runtime when `config.toml` changes, so edits from the CLI or the desktop app apply without a restart
- `api/src/main.rs`: runtime bootstrap

Routes:

- `GET /api/activity`: `202` with a partial `ActivityLog` while a collection runs (`meta.pending` names the connections still to come), `200` with the complete log, `200` with `CollectionFailedResponse` after a failure, `503` before the first collection
- `GET /api/providers`
- `POST /api/collect`: starts the collection and answers `202` at once with its first snapshot; a collection of the same range that already runs is answered with its snapshot, one of another range is stopped
- `POST /api/connections/recheck` with a `ConnectionId`: runs that connection's startup check again (`CollectionRuntime::recheck`) and answers `200` with `ProvidersResponse`, the connection checking; `404` for one the server does not have
- `POST /api/export`: `ExportRequest` (format, `[start, end)`, IANA `timeZone`, entries) → `200` with `ExportFile` (`fileName`, `content`), `400` with `ErrorResponse` for an unknown zone, a bad range or a body that is no request, `413` above 64 MiB (this route reads its body itself; Rocket's JSON limit is 1 MiB)
- Every range (collect, export, CLI) is checked by `CollectOptions::check`: `end` after `start`, at most `CollectOptions::MAX_DAYS` (ten years); the dashboard's `DeepLinkService` drops a link's range outside that
- `GET /api/logs`, `DELETE /api/logs`

### Desktop

- `desktop/src/commands/`: Tauri commands mirroring the API (`save_export` renders through `core::export` and opens a save dialog; `add_connection` answers with the `created` id; `recheck_connection` mirrors `POST /api/connections/recheck`)
- `desktop/src/provider_config/`: the settings pane's view of `config.toml` and its edits
- `desktop/src/deep_link/`: `activity-log:` links from the OS, see the Angular routes below
- `desktop/src/window_state/`: `tauri-plugin-window-state` from Rust only (no capability) saves size, position, maximized and fullscreen; size, position and maximized are restored while the window is hidden, fullscreen once it is shown; visibility and decorations are never saved. `WindowChromeService` hides the web view's own right-click menu in the desktop app except in text fields and over the text selected before the click (`utils/context-menu.ts`; `index.html` covers the splash until it starts); `desktop/src/devtools/`: `ACTIVITY_LOG_DEVTOOLS=1` opens the inspector in a debug build
- `desktop/tauri.windows.conf.json`: on Windows the window has no title bar (`decorations: false`). `index.html` marks the page `html.frameless`; the dashboard draws the caption buttons (`components/window-controls/`, `services/window-chrome.service.ts`), the header is the title bar, and pages over the window leave `--window-controls-width`/`--window-controls-height` free for the buttons. Windows 11 shows its snap layouts only over a window that answers `HTMAXBUTTON`, which the page cannot, so `desktop/src/window_chrome/` puts an invisible child window over the dashboard's maximize button (`attach_maximize_button`); it maximizes on click and reports the pointer as the `maximize-button` event, which the button shows instead of `:hover`. On macOS the traffic lights sit over the header (`titleBarStyle: Overlay`)

### CLI

- `cli/src/main.rs`: argument parsing and collection execution

Important behavior:

- `cargo run -p cli --bin generate-ts --` is the explicit sync point for Angular contract files
- `api` and `cli` must use `core` for all shared logic

### Angular

- `dashboard/src/app/generated/`: generated backend DTOs, not handwritten
- `dashboard/src/app/services/`: data access and app state; `services/backend/` holds the operations (`backend-contract.ts`) and the Tauri and HTTP transports
- `dashboard/src/app/providers/`: per-source UI adapters (`EntryUiAdapter`: title, meta, badges, detail fields, `trackKey` from the backend id) and the source catalog
- `dashboard/src/app/components/`: UI; `components/ui-status/` holds the status dot, pill and label shared by the sidebar and settings
- `dashboard/src/app/models/`: Angular-only view models
- `dashboard/src/app/utils/`: Angular-only helper logic
- `dashboard/src/app/app.routes.ts`: routes and router setup; `utils/app-url.ts` reads and builds the URLs

Routes (views are lazy-loaded into the `<router-outlet>`):

- `/timeline` (`/` and unknown paths redirect here, keeping the query); `TimelineReuseStrategy` in `app.routes.ts` keeps the timeline alive while another view shows, with its scroll position
- `/day/:date`, `/workweek/:date`, `/week/:date` with `date` a `YYYY-MM-DD` local day; a missing or invalid day redirects to today
- Query parameters: `q` (Timeline filter, dropped on a view switch), `from`/`to` (collection range; collected at startup when both are set), `settings` (`home` | `general` | `logs` | `add` | `<source>/<name>`, the settings over the view; `home` is the phone's list of sections and shows General on wider screens), `event` (an entry's `trackKey`, `<source>/<connection>/<id>`: its details over the view), `events` (repeated: overlapping events listed together; with `event`, the one open in the list)
- Views, days and settings open/close push history entries; filter typing, range changes and moving between settings sections beside the list replace the entry. On a phone a section opened from the settings home pushes one, so Back returns to the list; closing the settings goes back past what they pushed (`settingsDepth`), or replaces the URL after a link. Opening details, and an event from a group's list, push one; stepping through a group replaces it; closing goes back past what the details pushed (`detailDepth` in the history state), or replaces the URL when they came from a link or a reload. Details the entries do not hold show as loading until the log is in, then as not found
- Settings show in a dialog and on a phone as the last tab of the bottom bar: in the main content in place of the view, which stays alive underneath, hidden, under the same header without the date range and the source chips, which filter views (`SettingsPageHostComponent`; the list of sections, then one section). A resize across the phone breakpoint moves open settings between the two: the section is in the URL and unsaved source edits in `SourceDrafts`, so nothing is lost. Pages share `components/page-header/`
- On a phone the details are a page of their own, slid in and out by a view transition (`view-transitions.ts`: `page-in`/`page-out` on `event-detail`), and so are the settings' sections over their list (`settings-page`); opening and leaving the settings is a tab switch and keeps still. Other views switch with a cross-fade between calendar views only. The router would start a view transition for every navigation; `provideViewTransitionGate` starts only the ones that animate, since starting and skipping one costs WebKit about 0.2 s
- Deep links (`activity-log://day/2026-11-17?q=x`) and outside links go through `DeepLinkService.open(url)`, which validates them against the routes. In the desktop app `desktop/src/deep_link/` receives them (deep-link and single-instance plugins), holds the launch link until the dashboard calls `take_launch_deep_link` and sends later ones as the `deep-link` event; `services/deep-link-inbox.service.ts` listens through `BackendTransport.listenForDeepLinks` and hands each to `DeepLinkService.open`. Links are untrusted input: they only navigate
- Path URLs, not hash URLs: the Tauri asset protocol serves `index.html` for unknown paths and `ng serve` falls back to it; the Rocket API does not serve the dashboard

## Rust Rules

- Keep shared domain and API types in `core/src/model/`, not in `api` or `cli`
- Put each model struct in its own file under the matching domain/source folder
- For Rust modules with local tests, colocate them as a folder pair: `mod.rs` plus `tests.rs` in the same directory
- Do not keep Rust code in `foo.rs` while its tests live in `foo/tests.rs`; move the implementation into `foo/mod.rs` instead
- Prefer moving reusable logic into `core` instead of duplicating behavior in `api` or `cli`
- Preserve wire compatibility for public API fields unless the task explicitly changes the contract
- The backend deals in instants: requests carry a `[start, end)` range of RFC 3339 instants and entries carry RFC 3339 timestamps. `core` and `api` never read the machine's time zone, with the exceptions below. `core` may group by local day only in an IANA time zone the request carries (the dashboard sends the browser's zone, as `ExportRequest.timeZone`; the CLI passes the terminal's), as `core::export` does; the dashboard and the CLI turn local days into instants
- Exceptions: sources that report local wall-clock time without an offset, converted where they are parsed: macOS `last`, and ICS times without a zone (floating times), which `collect/calendar` reads in the machine's zone (`local_zone()`). Floating times mean the wall clock of whoever's calendar it is, the same kind of input as `last`; passing a zone in would put the reader's zone into collections and their cache, which are shared by every reader
- Use comments sparingly and only where behavior is not obvious
- Keep PowerShell and subprocess integrations parity-focused unless the task explicitly replaces them
- For OS-specific Rust implementations, gate child modules at the parent boundary with `#[cfg(target_os = "...")]`
- Name OS-specific child modules by platform (`windows`, `macos`, `linux`) and keep runtime dispatch in the parent module
- In parent modules, prefer a `*_impl` alias/stub pattern so runtime dispatch never calls raw `windows`/`macos`/`linux` modules directly
- Keep shared cross-platform logic in shared modules; only split into OS-specific modules when behavior is genuinely platform-specific

## Angular and TypeScript Rules

- Always maximize type safety
- No business logic in the dashboard: provider data, domain rules and derived results (dedupe and merging, what counts as meeting time, link finding, export formats, status semantics) are computed in Rust (`core`) and delivered through DTOs; the dashboard only formats, lays out, renders and filters what it shows. Examples: `meetingWeight` on meetings, `ProviderInfo.problem`, entry `id`s, carried entries in partial logs, and exports rendered by `core::export`
- Aggregating what the dashboard shows is presentation and stays there: adding up the meeting time of the visible entries per browser-local day (`utils/meeting-time.ts`, the interval union over entries by their `meetingWeight`), counting or grouping visible entries
- Do not handwrite backend DTOs in Angular
- Do not add compatibility wrapper files that only re-export or re-import generated backend DTOs
- Keep generated files under `dashboard/src/app/generated/`
- Keep Angular-only types and helpers separate from generated backend contract types
- Prefer explicit imports from generated files for backend shapes
- Prefer discriminated union narrowing over `as` casts whenever possible
- Avoid `any`, avoid weakening types for convenience, and keep helper APIs typed as specifically as possible
- Preserve existing Angular patterns unless the task explicitly calls for a redesign
- State that should survive a reload or be linkable lives in the URL: routes for views, query params for filter, range and settings. Services read it from the router; components navigate instead of setting that state. Transient UI state (selection, hover, hidden sources, zoom) stays in services
- Browser APIs, element observers, event listeners, timers and router state use `@signality/core` composables; hand-write only when Signality does not cover the case, and say why in a comment. The version is pinned exactly while it is 0.x; read the release notes before bumping
- For responsive SCSS in `dashboard/`, use named breakpoints from `dashboard/src/app/components/_dashboard-shared.scss` (for example `@include ui.down('phone')`) instead of raw `@media (max-width: ...)` where a named breakpoint exists
- For shared dashboard content width caps, use `var(--content-max-width)` instead of hardcoded pixel values (for example `1120px`)

## Script Rules

Use the namespaced root scripts so intent is obvious:

- `pnpm api:*`
- `pnpm cli:*`
- `pnpm core:*`
- `pnpm desktop:*`
- `pnpm dashboard:*`
- `pnpm types:generate`
- `pnpm verify`

Avoid reintroducing vague root scripts like `start`, `build`, or `collect` when a scoped script is clearer.

## Verification Rule

Always run the repo verification gate before considering a change complete:

```bash
pnpm verify
```

If `pnpm verify` cannot be run in the current environment, say so explicitly in the final handoff.

## Change Rules

- Do not reintroduce the old TypeScript backend
- Do not create thin migration or compatibility files when the code can be migrated directly
- Update docs when architecture, scripts, or workflow change
- Prefer deleting obsolete files over keeping dead compatibility layers
- Treat `desktop/gen/` as generated Tauri schema output: do not hand-edit it and keep it ignored
