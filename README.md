<p align="center">
  <img src="dashboard/src/favicon.svg" width="96" alt="Activity Log logo">
</p>

<h1 align="center">Activity Log</h1>

<p align="center">
  A unified activity tracker that aggregates everything you did into a single timeline — commits, GitLab events, Jira issues, calendar meetings, and system events.
</p>

<p align="center">
  <a href="#data-sources">Data sources</a> ·
  <a href="#features">Features</a> ·
  <a href="#sources-in-detail">Sources in detail</a> ·
  <a href="#getting-started">Getting started</a> ·
  <a href="#configuration">Configuration</a> ·
  <a href="#development">Development</a>
</p>

<p align="center">
  <img src="pics/workweek.png" alt="Work week view in the Light theme">
</p>

<p align="center"><sub>Work week view. All screenshots use demo data.</sub></p>

## Data sources

| Source       | What it collects                                                                   |
| ------------ | ---------------------------------------------------------------------------------- |
| **Git**      | Commits from local repositories via an embedded Git library                        |
| **GitLab**   | MRs, pushes, comments, reviews                                                     |
| **Jira**     | Issues assigned to or reported by you                                              |
| **Outlook**  | Calendar events from the running Outlook desktop app via native desktop APIs       |
| **System**   | Boot and shutdown events from the Windows Event Log and the macOS session history  |
| **Calendar** | Events from published ICS calendar links (Outlook, Google, iCloud) or `.ics` files |

A new install has no sources: add the ones you use with **Add source**. Git, GitLab, Jira and Calendar can be added more than once: two GitLab accounts, two Jira sites, Git on this computer and in each WSL distribution, several calendars. Each connection has its own label, settings, startup check and filter chip. The window opens without waiting for the checks or the first collection: each source shows a spinner until it is checked and collected, and its entries appear as soon as they are in, without waiting for the slowest source.

When a local git commit matches a GitLab push event (same commit SHA), they are merged into a single **git/lab** entry instead of appearing twice.

