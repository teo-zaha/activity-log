# Generic read step: search and open work items in Jira and GitLab

Status: partly built. Slices 1 and 8 are built; slices 2–7, 9 and 10 are planned, written against `feat/tauri` at 6413248 (2026-11-21); slice 11 comes with the MCP step. [Where the step stands](#where-the-step-stands-2026-11-21) lists each slice with the commits that built it and what the merged plan's steps built underneath. Revised after an independent review (approved with changes, all taken; see "Review changes"), and Q1–Q5 are decided. Where a slice meets [shared-contracts.md](shared-contracts.md) (§2 `action_target`, §5 push, §7 URLs, §8 registry, §9 requirements on slices), that document is the newer one and the slice says so. It turns the read half of [provider-actions.md](provider-actions.md) (phase 0 and 1a, plus the agent policy of 1b) into ordered slices, with the user's decisions from [open-decisions-design.md](../open-decisions-design.md) applied. The step is generic: the action model, executor, permissions, contract, CLI and Work items view are built once and proven on Jira (Data Center) and GitLab together. Odoo, writes, time tracking and the agent transport (MCP) come later and add to what this step builds.

## Rubric

| Dimension      | Score | Why                                                                                                                                                                    |
| -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope          | 2     | Every layer: core (API clients, settings, actions, runtime), Tauri, CLI, generated contract, dashboard (new view, settings section; the shortcut registry is built)    |
| Design impact  | 2     | The executor, `ActionRequest`/`ActionResponse`, `Capability`, `allow`/`agent_allow` and the `ActionProvider` trait are what every later phase builds on                |
| Failure impact | 1     | Read-only, so nothing remote changes. What can go wrong: token leaks (redirects, errors), path or query injection through keys, remote text in the webview or terminal |
| Uncertainty    | 1     | The architecture is reviewed. Some GitLab and Jira Data Center parameters and the CSP's effect on the packaged app are not verified (end of doc)                       |
| **Total**      | **6** | Q1–Q5 under "Decisions for the user" are decided (user, 2026-11-18), all with the recommended defaults                                                                 |

## Where the step stands (2026-11-21)

| Slice                                 | State                     | Built by                                                                                                                                                                               | Left                                                                                                                                      |
| ------------------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Shared API clients, error boundary | Done                      | 33cf813 (merged plan step 2, R-B4)                                                                                                                                                     | Nothing. Later slices call `JiraApi`/`GitLabApi` and read `IdentityEvidence`                                                              |
| 2. `allow` and `agent_allow`          | Planned                   | Its base is built: the config write protocol and `instance` (step 1, R-C1)                                                                                                             | All of it                                                                                                                                 |
| 3. Action model, executor, probe      | Planned                   | Its base is built: `Host`, recorded checks, collection keys (step 5)                                                                                                                   | All of it, plus the per-connection `access` on `ProvidersUpdate`, `ConnectionAccess` and `AccessContext` (shared-contracts §5, not built) |
| 4. Jira read                          | Planned                   | —                                                                                                                                                                                      | All of it                                                                                                                                 |
| 5. GitLab read                        | Planned                   | —                                                                                                                                                                                      | All of it                                                                                                                                 |
| 6. Tauri commands, opener, CSP        | Planned                   | `open_url` and the opener plugin exist                                                                                                                                                 | All of it                                                                                                                                 |
| 7. CLI `search` and `show`            | Planned                   | `cli/src/sources/`; R-L1 (step 12) lands first                                                                                                                                         | All of it                                                                                                                                 |
| 8. Keyboard shortcut registry         | Done                      | 5870ed0 (step 10), 90fd482, e8a6402 (WebView2 browser keys), 6260dbf and aea479e (macOS menu, step 10a)                                                                                | The Work items entries and the `work-items` scope, with slice 9                                                                           |
| 9. Work items view                    | Planned; groundwork built | b79f84e (routed layouts, slots, `NAV_GROUPS`, `TopLayer`, navigation motion, modal details), 0254071 and 6bde44a (phone details and layout), step 11 streams B and C (narrow services) | The view, its service and route, the Work nav group, the per-group phone bar, `?item=`, the `components/detail-panel/` lift (R-F13)       |
| 10. Access settings                   | Planned                   | The source pane it extends is built (field issues, drafts, `basedOn`; step 7)                                                                                                          | All of it                                                                                                                                 |
| 11. Agent projection                  | With the MCP step         | —                                                                                                                                                                                      | All of it                                                                                                                                 |

What the merged plan built that the remaining slices stand on, and the names they use for it:

- **Runtime host** (step 5): `core/src/runtime/host/` (`Host`, one per process), shared by `api` and `desktop` (`desktop/src/app_state/`). It replaces the `CollectionRuntime`, its read guard and `update_config_at` named in the first draft. A check is recorded before any reader uses it; `Host::action_target` (shared-contracts §2) is the slice 3 and 6 entry point.
- **Push** (steps 6 and 6a): the dashboard follows the host through `GET /api/events` in the browser and the Tauri events `state`, `activity` and `providers` plus `get_runtime_state` on the desktop (`services/runtime-state/`, `PushStateSource`); `ActivityDataService` is the coordinator. Polling is gone. "A changed providers response" in this plan means a pushed `ProvidersUpdate`; Work state is keyed and invalidated by the per-connection `access` (shared-contracts §5, §9), which is added with its first reader.
- **Typed errors** (step 6b): every command fails with a `BackendError { kind, message }` (`core/src/backend_error/`), and the dashboard's transports reject with a `BackendFailure` (`services/backend/backend-failure.ts`). An action's own outcome stays in `ActionResponse` (`Done`/`Failed`), inside the desktop envelope `HostActionResponseDto` (§9); only a refused call becomes a `BackendError`.
- **Layouts** (step 8): `layouts/shell/` (`ShellLayoutComponent`, `ShellSlots`, `TopLayer`, `shell-shortcuts.ts`) and `layouts/views/` (`ViewsLayoutComponent`); routes in `app.routes.ts`; `models/nav.ts` (`NAV_GROUPS`, one group `views` today); `navigation-motion.ts` (`NavigationMotionService`); `services/breakpoint-state.service.ts` (`BreakpointState`, with `desktopCompact`).
- **Touch basics** (step 9, NF-1): `ui.hover`, `ui.field-text`, the scroller rule, checked by `scripts/touch-styles.test.mjs`; new components follow them.
- **Shortcut registry and menu** (steps 10 and 10a): see slice 8.
- **Dashboard services** (step 11): `ActivitySelection` (shown entries, range coverage), `ConnectionSettings` (settings operations; it replaced `ProviderConfigFacade`), `NoticeService` (the snackbar's notices), `ActivityExportService`. `ActivityService` is deleted in step 11 Z, so new code injects the narrow services. Stream A (the calendar grid) is still running.
- **Config writes** (step 1, R-C1): every writer goes through `settings::edit_config` (`core/src/settings/write/`) with an `EditBase` and `Precondition`, under `config.toml.lock`; each table gets a reserved `instance` id; the settings view is `core/src/settings/view/` (`normalize`, `prepare_save`, `check_save`, `apply_save`). R-C8 splits `core/src/settings/mod.rs` after step 12, so paths under it may move.
- **Identity, keys and cache** (steps 2, 3, 5): `RemoteIdentity { id, username }` and `IdentityEvidence` (`Verified`, `Unnamed`, `Denied`, `None`) from the startup check, kept by `ProviderRuntime::identity()`; `CollectionKey` and the access token (`core/src/runtime/keys/`, shared-contracts §4); `ProviderRuntime::cache_scope`.

## What the step delivers

- Search Jira and GitLab from the desktop app and the CLI, including items that aren't yours, and open one item's details: description, labels, latest comments, time spent and estimate.
- Per-connection access in `config.toml` (`allow`, `agent_allow`), enforced in core for every caller, shown and edited in the settings next to what the token can do.
- AI access per connection (Off / Read only), stored in `config.toml` and enforced by core's policy now, so the later MCP step adds the transport and the agent projection. The settings show it only once an agent can connect, with that step (UI decisions of 2026-11-21, Q16).
- A keyboard shortcut registry with key tags and a `?` help dialog (built in slice 8 and already used by the app's own commands and the macOS menu); the Work items view adds its entries.

Out of scope: writes (no `plan`/`apply`, no confirm), `actions.jsonl` (reads go to the diagnostics log only, as the design says), time entries, the agent socket and MCP adapter, Odoo, the ⌘K command bar, the Time and Review views.

## Design choices this plan makes

These follow from the reviewed design but narrow or adjust it. Each is recorded in [provider-actions.md](provider-actions.md) as well.

1. **Reads only, so no `ActionCall` enum yet.** `run_action` takes `ActionCall { connection, request }` as a plain struct. When writes arrive, confirmation becomes its own command, `confirm_action(id)`, instead of a `Confirm` variant. Equally safe (the confirm still carries only the id), and `run_action` never has a variant it can't serve.
2. **`Capability` has the design's six names now** (`search`, `read`, `create`, `comment`, `update`, `log_time`), so a config written from the docs parses without warnings. What a kind **does** is `ActionProvider::capabilities()`: `search` and `read` for Jira and GitLab in this step, nothing for kinds without actions. Settings, probes and the CLI show only what the kind implements. `attendance` joins with Odoo.
3. **Probes check only what is shown**, and are advisory: they never change a connection's status. Write phases add their permissions to the same probe.
4. **`ActionRequest` and `ActionResult` hold only `Search` and `Get`; `ActionErrorCode` only the read codes** (`Conflict`, `OutcomeUnknown`, `Expired` come with writes). Adding variants later is additive.
5. **Opening an item takes its ref, never a URL.** `open_work_item` makes core parse the key again and build the URL from the connection's configured base.
6. **The Access section is a section in the source pane, not the mockup's tab**; a tab can come when write toggles make the pane long.
7. **"Read and prepare changes" is hidden until a write capability exists.** The AI select shows Off / Read only, and Custom when Advanced sets something else. The whole select appears only with the MCP step (UI decisions of 2026-11-21, Q16); this step stores and enforces `agent_allow` without showing it.
8. **The CLI verb is `show`**, as in the reviewed design, with `get` as a visible alias to match the request name.
9. **`WorkItemDetail.time` has spent and estimate only**; "you: 1h today" needs worklogs (phase 2).
10. **The search filter has one project**; the mockup's multi-project chip comes later.
11. **Native JQL and filters don't mix.** A `SearchQuery` with `native` set must carry the empty filter (`SearchFilter::any()`), else `InvalidInput` ("Filters and JQL can't be combined"). The dashboard disables the chips in JQL mode, the CLI rejects `--jql` with filter flags.

## Decisions for the user

The reviewer reached the same answers on Q1–Q4 independently. **Decided by the user on 2026-11-18: all five with the recommended defaults**, as recorded under each question; the plan is written with them.

**Q1. Does remote search ask one connection or all of them?** The name, "Search work items…", suggests all; the mockup draws a connection picker.

| Option                                                                                 | User experience | Correctness | Risk | Cost  |
| -------------------------------------------------------------------------------------- | --------------- | ----------- | ---- | ----- |
| **All by default, picker narrows to one; JQL only with one Jira connection (default)** | good            | good        | good | mixed |
| One connection at a time, as drawn                                                     | mixed           | good        | good | good  |

**Decided:** remote search asks all connections by default, and the picker narrows it to one; JQL only with one Jira connection chosen.

One `Search` per connection that allows search. Each connection keeps its own cursor and its own inline error, so one failing source doesn't hide the others. The list sorts only what is loaded, by `updated`; "Load more" asks each connection that still has a cursor. Every query carries a generation number, and a response for an older generation is dropped. About half a day more in slice 9; core and the contract are the same either way.

**Q2. How does the app open an item in the browser?** Today the only opener is the Rust `open_url` command for event links (`desktop/src/commands/mod.rs`) (`tauri-plugin-opener` without a webview permission); the webview must not get a general one.

| Option                                                                            | User experience | Correctness | Risk  | Cost  |
| --------------------------------------------------------------------------------- | --------------- | ----------- | ----- | ----- |
| **`tauri-plugin-opener`, called only from Rust, no webview permission (default)** | good            | good        | good  | good  |
| Own code per OS (`open`, `xdg-open`, `rundll32 url.dll,FileProtocolHandler`)      | good            | mixed       | mixed | mixed |

**Decided:** `tauri-plugin-opener`, called only from Rust; the web view gets no permission to open URLs.

The plugin is already a pinned desktop dependency (`tauri-plugin-opener` 2.7.3, used by `open_url`). The URL is rebuilt from the connection and the parsed ref, `http`/`https` only, same origin as the configured base, without user info (a base URL with `user:pass@` is refused for opening).

**Q3. What does AI "Read only" mean on Git, Outlook, Calendar and System connections?** They have no actions, but the agent's `activity` tool (MCP step) shows collected entries, and the design's own test has "allowed Git, denied GitLab".

| Option                                                                                      | User experience | Correctness | Risk | Cost  |
| ------------------------------------------------------------------------------------------- | --------------- | ----------- | ---- | ----- |
| **`read` in `agent_allow` means "the agent may be shown this connection's data" (default)** | good            | good        | good | good  |
| A separate `activity` capability                                                            | mixed           | good        | good | mixed |

**Decided:** `read` in `agent_allow` means the agent may see this connection's data.

Every connection gets the AI select; Read only writes `agent_allow = ["read"]`, plus `search` where the kind implements it. Disclosure and actions stay separate: a Git connection with `read` in its lists still implements no actions, so its kind advertises no capabilities and `Get` on it is `Unsupported`. The policy stays `allow ∩ agent_allow`, so `allow = []` also hides a connection from the agent. Needed before slice 2, since it decides the enum.

**Q4. Does the agent projection land in this step or with MCP?** The policy (`agent_allow`, the intersection, refusals for `Agent`) is in slices 2 and 3. The projection (agent response types, recursive filtering of nested GitLab events, truncation) has no caller until MCP, and the Time view comes first in the decided order.

| Option                                                                         | User experience | Correctness | Risk | Cost  |
| ------------------------------------------------------------------------------ | --------------- | ----------- | ---- | ----- |
| **With the MCP step, and required before any agent transport ships (default)** | good            | good        | good | good  |
| Now, as slice 11                                                               | good            | good        | good | mixed |

**Decided:** the agent projection comes with the MCP step and is required before any agent transport ships.

Slice 11 is written out below so it moves without being replanned.

**Q5. What does "Done" mean for GitLab merge requests?** Mapping Done to `merged` alone makes closed-unmerged merge requests disappear from both Open and Done.

| Option                                     | User experience | Correctness | Risk | Cost  |
| ------------------------------------------ | --------------- | ----------- | ---- | ----- |
| **Done = merged or closed (default)**      | good            | good        | good | good  |
| A separate Merged chip; Done = closed only | mixed           | good        | good | mixed |

**Decided:** for GitLab, Done means merged or closed.

Recommended: Done means "no longer open", like Jira's status category Done (which also covers Won't do). The merged/closed difference shows as the item's status. With the composite cursor (slice 5) this is one more stream (closed merge requests), not new machinery.

## Slices

Each slice is one reviewable change that passes `pnpm verify` on its own (and `pnpm win:check` where noted). Effort is focused working time for one person who knows the repo.

### Slice 1: shared API clients and the error boundary (phase 0) — done (33cf813)

Built as merged plan step 2 (R-B4, with B1, B4's identity, B30, B34, B39 and B31's validation). No new features; the check and the collector of each remote kind build the client, auth, paths and errors in one place. What later slices use:

- `core/src/provider/remote_api/` (`RemoteClient`, `identity_check`, the redirect policy and the test server): the one client both kinds wrap. `core/src/provider/jira/api/` (`JiraApi`: `endpoint`, `get_json`, `myself`) and `core/src/provider/gitlab/api/` (`GitLabApi`: `endpoint`, `get_json`, `get_page`, `current_user`), crate-private. Paths are pushed as segments (`endpoint(&[…])`), query values through `query_pairs_mut()`, never `format!`.
- **Credentials only go to the configured origin**: a redirect is followed only to the same scheme, host and port; another one stops with "… redirected to another address; set the URL it redirects to". The token is added per request after the origin check. A base URL with a user name or password leaves the connection not configured (user decision, 2026-11-19).
- `core/src/provider/remote_error/`: `ApiError { Status { status, message, retry_after }, Timeout, Unreachable, Decode }`, with `ApiError::problem()` (401/403 → `refused`, `timedOut`, `unreachable`) and `message(service)`; `RemoteFailure` keeps it in an error's `anyhow` chain so `problem_of` and `CollectFailure::of` read the kind, never the text. `remote_message(body, secrets)` keeps the server's own message, redacts secrets, cuts addresses to scheme, host, port and path, escapes control and bidirectional characters and caps at 300 characters; error bodies are read only in part (16 KiB). A body that breaks off is a transfer failure, never `Decode`.
- `get_json` retries a read once on 429 or 503 when `Retry-After` is at most 10 s.
- **Identity**: the startup check parses Jira `/myself` and GitLab `/user` into `RemoteIdentity { id, username }` and records `IdentityEvidence` (`Verified`, `Unnamed`, `Denied`, `None`), which `ProviderRuntime::identity()` keeps (the first draft's `ProviderCheckResult::with_identity` is not the built name). For GitLab that is the token's owner, not the configured `user` whose events are collected; slice 5's "me" reads it. An SSO or other non-API page in answer makes the connection unavailable (user decision, 2026-11-19).
- Jira paging follows what came back (B30), and truncated change histories are reported in the Logs pane (B31).

### Slice 2: `allow` and `agent_allow` in `config.toml` — 1 day (uses Q3)

- `core/src/model/actions/capability.rs`: `Capability` (serde `snake_case`, ts-rs exported).
- `core/src/provider/connection/mod.rs`: `Access { allow, agent_allow: BTreeSet<Capability> }`, default `{search, read}` and `{}`; `Access::for_origin(Origin)` (`Agent` → `allow ∩ agent_allow`, else `allow`). `Connection` gets it through `with_access()`, as it gets its `instance` through `with_instance()`, so `Connection::new` callers don't change. `Origin { Desktop, Cli, Agent }` in `core/src/model/actions/origin.rs`, Rust only: no request can carry it.
- `core/src/settings/mod.rs` (split by R-C8 after step 12): `allow` and `agent_allow` join `name`, `label`, `wsl` and `instance` as reserved fields (no "unknown setting" warning, not config keys, so not in the cache fingerprint or the cache scope). Missing `allow` → default; missing `agent_allow` → empty; anything that is not a list of strings → empty with a warning (fail closed); an unknown name → dropped with a warning; absent (the default) is kept apart from an explicit empty list (merged plan N-7). Writing goes through `settings::edit_config` like every other edit (lock, re-read, the caller's `Precondition`, private publish): a `ConfigFile::set_access(index, AccessList, Option<&BTreeSet<Capability>>)` applied inside the edit writes a list, or removes the key when it equals the default.
- The host: an `allow`/`agent_allow` edit changes the connection's access token only (shared-contracts §4): Work state is invalidated, collected activity stays.
- CLI parity in `cli/src/sources/mod.rs`: `sources set jira/work allow search read`, `sources unset jira/work agent_allow`.
- Tests: missing, empty, a string, a list with a number, an inline table → each to its documented set, for both keys; unknown name dropped and warned; accepted on every kind; changing `allow` keeps the cache scope and the collection key and changes the access token; writing the default removes the key; comments survive; a stale `EditBase` is refused as a conflict; `for_origin(Agent)` without `agent_allow` is empty even when `allow` has everything.

### Slice 3: action model, executor and probe in core — 2 days

The generic part, tested with a fake provider.

- Models, one per file under `core/src/model/actions/` (ts-rs exported unless noted):
  - `ActionCall { connection: ConnectionId, request: ActionRequest }`
  - `ActionRequest`, tagged by `action`: `Search { query: SearchQuery }`, `Get { key: String }`
  - `SearchQuery { filter: SearchFilter, native: Option<String>, cursor: Option<String>, limit: u16 }` (limit clamped to 1–50)
  - `SearchFilter { text, assignee: AssigneeFilter, reporter: ReporterFilter, state: StateFilter, project: Option<String> }` with `AssigneeFilter { Me, Unassigned, NotMe, Anyone }`, `ReporterFilter { Me, NotMe, Anyone }`, `StateFilter { Open, Done, Any }`
  - `ActionResponse`, tagged by `status`: `Done { result: ActionResult }`, `Failed { error: ActionError }`
  - `ActionResult`, tagged by `kind`: `Search { result: SearchResult }`, `Item { item: WorkItemDetail }`
  - `SearchResult { items, total: Option<u32>, cursor: Option<String> }`
  - `WorkItemRef { connection, key }` (responses only); `WorkItem { ref, title, item_type, status, done, priority, assignee: Option<Person>, reporter: Option<Person>, project, created, updated, url: Option<String> }`; `Person { id, display_name }`
  - `WorkItemDetail { item, description, labels, comments: Vec<WorkItemComment>, time: Option<TimeSummary> }`; `WorkItemComment { author, created, body }`; `TimeSummary { spent_seconds, estimate_seconds }`
  - `ActionError { code, message, field, retry_after_seconds }`; `ActionErrorCode { NotFound, Unsupported, NotAllowed, Unavailable, MissingScope, InvalidInput, RateLimited, Unreachable, Timeout, Remote }`
  - `CapabilityGrant { Granted, Missing, Unknown }`; `CapabilityState { capability, allowed, grant, reason }`
- `core/src/actions/action_provider.rs`: `ActionProvider` with `capabilities()`, `probe(&ActionScope)`, `search(&ActionScope, SearchQuery)`, `get(&ActionScope, &str)`, `item_url(&ActionScope, &str) -> Result<Url, ActionError>`, unimplemented methods `Unsupported`. `ActionScope` carries the connection's context and the identity from its recorded check (`IdentityEvidence`, slice 1; slice 3 reads it, it does not map it again). `ProviderAdapter::actions() -> Option<&dyn ActionProvider>` (`core/src/provider/provider_adapter.rs`) defaults to `None`.
- `core/src/actions/mod.rs` + `tests.rs`: `run(caller, &ProviderRuntime, &ActionLimits, ActionRequest) -> ActionResponse`, refusing in this order before any provider code. _Note (step 0): replaced by `run(caller, &ActionTarget, ActionRequest)` with the same refusal order; see `shared-contracts.md` §2 (`action_target`) and §9._
  1. the kind implements the request's capability → `Unsupported`
  2. `allow` has it → `NotAllowed` ("Search is off for Work Jira")
  3. `Access::for_origin(caller.origin)` has it → `NotAllowed`
  4. the connection is usable → `Unavailable` with the check's reason
  5. `ActionLimits`, then the provider, then errors mapped from `ApiError`
  - One diagnostics line per call under the connection's scope, without query text: `desktop search → 23 items (a91c03f2)`.
- `ActionLimits` (per connection, owned by the runtime): 2 reads at a time, at most 8 waiting (more → `RateLimited`, "Too many searches at once"), 20 s to get a slot (→ `Timeout`). A 429 or 503 whose `Retry-After` is not retried sets `blocked_until` for the whole connection; reads before it fail at once with `RateLimited` and the remaining seconds.
- `core/src/runtime/host/`: `Host::action_target(id) -> Option<ActionTarget>` (shared-contracts §2, §9) awaits the connection's recorded check, then reads the checked provider, the access policy, the runtime's `ActionLimits` and the `AccessContext` together under one state lock; `None` for an unknown id, which the desktop answers as `Unavailable` ("No connection gitlab/x"). It never waits for or blocks a collection; a superseded check moves the waiter to the newer one.
- Push (shared-contracts §5, "not built" in steps 6 and 6a): `access` on `ProvidersUpdate` (`ConnectionAccess { connection, access }`) and `AccessContext { epoch, access }` are added here, with their first readers (this slice's envelope and slice 9's Work keys).
- Probe: after an available or degraded check, inside the same timeout, the check calls `probe()`; on any failure the grants are `Unknown`. It never changes the status, so it sits beside the `CheckOutcome` (`core/src/provider/provider_check_result.rs`), not in it.
- Contract and every place that builds the changed DTOs (required fields, so they all move in this slice; find them with `rg 'ProviderInfo \{'` and `rg 'ProviderKindDto \{'`, the first draft's line numbers are gone):
  - `ProviderInfo.capabilities: Vec<CapabilityState>` — `core/src/provider/provider_runtime.rs`, `core/src/runtime/host/state.rs`, `core/src/runtime/host/tests.rs`, `api/src/tests.rs`, `cli/src/tests.rs`, `cli/src/sources/tests.rs`. The checking `ProviderInfo` already carries `capabilities` with `allowed` from the configuration and `grant: unknown` (shared-contracts §9), so the Work group can be decided before checks end.
  - `ProviderKindDto.capabilities: Vec<Capability>` — `core/src/settings/view/mod.rs` (`build_snapshot`) and its tests.
  - `core/src/ts/` exports, `pnpm types:generate`, the validator `services/backend/runtime-message.ts` (`decodeRuntimeMessage` checks what readers take apart), and the dashboard fixtures and specs that build `ProviderInfo` (`testing/activity-fixtures.ts` and the specs that use it).
- Tests with a fake `ActionProvider` that counts calls:
  - Refusal order, and the provider is never called on a refusal.
  - `Agent`: `agent_allow` missing → both reads refused; `["read"]` → search refused, get runs; `allow = []` with `agent_allow = ["search","read"]` → refused.
  - A kind without actions → `Unsupported` for `Get` even with `read` in both lists (Q3).
  - A call during the check waits for it; a failed check → `Unavailable`.
  - The third concurrent read waits; the ninth waiter is refused; a slot not free after 20 s → `Timeout`; after a 60 s `Retry-After` the next read fails at once without a request.
  - A probe that errors or times out leaves the status `Available` and grants `Unknown`.
  - A failed response's message never contains the fake's body.

### Slice 4: Jira read — 1 day

- `core/src/actions/jira/mod.rs` + `tests.rs`, `jql.rs`: `JiraActions` (`[Search, Read]`), wired in `provider/jira/mod.rs`.
- Search: `GET /rest/api/2/search` with `jql`, `startAt`, `maxResults` and the list's fields. JQL from the filter as in the design's table, literals quoted with `"` and `\` escaped, `ORDER BY updated DESC`; `native` sent as written (choice 11). Paging follows what came back: the next cursor is `startAt + issues.len()`, none when that reaches `total` or the page is empty (the server may cap `maxResults` below what was asked).
- Get: key parsed with `^[A-Z][A-Z0-9_]+-[1-9][0-9]*$` and at most 100 characters, then `GET /rest/api/2/issue/{key}` with the detail fields but **not** `comment`. Comments come from `GET /rest/api/2/issue/{key}/comment?orderBy=-created&maxResults=5`, so a ticket with a thousand comments costs one bounded page. Description and comments as plain text.
- `item_url`: `{base}/browse/{key}`, `http`/`https` only; remote `self` links ignored.
- Probe: `GET /rest/api/2/mypermissions`. Data Center may answer with every permission; `BROWSE_PROJECTS.havePermission` is picked out locally. It is advisory: true means "can browse some project", the reason says so, and a project the user can't see still answers `NotFound`.
- Error mapping: 400 on search → `InvalidInput` with Jira's message (bad JQL); 401/403 → `MissingScope`; 404 on get → `NotFound`.
- Tests with recorded JSON: each filter's JQL; text holding `" OR project = X` and a trailing `\` stays one literal; `native` with a non-empty filter → `InvalidInput`; paging with a server that returns 20 of 50 asked; malformed cursors (`-1`, `1e3`, `٣`, 30 digits) → `InvalidInput`; keys `abc-1`, `ABC-01`, `ABC-1/../../myself`, `ABC-1?x`, `ABC-١`, 101 characters → `InvalidInput` without a request; search and issue mapping including missing assignee and reporter; a fixture whose `self` points elsewhere doesn't change `url`; a `javascript:` base gives no URL; probe answers with the full map, with `false`, and an error.

### Slice 5: GitLab read — 2 days (uses Q5)

- `core/src/actions/gitlab/mod.rs` + `tests.rs`, `cursor.rs`: `GitLabActions` (`[Search, Read]`), wired in `provider/gitlab/mod.rs`.
- Keys: `group/subgroup/project#iid` for issues, `…!iid` for merge requests. Path segments of letters, digits, `.`, `_`, `-`; no empty segment, and no segment that is exactly `.` or `..`; the iid a positive number; at most 255 characters. The project path is pushed as **one** segment (`group%2Fapp`), never pre-encoded, so it is never double-encoded.
- "Me" is the authenticated user from the check (`RemoteIdentity.id`), not the configured `user`: `assignee_id`, `not[assignee_id]`, `author_id`, `not[author_id]`, which take single numbers on both issues and merge requests (no username arrays to encode). Without an identity the Me/NotMe filters answer `Unavailable` ("GitLab did not say who the token belongs to").
- **Search is a merge of streams.** A query becomes up to three streams, each with its own `state`: issues and merge requests for Open (`opened`); for Done (Q5) closed issues, merged and closed merge requests; for Any, `all` of each. `GET /api/v4/issues` and `/merge_requests` with `scope=all`, or under `/projects/{path}/…` when the filter names a project; `search` for text; `order_by=updated_at`; `assignee_id=None` for Unassigned. `native` → `InvalidInput` ("GitLab has no query language here").
- **The cursor is opaque and composite** (`cursor.rs`): a version and, per stream, the absolute number of items already returned or "exhausted", base64url-encoded. A page fetches, per stream that isn't exhausted, the GitLab pages that cover `[offset, offset + limit)` (at most two pages of `limit` each, the first skipping `offset % limit`), merges them by `updated` descending with ties broken by stream order then key, returns at most `limit` items in total, and advances each stream's offset by what it actually contributed. Items fetched but not returned are fetched again next time; nothing is skipped. A stream that returned fewer than asked is exhausted. Items created between pages can still shift offsets, as with any offset paging; the dashboard drops duplicate refs.
- Get: `GET /projects/{path}/issues/{iid}` or `…/merge_requests/{iid}`. Comments: for issues `…/notes?activity_filter=only_comments&sort=desc&per_page=5`; for merge requests (no such filter) `…/notes?sort=desc&per_page=20`, system notes dropped, at most two pages, so the promise is "up to 5 of the latest 40 notes". `time_stats` → `TimeSummary`.
- `item_url`: `{base}/{path…}/-/issues/{iid}` or `/-/merge_requests/{iid}`, from segments; `web_url` ignored.
- Probe: `GET /api/v4/personal_access_tokens/self`. `read_api` or `api` in `scopes` → `Granted`; a known scope list without either → `Missing`; any granular or unrecognised scope, a 404 (old GitLab, other token types) or an error → `Unknown`.
- Tests:
  - Refs `../x#1`, `group//app#1`, `group/./app#1`, `group/app#0`, `group/app#1/notes`, `%2e%2e/app#1`, `//evil.example/x#1`, spaces, 256 characters → `InvalidInput` without a request; the built URL's host is always the configured one; `group/app` arrives as `group%2Fapp`, not `group%252Fapp`.
  - Text `a&state=all` stays one encoded value; `not[assignee_id]` is encoded as one parameter; each filter's parameters per stream.
  - Cursor: skewed streams (all items from one stream, then the other); one stream exhausted early; ties on `updated`; the combined limit is never exceeded; walking every page of recorded streams returns each item exactly once; malformed cursors (bad base64, unknown version, unknown or repeated stream, negative or huge offset, trailing bytes) → `InvalidInput`.
  - Identity: Me uses the token's user id even when the configured `user` differs; no identity → `Unavailable`.
  - Comments: an MR fixture with eight recent system notes and three comments returns the three; an issue uses `activity_filter`.
  - Probe with `read_api`, `api`, `read_user` only, a granular scope list, 404.
- Acceptance on a real instance (recorded in the slice's review): NotMe includes unassigned items; closed-unmerged merge requests show under Done.

### Slice 6: Tauri commands, opener and CSP — 1.5 days (uses Q2)

- `desktop/src/commands/mod.rs` + `tests.rs`, using the transport envelope (`tauri-backend-transport.ts` sends `{ request }`); both fail with a `BackendError` like every command:
  - `run_action(state, request: ActionCall) -> HostActionResponseDto` builds `Caller { origin: Desktop }`, awaits `Host::action_target(connection)` (no lock is held across the read) and runs `actions::run(caller, &target, request)`; the answer is `{ context?: AccessContext, response: ActionResponse }` (shared-contracts §9). A settings save is never blocked by a slow read.
  - A read that already has its target when an edit installs finishes on that target (the old settings, token and `allow`); its answer carries the old `context`, and the dashboard drops it when the connection's `access` changed (slice 9).
  - `open_work_item(state, request: WorkItemRef)` gets the URL from core (`actions::item_url`: kind, parsed key, configured base, `http`/`https`, same origin, no user info) and opens it with the opener (Q2), as `open_url` does for event links.
  - Both registered in `desktop/src/lib.rs`; their names join the generated command names the dashboard's operations use.
- `desktop/tauri.conf.json`, `app.security`:

  ```json
  "csp": {
    "default-src": "'self'",
    "script-src": "'self'",
    "style-src": "'self' 'unsafe-inline'",
    "img-src": "'self' data:",
    "font-src": "'self'",
    "connect-src": "ipc: http://ipc.localhost",
    "object-src": "'none'",
    "frame-src": "'none'",
    "base-uri": "'self'",
    "form-action": "'none'"
  }
  ```

  - `devCsp` is the same with `connect-src` widened to `ipc: http://ipc.localhost http://localhost:3200 ws://localhost:3200` for the dev server and its live reload.
  - No `unsafe-inline` or `unsafe-eval` for scripts. The inline theme bootstrap in `dashboard/src/index.html` has to pass by the hash Tauri adds for inline scripts in bundled HTML; if it doesn't, it moves to a file.
  - `'unsafe-inline'` only for styles: Angular injects component styles at runtime. A browser ignores `'unsafe-inline'` once a hash or nonce is in the same directive, so if Tauri adds style hashes, `style-src` goes into `dangerousDisableAssetCspModification`.
  - No `asset:` source: the app doesn't use it.
  - Checked in `pnpm desktop:dev` and in a packaged build (Logs pane, settings, theme switch, Work items, no CSP errors in the console).

- No route in `api/`: actions stay off HTTP.
- The Tauri app command manifest stays deferred; the CSP is the control that matters while only the app's own code runs. When added: `tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(&[…])))` in `desktop/build.rs`, and `allow-run-action` and the others in `capabilities/default.json`.
- Tests (`desktop/src/commands/tests.rs`, over Tauri's mock IPC as the other commands' tests are): both commands deserialize from `{ "request": … }` and refuse the bare payload; `search` off → `NotAllowed`; the origin is always `Desktop`; an unknown connection answers `Unavailable` without a `context`; a settings save while a fake slow read is in flight completes at once, and the read then returns its answer with the old `context`; `open_work_item` with a bad key, a `javascript:` base or a base with user info opens nothing (opener behind a function seam); the CSP in `tauri.conf.json` has `script-src 'self'` and no `unsafe-eval` or script `unsafe-inline`.

### Slice 7: CLI `search` and `show` — 1.5 days

- Lands after R-L1 (merged plan step 12), which reworks `cli/src/main.rs` and `cli/src/range/`.
- `cli/src/main.rs`: `Command::External(Vec<String>)` through clap's `allow_external_subcommands`, parsed by a second clap parser in `cli/src/actions/mod.rs` + `tests.rs`.
  - Built-in subcommands (`sources`, `config`, `help`) win; a connection named like one is reached as `kind/name`.
  - The global `--config` works before or after the target; given twice, it's an error.
  - Collect flags (`--since`, `--until`, `--range`, `--output`, `--format`) together with a target → "Collect options can't be used with jira search".
  - `activity-log --help` lists `<source> search|show` in its footer; `activity-log jira --help` prints the action parser's help.
- Target: a kind name (`jira`) means its only connection, with an error naming them when there are none or several; `kind/name`; a bare name through `resolve_connection`. A kind name wins over a connection of the same name, which stays reachable as `kind/name`. Only that connection is checked, then `actions::run(Caller { origin: Cli }, &target, request)` with a target built from that one runtime (no host, `context: None`; shared-contracts §9).
- `activity-log jira search [TEXT] [--mine | --unassigned | --not-mine] [--reporter me|not-me] [--state open|done|any] [--project KEY] [--jql JQL] [--limit N] [--cursor C] [--json]`; `--jql` with filter flags is refused (choice 11).
- `activity-log gitlab show group/app#12 [--json]` (`get` is an alias).
- **Every human-readable line goes through one `terminal_safe()`**: items, details, comments, failures, check and probe reasons, also in the existing `sources` list. It escapes control and bidirectional characters. `--json` prints the `ActionResponse` (JSON escaping covers it). A `Failed` response exits non-zero.
- Tests: argument → `SearchQuery`; target resolution (kind with zero, one, two connections; `kind/name`; bare name; a connection named `config` and one named `gitlab` of kind Jira); `--config` in both positions; collect flags with a target; `\x1b[2J`, `\r`, U+202E in a title, a comment, an error message and a probe reason are escaped; `--jql` on GitLab gets core's `InvalidInput`.

### Slice 8: keyboard shortcut registry — done (5870ed0, 90fd482, e8a6402; macOS menu 6260dbf, aea479e)

Built as merged plan steps 10 (R-F14, with native decisions 3, 4, 6, 11, 12, 15 and 16) and 10a (NF-6, the macOS menu). The types, the resolution rule and the menu contract are in [shared-contracts.md](shared-contracts.md) §8; AGENTS.md ("Keyboard shortcuts") describes the behaviour. What exists:

- `dashboard/src/app/models/shortcuts.ts`: `SHORTCUT_IDS` and `ShortcutEntry { id, label, scope, chords, menu? }`, each `Chord { keys, os?, where?, whileTyping, menuAccelerator }`; `resolveChord` (a view scope's entry wins over a global one with the same chord) and `chordsIn`. Scopes are `global` and `timeline` (`SHORTCUT_SCOPES`); the first draft's `calendar` scope was not needed, since the calendar views have no field. Entries: `app.collect` (⌘R, F5 off macOS), `app.filter` (⌘F, "Filter Activity…", desktop only), `timeline.filter` (`/`, Timeline scope), `view.list`/`view.day`/`view.workweek`/`view.week` (⌘1–⌘4, desktop only), `app.settings` (⌘,), `app.export-markdown`, `app.export-json`, `app.copy-link` (menu items without a key), `app.back` (⌘[ on macOS, Alt+← elsewhere), `layer.close` (Escape), `app.help` (`?`). In the browser only `/`, `?` and a layer's Escape are the app's.
- `dashboard/src/app/utils/key-chord.ts`: parse, match and format (`mod` is ⌘ on macOS, Ctrl elsewhere).
- `dashboard/src/app/services/shortcut.service.ts`: the one key listener; `bind` (a stack per entry), `fallback`, `openLayer({ kind: 'page', hidesRoute } | { kind: 'modal' })`, `triggerChord`, `choose`, `handOff`, `resolve`, `available`, `menuOwnsAccelerators`. The scope comes from the deepest route's `data.shortcutScope`; a `page` layer that hides the route leaves only `global` entries. `scripts/shortcut-listeners.test.mjs` (in `dashboard:lint`) refuses a document or window key listener anywhere else.
- `dashboard/src/app/layouts/shell/shell-shortcuts.ts`: the app's own commands as fallbacks (Collect, ⌘F to the Timeline's filter through `handOff('timeline.filter')`, Back, the views, Settings, Export, Copy Link, help).
- `dashboard/src/app/components/shortcut/`: `[appShortcut]` (`shortcut.directive.ts`: clicks its host or focuses a field, sets `aria-keyshortcuts`), `<app-key-tag>` (key tags on text buttons and fields, not on icon buttons, not where the pointer cannot hover or in the phone layout), and the `?` help (`shortcut-help-dialog.component.*`).
- Escape and Back: the event details and the phone's Settings tab bind `layer.close` and `app.back` while they are on top (`TopLayer`); the search bar's `/` became `timeline.filter`.
- Desktop: `desktop/src/browser_keys/` turns WebView2's own browser keys off on Windows; `desktop/src/app_menu/` installs the macOS menu from the `MenuSpec` that `services/native-menu/` derives from the table (`menuPlanOf`: View lists the views per `NAV_GROUPS` group).

Left for slice 9:

- `SHORTCUT_SCOPES` gains `work-items`, which the Work items route declares.
- `work.search` ("Search work items…", `work-items` scope, ⌘F and `/`, in the Edit menu): it shares ⌘F with `app.filter`, so the one menu item's title follows `resolve('mod+f')`, as shared-contracts §8 describes.
- `work.open` (`o`, opens the selected item in its source). ↑/↓ and Enter are the list's own keys on its element, as the timeline's are, not registry entries.
- `VIEW_SHORTCUTS` is keyed by `ViewMode`, so `work-items` needs a `view.work-items` entry, listed in View with its group. Its chord is not decided; the native-feel plan kept ⌘5 onward free for the Work group.

Planned for the registry after step 11, outside this step (merged plan; UI decisions of 2026-11-21, Q4–Q6): browser keys as alternate unmodified chords on the existing entries (`R` Collect, `1`–`4` the views, `,` Settings, `/` filter), `P`/`N`/`T` in a new `calendar` scope and `P`/`N` in a new `details` scope, the same keys in the desktop app with the macOS menu unchanged; a "Single-key shortcuts" switch in Settings › General (on by default, stored on the device) that turns every unmodified character key off, `/` and `?` included; and letters matched by physical key (`KeyboardEvent.code`), punctuation by the character typed. `work.open`'s `o` is one of the keys that switch turns off.

### Slice 9: Work items view — 3.5 days (uses Q1)

Written against the routed layouts of merged plan step 8, the push contract and the step 11 services; shared-contracts §5 (Work keys), §7 (`?item=`, Work availability) and §9 are part of this slice.

- **Operations**: `services/backend/backend-contract.ts` gains `runAction` (`run_action`) and `openWorkItem` (`open_work_item`), desktop-only (`http: null`; the HTTP transport rejects them as `unsupported` without a request), with methods in `backend-client.ts`. They reject only with a `BackendFailure`.
- **URLs** (`utils/app-url.ts`, shared-contracts §7): `UNDATED_VIEWS` gains `'work-items'`; `AppLocation` gains its variant with `WorkDetail { item }`; `WorkItemKey` and `parseWorkItemKey`; `queryParamsOf` sets the other family's parameters to `null`; a guard on the activity and Work routes removes a parameter of the other family (replace, history depths unchanged). `DeepLinkService` validates `activity-log://work-items?item=…` against the routes like any link; the desktop Share (`components/share/share-link.ts`, `appLink`) keeps `item` beside `q`, `from`, `to`, `event` and `events`.
- **Route** (`app.routes.ts`): `work-items` is a direct child of `ShellLayoutComponent`, the sibling of `ViewsLayoutComponent`, so it has no range, Collect or source chips; `loadView(…)` like the other views; `data.shortcutScope: 'work-items'`; `canMatch` by the Work availability table (shared-contracts §7). Export and Share stay in the header there (native decision 17).
- **Navigation** (`models/nav.ts`, layout CHANGE 2): `NavGroupId` gains `'work'`, and `NAV_GROUPS` a `work` group (Work items), shown in the desktop app when some connection's `search` is allowed; each group gains its phone tab's label and icon. `components/sidebar/` draws the groups already, so the sidebar shows the Work group under the views; the rail puts its icon under a divider; the macOS View menu lists it with its group (`menuPlanOf`).
- **Phone bar** (native decision 14; UI decisions of 2026-11-21, Q14): the sidebar's phone form, today Timeline · Day · Work wk · Week · Settings, becomes one tab per group, with a segmented switch inside Calendar (Day, Work week, Week) and Work (`components/segmented-control/`). The desktop app shows Timeline · Calendar · Work · Settings; the browser, which has no Work group, gets the grouped bar too, Timeline · Calendar · Settings, so there is one phone layout. Settings stays the last tab. No extra button beside today's four views.
- **Fallback**: when the policy rows of §7's table hide the Work group while it shows, a replace navigation goes to `/timeline` without `item`; never before a pair or on a transport fault (shared-contracts §9).
- **Motion** (`navigation-motion.ts`, layout CHANGE 2, still to do): `navigationMotion` cross-fades between calendar views by `isCalendarView`, not by the registry. With the Calendar group it reads the group instead: `switch` only between members of a group that shows a switch, and changing tabs or groups keeps still. Work items entering or leaving keeps still. `pageDepth` counts `item` like `event`, so the phone's detail page slides in and out.
- **`services/work-items.service.ts`** (`WorkItemsService`): the query (text, chips, connection choice, JQL mode), results and cursor per connection, the selected ref (from the URL), details by ref, loading and per-connection errors.
  - Searches run on a deliberate submit: Enter in the search box, or a chip change after a 300 ms pause; typing alone doesn't search.
  - Each submit gets a generation number; answers of an older generation are dropped, so a slow earlier search never overwrites a newer one.
  - Every result, detail, cursor and outstanding read is keyed by `(epoch, connection, access)` (shared-contracts §5); an answer is applied only if its `context` equals the installed one for its connection (§9). The search re-runs only when a connection's `access` changes, never on an ordinary providers update (§9 replaces the first draft's "a changed providers response re-runs the current search").
  - Results are deduplicated by ref and sorted by `updated` among what is loaded.
  - It reads the pushed providers from `ActivityDataService` and never goes through `ActivityService`, which step 11 deletes.
- `utils/work-item-filter.ts` + spec: chips → `SearchFilter`; JQL mode sends `SearchFilter::any()` and disables the chips.
- **`components/work-items/`**: search box "Search work items…" bound to `work.search` with `[appShortcut]` (its `/` key tag), the chips, the JQL toggle (one Jira connection chosen), the list (key, title, status, priority, assignee, updated; ↑/↓ and Enter on the list), "Load more", empty and per-connection error states. Styles follow the touch rules: `ui.hover`, `ui.field-text` for the search box, `overscroll-behavior` on the list.
- **`components/work-item-detail/`**: fields, description and comments as plain text (interpolation only, no `innerHTML`; remote text in `unicode-bidi: isolate` spans), time spent and estimate, "Open in `<source>`" through `openWorkItem`, named by the item's own source with the label from the source catalog (`providers/source-catalog.ts`: "Open in Jira", "Open in GitLab"). No other actions.
  - **Stack** (UI decisions of 2026-11-21, Q12): a merge request opened here shows the same shared Stack section as the event details of merged plan step 16 (`<app-change-stack>`, its parents and children). Core computes it when the item is opened, through step 16's provider-neutral walk and within its budget rules, and `WorkItemDetail` carries it as the same `ChangeStack` type (`changeStack?`); slice 5's GitLab `get` fills it. If step 16 has not landed when slices 5 and 9 are built, this part follows right after it.
- **`components/detail-panel/`** (R-F13, moved from step 11 to this slice by the coordinator on 2026-11-21, after its behaviour landed in `event-detail`):
  - Already in `components/event-detail/`: Escape (`layer.close`) and `app.back` bound while the details are on top of `TopLayer`; `openLayer({ kind: 'page', hidesRoute })` on a phone; a full page on phones that slides in and out (0254071); the panel's own `@if` with its enter and leave bindings (layout CHANGE 3); focus saved on open, restored on close and taken back through `TopLayer.whenExposed`; a close that consumes one navigation (`ActivityViewStateService.closeDetail`, `detailDepth`); everything under the details `inert`.
  - The lift moves that chrome into the panel (the backdrop, `aside.flyout` with its role, `aria-modal`, focus trap and motion bindings, the layer and shortcut bindings, focus save and restore, `inert` under the settings dialog); `event-detail` keeps its pages (the group list, the event, pending), and the work-item detail is the second consumer. The panel takes its content as an `ng-template`, so the content lives only while it is open; prove first that a page swap inside it still slides and that a closing panel does not slide its page twice.
  - `TopLayer`'s `ShellLayer` gains the work-item detail, which is modal like the event details wherever it is not docked.
  - Layout: docked beside the list at `desktop-compact` and wider (`BreakpointState.desktopCompact`, `@include ui.up('desktop-compact')`), the flyout below, and on phones a full page, as event details are (native decision 13; no bottom sheet). Docked, it has no page motion and selecting another item swaps its content in place.
  - The open item is in the URL (`?item=<source>/<connection>/<key>`), with Back like `?event=`: opening pushes one entry with `detailDepth` + 1, selecting another replaces it, closing goes back past `detailDepth` or replaces the URL after a link or a reload (`ActivityViewStateService`, `HistoryStep`).
- Tests: group hidden in the browser build and when no connection allows search; the §7 availability rows (a cold deep link during checks, losing the last allowed connection while on the route, a failed handshake keeping the URL); chips → filter; merge of two connections and one failing connection keeping the other's results; a stale answer arriving after a newer one is dropped; an answer with another `context` is dropped; duplicates across "Load more" dropped; a `<img src=x onerror=…>` description shows as text; open passes the ref, not a URL; docked, flyout and page at their widths in a layout spec (`*.layout.spec.ts`); `/`, ⌘F, `o`, and ↑/↓ and Enter on the list; Share on Work never carries `q` or `event`, and on the Timeline never `item`; the `?item=` round trip with `/` and `#` (§7).

### Slice 10: Access settings — 1.5 days

- Contract, only the access fields here: `ProviderConfigConnectionStateDto.allow` and `.agent_allow`; `ProviderConfigSaveRequestDto.allow` and `.agent_allow` (optional). `core/src/settings/view/`: `build_snapshot` fills them, `normalize`/`prepare_save` check them like other values (a save's issues as `ProviderConfigFieldIssueDto`), and `apply_save` writes them with `set_access` inside `settings::edit_config`; the desktop's save (`desktop/src/provider_config/`, `basedOn` as its precondition) and `check_provider_config` need no change of their own. The host installs the edit, which changes the access token only (slice 2), so the lists apply at once and Work state is invalidated by the pushed `access`; fixtures for the snapshot DTO update with it.
- `components/settings-dialog/source-pane/access-section/` (`settings-access-section.component.*`), desktop only, a section of the source pane beside its health (`source-health/`):
  - Kinds with actions: "Search and read items" (one toggle for both), with the probe's grant ("Token can", "Token lacks `read_api`", "Can browse some projects", "Not checked").
  - A section of the pane, not a tab (generic choice 6; UI decisions of 2026-11-21, Q15); there is no access summary in the sidebar.
  - Not in this step (UI decisions of 2026-11-21, Q16): the "AI access" select (Off / Read only, Custom when the list matches neither; Advanced with one checkbox per capability the kind implements, and `read` alone for kinds without actions, Q3), `utils/agent-access.ts` (list ↔ level) and the small AI badge on the source chip of a connection whose AI access is on. They come with the MCP step, once an agent can connect. Until then `agent_allow` is stored, carried through the snapshot and saves unchanged, and enforced by core.
- The pane's draft (`SourceDraft`, `source-draft-request.ts`) records the lists as intent like its other edits, and `ConnectionSettings` (`services/connection-settings.service.ts`, which replaced `ProviderConfigFacade` in step 11) sends them with the save.
- Tests: defaults shown when the keys are missing; saving the default removes the key (desktop test); turning search off hides the Work group after the save; a hand-written `agent_allow` survives a save of the section unchanged and nothing shows it; the section is absent in the browser build, like the other editable settings. The select's tests (its lists for Jira and for Git, Custom for a hand-written `agent_allow = ["search"]`) move with it to the MCP step.

### Slice 11 (Q4): agent projection in core — 1 day, with the MCP step, required before any agent transport

- `core/src/model/agent/`: `AgentEntry { connection, kind, at, title, key }`, `AgentWorkItem`, `AgentWorkItemDetail` (not exported to TypeScript).
- `core/src/actions/agent/mod.rs` + `tests.rs`: `visible_connections` from `Access::for_origin(Agent)` having `read`; `project_activity(&ActivityLog, &visible)` and `project_item(…)`, recursive: a Git entry's `gitlab_event` only when its GitLab connection is visible, never `raw`; summaries, counts, `pending` and errors name only visible connections; titles 200 characters, descriptions 2,000, the last 5 comments at 500; no URL except the core-built one; text through the slice 1 escaping.
- Tests: allowed Git with denied GitLab → a merged entry has no GitLab part, no `raw`, no GitLab count, no GitLab name in `pending` or errors; truncation; a remote `web_url` never appears.

## Order and total

1 → 2 → 3 → (4 ∥ 5) → (6 ∥ 7) → 9 → 10, with 8 any time before 9; 11 with the MCP step. Slices 1 and 8 are done. Of the rest, slice 7 waits for R-L1 (merged plan step 12), and slice 9 for the end of step 11 (stream A's calendar grid and Z, which deletes `ActivityService`), since both edit the views, `event-detail` and the shell.

| Slice | 1   | 2   | 3   | 4   | 5   | 6   | 7   | 8   | 9   | 10  | Total    |
| ----- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | -------- |
| Days  | 1.5 | 1   | 2   | 1   | 2   | 1.5 | 1.5 | 1   | 3.5 | 1.5 | **16.5** |

Plus about a day for integration across slices and the packaged-app check (CSP, opener, Windows build): **about 17.5 focused days**, without slice 11. The first version of this plan said 13; its own slices added up to 14, and the review changes add 2.5. With slices 1 and 8 done, **about 15 days** are left.

## Review changes

All taken; none rejected. Each was checked against the code.

| Point                                    | Checked                                                                                                     | Where         |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------- |
| GitLab composite cursor                  | A single page number lost or repeated items when two lists are merged                                       | Slice 5       |
| `{ request }` envelope                   | `tauri-backend-transport.ts` sends `{ request }`; the first draft named the parameters `call` and `item`    | Slice 6       |
| Redirects and credentials                | `http_client_builder()` follows redirects; `PRIVATE-TOKEN` is a custom header reqwest doesn't strip         | Slice 1       |
| Error boundary                           | reqwest's error text includes the URL; the first draft escaped only CLI item text                           | Slices 1, 7   |
| Lock during actions                      | `collect` holds the runtime read lock; a long read under it would block `update_config_at`'s write lock     | Slice 6       |
| Latest comments                          | `fields=comment` returns every comment; GitLab notes include system notes                                   | Slices 4, 5   |
| GitLab "me"                              | The check calls `/user` but discards it; `config.user` is whose events are collected, not the token's owner | Slices 1, 5   |
| MR "Done"                                | `state=merged` alone drops closed-unmerged merge requests                                                   | Q5            |
| Every DTO literal in slice 3             | `ProviderKindDto` at `desktop/src/provider_config/mod.rs:137`; `ProviderInfo` literals in core, api and cli | Slice 3       |
| `/` handler migration                    | `search-bar.component.ts` listens on `document:keydown`                                                     | Slice 8       |
| CLI details, paging, probes, limits, CSP | As listed in the slices                                                                                     | Slices 3–7, 9 |

## Docs to update

- `README.md`: the Work items view, `activity-log <source> search|show`, `allow` and `agent_allow` in the config example, actions not served over HTTP.
- `AGENTS.md`: `core/src/actions/` and `core/src/model/actions/`; `allow`/`agent_allow` as reserved fields; the `run_action`/`open_work_item` commands ("no HTTP route") and `HostActionResponseDto`; the `work-items` route, the Work nav group, the per-group phone bar, `?item=` and `components/detail-panel/`; "CLI output of remote text goes through `terminal_safe()`". Already there: the `JiraApi`/`GitLabApi` rules (slice 1) and the shortcut registry with its one listener (slice 8).
- `docs/adr/0006-provider-actions.md`: option C (typed verbs, one executor), origins, per-connection `allow`/`agent_allow`.
- `docs/plans/provider-actions.md`: updated with this plan (status line, phases, section 7, the superseded points).

## Risks

- **GitLab filters** (`not[assignee_id]` including unassigned items, merged and closed states, `activity_filter`) are checked on a real instance as slice 5's acceptance; if one differs, that chip maps differently for GitLab and the UI says so.
- **The CSP can break the packaged app** (runtime styles, the inline theme script, Tauri's IPC origin); slice 6 is done only after a packaged build has been clicked through.
- **Offset paging drifts** when items change between pages; duplicates are dropped by ref, a skipped item shows on the next search.
- **The probe adds one request** per Jira and GitLab connection to every startup check, and to every check an install or Check again runs.
- **Remote text in the webview** relies on Angular's escaping plus the CSP; a later `innerHTML` for rich text would undo it.
- **AI access is enforced before anything shows it** (UI decisions of 2026-11-21, Q16): its tests carry it alone until the MCP step, and a hand-written `agent_allow` has no visible effect until then.

## Not verified while planning

No network access while writing this; each is checked in the slice that uses it.

- Jira Data Center: `mypermissions` with and without the `permissions` parameter; `/issue/{key}/comment` with `orderBy=-created`.
- GitLab: `personal_access_tokens/self` (version, `granular_scopes`), `not[assignee_id]`/`not[author_id]` and `assignee_id=None` on issues and merge requests, `activity_filter=only_comments` on issue notes, and `/issues?scope=all` speed on large instances.
- Tauri 2: the CSP keys, the hashes it adds to bundled HTML (inline scripts and styles), `dangerousDisableAssetCspModification`, `devCsp`, the app manifest API, and `tauri-plugin-opener` used from Rust without a webview permission.
- clap: `allow_external_subcommands` with the flattened collect arguments and a global `--config`.
- That step 11 has finished before slice 9 lifts the detail panel chrome out of `event-detail` (the calendar work it once waited for has merged).
- Checked since: reqwest's redirect policy and the headers it strips (slice 1, 33cf813).
