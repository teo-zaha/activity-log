# activity-log - your MONI companion when your outlook calendar just aint doing it

A unified activity tracker that aggregates everything you did into a single timeline — commits, GitLab events, Jira issues, calendar meetings, and system events.

The backend is now a Rust workspace:

- `core` contains shared DTOs, collectors, cache, merge logic, markdown export, and TS generation.
- `api` exposes the Rocket API.
- `cli` provides the standalone collection CLI and a separate TypeScript generator binary for Angular.
- `desktop/` packages the Angular UI as a Tauri desktop app that talks to Rust commands directly.

The Angular app stays in `dashboard/` and now consumes generated DTOs from `dashboard/src/app/generated/`.

## Screenshots

![Day view](pics/day.png)

![Work week view](pics/workweek.png)

![List view](pics/list.png)

## Data sources

| Source      | What it collects                                                           |
| ----------- | -------------------------------------------------------------------------- |
| **Git**     | Commits from all local repositories via an embedded Git library            |
| **GitLab**  | MRs, pushes, comments, reviews                                             |
| **Jira**    | Issues you worked on or reported                                           |
| **Outlook** | Calendar events from the local Outlook desktop app via native desktop APIs |
| **System**  | Boot and shutdown events from Windows Event Log and macOS session history  |

When a local git commit matches a GitLab push event (same commit SHA), they are merged into a single **git/lab** entry instead of appearing twice.

## Requirements

