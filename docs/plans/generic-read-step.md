# Generic read step: search and open work items in Jira and GitLab

Status: implementation plan, not built; revised after an independent review (approved with changes, all taken; see "Review changes"). It turns the read half of [provider-actions.md](provider-actions.md) (phase 0 and 1a, plus the agent policy of 1b) into ordered slices, with the user's decisions from [open-decisions-design.md](../open-decisions-design.md) applied. The step is generic: the action model, executor, permissions, contract, CLI and Work items view are built once and proven on Jira (Data Center) and GitLab together. Odoo, writes, time tracking and the agent transport (MCP) come later and add to what this step builds.

## Rubric

| Dimension      | Score | Why                                                                                                                                                                    |
| -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope          | 2     | Every layer: core (API clients, settings, actions, runtime), Tauri, CLI, generated contract, dashboard (new view, settings section, shortcut registry)                 |
| Design impact  | 2     | The executor, `ActionRequest`/`ActionResponse`, `Capability`, `allow`/`agent_allow` and the `ActionProvider` trait are what every later phase builds on                |
| Failure impact | 1     | Read-only, so nothing remote changes. What can go wrong: token leaks (redirects, errors), path or query injection through keys, remote text in the webview or terminal |
| Uncertainty    | 1     | The architecture is reviewed. Some GitLab and Jira Data Center parameters and the CSP's effect on the packaged app are not verified (end of doc)                       |
| **Total**      | **6** | Q1–Q5 under "Decisions for the user" are decided (user, 2026-11-18), all with the recommended defaults                                                                 |

## What the step delivers

- Search Jira and GitLab from the desktop app and the CLI, including items that aren't yours, and open one item's details: description, labels, latest comments, time spent and estimate.
- Per-connection access in `config.toml` (`allow`, `agent_allow`), enforced in core for every caller, shown and edited in the settings next to what the token can do.
- AI access per connection (Off / Read only), stored and enforced by core's policy now, so the later MCP step adds the transport and the agent projection.
- A keyboard shortcut registry with key hints and a `?` help dialog, first used by the Work items view.

Out of scope: writes (no `plan`/`apply`, no confirm), `actions.jsonl` (reads go to the diagnostics log only, as the design says), time entries, the agent socket and MCP adapter, Odoo, the ⌘K command bar, the Time and Review views.

## Design choices this plan makes

These follow from the reviewed design but narrow or adjust it. Each is recorded in [provider-actions.md](provider-actions.md) as well.

1. **Reads only, so no `ActionCall` enum yet.** `run_action` takes `ActionCall { connection, request }` as a plain struct. When writes arrive, confirmation becomes its own command, `confirm_action(id)`, instead of a `Confirm` variant. Equally safe (the confirm still carries only the id), and `run_action` never has a variant it can't serve.
2. **`Capability` has the design's six names now** (`search`, `read`, `create`, `comment`, `update`, `log_time`), so a config written from the docs parses without warnings. What a kind **does** is `ActionProvider::capabilities()`: `search` and `read` for Jira and GitLab in this step, nothing for kinds without actions. Settings, probes and the CLI show only what the kind implements. `attendance` joins with Odoo.
3. **Probes check only what is shown**, and are advisory: they never change a connection's status. Write phases add their permissions to the same probe.
4. **`ActionRequest` and `ActionResult` hold only `Search` and `Get`; `ActionErrorCode` only the read codes** (`Conflict`, `OutcomeUnknown`, `Expired` come with writes). Adding variants later is additive.
5. **Opening an item takes its ref, never a URL.** `open_work_item` makes core parse the key again and build the URL from the connection's configured base.
6. **The Access section is a section in the source pane, not the mockup's tab**; a tab can come when write toggles make the pane long.
7. **"Read and prepare changes" is hidden until a write capability exists.** The AI select shows Off / Read only, and Custom when Advanced sets something else.
8. **The CLI verb is `show`**, as in the reviewed design, with `get` as a visible alias to match the request name.
9. **`WorkItemDetail.time` has spent and estimate only**; "you: 1h today" needs worklogs (phase 2).
10. **The search filter has one project**; the mockup's multi-project chip comes later.
11. **Native JQL and filters don't mix.** A `SearchQuery` with `native` set must carry the empty filter (`SearchFilter::any()`), else `InvalidInput` ("Filters and JQL can't be combined"). The dashboard disables the chips in JQL mode, the CLI rejects `--jql` with filter flags.

