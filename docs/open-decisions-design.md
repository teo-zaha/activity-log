# Open decisions, 17 November 2026

Decisions taken in one question round after the progressive collection, the calendar recurrence fix, the date index rail and the read/write action design ([provider-actions.md](plans/provider-actions.md)). They settle what is built next and in which order: first the uncommitted work is committed in topic chunks that are reviewed one by one, then the version becomes 1.0.0, then the calendar work (meeting time, busy status, a current-time line), then the generic read step of the action system for Jira and GitLab, laid out as direction A of the UI mockups with the calendar look of direction C. The local API server stays local without a Host check. The mockups are in [mockups/ui-directions/](mockups/ui-directions/index.html); the date index mockup is [mockups/date-index-mockup.html](mockups/date-index-mockup.html).

## Terms

- **Meeting time**: the time per day or week covered by calendar and Outlook meetings, with overlaps counted once. Avoid: meeting load, busy time.
- **Filter**: narrowing the activity already collected (your own commits, meetings, tickets) in the timeline; what the timeline's search field does today. Avoid: search.
- **Remote search**: asking Jira or GitLab directly for items, including ones you never touched. Avoid: filter, search field.
- **Draft**: a write the AI agent asked for, saved with where it came from; the desktop prepares it again with a fresh preview when you open it, and you confirm it. Avoid: pending plan, queued action.
- **Shortcut hint**: a small key tag (⌘K, ?) on a button or menu item, generated from the same registry that handles the key and lists it in the help. Avoid: hotkey label, tooltip.
- **Host check**: the local API server answering only requests addressed to `localhost` or `127.0.0.1`. Avoid: CORS fix, firewall.

## Why

The app grew in one day from a read-only activity log into the start of a tool that also searches and writes to work systems, tracks time and is driven by an AI agent. Many small and large choices were left open in the reports; this round answers them so the next steps can start without stopping for each one.

## Locked decisions

### UI direction: A as the backbone, with C's calendar look and review cards

| Option                                                      | User experience | Correctness | Risk  | Cost |
| ----------------------------------------------------------- | --------------- | ----------- | ----- | ---- |
| **A with C's calendar look and review cards, B's ⌘K later** | good            | good        | good  | good |
| A only, as drawn                                            | mixed           | good        | good  | good |
| C, calendar workspace with docked tray                      | good            | mixed       | poor  | good |
| B, command bar first                                        | mixed           | mixed       | mixed | good |

**Decision: A, plus from C the calendar look (done, booked and drafted time as solid, hatched and dashed blocks in the Day and Week views, logged-of-present totals in each day header) and C's review cards inside A's Review view; B's ⌘K command bar comes later and only lists existing actions and opens the existing dialogs.** A adds a Work group to the sidebar with Work items (remote search, the item docked beside the list, an action bar), Time (a week grid of items × days, an attendance row, a "not logged yet" row that turns into suggestions from activity) and Review (agent drafts). Every write goes through one shared preview-and-confirm dialog. The user explicitly likes C's calendar; it is part of the decision.

- A only was rejected because the calendar is where unlogged time is easiest to see.
- Full C was rejected for its docked tray, which folds the sidebar to icons at 1280 px (a new layout state), and because it is built around the agent first.
- B first was rejected: hard to discover, poor on phones, and Jira's create forms do not fit in a bar, so A's dialogs are needed anyway.

### Agent drafts survive an app restart

| Option                                                   | User experience | Correctness | Risk  | Cost |
| -------------------------------------------------------- | --------------- | ----------- | ----- | ---- |
| **Saved locally, 7-day expiry, re-prepared when opened** | good            | good        | mixed | good |
| Memory only                                              | poor            | good        | good  | good |

**Decision: drafts are saved in the app's private data folder (request and origin only, never tokens), expire after 7 days, and are prepared again under current settings with a fresh preview before you confirm.** Memory only was rejected because an inbox that empties on every restart is confusing and makes the agent repeat work. The weak point is that request text (ticket titles, comments) sits on disk, hence the private folder and expiry. This also settles the design's open point that a prepared plan expires after 5 minutes: a draft is stored, a plan is not.

### New dependencies for the agent step are approved

| Option                                                    | User experience | Correctness | Risk  | Cost |
| --------------------------------------------------------- | --------------- | ----------- | ----- | ---- |
| **Approve `rmcp` and `tauri-plugin-single-instance` now** | good            | good        | mixed | good |
| Decide when the agent step starts                         | mixed           | good        | good  | good |

**Decision: approved now.** `rmcp` is the official Rust SDK for the Model Context Protocol; `tauri-plugin-single-instance` is the official Tauri plugin that keeps a second desktop window from starting. Both are pinned to a reviewed release with only the needed features.

### A meeting in both Outlook and a calendar feed is not merged