The entry types, displayed fields, supported platforms and configuration of each source are listed under [Sources in detail](#sources-in-detail).

## Features

### Four views

The sidebar switches between **Timeline** (a list grouped by day, newest day first), **Day**, **Work week** (Monday to Friday) and **Week** (Monday to Sunday). The three calendar views have previous, Today and next buttons and a zoom control.

The calendar views show **meeting time**: in Day view under the date, in Work week and Week view under each day and once for the shown week beside the month. It is the time covered by Outlook and Calendar meetings on that local day, with overlapping meetings counted once and a meeting across midnight split between both days. Busy and working-elsewhere meetings count, as do meetings without a status; free and out-of-office ones do not (the backend sends this as each meeting's `meetingWeight`: `busy`, `tentative` or `none`). Tentative time is shown apart as a lighter extra (`5h 30m · +1h tentative`), without the part a busy meeting already covers. Only the meetings the source filter shows are counted. A meeting that starts before the shown range and runs into it counts on the days it covers. Connecting both Outlook and the Calendar feed of the same calendar does not change meeting time, since identical meetings overlap and count once, but every such meeting is listed and drawn twice.

Each meeting carries its **busy status** where the source has one (Busy, Tentative, Free, Out of office, Working elsewhere): the detail panel lists it, and tentative meetings are drawn hatched and free ones outlined in the calendar views, with a Tentative or Free tag in the Timeline. While today is in view, a line in the accent colour marks the current time across today's column, with the time at the left edge; it moves every minute.

When the Timeline list is longer than the window, a day index runs down its right edge with one row for every day of the selected range, newest at the top. The highlighted day follows the day header stuck at the top of the list. Hovering shows the day and its event count, and clicking or dragging jumps to that day; a day without events jumps to the nearest day that has some. Long ranges show Monday labels only or, past a few months, one bar per day sized by its event count. When the range spans more than one year, the oldest month heading also shows its year (`Nov ’25`). From the keyboard it is the next stop after the filter field: <kbd>↑</kbd> <kbd>↓</kbd> step between days with events, <kbd>Page Up</kbd> <kbd>Page Down</kbd> move a week, <kbd>Home</kbd> <kbd>End</kbd> go to the newest and oldest day, and <kbd>Enter</kbd> moves focus to that day's first event.

<table>
  <tr>
    <td width="50%"><img src="pics/timeline.png" alt="Timeline view"><br><sub>Timeline</sub></td>
    <td width="50%"><img src="pics/day.png" alt="Day view"><br><sub>Day</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="pics/workweek.png" alt="Work week view"><br><sub>Work week</sub></td>
    <td width="50%"><img src="pics/week.png" alt="Week view"><br><sub>Week</sub></td>
  </tr>
</table>

### Links and history

Every view has its own address, so Back and Forward step through views and days, and a reload or a copied link opens the same place:

| URL                                                               | Shows                                                               |
| ----------------------------------------------------------------- | ------------------------------------------------------------------- |
| `/timeline` (`/` and unknown paths lead here)                     | Timeline                                                            |
| `/day/2026-11-17`, `/workweek/2026-11-17`, `/week/2026-11-17`     | A calendar view on that local day; a bad or missing day shows today |
| `?q=review`                                                       | The Timeline filter text; switching views clears it                 |
| `?from=2026-11-01&to=2026-11-14`                                  | The collection range; opening the app on it collects it             |
| `?settings=general`, `logs`, `add` or `gitlab/oss` (a connection) | Settings open over the view on that section                         |

Views, days and opening or closing settings add a history entry; typing in the filter, changing the range and moving between settings sections replace the current one. Selection, hidden sources and zoom are not part of the URL.

A range the backend would not collect (the end before the start, or longer than ten years, counted in real time across changes to summer time) is not collected when the app opens on it, and a link drops it.

**Share** in the header shares the link to the view on screen. In the browser it is the page's own address, with its whole query (the settings section too); in the desktop app it is an `activity-log:` link with the filter, the range and the open details. An event's own Share names that event in the range on screen. Where the system has no share sheet, the link is copied; without a clipboard, or when it refuses, the app says the link was not copied.

### Deep links

The desktop app opens `activity-log:` links: the part after the scheme is the same path and query as above.

| Link                                                    | Opens                                             |
| ------------------------------------------------------- | ------------------------------------------------- |
| `activity-log://day/2026-11-17?q=standup`               | The day view on 17 November                       |
| `activity-log://timeline?from=2025-08-01&to=2026-11-17` | The Timeline, collecting that range               |
| `activity-log://week/2026-11-16?settings=logs`          | The week view with the Logs settings open over it |

Hand-written links must percent-encode a `#` in a value as `%23` and a `+` as `%2B`: a raw `#` starts the fragment, so the rest of the link is lost, and a raw `+` reads as a space. Event ids can hold both (a repeated id is numbered `id#2`); the links the app builds encode them.

A link brings the window to the front. A link that starts the app opens once it is loaded; on Windows and Linux a second launch with a link hands it to the running app and quits. A link only navigates: unknown views and query parameters are dropped, and nothing is written. The scheme is registered by the installed app: on macOS the bundled `.app` (not `pnpm desktop:dev`), on Windows the installer; debug builds on Windows and Linux register it when they start.

### Themes

**Settings → General** has a theme picker with eight themes: Light, Green, Lilac, Solarized, Dim, Mocha, Dark and Midnight. The selected theme is stored in the browser's local storage. Until a theme is selected, the app uses Dark or Light to match the device's appearance, and follows it when it changes.

<p align="center">
  <img src="pics/theme-picker.png" width="820" alt="Theme picker in Settings, General">
</p>

<table>
  <tr>
    <td width="25%"><img src="pics/workweek.png" alt="Work week view in the Light theme"><br><sub>Light</sub></td>
    <td width="25%"><img src="pics/theme-green.png" alt="Work week view in the Green theme"><br><sub>Green</sub></td>
    <td width="25%"><img src="pics/theme-lilac.png" alt="Work week view in the Lilac theme"><br><sub>Lilac</sub></td>
    <td width="25%"><img src="pics/theme-solarized.png" alt="Work week view in the Solarized theme"><br><sub>Solarized</sub></td>
  </tr>
  <tr>
    <td width="25%"><img src="pics/theme-dim.png" alt="Work week view in the Dim theme"><br><sub>Dim</sub></td>
    <td width="25%"><img src="pics/theme-mocha.png" alt="Work week view in the Mocha theme"><br><sub>Mocha</sub></td>
    <td width="25%"><img src="pics/theme-dark.png" alt="Work week view in the Dark theme"><br><sub>Dark</sub></td>
    <td width="25%"><img src="pics/theme-midnight.png" alt="Work week view in the Midnight theme"><br><sub>Midnight</sub></td>
  </tr>
</table>

### Filter

The Timeline view has a **Filter activity…** field that narrows the collected entries as you type, ignoring case. <kbd>/</kbd> focuses it and <kbd>Esc</kbd> clears it. It matches fields such as commit message, repository, branch, GitLab event title, Jira key and status, and meeting title, location, organizer and description.

<p align="center">
  <img src="pics/search.png" width="820" alt="Timeline filtered by the text order history">
</p>

### Source filter

Every connection has a chip with its label, such as "Work GitLab" or "WSL · NixOS", and its entry count. Click a chip to toggle that source. Double-click it to show only that source, and double-click it again to show all sources.

<p align="center">
  <img src="pics/filter-solo.png" width="820" alt="Work week view showing only Outlook entries">
</p>

<p align="center"><sub>Work week after double-clicking the Outlook chip.</sub></p>

### Entry details

Clicking an entry in any view opens a panel with the fields collected for it. The fields of each source are listed under [Sources in detail](#sources-in-detail).

<p align="center">
  <img src="pics/detail.png" width="820" alt="Detail panel for a Jira entry opened from the Work week view">
</p>

### git/lab merge

A local commit whose SHA matches a GitLab push event is shown once, with a **git/lab** badge. Its detail panel lists the Git fields and the GitLab fields in separate sections.

<p align="center">
  <img src="pics/timeline-detail.png" width="820" alt="Detail panel of a merged git/lab entry with a Git section and a GitLab section">
</p>

### Export

The two buttons in the header export the current filtered view (date range, active sources and, in the Timeline view, the filter text) as Markdown grouped by day or as JSON. The dashboard sends the shown entries, the range and the browser's time zone; the backend renders the file (the same renderer as `pnpm cli:collect --format markdown|json`). The browser downloads it as `activity-<first day>-<last day>.md` or `.json` from `POST /api/export`; the desktop app opens a native save dialog instead.

The JSON export is a full `ActivityLog` (`meta`, `summary`, `entries`), the shape the CLI writes, rather than only the entries. The Markdown lists the range as local times, `[start, end)`, so a range ending on Friday ends at Saturday 00:00.

Start of the Markdown export of the demo data:

```markdown
# Activity Summary — 2026-11-02 00:00 to 2026-11-07 00:00

Total: 64 events

## Friday, 6 November 2026

- **[outlook]** Weekly sync (16:00, 30m)
- **[git]** `web-portal` docs: add order history to changelog (15:40)
- **[outlook]** Sprint 47 retro @ Room Ada (14:30, 1h)
- **[outlook]** Sprint 47 review @ Room Ada / Teams (14:00, 1h)
- **[jira]** ACME-151: Flaky e2e: cart badge count after login [Done] (13:15)
- **[git]** `checkout-service` spike(events): consumer with retry and DLQ (12:15)
```

### Source settings and startup status

**Settings** lists every connection with the result of its startup check and the operating systems it supports. In the desktop app, each pane also has the connection's settings, its **Label** and **Remove source**. Secret fields are write-only and only show whether a value is set.

**Add source** (in the sidebar, in Settings, and on the empty dashboard before the first source) shows a tile per source with what it collects. A tile adds the source and opens its settings; for Git on Windows it first asks for this computer or an installed WSL distribution. Outlook and System can be added once, and a source that does not run on this operating system is shown but disabled. GitHub, Microsoft Teams, Figma, Miro and Odoo are listed as coming soon.

<p align="center">
  <img src="pics/settings-gitlab.png" width="820" alt="GitLab source pane with startup check and connection settings in the desktop app">
</p>

### Logs

**Settings → Logs** shows the in-memory diagnostic log of provider checks, collection runs and settings saves. It can be filtered to warnings or errors, copied and cleared.

<p align="center">
  <img src="pics/settings-logs.png" width="820" alt="Logs pane in the settings dialog">
</p>

### Phone-width layout

At a width of 640px or less, the view switcher and Settings move to a bottom tab bar and the source chips become a horizontally scrolling row.

<table align="center">
  <tr>
    <td align="center"><img src="pics/phone-mocha.png" width="260" alt="Timeline at phone width in the Mocha theme"><br><sub>Timeline (Mocha)</sub></td>
    <td align="center"><img src="pics/phone-day.png" width="260" alt="Day view at phone width in the Light theme"><br><sub>Day (Light)</sub></td>
  </tr>
</table>

### Per-day cache

GitLab and Jira results are cached on disk per day, so a collection only requests the days that are missing or stale. The Logs pane records what was fetched and what was served from cache. TTLs and the cache location are described under [Configuration](#configuration).

### CLI

`pnpm cli:collect` runs a collection without the dashboard and writes JSON (the default) or Markdown to a file. All flags are listed in the CLI reference under [Development](#development).

```bash
pnpm cli:collect --range week
pnpm cli:collect --range week --format markdown --output reports/week.md
```

## Sources in detail

For each source, this section lists the entries it produces, the line shown under the entry title in the Timeline view, and the fields of the detail panel. A field without a value is left out of the panel. The calendar views show the entry title only. The panels below show demo data.

### Git

<table>
<tr>
<td width="56%" valign="top">

- **Entries:** one per commit authored with one of your emails, read from all refs of each repository. Your emails are every `user.email` found in any Git connection: in each repository's config (including the global one) and in each WSL distribution's global config. Emails compare case-insensitively, so a repository without `user.email` still contributes commits made with an email configured elsewhere. Commits count by their author time; history is read from 30 days before the range by commit time, and every branch tip with what is behind it however old its own date, so work committed up to that much before it was authored (a clock off, a rebase that keeps dates) is still found. The title is the commit subject.
- **Timeline line:** repository · branch · first 7 characters of the commit hash
- **Detail panel:** Repository (folder name), Branch, Commit (full hash), Author (email), Message
- **Branch:** only set for the commit that is the checked-out `HEAD` of its repository. Other commits have no branch.
- **Merged entries:** a commit that matches a GitLab push event becomes a [git/lab entry](#gitlab-merge) with an additional GitLab section.
- **Platforms:** Windows, macOS, Linux
- **Configuration:** `dirs` (optional list) per Git connection. A listed folder that contains `.git` is used as a repository; any other folder is searched for repositories up to four levels deep. Unset means the home folder. Repositories are only skipped when no `user.email` is configured anywhere. Worktrees of one repository are read once, under the name of its main folder, and a commit that several clones or Git sources hold is listed once.
- **Locations:** each Git connection reads one place: this computer, or (Windows) a WSL distribution. Without configuration there is one connection for this computer and, on Windows, one per registered distribution searching its `~`; Docker, Rancher and Podman distributions are skipped. Removing a distribution's connection excludes it.
- **WSL (Windows):** repositories inside WSL are read by running `git` in the distribution (`wsl.exe -d <distro>`), not over `\\wsl.localhost` paths, which is faster and avoids Git's ownership check. Folders are Linux paths; `~` stands for the distribution's home.

</td>
<td width="44%" valign="top"><img src="pics/source-git.png" alt="Detail panel of a Git commit with the fields Repository, Branch, Commit, Author and Message"></td>
</tr>
</table>

### GitLab

<table>
<tr>
<td width="56%" valign="top">

- **Entries:** one per event in the GitLab event feed of `GITLAB_USER`. The title is the title of the event's target, for example the merge request title. An event without a target title shows its action as the title.
- **Actions:** `pushedTo`, `created`, `opened`, `closed`, `reopened`, `updated`, `merged`, `accepted`, `commentedOn`, `approved`, `joined`, `destroyed` and `expired`. Any other action is stored as `unknown`.
- **Timeline line:** action · target type as reported by GitLab, for example `merged · MergeRequest` or `commentedOn · DiffNote`
- **Detail panel:** Action, Title and Target type. MR (`!` and the target IID) when the event has one. Branch and Commits for push events. File, Comment and Resolved for comments.
- **Merged entries:** a push event whose commit SHA matches a local commit is shown inside that commit's [git/lab entry](#gitlab-merge) instead of as its own entry.
- **Platforms:** Windows, macOS, Linux
- **Configuration:** `url`, `token` and `user` (all required). Results use the [per-day cache](#per-day-cache).

</td>
<td width="44%" valign="top"><img src="pics/source-gitlab.png" alt="Detail panel of a GitLab merged event with the fields Action, Title, Target type and MR"></td>
</tr>
</table>

### Jira

<table>
<tr>
<td width="56%" valign="top">

- **Entries:** taken from issues assigned to or reported by you. The title is the issue key followed by the summary.
  - `issueCreated`: the issue was created in the date range.
  - `statusChanged`: one entry per status change in the date range.
  - `issueUpdated`: the issue's latest change falls in the date range and is no status change (one that is shows as `statusChanged`).
- **Timeline line:** issue type · status
- **Detail panel:** Event, Key, Title, Status and Issue type. From status and To status for status changes.
- **Platforms:** Windows, macOS, Linux
- **Configuration:** `url` and `token` (required), `insecure` (optional). Results use the [per-day cache](#per-day-cache).

</td>
<td width="44%" valign="top"><img src="pics/source-jira.png" alt="Detail panel of a Jira status change with the fields Event, Key, Title, Status, Issue type, From status and To status"></td>
</tr>
</table>

### Outlook

<table>
<tr>
<td width="56%" valign="top">

- **Entries:** one per event in Outlook's default calendar that overlaps the date range. The title is the event subject.
- **Timeline line:** duration · location · `organizer` or `attendee` · `org:` and the organizer's name when you are not the organizer
- **Detail panel:** Title, Duration, Status, Location, Organizer and Role (Organizer or Attendee; left out when Outlook would not say, for example when its Object Model Guard refuses the read), a **Join meeting** button for online meetings and the **Description**
- **Calendar views:** a meeting is drawn as a block that spans its duration. In the Timeline an online meeting has a camera mark.
- **Busy status:** the appointment's Show As setting (on macOS its free/busy status). A meeting whose status cannot be read has none and counts as busy.
- **Description and join link:** on Windows from the appointment's plain-text body, cleaned like a [Calendar](#calendar) description. `Body` is one of the properties Outlook's Object Model Guard protects (like `Organizer`, which is already read): with antivirus that Windows reports as healthy Outlook does not ask, otherwise it can show a security prompt; if access is denied, meetings come without a description. On macOS from the event's plain-text content, read through AppleScript; should that Outlook not offer busy status and body to scripts, or fail to give them, meetings come without them and the Logs pane says so.
- **Platforms:** Windows, macOS
- **Configuration:** none. Outlook has to be installed and running. On Windows this must be classic Outlook: the new Outlook has no interface for other apps, and Activity Log cannot reach Outlook while it runs as administrator. On macOS it also needs a signed-in Exchange account. With the new Outlook, publish the calendar and add its ICS link as a [Calendar](#calendar) source.

</td>
<td width="44%" valign="top"><img src="pics/source-outlook.png" alt="Detail panel of an Outlook meeting with the fields Title, Duration, Location, Organizer and Role"></td>
</tr>
</table>

### Calendar

- **Entries:** one per event occurrence in the feed that overlaps the date range. Recurring meetings are expanded, with moved and removed occurrences applied: each occurrence is named by when its series planned it, a change (`RECURRENCE-ID`) replaces the occurrence planned at that time, a "this and future" change every later one (when it changes the length, every later one takes the new length; otherwise each keeps its own, an explicit period too), and a change that names no planned occurrence is shown as a meeting of its own. A date that is listed twice is one meeting; a change's extra date is a meeting of its own unless it repeats the change's own start. All-day and cancelled events are left out. The title is the event summary.
- **Timeline line:** duration · location · `org:` and the organizer's name
- **Detail panel:** Title, Duration, Status, Location, Organizer, a **Join meeting** button for online meetings and the **Description**
- **Calendar views:** a meeting is drawn as a block that spans its duration. In the Timeline an online meeting has a camera mark.
- **Description:** the event's `DESCRIPTION`, or the HTML version Outlook adds as `X-ALT-DESC` when the feed has one. It keeps its layout: paragraphs, headings, bulleted and numbered lists (also nested), bold, italic, links and images. From HTML only that structure is read; scripts, styles and other markup are dropped. An image is shown from its `https://` source; one whose data the feed does not hold (an attachment, `cid:`) shows as its alt text, and tracking pixels (width or height 0 or 1, in attributes or inline style), hidden images and the link around an image are dropped. An image inside a list is shown after the list. Plain text is read with the marks Outlook writes for a formatted text and with basic Markdown: items starting with `*`, `-`, `•` or a number, `# headings`, `**bold**`, `*italic*`, `[label](url)`, and `label<url>`, which becomes a link named by its label. Outlook Safe Links are replaced by the links they wrap, the Teams invitation block between two underscore lines (join link, dial-in numbers, meeting ID, help links) and runs of blank lines become one. Outlook writes an inline image into plain text as a line of its own in brackets: `[https://…/logo.jpg]`, an `https://` link to an image file (`.jpg`, `.png`, `.gif`, `.webp`, `.svg` and the like), is shown as the image; another link alone in brackets becomes a link; `[Company Logo]`, text of 2 to 80 characters, is an image the feed does not hold and shows as that alt text. In search and in the plain-text description an image counts as its alt text. It is capped at 4,000 characters. A moved or changed occurrence of a series uses its own description. In the panel it is cut to 12 lines with Show more, links in it open in the browser, and images load from their host without a referrer; one that fails to load shows as its alt text.
- **Join link:** `X-MICROSOFT-SKYPETEAMSMEETINGURL`, `X-MICROSOFT-ONLINEMEETINGEXTERNALLINK` or `X-GOOGLE-CONFERENCE` when the feed has one, then `URL` when it is a meeting link; otherwise the first Teams, Zoom, Google Meet or Webex link in the description. Only `https://` links are used. Join meeting opens it in the browser (on desktop, the default browser).
- **Busy status:** Outlook's `X-MICROSOFT-CDO-BUSYSTATUS` when the feed has it; otherwise `TRANSP:TRANSPARENT` is free (a transparent event blocks no time, even when tentative), then `STATUS:TENTATIVE` is tentative and `TRANSP:OPAQUE` busy. A moved or changed occurrence of a series uses its own value.
- **Platforms:** Windows, macOS, Linux
- **Configuration:** `ics_url` (required) per calendar: an `https://` or `webcal://` link, or the path of an `.ics` file. Plain `http://` is refused. The link is stored like a token and never written to the logs. Times without a time zone are read in this computer's time zone, and so are times whose `TZID` names a zone neither the feed nor the IANA database knows, with a warning in the Logs pane. A series that would run through more than 50,000 occurrences is cut there with a warning, without holding back the other events.
- **Outlook (also the new Outlook):** Settings → Calendar → Shared calendars → Publish a calendar, then copy the ICS link. The tenant may only allow availability without titles. Anyone who has the link can read the calendar; unpublishing revokes it.

### System

<table>
<tr>
<td width="56%" valign="top">

- **Entries on Windows:** System event log events 6005 (`boot`, "System started"), 6006 (`shutdown`, "System shutdown") and 6008 (`unexpected_shutdown`, "Unexpected shutdown").
- **Entries on macOS:** the reboot and shutdown records of the `last` command, as `boot` ("System started") and `shutdown` ("System shutdown"), and sleep and wake from the power-management log (`pmset -g log`), as `sleep` ("Sleep (lid closed)", "Sleep (idle)") and `wake` ("Wake (lid opened)", "Wake (user activity)"). macOS keeps that log for only about a week, so older ranges show no sleep or wake. Only sleeps and full wakes a person would recognise are listed: maintenance sleeps (Power Nap, backups), going back to sleep after one, and dark wakes are left out. When the power log cannot be read, the Logs pane says so and boots and shutdowns still show. Both are read in the C locale, so they are found whatever language macOS runs in.
- **Timeline line:** the event type
- **Detail panel:** Event type and Label
- **Platforms:** Windows, macOS
- **Configuration:** none

</td>
<td width="44%" valign="top"><img src="pics/source-system.png" alt="Detail panel of a System boot event with the fields Event type and Label"></td>
</tr>
</table>

## Getting started

### Requirements

- Rust toolchain (`cargo`, `rustc`)
- [Node.js](https://nodejs.org) `^22.22.3`, `^24.15.0` or `>= 26` (Angular 22 requirement)
- [pnpm](https://pnpm.io) >= 10
- Windows or macOS for the local Outlook and System collectors

### Setup

```bash
git clone https://git.inform-software.com/gb100/shared/playground/tzaharin/activity-log.git
cd activity-log
pnpm install
pnpm cli:collect sources add gitlab   # or add sources in the desktop app
```

### Run

| What                | Command                         | Result                                                               |
| ------------------- | ------------------------------- | -------------------------------------------------------------------- |
| Web dashboard + API | `pnpm dev`                      | API on `http://localhost:3001`, dashboard on `http://localhost:3200` |
| Desktop app (Tauri) | `pnpm desktop:dev`              | The same Angular UI in a desktop window, talking to Rust commands    |
| CLI                 | `pnpm cli:collect --range week` | Writes `logs/activity_log.json` (`--format markdown` writes a `.md`) |

The last 30 days are collected automatically on startup. Use the date pickers in the header to change the range and re-collect on the fly. A typed date is read day first, as the pickers show it (`03/05/2026` is 3 May).

## Configuration

### Config file

The CLI, the API server and the desktop app share one file: `activity-log/config.toml` in the OS config folder (`%APPDATA%` on Windows, `~/Library/Application Support` on macOS, `$XDG_CONFIG_HOME` or `~/.config` on Linux). `ACTIVITY_LOG_CONFIG` or `--config <path>` (CLI and API server) points elsewhere. `activity-log config path` prints it.

Each connection is a table in the list of its kind. Field names are the setting names without the kind prefix (`GITLAB_URL` → `url`):

```toml
[[gitlab]]
label = "Work GitLab"
url = "https://gitlab.example.com"
token = "glpat-…"
user = "me"

[[gitlab]]
name = "oss"
label = "Open source"
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

- The file is the only list of sources: a kind it does not mention is not collected. An empty or missing file means no sources.
- A connection is identified by its kind and `name`, such as `gitlab/oss`. Without `name`, the name comes from the label (`Work GitLab` → `work-gitlab`). The name never changes with the label. Names are compared as written in ids (`Work` and `work` are the same); when two connections of a kind end up with the same one, the later is read as `work-2`, the Logs pane says so, and the next change in Settings writes that name into the file.
- Without `label`, a connection is named after its kind (`GitLab`), or for Git after its location (`This computer`, `WSL · NixOS`).
- `[[outlook]]` and `[[system]]` have no settings; the empty table adds them.
- The file holds tokens. It is written readable only by your user on macOS and Linux.
- The desktop app and the CLI edit it in place and keep comments. The API server and the desktop app look at it every 2 seconds (the server also before it answers for the activity, the providers, the runtime state, a collection or a recheck; exports and logs do not look) and apply a change once it has stayed the same for 0.2 seconds; a file that does not parse is reported once in the Logs pane and the last good settings stay.

### Environment variables

Sources are set up only in the [config file](#config-file). These variables (also read from `.env`) change where files live and how the server runs:

| Variable                 | Description                                                                         |
| ------------------------ | ----------------------------------------------------------------------------------- |
| `ACTIVITY_LOG_CONFIG`    | Path of the config file                                                             |
| `PORT`                   | API server port (default: `3000`; `pnpm dev`/`pnpm api:dev` use `3001`)             |
| `CACHE_TTL_MINUTES`      | How long today's and other recent cached GitLab/Jira days stay fresh (default: `5`) |
| `ACTIVITY_LOG_CACHE_DIR` | Override the per-day cache directory (default: the OS cache dir)                    |

Git history is read in-process. The `git` command is only asked for `user.email` when the config files the library reads have none (some Windows setups keep the global config elsewhere). Outlook and System collectors need no credentials: they read from the local Outlook app and the platform's native event and session history APIs. Outlook is never started by the app: its calendar is collected only while Outlook is already open.

<details>
<summary><b>Per-day cache</b></summary>

<br>

GitLab and Jira opt into a per-day disk cache. Each collection serves the days it already has and fetches only missing or stale days. Today, yesterday and any day fetched before it ended stay fresh for `CACHE_TTL_MINUTES`. Days fetched more than an hour after they ended are "settled" and stay fresh for 7 days. Files live under `activity-log/cache` in the OS cache directory (`%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache` on Linux) unless `ACTIVITY_LOG_CACHE_DIR` is set.

Each file belongs to one connection and one account: the account the startup check names for the token (Jira `/myself`, GitLab `/user`), together with the connection's settings other than the token, with the URL counted as its address (the host's case and trailing slashes do not matter; a context path such as `/jira` does, in its own case). Two tokens of the same account share a file; another account behind the same URL gets its own. A connection whose check does not say whose token it is, or whose URL has a query or fragment, is collected without the cache, with a line in the Logs pane; a URL with a user name or password is not collected at all (the check asks to remove them). Cache files are owner-only. A cache folder that cannot keep them private (a drive that does not keep permissions, such as FAT or exFAT, or a folder other accounts can write to) is neither read nor written; the app logs "… cannot keep cached activity private; collecting without a cache" once and uses the cache again as soon as the folder is private. Several processes (the desktop app, the API server, the CLI) can share the folder: each fetches without waiting, then adds the days it fetched to the file, keeping a day another process fetched later; a file it cannot read, or one written by a later version, is left as it is.

Since the cache files are named by the account, GitLab and Jira days are fetched once again after this update. Files of older versions are no longer read; they can be deleted from the cache folder.

</details>

<details>
<summary><b>Desktop dynamic provider settings</b></summary>

<br>

In the desktop app, provider configuration fields are generated from Rust provider `config_requirements()` metadata and rendered dynamically in the Settings dialog.

- Scope: desktop (Tauri) only. In the web dashboard the source panes show status but not editable settings.
- Storage: `config.toml`, shared with the CLI and the API server.
- Secret fields: values are write-only in UI/API; the frontend only receives `isSet`.
- Runtime behavior: save operations rebuild provider runtime config and refresh startup status.
- Lists: `GIT_DIRS` is edited as rows and stored as a TOML array. An unset list shows its default (your home directory, or `~` in WSL); **Use default** returns to it, and removing every row saves an empty list, which means none.
- Connections: **Add source** adds a connection with a default label; GitLab, Jira, Calendar and Git can have several, Outlook and System one. For Git on Windows it offers this computer or an installed WSL distribution. Every connection can be renamed and removed.

</details>

## Development

The backend is a Rust workspace and the frontend is an Angular app:

| Part         | Role                                                                                |
| ------------ | ----------------------------------------------------------------------------------- |
| `core`       | Shared DTOs, collectors, cache, merge logic, markdown export, and TS generation     |
| `api`        | Rocket API                                                                          |
| `cli`        | Standalone collection CLI and a separate TypeScript generator binary for Angular    |
| `desktop/`   | Packages the Angular UI as a Tauri desktop app that talks to Rust commands directly |
| `dashboard/` | The Angular app. It consumes generated DTOs from `dashboard/src/app/generated/`     |

### Verification

Run this before treating changes as done:

```bash
pnpm verify
```

`verify` is the repo-wide gate. It runs TypeScript generation, formatter checks, Rust checks, clippy, Rust tests, dashboard lint, dashboard tests, Angular type-checking, and the dashboard build.

For Windows-specific backend coverage that also runs on macOS/Linux, use:

```bash
pnpm core:test:windows
```

That script exercises host-runnable Windows logic such as provider metadata, event-log query generation, XML parsing, and event mapping. For a full Windows-target compile (including the desktop crate), run `nix develop .#windows -c pnpm win:check`. See [`docs/testing-windows-on-mac.md`](docs/testing-windows-on-mac.md) for the full Windows testing strategy.

### Reference

<details>
<summary><b>Web dashboard + API</b></summary>

<br>

Hot-reload development (recommended):

```bash
pnpm dev
# API server  → http://localhost:3001
# Dashboard   → http://localhost:3200  (live reload, /api proxied to :3001)
```

Or run the two halves in separate terminals:

```bash
pnpm api:dev          # API on :3001, the port the dashboard dev server proxies to
pnpm dashboard:dev    # dashboard on :3200
```

The dev server's proxy (`dashboard/proxy.conf.mjs`) passes `GET /api/events` through as a stream, event by event, and ends it for the browser when the API stops answering mid-stream (for example when it is killed), so the dashboard connects again at once.

`pnpm api:start` runs the API on its default port, [http://localhost:3000](http://localhost:3000) (override with `PORT`). The API routes are:

- `GET /api/activity`: the last collection's log. While it runs the answer is `202` with the log of what is in so far: `meta.pending` lists the connections still being checked or collected, and is absent once the log is complete (`200`). After a collection failed as a whole: `200` with `{ "start", "end", "error" }`. Before the first collection: `503`
- `GET /api/providers`: every connection with its startup check result, or `"status": "checking"` while the check still runs. A connection whose last collection failed is `"degraded"`, with the error as its `reason`
- `GET /api/runtime-state`: both of the above from one moment: `{ "activity": { "epoch", "authority", "revision", "state" }, "providers": { "epoch", "authority", "revision", "providers" } }`. `state` is `{ "kind": "empty" }` before the first collection, else `running` or `ready` with the `log`, or `failed` with the `failure`. `epoch` names the server process, `authority` changes with each settings change it installs, and each part's `revision` (one counter for both) only grows while the server runs
- `GET /api/events`: the same state as server-sent events, which is how the dashboard follows the server. The first event is `state`, the `GET /api/runtime-state` pair; then `activity` (`{ "epoch", "authority", "revision", "state" }`) and `providers` (`{ "epoch", "authority", "revision", "providers" }`) come as each part changes, and another `state` after each settings change the server installs, and after a check finds that a connection's token belongs to another account (the source and the entries it no longer shows come together). While nothing changes the stream sends `ping` (`{ "epoch" }`) every 15 seconds. Each event is one line of compact JSON; there are no event ids and no replay, so a client that connects again gets a fresh `state`. A slow client skips values in between, never the latest of a part. Try it with `curl -N http://localhost:3000/api/events`
- `POST /api/connections/recheck` with `{ "source": "<kind>", "name": "<connection>" }`: runs that connection's startup check again, then collects the connections of its kind again for the range of the last collection; the other sources keep what they show. Answers `200` with the providers (this one checking) and their `stamp` (`epoch`, `authority`, `revision`, as in `GET /api/runtime-state`), `404` for an unknown connection
- `POST /api/collect` with `{ "start": "<RFC 3339>", "end": "<RFC 3339>" }`, the range `[start, end)`: starts the collection and answers `202` at once with its first log (every connection pending, showing what the previous log had for it), or with the log of the collection of the same range that already runs; a collection of another range is stopped and this one starts. The answer also carries the host's `kind` for what it shows (`running`, `ready` or `failed`, as in `GET /api/runtime-state`) and the activity's `stamp`. Follow `GET /api/events` (or read `GET /api/activity`) for the result
- `POST /api/export`: renders the entries it is sent as Markdown or JSON (see the export buttons below)
- `GET /api/logs`, `DELETE /api/logs`

Every error answers `{ "kind", "error" }`: `error` says it in words, `kind` says what it is, for a program to decide on (`noData` before the first collection, `invalidRequest` for a bad range, zone or body, `notFound` for an unknown connection or route, `tooLarge` for an export above 64 MiB, `internal` for anything else). The status stays as listed above.

The last 30 days are collected automatically on startup; the date pickers show that range while it runs, and keep it if it fails. The startup checks run in the background: a source is listed as checking until its own check finishes, and a collection reads each source as soon as that source is checked, so a slow check or a slow source holds back only itself. Results are progressive: the dashboard shows each source's entries when they arrive, and while a range is collected again the backend keeps a source's previous entries in the partial log until the new ones are in (with GitLab pushes still folded into their commits). Every entry carries an `id` from its source (a commit hash, a GitLab event id, an ICS UID with the occurrence), unique within its connection; a source that repeats an id gets it numbered (`id#2`) with a warning in the Logs pane. GitLab entries no longer carry the raw API event (`raw`); a push names its commit in `pushedCommit`. The per-day cache was bumped for this, so GitLab and Jira days are fetched once again after the update. Saving settings (or editing `config.toml` while the server or the desktop app runs) re-runs the checks the same way. The backend then collects again only the kinds the change affects: a kind with a connection added, removed (also when it is added again with the same settings), reordered, or with other values (a token, a URL, a folder). A label rename collects nothing, unless a connection of that kind has nothing collected yet or its new check finds that the token belongs to another account. A collection that was running, or that failed, runs again whole. Entries are kept per connection and per account: once a connection's values change, or its check finds that the token belongs to someone else, what was collected before, and a failure it had, is no longer shown or carried for it. The server pushes every change of the sources and the collection to the dashboard as it happens (`GET /api/events` in the browser, events in the desktop app), so the dashboard shows each one, entries a check took away included, also after an edit of `config.toml` and for sources that are new; it does not poll. When the connection drops, the dashboard keeps what is on screen with "Reconnecting" over it (marked as possibly out of date after five seconds) and connects again after 0.5, 1, 2, 4 and 8 seconds; a connection that brings nothing for 10 seconds before its first state, or nothing at all (not even a `ping`) for 35 seconds after it, counts as dropped, also behind a proxy that keeps a dead connection open. Bytes count, not whole events, so a large update on a slow link is not cut off while it arrives. After that it says it lost the backend and offers Retry, which reconnects without collecting again; a command the backend answers meanwhile also has it connect again. The dashboard and the API are deployed together: a dashboard against an API without `GET /api/events` says the backend refuses it, without Retry. Each browser tab holds one connection open for its stream, which counts against the browser's limit of six connections per server over HTTP/1.1. Use them to change the range and re-collect on the fly.

If you only build the dashboard (`pnpm dashboard:build`), it produces static files under `dashboard/dist/dashboard/` and does not serve them. To preview that build, run a static file server (for example `pnpm --dir dashboard exec npx http-server dist/dashboard/browser -p 3200`) or use `pnpm dashboard:dev`.

The header has two export buttons. Both export what the view shows over the range: the Timeline's filtered list, or what a calendar view draws (a meeting that runs into the range from the day before included):

- **Markdown** (`description` icon): downloads a `.md` summary grouped by day in the browser's time zone
- **JSON** (`data_object` icon): downloads the `ActivityLog` (range, per-source counts and the shown entries) as `.json`

A collection or export range can span at most ten years. Both are rendered by `POST /api/export`, which takes `{ "format": "markdown" | "json", "start", "end", "timeZone": "<IANA zone>", "entries": [...] }` and answers `200` with `{ "fileName", "content" }`, or `400` with `{ "kind": "invalidRequest", "error" }` for an unknown zone or a bad range.

In the desktop app, exports open a native save dialog instead. A saved export is confirmed with "Exported"; closing the dialog saves nothing and says nothing.

A collection the backend refuses as asked (a range it cannot collect) shows its reason without Retry, since asking again would be refused the same way; a lost backend or a failed collection offers Retry.

</details>

<details>
<summary><b>Desktop app (Tauri)</b></summary>

<br>

The desktop app uses the same Angular UI, but swaps the transport layer from HTTP to direct Rust commands when it is running inside Tauri; the server's event stream becomes the Tauri events `state`, `activity` and `providers`, with the same JSON, which the dashboard listens to before it reads the current state once (`get_runtime_state`).

```bash
pnpm desktop:dev      # development
pnpm desktop:build    # debug build / packaging
```

The window opens at its last size and position, maximized or fullscreen as it was; on a screen that is no longer connected the OS places it. Right-click shows the Cut, Copy and Paste menu only in text fields and on selected text. `ACTIVITY_LOG_DEVTOOLS=1 pnpm desktop:dev` opens the web inspector.

**Windows installer from macOS/Linux**

Cross-builds the NSIS installer for `x86_64-pc-windows-msvc` with [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (Microsoft CRT/SDK are downloaded on first run). The `windows` flake shell provides the Rust target, cargo-xwin, LLVM (`clang-cl`, `lld-link`, `llvm-rc`) and NSIS:

```bash
nix develop .#windows -c pnpm desktop:build:windows
```

The installer lands in `target/x86_64-pc-windows-msvc/release/bundle/nsis/`. MSI (WiX) bundles can only be produced on Windows; use the `Desktop Build` workflow for those.

**Regenerate app icons from the Angular favicon**

```bash
pnpm --dir dashboard exec tauri icon src/favicon.svg -o ../desktop/icons
```

This writes the platform icon set into `desktop/icons/`.

</details>

<details>
<summary><b>CLI reference</b></summary>

<br>

No dashboard build needed.

| Flag                | Description                                                               |
| ------------------- | ------------------------------------------------------------------------- |
| `--range <preset>`  | `today`, `yesterday`, `week`, `lastweek`, `month` or `lastmonth`          |
| `--since <date>`    | Start date (`YYYY-MM-DD`). Without `--range`, defaults to 30 days ago     |
| `--until <date>`    | End date (`YYYY-MM-DD`). Without `--range`, defaults to today             |
| `--format <format>` | `json` (default) or `markdown`                                            |
| `--output <path>`   | Output file (default: `logs/activity_log.json` or `logs/activity_log.md`) |
| `--config <path>`   | Config file to read (default: see [Config file](#config-file))            |

```bash
pnpm cli:collect --range today
pnpm cli:collect --range yesterday
pnpm cli:collect --range week
pnpm cli:collect --range lastweek
pnpm cli:collect --range month
pnpm cli:collect --range lastmonth
pnpm cli:collect --since 2026-03-01 --until 2026-03-18
```

Without `--range`, `--since` defaults to 30 days ago and `--until` to today. Dates are whole days in your local time zone.

**Output format**

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

**Managing sources**

```bash
pnpm cli:collect sources                              # every connection with its status
pnpm cli:collect sources add gitlab                   # asks for url, token (hidden), user
pnpm cli:collect sources add gitlab --label "Work"    # a label of your own; default: the kind's name
pnpm cli:collect sources add git --wsl NixOS          # a Git location inside WSL
pnpm cli:collect sources set gitlab/oss url https://gitlab.com
pnpm cli:collect sources set gitlab/oss token         # asks without echo
pnpm cli:collect sources set git/this-computer dirs ~/Projects ~/work
pnpm cli:collect sources unset git/this-computer dirs # back to the default
pnpm cli:collect sources rename gitlab/oss "Open source"
pnpm cli:collect sources remove gitlab/oss
pnpm cli:collect config edit                          # edits a private copy in $VISUAL or $EDITOR
```

A connection is named `kind/name`, or by its name alone when that is unique. Values are checked like in the desktop app before the file is written.

</details>

<details>
<summary><b>Generate Angular DTO types</b></summary>

<br>

```bash
pnpm types:generate
```

This writes Rust-backed DTO types into `dashboard/src/app/generated/`. The Rust contract is the source of truth; don't edit the generated files by hand.

</details>

<details>
<summary><b>Dashboard styling rules</b></summary>

<br>

- Use named breakpoints from `dashboard/src/app/components/_dashboard-shared.scss` (for example `@include ui.down('phone')`) instead of raw `@media (max-width: ...)` when a named breakpoint exists.
- Use `var(--content-max-width)` for shared content width caps instead of hardcoded `1120px`.

</details>

<details>
<summary><b>Project structure</b></summary>

<br>

```text
activity-log/
├── core/
│   └── src/
│       ├── collect/           Git, GitLab, Jira, Outlook, System collectors
│       ├── cache/             Per-day disk cache for API providers (GitLab, Jira)
│       ├── diagnostics/       In-memory diagnostic log shown in the Logs pane
│       ├── model/             Shared models grouped by domain/source
│       ├── runtime/           Shared collection runtime used by Rocket and Tauri
│       ├── ts/                Angular DTO generator and the constants the dashboard shares
│       └── utils/             Date utilities, markdown formatter, git/gitlab merge
├── api/                       Rocket API crate
├── cli/                       Rust CLI crate
├── desktop/                   Tauri desktop crate, config, capabilities, and icons
├── dashboard/
│   ├── src/app/
│   │   ├── components/        header, filters, timeline, calendar views, settings dialog
│   │   ├── generated/         Rust-generated DTOs
│   │   ├── services/          app state; backend/ holds the operation table and HTTP/Tauri transports
│   │   ├── models/            Angular-only view models
│   │   └── utils/             Angular-only helper logic
├── pics/                      README screenshots (demo data)
├── logs/                      Generated output (gitignored)
├── Cargo.toml
├── .env.example
├── package.json
└── README.md
```

</details>