## Decisions for the user

The reviewer reached the same answers on Q1–Q4 independently. **Decided by the user on 2026-11-18: all five with the recommended defaults**, as recorded under each question; the plan is written with them.

**Q1. Does remote search ask one connection or all of them?** The name you chose, "Search Jira & GitLab…", suggests all; the mockup draws a connection picker.

| Option                                                                                 | User experience | Correctness | Risk | Cost  |
| -------------------------------------------------------------------------------------- | --------------- | ----------- | ---- | ----- |
| **All by default, picker narrows to one; JQL only with one Jira connection (default)** | good            | good        | good | mixed |
| One connection at a time, as drawn                                                     | mixed           | good        | good | good  |

**Decided:** remote search asks all connections by default, and the picker narrows it to one; JQL only with one Jira connection chosen.

One `Search` per connection that allows search. Each connection keeps its own cursor and its own inline error, so one failing source doesn't hide the others. The list sorts only what is loaded, by `updated`; "Load more" asks each connection that still has a cursor. Every query carries a generation number, and a response for an older generation is dropped. About half a day more in slice 9; core and the contract are the same either way.

**Q2. How does the app open an item in the browser?** Today the only opener is the Rust `open_url` command for event links (`tauri-plugin-opener` without a webview permission); the webview must not get a general one.

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

### Slice 1: shared API clients and the error boundary (phase 0) — 1.5 days

No new features. The check and the collector of each remote kind build the client, auth, paths and errors in one place.

- `core/src/provider/jira/api/mod.rs` + `tests.rs`: `JiraApi { base: Url, client: Client, token: Secret }`. `JiraApi::new(&JiraConfig)` normalizes the base (trailing slashes removed, a context path like `https://host/jira` kept). `endpoint(&self, segments: &[&str]) -> Url` pushes segments with `path_segments_mut()`; query values go in through `query_pairs_mut()`. Replaces `jira_client()` and the `format!` URLs in `provider/jira/mod.rs` and `collect/jira/mod.rs`.
- `core/src/provider/gitlab/api/mod.rs` + `tests.rs`: `GitLabApi` the same way. The collector keeps reading events of the configured `user` (`/users/{user}/events`, now one segment whatever the name holds). The startup check parses the `/user` answer it already fetches into `RemoteIdentity { id, username }` and hands it to the runtime (`ProviderCheckResult::with_identity`), for "me" in slice 5.
- **Credentials only go to the configured origin.** Both clients get their own redirect policy (`reqwest::redirect::Policy::custom`): a redirect is followed only to the same scheme, host and port as the configured base, so a cross-origin redirect and an `https` → `http` downgrade stop with `Remote` ("Jira redirected to another address; set the URL it redirects to"). `send` checks the request URL's origin against the base before it adds `Authorization` or `PRIVATE-TOKEN`. reqwest strips `Authorization` on a redirect to another host, but not on a scheme change, and never `PRIVATE-TOKEN`. The global `http_client_builder()` keeps default redirects, since the calendar feed may need them.
- `core/src/provider/remote_error/mod.rs` + `tests.rs`, the one error boundary:
  - `ApiError { Status { status, message, retry_after }, Timeout, Unreachable, Decode }`. Timeout, Unreachable and Decode have fixed messages ("Jira did not answer within 30 s", "Could not reach Jira", "Jira sent an answer Activity Log could not read") and never include reqwest's text, which carries the URL and its query.
  - `remote_message(status, body, secrets: &[&str]) -> String` keeps Jira's `errorMessages`/`errors` or GitLab's `message`/`error` only, replaces each known secret (the token, user info in the base URL) with `[redacted]`, turns control characters and bidirectional controls (U+061C, U+200E/F, U+202A–202E, U+2066–2069) into visible `\u{…}` escapes, and cuts to 300 characters.
  - Collectors return it instead of `{status}: {body}`, so the Logs pane and the CLI get the same cleaned text.