- Rust toolchain (`cargo`, `rustc`)
- [Node.js](https://nodejs.org) `^22.22.3`, `^24.15.0` or `>= 26` (Angular 22 requirement)
- [pnpm](https://pnpm.io) >= 10
- Windows or macOS for the local Outlook and System collectors

## Setup

```bash
git clone https://git.inform-software.com/gb100/shared/playground/tzaharin/activity-log.git
cd activity-log
pnpm install
cp .env.example .env   # fill in your credentials
```

---

## Option A — UI (web dashboard)

### Start the web dashboard + API

Run these in separate terminals:

```bash
pnpm api:start
pnpm dashboard:dev
```

The API listens on [http://localhost:3000](http://localhost:3000) by default. The last 30 days are collected automatically on startup. Use the date pickers in the header to change the range and re-collect on the fly.

If you only build the dashboard (`pnpm dashboard:build`), it produces static files under `dashboard/dist/dashboard/` and does not serve them. To preview that build, run a static file server (for example `pnpm --dir dashboard exec npx http-server dist/dashboard/browser -p 4200`) or use `pnpm dashboard:dev`.

The header has two export buttons:

- **Markdown** (`description` icon) — downloads a `.md` summary of the current filtered view
- **JSON** (`data_object` icon) — downloads the raw entries as `.json`

### Development mode (hot reload)

```bash
pnpm dev
# API server  → http://localhost:3001
# Dashboard   → http://localhost:4200  (live reload)
```

---

## Option B — Desktop app (Tauri)

The desktop app uses the same Angular UI, but swaps the transport layer from HTTP to direct Rust commands when it is running inside Tauri.

### Development

```bash
pnpm desktop:dev
```

### Debug build / packaging

```bash
pnpm desktop:build
```

### Windows installer from macOS/Linux

Cross-builds the NSIS installer for `x86_64-pc-windows-msvc` with
[cargo-xwin](https://github.com/rust-cross/cargo-xwin) (Microsoft CRT/SDK are
downloaded on first run). The `windows` flake shell provides the Rust target,
cargo-xwin, LLVM (`clang-cl`, `lld-link`, `llvm-rc`) and NSIS:

```bash
nix develop .#windows -c pnpm desktop:build:windows
```

The installer lands in
`target/x86_64-pc-windows-msvc/release/bundle/nsis/`. MSI (WiX) bundles can only
be produced on Windows; use the `Desktop Build` workflow for those.

### Regenerate app icons from the Angular favicon

```bash
pnpm --dir dashboard exec tauri icon src/favicon.svg -o ../desktop/icons
```

This writes the platform icon set into `desktop/icons/`.

---

## Option C — CLI

No dashboard build needed.

```bash
pnpm cli:collect --range today
pnpm cli:collect --range yesterday
pnpm cli:collect --range week
pnpm cli:collect --range lastweek
pnpm cli:collect --range month
pnpm cli:collect --range lastmonth
pnpm cli:collect --since 2026-03-01 --until 2026-03-18
```

### Output format

Default output is JSON. Use `--format markdown` to get a Markdown summary instead:

```bash
pnpm cli:collect --format json            # → logs/activity_log.json  (default)
pnpm cli:collect --format markdown        # → logs/activity_log.md
```

Override the output path with `--output`:

```bash
pnpm cli:collect --range week --output reports/week.json
pnpm cli:collect --range week --format markdown --output reports/week.md
```

`--output` is a CLI-only export setting. It is not part of backend `CollectOptions`.

### Provider config overrides

```bash
pnpm cli:collect --range week --set GIT_DIRS=/path/to/repo1,/path/to/repo2
pnpm cli:collect --range month --set JIRA_URL=https://jira.example.com --set JIRA_INSECURE=true
```

`--set KEY=VALUE` is repeatable and validated against Rust provider config metadata. CLI `--set` values take precedence over environment variables.

### Generate Angular DTO types

```bash
pnpm types:generate
```

This writes Rust-backed DTO types into `dashboard/src/app/generated/`.

## Dashboard styling rules

- Use named breakpoints from `dashboard/src/app/components/_dashboard-shared.scss` (for example `@include ui.down('phone')`) instead of raw `@media (max-width: ...)` when a named breakpoint exists.
- Use `var(--content-max-width)` for shared content width caps instead of hardcoded `1120px`.

## Verification

Run this before treating changes as done:

```bash
pnpm verify
```

`verify` is the repo-wide gate. It runs TypeScript generation, formatter checks, Rust checks, clippy, Rust tests, Angular type-checking, and the dashboard build.

For Windows-specific backend coverage that also runs on macOS/Linux, use:

```bash
pnpm core:test:windows
```

That script exercises host-runnable Windows logic such as provider metadata, event-log query generation, XML parsing, and event mapping. For a full Windows-target compile (including the desktop crate), run `nix develop .#windows -c pnpm win:check`. See [`docs/testing-windows-on-mac.md`](docs/testing-windows-on-mac.md) for the full Windows testing strategy.

---

## Environment variables

Copy `.env.example` to `.env` and fill in the sources you want to enable. Any source without credentials is silently skipped.
For CLI runs, you can override any provider key dynamically with `--set KEY=VALUE`.

| Variable        | Description                                                                 |
| --------------- | --------------------------------------------------------------------------- |
| `GITLAB_URL`    | Your GitLab instance URL                                                    |
| `GITLAB_TOKEN`  | GitLab personal access token                                                |
| `GITLAB_USER`   | Your GitLab username                                                        |
| `JIRA_URL`      | Your Jira instance URL                                                      |
| `JIRA_TOKEN`    | Jira access token                                                           |
| `JIRA_INSECURE` | Set to `true` to skip TLS verification (self-signed certs)                  |
| `GIT_DIRS`      | Comma-separated repo paths or parent folders (defaults to your OS home dir) |
| `PORT`          | Server port (default: `3000`, UI mode only)                                 |

Git history is read in-process; the `git` command is only asked for `user.email` when the config files the library reads have none (some Windows setups keep the global config elsewhere). Outlook and System collectors require no credentials — they read from the local Outlook app plus the platform's native event and session history APIs. Outlook is never started by the app: its calendar is collected only while Outlook is already open.

### Desktop dynamic provider settings

In the desktop app, provider configuration fields are generated from Rust provider
`config_requirements()` metadata and rendered dynamically in the Settings dialog.

- Scope: desktop (Tauri) only in this pass.
- Storage: persisted in Tauri Store (`settings.json`).
- Secret fields: values are write-only in UI/API; the frontend only receives `isSet`.
- Runtime behavior: save operations rebuild provider runtime config and refresh startup status.
- Git defaults: when `GIT_DIRS` is unset, the desktop settings UI shows your OS home directory as the default Git root.

---

## Project structure

```text
activity-log/
├── core/
│   └── src/
│       ├── collect/           Git, GitLab, Jira, Outlook, System collectors
│       ├── cache/             Per-day disk cache for API providers (GitLab, Jira)
│       ├── model/             Shared models grouped by domain/source
│       ├── runtime/           Shared collection runtime used by Rocket and Tauri
│       ├── ts.rs              Angular DTO generator
│       └── utils/             Date utilities, markdown formatter, git/gitlab merge
├── api/                       Rocket API crate
├── cli/                       Rust CLI crate
├── desktop/                   Tauri desktop crate, config, capabilities, and icons
├── dashboard/
│   ├── src/app/
│   │   ├── components/        header, filters, timeline, calendar views
│   │   ├── generated/         Rust-generated DTOs
│   │   ├── services/          app state; backend/ holds the operation table and HTTP/Tauri transports
│   │   ├── models/            Angular-only view models
│   │   └── utils/             Angular-only helper logic
├── logs/                      Generated output (gitignored)
├── Cargo.toml
├── .env.example
├── package.json
└── README.md
```