| Option                                  | User experience | Correctness | Risk  | Cost |
| --------------------------------------- | --------------- | ----------- | ----- | ---- |
| Merge on start, end and title           | good            | mixed       | mixed | good |
| **Don't merge**                         | poor            | good        | good  | good |
| Don't merge, count once in meeting time | mixed           | mixed       | good  | good |

**Decision: don't merge, and don't show a hint either.** Outlook and a published ICS feed of the same calendar give the same information; connecting both is the user's choice and makes no sense, so nothing is built for it. Merging was rejected because it is work for a setup nobody should use, and a renamed meeting would still duplicate.

### The local API server stays local, without a Host check

| Option                                              | User experience | Correctness | Risk | Cost  |
| --------------------------------------------------- | --------------- | ----------- | ---- | ----- |
| Local only, Host check now                          | good            | good        | good | good  |
| Reachable from other machines, with login and HTTPS | mixed           | mixed       | poor | mixed |
| Local, Host check, documented routes                | good            | good        | good | good  |

**Decision: no Host check; the server stays as it is.** The user considers the DNS-rebinding exposure of the local server acceptable. The read/write design's phase 0 lists the Host check as a fairing; that item is dropped. Actions over HTTP stay off (decided in the design), so the server only ever serves collected activity.

### Version 1.0.0

| Option                                  | User experience | Correctness | Risk | Cost |
| --------------------------------------- | --------------- | ----------- | ---- | ---- |
| Bump to 0.2.0, then per release         | good            | good        | good | good |
| Build number on test builds, keep 0.1.0 | mixed           | mixed       | good | good |
| Keep 0.1.0                              | poor            | mixed       | good | good |

**Decision: the version becomes 1.0.0** (the user's own answer), in the Cargo packages, the Tauri config and the dashboard package, so Windows installers and Apps & features show it. Later releases bump from there. There was no full release before, which is also why the `settings.json` migration was removed.

## Routine choices

- **Calendar features:** build meeting time per day and week (Day and Week views, overlaps counted once) and busy / tentative / free status on each meeting. Repeating-vs-one-off marks and the online-meeting flag are not built. The Day and Week views also get a line at the current time across today's column, moving every minute, with the time as a small label.
- **Tentative time:** counted separately: meeting time shows busy time, with tentative as a lighter extra total next to it ("5h 30m · +1h tentative").
- **Committing:** the uncommitted work is split into topic commits (connections and config file, add-source UI and UI cleanup, progressive collection, calendar recurrence fix, date index, labels and Odoo coming soon, docs). A commit plan is written first; then each commit is staged and its diff shown for review before it is committed. Commits that only build together with the next one are marked.
- **Order of work:** commits first, then 1.0.0, then the calendar work, then the generic read step.
- **First generic read step:** not Jira-specific. The shared parts (remote search, open item, per-connection permissions, Work items view, read-only agent access) are built once and proven on Jira and GitLab together; Odoo is the step after, as a new source.
- **Search and open on by default** for desktop and CLI on new connections; writing is always off until switched on, and the agent gets nothing until allowed.
- **AI access per connection:** one select, Off / Read only / Read and prepare changes (every change confirmed by you), with an Advanced toggle for per-action checkboxes. Read only is possible even where the source and you can write.
- **Naming:** the timeline field becomes "Filter activity…"; the new remote search is "Search Jira & GitLab…" in the Work items view.
- **Time view early:** time is reported per item per week, so A's Time view comes right after the first read step, with Jira worklogs first, then Odoo.
- **Keyboard shortcuts:** a small in-app registry (typed list of shortcuts with scopes, a directive that binds one to a button and renders its shortcut hint, ⌘ on macOS and Ctrl elsewhere, never firing while typing unless meant to) and a help dialog opened with `?`, built from the same list. No new dependency.
- **Date index rail:** the oldest month label shows the year when the range spans more than one year ("Nov ’25"); in dense mode the top month label moves so the today pill never covers it.

## Verified facts

- The Day and Week views have no current-time line today.
- The app has no shortcut system, only local key handlers in the search field, timeline, date index and event detail.
- `@angular/cdk` is already a dashboard dependency.
- The calendar file analysed earlier has busy status on every event (176 busy, 32 tentative, 1 free), repeating-series data, Teams links in 90% of events, and no organizer or attendees.

## Risks

- Meeting time is only right if the user connects one of Outlook or the calendar feed, not both (decided not to guard against it).
- The local API server can be read by a web page through DNS rebinding while it runs; accepted.
- Saved agent drafts keep request text on disk for up to 7 days.
- Topic commits out of intertwined changes may leave intermediate commits that only build together with the next one.

## Deferred

- Repeating-vs-one-off marks and the online-meeting flag; reopen if day reviews need them.
- B's ⌘K command bar; after the actions exist, on top of the shortcut registry.
- C's docked agent tray; only if the agent ends up supervised all day.

## Open threads

- The first question's answer ("the API should be public") was settled as "no Host check, it is fine"; whether the API should ever be reachable from other machines was not decided and would need login and HTTPS first.