- `get_json` retries a read once on 429 or 503 when `Retry-After` is ≤ 10 s, otherwise returns the status with `retry_after`.
- Tests:
  - Paths with user names holding `/`, `..`, `?`, `#`, spaces; bases with a context path, one and several trailing slashes.
  - A JQL with quotes, `&` and `#` survives encoding.
  - Redirects (a local `TcpListener` answering 302, no new dependency): same origin followed; another host, another port and `https` → `http` stopped, and the second server never sees `PRIVATE-TOKEN` or `Authorization`.
  - Error cleanup with malicious strings inside the allowed fields: an `errorMessages` entry holding the token, ANSI `\x1b[2J`, U+202E, a URL with `user:pass@` → redacted and escaped; an HTML body never appears; the cap holds; a transport error's message has no URL.
  - `Retry-After` 3 s retries once; 30 s doesn't.
- Behaviour change to note in the commit: collector error messages get shorter.

### Slice 2: `allow` and `agent_allow` in `config.toml` — 1 day (uses Q3)

- `core/src/model/actions/capability.rs`: `Capability` (serde `snake_case`, ts-rs exported).
- `core/src/provider/connection/mod.rs`: `Access { allow, agent_allow: BTreeSet<Capability> }`, default `{search, read}` and `{}`; `Access::for_origin(Origin)` (`Agent` → `allow ∩ agent_allow`, else `allow`). `Connection` gets it through `with_access()`, so `Connection::new` callers don't change. `Origin { Desktop, Cli, Agent }` in `core/src/model/actions/origin.rs`, Rust only: no request can carry it.
- `core/src/settings/mod.rs`: `allow` and `agent_allow` join `name`, `label`, `wsl` as reserved fields (no "unknown setting" warning, not config keys, so not in the cache fingerprint). Missing `allow` → default; missing `agent_allow` → empty; anything that is not a list of strings → empty with a warning (fail closed); an unknown name → dropped with a warning. `ConfigFile::set_access(id, AccessList, Option<&BTreeSet<Capability>>)` writes a list, or removes the key when it equals the default.
- CLI parity in `cli/src/sources/mod.rs`: `sources set jira/work allow search read`, `sources unset jira/work agent_allow`.
- Tests: missing, empty, a string, a list with a number, an inline table → each to its documented set, for both keys; unknown name dropped and warned; accepted on every kind; changing `allow` keeps the cache scope; writing the default removes the key; comments survive; `for_origin(Agent)` without `agent_allow` is empty even when `allow` has everything.

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
- `core/src/actions/action_provider.rs`: `ActionProvider` with `capabilities()`, `probe(&ActionScope)`, `search(&ActionScope, SearchQuery)`, `get(&ActionScope, &str)`, `item_url(&ActionScope, &str) -> Result<Url, ActionError>`, unimplemented methods `Unsupported`. `ActionScope` carries the connection's context and the `RemoteIdentity` from its check. `ProviderAdapter::actions() -> Option<&dyn ActionProvider>` defaults to `None`.
- `core/src/actions/mod.rs` + `tests.rs`: `run(caller, &ProviderRuntime, &ActionLimits, ActionRequest) -> ActionResponse`, refusing in this order before any provider code. _Note (step 0): replaced by `run(caller, &ActionTarget, ActionRequest)` with the same refusal order; see `shared-contracts.md` §2 (`action_target`) and §9._
  1. the kind implements the request's capability → `Unsupported`
  2. `allow` has it → `NotAllowed` ("Search is off for Work Jira")
  3. `Access::for_origin(caller.origin)` has it → `NotAllowed`
  4. the connection is usable → `Unavailable` with the check's reason
  5. `ActionLimits`, then the provider, then errors mapped from `ApiError`
  - One diagnostics line per call under the connection's scope, without query text: `desktop search → 23 items (a91c03f2)`.
- `ActionLimits` (per connection, owned by the runtime): 2 reads at a time, at most 8 waiting (more → `RateLimited`, "Too many searches at once"), 20 s to get a slot (→ `Timeout`). A 429 or 503 whose `Retry-After` is not retried sets `blocked_until` for the whole connection; reads before it fail at once with `RateLimited` and the remaining seconds.
- `core/src/runtime/mod.rs`: `CollectionRuntime::run_action(caller, call)` finds the connection's check (awaiting it while it runs), uses the runtime's `ActionLimits`, and answers `Unavailable` ("No connection gitlab/x") for an unknown id. It never waits for or blocks a collection.
- Probe: `build_provider_info` (`core/src/provider/provider_runtime.rs`) calls `probe()` after an available or degraded check, inside the same timeout; on any failure the grants are `Unknown`. It never changes the status.
- Contract and every place that builds the changed DTOs (required fields, so they all move in this slice):
  - `ProviderInfo.capabilities: Vec<CapabilityState>` — `core/src/provider/provider_runtime.rs`, `core/src/runtime/tests.rs:65`, `api/src/tests.rs:32,42`, `cli/src/tests.rs:84,94`, `cli/src/sources/tests.rs:160`.
  - `ProviderKindDto.capabilities: Vec<Capability>` — `desktop/src/provider_config/mod.rs:137` and its tests.
  - `core/src/ts.rs` exports, `pnpm types:generate`, and the dashboard fixtures and specs that build `ProviderInfo` (`testing/activity-fixtures.ts`, `sidebar.component.spec.ts`, `activity-data.service.spec.ts`, `settings-source-pane` spec).
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

- `desktop/src/commands/mod.rs`, using the transport envelope (`tauri-backend-transport.ts` sends `{ request }`):
  - `run_action(state, request: ActionCall) -> ActionResponse` builds `Caller { origin: Desktop }`, clones the `CollectionRuntime` under a short read guard, **releases the guard**, then awaits `run_action`. A settings save is never blocked by a slow read. _Note (step 0): replaced by `Host::action_target` and the `HostActionResponseDto` envelope; see `shared-contracts.md` §9._
  - In-flight reads after a settings swap finish on the runtime they started on: the old connection settings and token, checked against the old `allow`. Their answer is still returned; the dashboard re-runs the visible search when the providers response changes, and drops answers of an older query generation (slice 9). The old runtime is freed when its last read ends.
  - `open_work_item(state, request: WorkItemRef)` gets the URL from core (`actions::item_url`: kind, parsed key, configured base, `http`/`https`, same origin, no user info) and opens it with the opener (Q2).
  - Both registered in `desktop/src/lib.rs`.
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
- Tests (`desktop/src/commands/tests.rs`): both commands deserialize from `{ "request": … }` and refuse the bare payload; `search` off → `NotAllowed`; the origin is always `Desktop`; a settings save while a fake slow read is in flight completes at once, and the read then returns its answer; `open_work_item` with a bad key, a `javascript:` base or a base with user info opens nothing (opener behind a function seam); the CSP in `tauri.conf.json` has `script-src 'self'` and no `unsafe-eval` or script `unsafe-inline`.

### Slice 7: CLI `search` and `show` — 1.5 days

- `cli/src/main.rs`: `Command::External(Vec<String>)` through clap's `allow_external_subcommands`, parsed by a second clap parser in `cli/src/actions/mod.rs` + `tests.rs`.
  - Built-in subcommands (`sources`, `config`, `help`) win; a connection named like one is reached as `kind/name`.
  - The global `--config` works before or after the target; given twice, it's an error.
  - Collect flags (`--since`, `--until`, `--range`, `--output`, `--format`) together with a target → "Collect options can't be used with jira search".
  - `activity-log --help` lists `<source> search|show` in its footer; `activity-log jira --help` prints the action parser's help.
- Target: a kind name (`jira`) means its only connection, with an error naming them when there are none or several; `kind/name`; a bare name through `resolve_connection`. A kind name wins over a connection of the same name, which stays reachable as `kind/name`. Only that connection is checked (`ProviderRuntime::inspect` on it alone), then `actions::run` with `Caller { origin: Cli }`.
- `activity-log jira search [TEXT] [--mine | --unassigned | --not-mine] [--reporter me|not-me] [--state open|done|any] [--project KEY] [--jql JQL] [--limit N] [--cursor C] [--json]`; `--jql` with filter flags is refused (choice 11).
- `activity-log gitlab show group/app#12 [--json]` (`get` is an alias).
- **Every human-readable line goes through one `terminal_safe()`**: items, details, comments, failures, check and probe reasons, also in the existing `sources` list. It escapes control and bidirectional characters. `--json` prints the `ActionResponse` (JSON escaping covers it). A `Failed` response exits non-zero.
- Tests: argument → `SearchQuery`; target resolution (kind with zero, one, two connections; `kind/name`; bare name; a connection named `config` and one named `gitlab` of kind Jira); `--config` in both positions; collect flags with a target; `\x1b[2J`, `\r`, U+202E in a title, a comment, an error message and a probe reason are escaped; `--jql` on GitLab gets core's `InvalidInput`.

### Slice 8: keyboard shortcut registry — 1 day

Built before the view that needs it; independent of the backend slices.

- `dashboard/src/app/models/shortcuts.ts`: a typed list, `ShortcutId` as a union of literals, each `{ id, chord, scope: 'global' | 'work-items', label, whileTyping?: true }`. Starting set: `?` help, `/` focus the current view's search, and in Work items `↑`/`↓` move the selection, `Enter` opens the detail, `o` opens in the browser, `Esc` closes the detail.
- `dashboard/src/app/utils/key-chord.ts`: pure parse, match and format (`mod` is ⌘ on macOS, Ctrl elsewhere).
- `dashboard/src/app/services/shortcut.service.ts`: one `keydown` listener on the document; views activate their scopes; a shortcut never fires while focus is in an input, textarea, select or contenteditable unless `whileTyping`, during IME composition, or while a dialog other than the help is open.
- `dashboard/src/app/components/shortcut/shortcut.directive.ts`: `[appShortcut]="'work.open'"` binds the shortcut to the host's click (or focus for inputs), sets `aria-keyshortcuts`, and renders the shortcut hint from the registry entry.
- `dashboard/src/app/components/shortcut/shortcut-help-dialog.component.ts`: opened by `?`, lists the registry by scope.
- **Migration:** the `/` handler in `search-bar.component.ts` (`(document:keydown)` → `focusOnSlash`) is removed; the search bar registers `/` through the directive. The local list-navigation handlers (timeline, date index, event detail) stay: they are widget behaviour, not app shortcuts.
- Tests: no shortcut fires in an input; `whileTyping` does; composition ignored; inactive scope doesn't fire; `/` doesn't focus the background search while the settings dialog is open; hints read ⌘ on macOS and Ctrl elsewhere; the help lists exactly the registry; hint and `aria-keyshortcuts` come from the entry; the search bar's existing `/` behaviour still works.

### Slice 9: Work items view — 3.5 days (uses Q1)

- `services/backend/backend-contract.ts`: `runAction` (`run_action`) and `openWorkItem` (`open_work_item`), desktop-only (`http: null`); methods in `backend-client.ts`.
- `services/types.ts`: `ViewMode` gains `'work-items'`. `models/view-option.ts`: `WORK_OPTIONS` for the sidebar's Work group.
- `components/sidebar/`: a "Work" group under the views with Work items, shown when `isTauri()` and some connection's `capabilities` has `search` allowed. Rail: an icon under a divider. Phone: one more button. If the current view is `'work-items'` while the group is hidden (browser build, search turned off), the view falls back to Timeline.
- `services/work-items.service.ts`: the query (text, chips, connection choice, JQL mode), results and cursor per connection, the selected ref, details by ref, loading and per-connection errors.
  - Searches run on a deliberate submit: Enter in the search box, or a chip change after a 300 ms pause; typing alone doesn't search.
  - Each submit gets a generation number; answers of an older generation are dropped, so a slow earlier search never overwrites a newer one. A changed providers response (settings save) re-runs the current search.
  - Results are deduplicated by ref and sorted by `updated` among what is loaded.
- `utils/work-item-filter.ts` + spec: chips → `SearchFilter`; JQL mode sends `SearchFilter::any()` and disables the chips.
- `components/work-items/`: search box "Search Jira & GitLab…" with the `/` hint, the chips, the JQL toggle (one Jira connection chosen), the list (key, title, status, priority, assignee, updated), "Load more", empty and per-connection error states.
- `components/work-item-detail/`: fields, description and comments as plain text (interpolation only, no `innerHTML`; remote text in `unicode-bidi: isolate` spans), time spent and estimate, "Open in Jira/GitLab" through `openWorkItem`. No other actions.
- Layout: docked beside the list above `desktop-compact` (`@include ui.up('desktop-compact')`), the flyout below and the bottom sheet on phones. The flyout and sheet chrome (close, focus trap, Escape, focus restore) is lifted from `event-detail` into `components/detail-panel/`, used by both, after the calendar work has merged (it also edits the Day and Week views).
- The timeline's field is renamed "Filter activity…" (decided naming).
- Tests: group hidden in the browser build and when no connection allows search; chips → filter; merge of two connections and one failing connection keeping the other's results; a stale answer arriving after a newer one is dropped; duplicates across "Load more" dropped; a `<img src=x onerror=…>` description shows as text; open passes the ref, not a URL; docked/flyout switch at the breakpoint; `/`, `↑`/`↓`, `Enter`, `o`.

### Slice 10: Access settings — 1.5 days

- Contract, only the access fields here: `ProviderConfigConnectionStateDto.allow` and `.agent_allow`; `ProviderConfigSaveRequestDto.allow` and `.agent_allow` (optional). `desktop/src/provider_config/mod.rs`: `build_snapshot` fills them, `apply_save` writes them with `set_access`. Saving rebuilds the runtime, so the lists apply at once; fixtures for the snapshot DTO update with it.
- `components/settings-dialog/source-pane/access-section/` (`settings-access-section.component.*`), desktop only:
  - Kinds with actions: "Search and read items" (one toggle for both), with the probe's grant ("Token can", "Token lacks `read_api`", "Can browse some projects", "Not checked").
  - Every connection: "AI access", a select with Off / Read only, and Custom when the list matches neither; Advanced shows one checkbox per capability the kind implements, and `read` alone for kinds without actions (Q3). A line under it says the AI connection itself comes in a later version.
- `utils/agent-access.ts` + spec: list ↔ level for a kind's capabilities.
- `services/provider-config-facade.service.ts`: save with the lists.
- Tests: defaults shown when the keys are missing; saving the default removes the key (desktop test); turning search off hides the Work group after the save; the select's lists for Jira and for Git; Custom for a hand-written `agent_allow = ["search"]`; the section is absent in the browser build, like the other editable settings.

### Slice 11 (Q4): agent projection in core — 1 day, with the MCP step, required before any agent transport

- `core/src/model/agent/`: `AgentEntry { connection, kind, at, title, key }`, `AgentWorkItem`, `AgentWorkItemDetail` (not exported to TypeScript).
- `core/src/actions/agent/mod.rs` + `tests.rs`: `visible_connections` from `Access::for_origin(Agent)` having `read`; `project_activity(&ActivityLog, &visible)` and `project_item(…)`, recursive: a Git entry's `gitlab_event` only when its GitLab connection is visible, never `raw`; summaries, counts, `pending` and errors name only visible connections; titles 200 characters, descriptions 2,000, the last 5 comments at 500; no URL except the core-built one; text through the slice 1 escaping.
- Tests: allowed Git with denied GitLab → a merged entry has no GitLab part, no `raw`, no GitLab count, no GitLab name in `pending` or errors; truncation; a remote `web_url` never appears.

## Order and total

1 → 2 → 3 → (4 ∥ 5) → (6 ∥ 7) → 9 → 10, with 8 any time before 9; 11 with the MCP step.

| Slice | 1   | 2   | 3   | 4   | 5   | 6   | 7   | 8   | 9   | 10  | Total    |
| ----- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | -------- |
| Days  | 1.5 | 1   | 2   | 1   | 2   | 1.5 | 1.5 | 1   | 3.5 | 1.5 | **16.5** |

Plus about a day for integration across slices and the packaged-app check (CSP, opener, Windows build): **about 17.5 focused days**, without slice 11. The first version of this plan said 13; its own slices added up to 14, and the review changes add 2.5.

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
- `AGENTS.md`: `core/src/actions/` and `core/src/model/actions/`; "remote requests go through `JiraApi`/`GitLabApi`: paths from segments, credentials only to the configured origin, errors through `remote_message`"; `allow`/`agent_allow` as reserved fields; the `run_action`/`open_work_item` commands ("no HTTP route"); "app shortcuts are registered in `models/shortcuts.ts`; no document key listeners elsewhere"; "CLI output of remote text goes through `terminal_safe()`".
- `docs/adr/0006-provider-actions.md`: option C (typed verbs, one executor), origins, per-connection `allow`/`agent_allow`.
- `docs/plans/provider-actions.md`: updated with this plan (status line, phases, section 7, the superseded points).

## Risks

- **GitLab filters** (`not[assignee_id]` including unassigned items, merged and closed states, `activity_filter`) are checked on a real instance as slice 5's acceptance; if one differs, that chip maps differently for GitLab and the UI says so.
- **The CSP can break the packaged app** (runtime styles, the inline theme script, Tauri's IPC origin); slice 6 is done only after a packaged build has been clicked through.
- **Offset paging drifts** when items change between pages; duplicates are dropped by ref, a skipped item shows on the next search.
- **The probe adds one request** per Jira and GitLab connection to every startup check, and every settings save re-runs all checks.
- **Remote text in the webview** relies on Angular's escaping plus the CSP; a later `innerHTML` for rich text would undo it.
- **An AI setting that does nothing yet** may confuse; the line under it says so until the MCP step.

## Not verified while planning

No network access while writing this; each is checked in the slice that uses it.

- Jira Data Center: `mypermissions` with and without the `permissions` parameter; `/issue/{key}/comment` with `orderBy=-created`.
- GitLab: `personal_access_tokens/self` (version, `granular_scopes`), `not[assignee_id]`/`not[author_id]` and `assignee_id=None` on issues and merge requests, `activity_filter=only_comments` on issue notes, and `/issues?scope=all` speed on large instances.
- reqwest 0.13: `redirect::Policy::custom` and which headers it strips on a scheme change.
- Tauri 2: the CSP keys, the hashes it adds to bundled HTML (inline scripts and styles), `dangerousDisableAssetCspModification`, `devCsp`, the app manifest API, and `tauri-plugin-opener` used from Rust without a webview permission.
- clap: `allow_external_subcommands` with the flattened collect arguments and a global `--config`.
- That the calendar work has merged before slice 9 lifts the detail panel chrome out of `event-detail`.
