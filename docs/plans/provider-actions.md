# Provider actions: search, read, write and time tracking

Status: draft, revised after an independent review, with the user's decisions recorded in section 10 (Jira Data Center only, plain Jira worklogs, Odoo latest version on any hosting). Nothing here is built. The first step, the generic read part for Jira and GitLab together (phase 0, 1a, and the agent policy of 1b), is planned slice by slice in [generic-read-step.md](generic-read-step.md); where the two differ, that plan is newer. It fits next to the provider connections (`docs/adr/0005-provider-connections.md`) and the progressive collection, and does not depend on how the collection pipeline works internally.

## Goal

Today Activity Log only **collects** your own activity. This plan adds a second job: working with the systems behind the sources. You could search for tickets that are not yours, open a ticket's details, create a ticket, comment on it, move it to another status, and track time. Time tracking means reading worklogs and timesheets and logging new time, including from entries already on the timeline ("log this meeting as 1h on ABC-123").

The system has to be general, so GitLab and the upcoming Odoo provider (and later Microsoft Graph) can join without new plumbing. Reading and writing are configured per connection, and writing is off unless you turn it on.

Non-goals: a full Jira or GitLab client, bulk edits, deleting remote items, writing to Git repositories, writing to calendars, and actions over the HTTP API server. An AI agent driving actions is in scope (section 5, "Agent surface").

## Rubric

| Dimension      | Score | Why                                                                                                                                |
| -------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Scope          | 2     | Every layer changes: core (new `actions` module, provider adapters, config, time model), API guard, Tauri, CLI, dashboard          |
| Design impact  | 2     | New shared contract (generated DTOs, commands, `config.toml` keys, `ActivityLog.timeEntries`) every provider implements            |
| Failure impact | 2     | Writes change other people's systems and can't be undone from here; tokens with write scope are a bigger target                    |
| Uncertainty    | 1     | The architecture is clear and reviewed, and scope is decided. Whether Tempo is in use is unknown; agent confirmation is new ground |
| **Total**      | **7** | Needs the remaining answer in section 10 and a security review before phase 1c (the first write) ships                             |

## 1. Concepts and naming

| Term           | Meaning                                                                                                                                                                                                                                                                     |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Collect**    | What exists today: your own activity in a time range, as timeline entries. Unchanged.                                                                                                                                                                                       |
| **Work item**  | A thing in a remote system that has a current state: a Jira issue, a GitLab issue or merge request, an Odoo task or helpdesk ticket. A work item is not an activity entry: an entry is something that happened at an instant, a work item is something that exists now.     |
| **Time entry** | Time booked against a work item: a Jira worklog, a GitLab timelog, an Odoo timesheet line (`account.analytic.line`). It has a start or only a day, a duration, a description and an author. Time entries have their own list, separate from entries (section 4).            |
| **Capability** | Something a provider **kind** can do, declared in code: `search`, `read`, `create`, `comment`, `update`, `log_time`.                                                                                                                                                        |
| **Allow list** | Which capabilities the user allows for one **connection**, stored in `config.toml`. Collecting (including your own time entries) is always allowed; everything else has to be on the list.                                                                                  |
| **Action**     | One call of a capability on one connection, such as "search `work-jira` for unassigned tickets in ABC" or "log 1h on ABC-123". Reads run at once. Writes go through **prepare** (a preview of exactly what will be changed; this is also the dry run) and then **confirm**. |
| **Caller**     | The channel an action came from: `desktop`, `cli` or `agent`. Core decides on it, and the audit records it.                                                                                                                                                                 |

Capabilities stay coarse on purpose. Six names cover everything in the provider matrix, and every one is something a user can judge in settings. Odoo adds a seventh, `attendance`, which only it supports (section 6):

| Capability   | Kind  | Covers                                                                                                         |
| ------------ | ----- | -------------------------------------------------------------------------------------------------------------- |
| `search`     | read  | Find work items by filter or native query (JQL), whoever they belong to                                        |
| `read`       | read  | One item's details: description, comments, available transitions, time spent; form metadata for creating items |
| `create`     | write | Create a work item                                                                                             |
| `comment`    | write | Add a comment or note                                                                                          |
| `update`     | write | Change an item: transition its status, assign it, edit labels                                                  |
| `log_time`   | write | Create a time entry                                                                                            |
| `attendance` | write | Odoo only, optional: check in or check out (presence, not time on an item)                                     |

## 2. Comparing the architecture options

The question is how a provider declares what it can do and how the shells (Tauri, CLI, dashboard) call it.

### A. A generic action registry with schema descriptors

Each adapter returns a list of `ActionDescriptor { id: "jira.create_issue", kind: read|write, input_schema: [FieldDescriptor], output: … }` and implements `execute(id, serde_json::Value) -> serde_json::Value`. The dashboard and the CLI build forms and output from the descriptors.

- Pro: one command and one CLI path, and a new action needs no contract change.
- Con: inputs and outputs are untyped JSON on both sides. That goes against the repo's "maximize type safety" and "no `as` casts" rules: the dashboard would need casts or a runtime validator for every result. Outputs have no shared meaning, so the timeline, time totals and "log time from entries" cannot work across providers. Each provider would also invent its own names for the same idea (`jira.add_worklog`, `gitlab.add_spent_time`, `odoo.create_timesheet`).

### B. Hand-written commands and DTOs per provider

`jira_search`, `jira_create_issue`, `gitlab_search_issues`, and so on, each with its own Tauri command, CLI subcommand and DTOs.

- Pro: fully typed, and every endpoint is simple to read.
- Con: the work grows with providers × actions. Every new pair needs a command, a CLI subcommand, a dashboard service method and UI. Permission checks, audit and confirmation would be repeated in each handler, so the security boundary is only as strong as the weakest copy. Odoo would repeat everything Jira has.

### C. Shared typed verbs with provider-specific details (recommended)

A small closed set of **typed** requests and results that every provider maps onto: a Rust enum `ActionRequest` (`Search`, `Get`, `DescribeCreate`, `Create`, `Comment`, `Transition`, `Assign`, `LogTime`) and normalized results (`WorkItem`, `WorkItemDetail`, `TimeEntry`). Providers implement one trait whose methods default to "unsupported". The parts that really differ by provider (projects, issue types, required fields, transitions) come back as typed **form metadata** from `DescribeCreate` and `Get`, and go back in as a closed `FieldValue` union. One core executor applies the caller, permissions, confirmation, limits and audit.

- Pro: typed end to end, and ts-rs turns the internally tagged enum into a TypeScript discriminated union. There is one command, one CLI dispatcher and one security path. Time tracking works the same everywhere: "log time from entries" targets any connection that allows `log_time`. Odoo implements the trait and inherits the UI, CLI and safety.
- Con: the verbs have to be chosen with care. A provider feature that fits none of them (a GitLab MR approval, say) needs a new verb, which is a contract change. That is the right price for a write path: every new kind of write should get a review.

**Recommendation: C** (the review agreed). It takes A's single path and B's types. The closed verb set is also a security property: everything Activity Log can write is listed in one enum.

## 3. Core architecture

### Layout

Following the repo rules (one model struct per file, `mod.rs` + `tests.rs`):

- `core/src/model/actions/`: `capability.rs`, `caller.rs`, `action_call.rs`, `action_request.rs`, `action_response.rs`, `action_preview.rs`, `action_error.rs`, `work_item_ref.rs`, `work_item.rs`, `work_item_detail.rs`, `search_query.rs`, `search_filter.rs`, `search_result.rs`, `field_value.rs`, `form_field.rs`, `create_form.rs`, `capability_state.rs`.
- `core/src/model/time/`: `time_entry.rs`, `day_range.rs`.
- `core/src/actions/`: the executor (`mod.rs`), `policy.rs` (allow list), `pending.rs` (prepared writes), `limits.rs` (per-connection concurrency), `audit.rs` (the durable audit file), `keys.rs` (strict key parsing helpers).
- Per-provider action code next to the collectors: `core/src/actions/jira/`, `core/src/actions/gitlab/`, later `core/src/actions/odoo/`.
- Per-provider API clients shared by the startup check, the collector and the actions: `core/src/provider/jira/api.rs` (`JiraApi`: base URL, flavour, auth, HTTP client, path building) and `core/src/provider/gitlab/api.rs`. Today the client is built in two places (`provider/jira/mod.rs` and `collect/jira/mod.rs`, and the same for GitLab).

### The adapter side

`ProviderAdapter` gains two optional methods. The action logic lives in a separate trait, so collectors stay untouched and providers without actions change nothing:

```rust
pub trait ProviderAdapter {
    // …existing methods…
    fn actions(&self) -> Option<&dyn ActionProvider> { None }
    /// Your own time entries in the range, fetched on every collection, outside the day cache.
    fn collect_time(&self, options: &CollectOptions, ctx: ProviderContext) -> BoxFuture<'_, anyhow::Result<Vec<TimeEntry>>> { /* empty */ }
}

pub trait ActionProvider: Send + Sync {
    /// What this kind can do; constant per kind.
    fn capabilities(&self) -> &'static [Capability];
    /// What the token can do, probed during every startup check (see below).
    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Vec<CapabilityState>>;

    fn search(&self, ctx: &ProviderContext, query: SearchQuery) -> BoxFuture<'_, ActionResult<SearchResult>> { unsupported() }
    fn get(&self, ctx: &ProviderContext, key: &str) -> BoxFuture<'_, ActionResult<WorkItemDetail>> { unsupported() }
    fn describe_create(&self, ctx: &ProviderContext, scope: CreateScope) -> BoxFuture<'_, ActionResult<CreateForm>> { unsupported() }
    /// Validates, parses keys strictly, does every lookup the write needs (createmeta, transitions,
    /// the Odoo employee) and returns the one mutation it will make. Sends no change.
    fn plan(&self, ctx: &ProviderContext, request: &WriteRequest) -> BoxFuture<'_, ActionResult<WritePlan>> { unsupported() }
    /// Sends exactly the plan's mutation, adding credentials only at send time.
    fn apply(&self, ctx: &ProviderContext, plan: &WritePlan) -> BoxFuture<'_, ActionResult<WriteOutcome>> { unsupported() }
}
```

`WriteRequest` is the write half of `ActionRequest` (`Create`, `Comment`, `Transition`, `Assign`, `LogTime`). A `WritePlan` has three parts:

- **A typed mutation, one per provider and opaque to the shells:** `enum Mutation { Jira(JiraMutation), Gitlab(GitLabMutation), Odoo(OdooMutation) }`, for example `JiraMutation::AddWorklog { key, started, seconds, comment }`. The plan holds **no credentials** and **exactly one mutating call**; all lookups happen in `plan`.
- **The preview data:** connection, capability, target, and the fields as label and display value. Core renders the `ActionPreview` from this; the provider never writes preview text itself. The preview never shows HTTP method, path or body: an Odoo JSON-RPC body carries the API key, and every GitLab GraphQL call is `POST /api/graphql`, so neither says anything to a person.
- **The policy fingerprint** at plan time (see "Pending plans").

Turning a mutation into an HTTP request is a pure function in the provider's API client, which tests check without a network or a mock server.

The `provider!` macro already passes extra items through (`$($rest)*`), so an adapter adds `fn actions(&self) -> Option<&dyn ActionProvider> { Some(&JiraActions) }` without a macro change.

### Execution path

```
shell (Tauri command / CLI / the desktop's agent socket)
  → core::actions::run(caller: Caller { origin }, runtime: &ProviderRuntime, call: ActionCall)
      1. kind has actions and the capability                → Unsupported
      2. connection's allow list has the capability         → NotAllowed   (enforced here, not in the UI)
      3. origin may call it, with its policy (see below)    → NotAllowed
      4. connection checked and usable; a running check is awaited, not failed  → Unavailable
      5. read:  limiter → provider → map errors → result
         Write:   plan → PendingActions.insert(plan, origin, policy fingerprint) → Preview { id }
         Confirm: PendingActions.take(id) → re-check origin's policy, confirmer and fingerprint → limiter
                  → apply → audit → Done { created item or time entry }
```

- **The runtime hands the executor a `&ProviderRuntime`** for the one connection in the call. The progressive collection keeps a shared check future per connection (`Providers::Checking`), so the executor awaits it instead of failing while the connection is still being checked.
- **The CLI inspects only the connection it targets.** Today it inspects every connection before collecting (`inspect_provider_runtimes_with`); for actions that would mean slow, unrelated checks.
- Actions never wait for or block a collection.
- **After a write, nothing is collected again.** There is no per-connection re-collect in the runtime, and a whole collection is the wrong tool. The result carries the created `WorkItem` or `TimeEntry`, and the client inserts it into what it shows. The next collection brings the remote truth.

### ActionCall

A call names one connection. Requests carry only keys, and responses carry full refs:

```rust
pub struct ActionCall { pub connection: ConnectionId, pub request: ActionRequest } // reads, and prepare for writes
```

Confirming a prepared write is its own command, `confirm_action(id)`, carrying only the id: nothing that can differ from the plan. (An earlier draft had a `Confirm` variant in an `ActionCall` enum; the read step ships without writes, so `run_action` never gets a variant it can't serve.)

### Callers

`Caller { origin: Desktop | Cli | Agent }`, set by the listener that received the call (Tauri command, CLI process, the desktop's agent socket), never by request content. The rules:

- A `Desktop` or `Cli` plan is confirmed only by the channel that prepared it.
- An `Agent` request is confirmed only by a human in the desktop app. The pending write keeps the immutable `origin` separate from the `confirmer`, and the agent's policy (`allow ∩ agent_allow`) is checked at planning and at confirmation (section 5, "Agent surface").
- There is no `Http` origin in phases 1–3: the HTTP server does not route actions at all (see section 8).
- Origin and confirmer are written to the audit.

### Pending plans

- **Plans live in the runtime that the shells already replace.** `PendingActions` is part of `CollectionRuntime`, and both shells replace that runtime when the config changes: the desktop in `update_config_at` after a settings save, the API server in `reload` when the file changes. When it is replaced, every pending plan disappears and a later confirm gets `Expired`. `take_over` hands over the running collection only, not the plans.
- **Confirm checks the plan against the current settings again.** Each plan stores a fingerprint of the connection's config, `allow` and `agent_allow`. Confirm compares it and re-runs the allow-list and channel checks, so a change that slipped past a runtime swap still can't confirm an old plan.
- **Plans expire after 5 minutes and confirm only once.** Confirming moves a plan to `running` and then `done(result)`. A second confirm with the same id gets the stored result, which covers double clicks and a dropped response in the desktop app.
- **The confirm id** is 128 random bits (base64url) and is never logged. Logs and the audit use a separate short correlation id (8 hex characters).
- **The CLI has no idempotency.** Each run of the CLI is its own process with its own plans, so running it again makes a new write. Instead, the CLI and the desktop both warn before confirming when the audit file shows the same capability, connection and target written in the last 10 minutes, and for `log_time` the same duration ("You logged 1h on ABC-123 7 minutes ago").

### Permissions (config.toml)

The allow list is a connection field next to `name`, `label` and `wsl`:

```toml
[[jira]]
label = "Work Jira"
url = "https://jira.example.com"
token = "…"
allow = ["search", "read", "create", "log_time"]
```

- `allow` joins `name`, `label` and `wsl` as a reserved field in `core::settings` (`entry_values` would otherwise warn "unknown setting `allow`"). It is not a provider config key, so it does not change the cache fingerprint.
- Without `allow`, the default is `["search", "read"]`, for the desktop and the CLI only. Writes are never on by default.
- `allow = []` means collect only.
- An `allow` that is not a list of strings **fails closed** to `[]`, with a config warning. An unknown name in the list is dropped with a warning.
- `core::settings` parses it into `Connection.allow: BTreeSet<Capability>`. The CLI and the desktop write it with `toml_edit` like other fields.
- Turning on a write capability in the desktop settings asks for confirmation once ("Activity Log will be able to create tickets in Work Jira as you").

### Token scopes and the startup check

Every startup check of a kind with actions probes the token once (one extra request), whatever the allow list says, so settings can show what each toggle would be able to do. The probe is advisory (it never changes the status) and checks only the capabilities the kind implements so far; each write phase adds its permissions. It reports a `CapabilityState { capability, allowed, granted: yes|no|unknown, reason? }` per capability:

- **GitLab**: `GET /api/v4/personal_access_tokens/self` returns `scopes`; `read_api` is enough for search and read, writes need `api`. Fine-grained tokens report `granular_scopes` when the instance has them; those are mapped too. Missing endpoint (old GitLab) → `unknown`.
- **Jira** (Data Center):
  - Personal access tokens have no scopes; they act with the user's permissions.
  - `GET /rest/api/2/mypermissions?permissions=BROWSE_PROJECTS,CREATE_ISSUES,ADD_COMMENTS,TRANSITION_ISSUES,ASSIGN_ISSUES,WORK_ON_ISSUES` gives the global answer. Project answers are checked again in `plan`, and a 401 or 403 in `plan` is reported as `MissingScope`.
  - The same probe looks for Tempo Timesheets and reports it as information (section 6).
- **Odoo**: access rights per model and operation, plus which booking targets are installed (section 6).

A missing scope never makes the connection unavailable (collecting still works). It shows next to that capability in settings, and the action is refused in `plan` with `MissingScope`.

### Keys and paths

Keys come from a client, so they are untrusted. Each provider parses them strictly in `plan` and in `get`:

- Jira: `^[A-Z][A-Z0-9_]+-[1-9][0-9]*$`.
- GitLab: `group/subgroup/project#iid` or `!iid`, with the project path checked segment by segment.
- Odoo: `task/<positive int>`, `ticket/<positive int>` or `project/<positive int>`.

Paths are built with `Url::path_segments_mut()`, never `format!`. The current GitLab collector builds `/users/{user}/events` with `format!`; it moves to the same builder in phase 0. The config value is the user's own, but the pattern must not spread to client input.

### Error model

One typed error for all providers, with no response bodies, headers or tokens in its message:

```rust
pub enum ActionErrorCode {
    NotFound, Unsupported, NotAllowed, Unavailable, MissingScope,
    InvalidInput,   // with `field`
    Conflict,       // transition no longer available, item changed
    RateLimited,    // with `retry_after_seconds`
    Unreachable, Timeout,
    OutcomeUnknown, // a write timed out or the connection dropped after sending
    Expired,        // confirm for a plan that expired, was used, or whose runtime was replaced
    Remote,         // other remote error: status and a short, cleaned message
}
```

The current collectors put the whole response body into errors (`[jira] API error {status}: {body}`). Actions use a cleanup function that keeps the status and Jira's `errorMessages`/`errors`, GitLab's `message` or Odoo's `error.message`, cut to 300 characters. The collectors switch to it in phase 0.

### Writes and their outcome

- **Prepare is the dry run.** It returns an `ActionPreview { id, correlation, connection, label, summary, target?, fields: [{label, value}], warnings }` and sends nothing. The CLI's `--dry-run` stops here.
- **Writes are never retried automatically.** Remote APIs (Jira, GitLab, Odoo) have no idempotency keys, so a timed-out create may have succeeded. The result is `OutcomeUnknown` with what to check by hand, and the audit records it. Search is not a reliable check: a search index can lag behind a write.
- For `log_time`, the preview lists your existing time on that item that day, which makes a double booking visible.

### Limits and timeouts

- Per connection: at most 2 reads and 1 write at a time (a semaphore in `limits.rs`). Collection keeps its own pacing.
- HTTP: the existing `http_client_builder()` (10 s connect, 30 s total).
- A 429 or 503 with `Retry-After`: reads retry once if the wait is ≤ 10 s, otherwise `RateLimited`. Writes don't retry.
- Search returns one page (`limit` ≤ 50) with a `cursor`; there are no unbounded loops behind a search box.

### Audit

- **The durable record is `actions.jsonl` in the OS data folder.** It is a **local operational audit**: it helps you see what happened, but any process running as you can edit it. It ships in phase 1b with the agent's reads, before the first write (1c). The diagnostics log is not enough: it is an in-memory ring of 1,000 lines that is lost on restart and cleared by `DELETE /api/logs` or the Logs pane.
- **Writes and approvals only, metadata only.**
  - One line per request, approval or rejection, `applying`, and outcome.
  - Each line holds the time, origin, confirmer, connection id, capability, target key, correlation id, outcome and remote id, and for `log_time` the duration and day.
  - Never titles, descriptions, comment text, JQL, field values, handles or tokens.
  - Reads go to the diagnostics log only, so read volume can never rotate write history out.
- **The approval is written before sending.** The `applying` line is appended and flushed before the request goes out. If the audit can't be written, the write is refused (fail closed).
- **Several processes may append.** The desktop writes for itself and for agents, and the CLI for its own writes. Each append, and rotation at 5 MB keeping 3 files, happens under an exclusive file lock (`std::fs::File::lock`). Each line is one `write`. The file is created `0600` on Unix and opened without following symlinks.
- The same metadata goes to the diagnostics log under the connection's scope, so it shows in the Logs pane:

  ```
  [jira/work] desktop log_time ABC-123 1h00m 2026-11-17 → ok, worklog 104233 (a91c03f2)
  [jira/work] cli create in ABC → failed: MissingScope (5e0b7d11)
  ```

- Reads are logged at info level without query text (JQL can contain names).
- The remote system remains the full record of content: Jira history and GitLab activity show the user as the author.

## 4. Time tracking as a first-class capability

### The rule: civil dates are data

Some systems store time as a day and hours (Odoo timesheets), some as an instant and a duration (Jira, GitLab). Core keeps to its instants-only rule this way: **a civil date (`YYYY-MM-DD`) is data supplied by the client or by the remote system; core passes it through and never converts between dates and instants.** This is proposed as an addition to the Rust rules in AGENTS.md:

> Civil dates (`YYYY-MM-DD`) are data: they come from the client (the dashboard's or the CLI's local day) or from a remote system (an Odoo timesheet's date) and core passes them through. Core never turns a date into an instant or an instant into a date.

### Reading

- **Time entries get their own list:** `ActivityLog.timeEntries: TimeEntry[]`, next to `entries`, not inside them.

  ```rust
  pub struct TimeEntry {
      pub connection: ConnectionId,
      pub item: WorkItemRef,          // key plus connection
      pub item_title: String,
      pub started: Option<String>,    // RFC 3339, when the system has an instant (Jira, GitLab)
      pub day: Option<String>,        // YYYY-MM-DD, when the system has a date (Odoo)
      pub duration_seconds: u32,
      pub description: String,
      pub remote_id: String,
  }
  ```

  - **Why separate:** totals, the "log time" preview and the calendar blocks all want time entries as one type. Mixing them into the entries union would make every view filter them back out.
  - **Collected outside the day cache:** the runtime calls `ProviderAdapter::collect_time` on every collection. The settled-day cache keeps old days fresh for 7 days, so a worklog added later for an old day would never appear.

- **Date-based sources need the range as days:** `CollectRequest` gains optional `days: { first, last }`. The dashboard and the CLI already work out the local days of the range, so they send them along. Core passes them to date-based sources (Odoo), which match on `date` between them. Instant-based sources keep using `[start, end)`. Without `days`, date-based sources return no time entries and log why.
- **Display:**
  - The day and week views draw time entries with `started` as blocks, like meetings, in a "logged" style.
  - Entries with only a `day` go into that day's header row.
  - Per-day totals ("Logged 6h15m") are computed in the dashboard (`dashboard/src/app/utils/time-totals.ts`) and the CLI (`time report`). Grouping by local day is the client's job: core never computes day totals from instants.
- **Per item:** `Get` returns `time: { spentSeconds, estimateSeconds?, entries: [TimeEntry] (latest 20) }`.

### Writing

There is one request for every provider:

```rust
LogTime {
    key: String,
    started: String,          // RFC 3339 instant
    day: String,              // the client's local day of `started`; core does not derive it
    duration_seconds: u32,    // ≥ 60; rounded to the minute
    description: String,
}
```

| Provider | Call                                                                                                                                                                                                                                                                                                                                                           |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Jira     | `POST /rest/api/2/issue/{key}/worklog` `{ started, timeSpentSeconds, comment }`, with `started` in Jira's `yyyy-MM-dd'T'HH:mm:ss.SSSZ` format (offset `+0000` from the instant), and a plain-text comment. The remaining estimate stays as it is (`adjustEstimate=leave`). Tempo is not involved (section 6).                                                  |
| GitLab   | GraphQL `timelogCreate(issuableId, timeSpent: "1h30m", spentAt, summary)`; the issuable's global id is looked up in `plan`. REST `add_spent_time` has no start time.                                                                                                                                                                                           |
| Odoo     | `create` on `account.analytic.line` `{ date: day, unit_amount: hours, name: description, project_id, employee_id }`, plus `task_id` or `helpdesk_ticket_id` depending on the connection's `time_target` and the key. The employee is looked up in `plan` (`hr.employee` with `user_id = uid`); without one, `MissingScope` ("your Odoo user has no employee"). |

The `Done` result returns the created `TimeEntry`, which the client adds to `timeEntries`.

### Logging time from activity

This is the main reason to make time tracking first class.

1. Select one or more entries on the timeline or in the day or week view (a meeting, a run of commits, an MR review) and choose **Log time…**.
2. The dialog fills itself in from the entries:
   - **Duration:** a meeting's length; for other entries, first to last entry rounded up to 15 minutes, with a minimum of 15 minutes.
   - **Start and day:** the first entry's start, and its local day as the dashboard sees it.
   - **Description:** the titles, deduplicated.
   - **Target:** a work item key found in the entries (Jira keys `[A-Z][A-Z0-9_]+-\d+` in commit messages, branches, MR titles and meeting titles; a GitLab `#iid` with its project; a Jira entry's own key). Otherwise you pick from recent items or search.
3. The preview shows what will be sent and the time already logged on that item that day.
4. You confirm.

The suggestion logic is pure, testable code in `dashboard/src/app/utils/time-suggestion.ts`.

Later (not planned): mark entries that a time entry already covers, and an "unlogged time" view comparing tracked activity with logged time per day.

## 5. Contract

### DTOs (ts-rs, in `core/src/ts.rs`)

```rust
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ActionRequest {
    Search { query: SearchQuery },
    Get { key: String },
    DescribeCreate { scope: CreateScope },                 // project and optional issue type
    Create { project: String, item_type: String, fields: Vec<FieldInput> },
    Comment { key: String, body: String },
    Transition { key: String, transition_id: String },
    Assign { key: String, assignee: Assignee },             // Me | Unassigned | User { id }
    LogTime { key: String, started: String, day: String, duration_seconds: u32, description: String },
}

pub struct FieldInput { pub key: String, pub value: FieldValue }

/// Closed on purpose: never `serde_json::Value`.
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldValue {
    Text { value: String },
    Option { id: String },
    Options { ids: Vec<String> },
    User { id: String },
    Labels { values: Vec<String> },
}

#[serde(tag = "status", rename_all = "camelCase")]
pub enum ActionResponse {
    Done { result: ActionResult },        // tagged: search, item, form, created item, time entry, …
    NeedsConfirmation { preview: ActionPreview },
    Failed { error: ActionError },
}

pub struct SearchQuery {
    pub filter: SearchFilter,             // text, assignee: Me|Unassigned|NotMe|Anyone, reporter: Me|NotMe|Anyone,
                                          // state: Open|Done|Any, project?, updatedSince?
    pub native: Option<String>,           // provider query language when the kind has one (JQL)
    pub cursor: Option<String>,
    pub limit: u16,
}

pub struct WorkItemRef { pub connection: ConnectionId, pub key: String } // responses only
pub struct WorkItem { pub r#ref: WorkItemRef, pub title: String, pub item_type: String, pub status: String,
                      pub assignee: Option<Person>, pub reporter: Option<Person>, pub project: String,
                      pub created: String, pub updated: String, pub url: String }
```

- **`FieldValue` and the create form:**
  - `FormField { key, label, required, kind: Text | Multiline | Select { options } | MultiSelect { options } | User | Labels }` describes what `DescribeCreate` returns.
  - `plan` fetches createmeta again and rejects a field key the form doesn't have, or a `FieldValue` kind that doesn't match the field.
  - It is a separate type from `ProviderConfigFieldStateDto`, which is about config values and secrets.
- **`WorkItem.url`** is built by core from the connection's configured URL and the parsed key, and only for `http`/`https` URLs. Core never passes on a URL the remote system sent (`self`, web links, avatars).
- **`Person`** is `{ id, displayName }`: `id` is the username on Jira Data Center and the user id on GitLab and Odoo.
- **Existing DTOs** gain only additive fields:
  - `ProviderKindDto.capabilities: Capability[]`
  - `ProviderInfo.capabilities: CapabilityState[]`
  - `ActivityLog.timeEntries: TimeEntry[]`
  - `CollectRequest.days?: { first, last }`
  - the connection snapshot in settings gains `allow: Capability[]`, and `ProviderConfigSaveRequestDto` gains `allow?`

  Existing routes and commands keep their shapes. No versioning scheme is needed: this repo is the only consumer and the contract is generated.

### JQL from filters

Structured filters build JQL with every literal quoted and escaped (`"` and `\`):

| Filter               | JQL                                                                                            |
| -------------------- | ---------------------------------------------------------------------------------------------- |
| assignee: Unassigned | `assignee is EMPTY`                                                                            |
| assignee: NotMe      | `(assignee != currentUser() OR assignee is EMPTY)`                                             |
| reporter: NotMe      | `(reporter != currentUser() OR reporter is EMPTY)`; `!=` alone skips issues without a reporter |
| project              | `project = "ABC"` (quoted, so keys that are JQL words work)                                    |
| text                 | `text ~ "…"`                                                                                   |
| state: Open / Done   | `statusCategory != Done` / `statusCategory = Done`                                             |

"Not assigned to me and not created by me" is the NotMe assignee filter combined with the NotMe reporter filter. Native JQL is sent as written, since it can only read.

### Tauri

- `run_action(request: ActionCall) -> ActionResponse`, with the transport's `{ request }` envelope. The shell builds `Caller { origin: Desktop }`; the webview cannot choose the origin. It clones the runtime under a short read guard and awaits outside it, so a settings save is never blocked by a read.
- `open_work_item(request: WorkItemRef)` opens a core-built URL; `confirm_action(request: id)` comes with the first write.
- Settings saves already rebuild the runtime, so allow-list changes apply like other settings and drop pending plans.
- The dashboard's `backend-contract.ts` gets one operation, `runAction`, marked desktop-only (`http: null`) like the settings operations.

### API (Rocket)

- **No action route in phases 1–3**, reads included. The default read access would otherwise make the port a search proxy into Jira and GitLab for any local process or web page, and the web dashboard is the development path.
- The web dashboard hides the Work items view and entry actions, as it already hides editable settings.
- If HTTP actions are ever wanted, they need a random session token per server start (printed and handed to the dashboard), on top of the Host and Origin checks, an `Http` origin in the caller rules, and their own review.
- No Host check: decided against (see [open-decisions-design.md](../open-decisions-design.md)); the server stays local and serves collected activity only.

### CLI

A target (`jira`, `jira/work` or `work`, resolved like `sources`) followed by a verb. `sources` and `config` stay reserved. The CLI checks only the targeted connection.

```text
activity-log jira search --unassigned --project ABC "login"
activity-log jira search --jql 'project = ABC AND reporter != currentUser()'
activity-log jira show ABC-123
activity-log jira create --project ABC --type Task --summary "Fix login" [--description -] [--dry-run] [--yes]
activity-log jira comment ABC-123 "Deployed to staging"
activity-log jira transition ABC-123 "In Review"
activity-log jira assign ABC-123 me
activity-log time log ABC-123 1h30m --at "2026-11-17 09:00" -m "Pairing on login" [--dry-run] [--yes]
activity-log time report --range week [--format markdown]
```

- Writes print the preview and any recent-write warning, then ask `Send? [y/N]`. `--yes` skips the prompt. Without a terminal and without `--yes`, writes fail instead of hanging.
- `--json` prints the `ActionResponse` for scripts.
- `time log` turns `--at` into an instant and a local day in the terminal's time zone, like `--since` does today. It finds the connection from the key's form (a Jira key, `group/app#12` for GitLab, `odoo:task/42`); `--connection` is needed when more than one connection could own it.

### Agent surface

Decision: an AI agent driving these actions is a near-term goal. It gets its own origin from the start, reads first, and writes only through a human.

#### Design: the desktop app is the authority

Two designs were compared. The desktop app owns everything, and a thin MCP adapter talks to it:

```
agent host ──stdio (MCP)──▶ activity-log mcp ──local socket / named pipe──▶ desktop app
                            (no config, no tokens)                           policy · pending requests · confirmation UI · audit · provider calls
```

|                | **Desktop as authority (chosen)**                                                                                                            | Shared file queue (earlier draft)                                                                                                      |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Who decides    | One process: policy, pending requests, confirmation, audit and provider calls                                                                | Producer (MCP process) and consumers (desktop, CLI) each read the config, plan and log                                                 |
| Queue protocol | In-memory requests in the desktop runtime; no files                                                                                          | Atomic claiming, one consumer, durable identity, terminal states, crash recovery, filesystem hardening (symlinks, ACLs, bounded reads) |
| Credentials    | The MCP adapter never reads `config.toml` or tokens                                                                                          | The MCP process loads the config and tokens to answer reads                                                                            |
| Audit          | One main writer (the desktop; the CLI appends its own writes under a lock)                                                                   | Several processes append and rotate                                                                                                    |
| Costs          | The desktop must be running ("Activity Log is not running" otherwise); a local socket server; CLI confirmation of agent requests is deferred | Works without the desktop, at the price of everything in the rows above                                                                |

**Chosen: desktop as authority.** It is the smaller design and the safer one: the queue protocol, the cross-process consistency rules and most of the filesystem hardening disappear instead of being implemented. Needing the desktop app to run fits a desktop tool. The file queue is dropped.

#### Transport

- **MCP over stdio** (`activity-log mcp`, a subcommand of the CLI binary).
  - It opens no listening endpoint. The desktop still calls Jira and the other providers over the network as usual.
  - stdout is reserved for the protocol; logs go to stderr.
  - It uses `rmcp`, the official Rust SDK, pinned to a reviewed release with minimal features (server and the stdio transport). It is a new dependency to approve.
- **Adapter to desktop:** a local socket served by the desktop.
  - **macOS and Linux:** a Unix domain socket in a private `0700` folder the desktop creates, with a fixed, internally generated name. The desktop refuses to start the socket if the path is a symlink or isn't owned by the user, and checks the peer's user id on accept (`getpeereid` / `SO_PEERCRED`).
  - **Windows:** a named pipe with an owner-only DACL, `first_pipe_instance` and `reject_remote_clients`. Tokio supports both kinds of socket, so no new dependency is needed.
  - **One desktop instance** (`tauri-plugin-single-instance`, a dependency to approve), so there is exactly one authority. A second instance would fail to take the socket anyway.
- **Protocol:** length-prefixed JSON frames, at most 256 KB, of a typed, versioned `AgentRequest` enum (search, get, activity, time report, request write, request status). No field in it can name a channel: everything that arrives on the socket has origin `Agent`.
- **Why not the CLI:** an agent running `activity-log …` in a shell has origin `Cli`. Core can't tell it from you, so `--yes` would let it write.

#### Boundary

The rules below only hold for an agent that reaches Activity Log through MCP alone. An agent that can do any of the following acts as you, and no design here can stop it:

- run a shell or a PTY as you;
- read your files (`config.toml` holds the tokens);
- drive your desktop (UI automation, accessibility APIs).

The docs say so plainly.

#### Origin, confirmer and policy

- **Every pending write has two parts:**
  - an immutable `origin` (`Desktop | Cli | Agent`), set by the process that received it;
  - a `confirmer` (`Desktop | Cli`), the channel that approves it.
- **Who may confirm:**

  | Origin  | Confirmer                                                                                      |
  | ------- | ---------------------------------------------------------------------------------------------- |
  | Desktop | Desktop                                                                                        |
  | Cli     | Cli, in the same process                                                                       |
  | Agent   | **Desktop only.** CLI confirmation of agent requests is deferred; an agent has no confirm call |

- **Effective policy:** for origin `Agent` it is `allow ∩ agent_allow` (section 3); for the others, `allow`. It is checked when planning **and** again at confirmation.
- **The fingerprint stored with the plan** covers the connection's config, `allow` and `agent_allow`, so changing either list voids pending agent requests.
- **Nothing in a request selects a channel.** The origin comes from the listener (socket = `Agent`, Tauri command = `Desktop`, CLI process = `Cli`).

`agent_allow = [...]` is a per-connection field, reserved and parsed like `allow` (malformed fails closed). Without it, the agent gets **nothing** on that connection, not even reads, because ticket text is what prompt injection rides on. In the settings it is one select per connection, Off / Read only / Read and prepare changes, with Advanced per-action checkboxes (the last option appears once a write exists). Decided (user, 2026-11-18; Q3 in `generic-read-step.md`): `read` in `agent_allow` also means the agent may be shown that connection's collected activity, so every kind (Git and Outlook too) has the setting, while a kind without actions still implements no actions.

#### Agent requests in the desktop

1. `request_write` arrives on the socket with a connection and a `WriteRequest`. The desktop checks limits and policy, runs `plan`, and stores an **immutable snapshot**: the request, the plan, the core-rendered preview and the fingerprint.
2. It answers with a **request handle**: 128 random bits. Holding the handle is what lets the agent ask for the status. A separate 8-hex correlation id goes to logs and the audit; the handle is never logged.
3. The request appears in an **inbox** (a badge, no unsolicited dialog). Approving or rejecting acts on that snapshot. Nothing is read again from the agent before sending.
4. **States:** `pending` → `approved` → `applying` → `done | failed | outcome_unknown`, or `rejected` / `expired`.
   - `applying` is written to the audit (and flushed) **before** the request is sent. If that write fails, nothing is sent.
5. **Crash recovery:**
   - Superseded by the decision on drafts ([open-decisions-design.md](../open-decisions-design.md)): an agent request is saved as a draft (request and origin only, never tokens) in the app's private data folder, expires after 7 days, and is prepared again under the current settings with a fresh preview when opened. The prepared plan itself still lives only in the runtime's memory.
   - On start, an `applying` audit line without an outcome shows in the inbox as `outcome_unknown`. It is never replayed.
6. **`request_status(handle)`:**
   - It returns the state and, when done, the result in the agent projection (below).
   - It is authorized against the **current** policy.
   - An unknown handle, an expired request and a request the agent may no longer see all get the same `not_found`.

#### What the agent can read

- **Tools:** `search`, `get_item`, `activity` (your own entries in a range) and `time_report` (time entries in a range). The adapter sends local days computed in its own time zone, like the CLI. No settings tools and no confirm tool.
- **Agent results are their own response types**, for example `AgentEntry { connection, kind, at, title, key? }`, `AgentWorkItem` and `AgentTimeEntry`. Each is built by a projection that takes the set of connections the agent may see. They are never the dashboard DTOs filtered after the fact. The projection applies **recursively**:
  - A Git entry merged with a GitLab event carries that event (`GitEntry.gitlab_event`, filled in by the merge in `core/src/utils/mod.rs`). The agent sees the GitLab part only when it may see that GitLab connection, and never `raw`.
  - Summaries, counts, `pending` and errors name only visible connections.
  - Provider statuses and the diagnostics log are not exposed.
- **Size limits:** titles 200 characters, descriptions 2,000, the last 5 comments at 500 each, search 50 items, and a 256 KB cap on any response.
- **Text and links:**
  - Remote text sits in fields marked as remote text and is never acted on.
  - No URLs except core-built `WorkItem.url`.
  - No tokens, config values, handles of other requests or confirm ids.

#### Limits

Limits are enforced at the desktop's socket, so they cover any number of adapters:

- **Connections and requests:** at most 4 connections; 120 reads per minute; 60 status calls per minute.
- **Write requests:** 10 pending per connection and 20 in total; 30 per hour.
- **Ranges:** `activity` up to 31 days, `time_report` up to 93.
- **Input lengths:** comment and description text up to 5,000 characters; other text fields up to 500.

#### Confirmation display

- **Trusted chrome above, untrusted text below.**
  - The top shows connection, action and "Requested by an agent", drawn by the app.
  - Everything the agent wrote is in a separate box labelled "Text from the agent", with control and bidirectional-override characters made visible.
- **A real challenge for agent writes.** You type the target before Confirm unlocks: the issue key for item writes, the project key for a create. You must look at where the write goes.
- **No carry-over between requests.** After a decision, the next request does not open by itself and does not get focus on its buttons. Buttons ignore input for the first second after a request is shown.
- **The CLI uses the same rules for its own writes.** It shows the same core-rendered preview with control and bidirectional characters escaped. It cannot confirm agent requests, and `--yes` never applies to them.

#### Elicitation

MCP elicitation (the agent host asking you something) is not used to confirm. Its answer comes back over the agent's own connection, so an "accept" from the client proves nothing about a human. The URL mode, which sends you out of band, is no better than the desktop inbox, so the inbox stays the one place to confirm.

## 6. Provider matrix

| Kind                        | Collect (today, + time)                                                              | search                                                                                                                     | read                                                                                          | create                               | comment                                                            | update                                                               | log_time                                                                            | Token scope                                                                                                                    |
| --------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------ | -------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| **Jira**                    | Issues; your worklogs as time entries                                                | Phase 1a                                                                                                                   | Phase 1a                                                                                      | Phase 1c                             | Phase 3                                                            | Phase 3 (transition, assign)                                         | Phase 2                                                                             | Data Center PAT: the user's permissions, probed with `mypermissions`                                                           |
| **GitLab**                  | Events; your timelogs as time entries                                                | Phase 1a: `GET /issues?scope=all`, `GET /merge_requests?scope=all` with `search`, `assignee_id=None`, `author_id`, `state` | Phase 1a: `GET /projects/:id/issues/:iid`, `…/merge_requests/:iid`, `…/notes`, `…/time_stats` | Phase 3: `POST /projects/:id/issues` | Phase 3: `POST …/issues/:iid/notes`, `…/merge_requests/:iid/notes` | Later: `PUT …/issues/:iid` (`state_event`, `assignee_ids`, `labels`) | Phase 3: GraphQL `timelogCreate`                                                    | `read_api` for search and read; `api` for writes. Probed with `GET /personal_access_tokens/self` (`scopes`, `granular_scopes`) |
| **Odoo** (coming soon)      | Your timesheet lines as time entries; check-ins and check-outs as entries (optional) | Phase 4: the `time_target` model (`project.task` by default, `helpdesk.ticket` or `project.project`)                       | Phase 4: the task or ticket, its chatter (`mail.message`), its timesheets                     | Phase 4: `project.task`              | Later: `message_post`, which notifies followers                    | Later: `stage_id`, `user_ids`                                        | Phase 4: `account.analytic.line`; check-in and check-out as `attendance` (optional) | The user's access rights; probed with access checks and `fields_get`                                                           |
| **Git**                     | Your commits                                                                         | Maybe later: commits by anyone mentioning a key, across your repositories                                                  | Maybe later: one commit's details                                                             | —                                    | —                                                                  | —                                                                    | —                                                                                   | None (local)                                                                                                                   |
| **Outlook**                 | Calendar events                                                                      | —                                                                                                                          | —                                                                                             | —                                    | —                                                                  | —                                                                    | —                                                                                   | Stays read-only: writing a calendar over COM or AppleScript is risky and not needed                                            |
| **Calendar** (ICS)          | Events                                                                               | —                                                                                                                          | —                                                                                             | —                                    | —                                                                  | —                                                                    | —                                                                                   | Read-only by nature                                                                                                            |
| **System**                  | Boot and shutdown                                                                    | —                                                                                                                          | —                                                                                             | —                                    | —                                                                  | —                                                                    | —                                                                                   | Read-only                                                                                                                      |
| **Microsoft Graph** (later) | Calendar, Teams                                                                      | Mail and calendar search would fit `search`/`read`; no writes planned                                                      | —                                                                                             | —                                    | —                                                                  | —                                                                    | —                                                                                   | OAuth delegated scopes; a separate kind                                                                                        |

### Jira (Data Center)

Decision: only the Jira you use today, Data Center/Server. The current code already matches it: `Authorization: Bearer <PAT>` (personal access tokens, 8.14+) and `GET /rest/api/2/search`. Everything goes through `/rest/api/2`.

| Concern     | Data Center / Server                                                                                                                                                                    |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Auth        | `Bearer <PAT>`                                                                                                                                                                          |
| Search      | `GET/POST /rest/api/2/search`, `startAt` paging                                                                                                                                         |
| Users       | `name` / `key`                                                                                                                                                                          |
| Assign      | `PUT /rest/api/2/issue/{key}/assignee` `{ name }`                                                                                                                                       |
| Rich text   | Wiki markup; Activity Log writes plain text and shows description and comments as plain text                                                                                            |
| Create form | `GET /rest/api/2/issue/createmeta/{project}/issuetypes[/{typeId}]` (8.4+)                                                                                                               |
| Transitions | `GET/POST /rest/api/2/issue/{key}/transitions`                                                                                                                                          |
| Comment     | `POST /rest/api/2/issue/{key}/comment` `{ body }`                                                                                                                                       |
| Worklogs    | `GET/POST /rest/api/2/issue/{key}/worklog`; your worklogs: JQL `worklogAuthor = currentUser() AND worklogDate >= "…"`, then each issue's worklogs filtered by author and `[start, end)` |
| Permissions | `GET /rest/api/2/mypermissions`                                                                                                                                                         |

- **Create:** phase 1c creates only issue types whose required fields Activity Log can render: summary, description, assignee, labels, priority, components and simple selects. For other types the form says "This issue type has required fields Activity Log can't fill; open in Jira" and links to the create screen.
- **Not planned: Jira Cloud.** Cloud would need different auth (Basic `email:api_token`, or scoped tokens and OAuth through the `api.atlassian.com` gateway), a different search endpoint and paging, and `accountId` instead of user names. It stays out of scope. If it is ever wanted, all of it belongs inside `JiraApi`, and the startup check can already tell a Cloud URL apart (`GET /rest/api/2/serverInfo`, `deploymentType`) to say so instead of failing vaguely.

#### Time in Jira: plain worklogs, Tempo as information

Decision: time is read and written as **plain Jira worklogs** through the worklog REST API above. Tempo-specific APIs (accounts, work attributes, approvals) come later and only if a real need shows up.

- **Assumption to check before building phase 2:** Tempo Timesheets on Data Center keeps its time in Jira's own worklogs, so worklogs created in Tempo are visible through `/rest/api/2/issue/{key}/worklog`, and worklogs created through that API show up in Tempo. If that holds, reading and writing plain worklogs works whether or not your company uses Tempo. It is stated from memory, not verified.
- **What a plain worklog can't do on a Tempo site:** set Tempo work attributes such as an account. If Tempo makes an attribute required, a plain worklog may be refused or arrive without it.
  - A refusal shows as `Remote` with Jira's message.
  - A worklog without the attribute is the risk to check on a real Tempo site in phase 2.
- **Detecting Tempo, as information only:**
  - The Jira probe sends one read-only request to a Tempo REST resource (for example under `/rest/tempo-timesheets/4/`). The exact resource is picked when building, since it is not verified here.
  - A `404` means not installed. A `200`, `401` or `403` means installed.
  - No admin rights are needed. The plugin list (`/rest/plugins/1.0/`) needs admin, so it is not used.
  - Settings show "Tempo Timesheets detected: time is logged as plain Jira worklogs; Tempo attributes such as accounts are not set." Nothing is blocked on it.

### Odoo (coming soon)

Decision: target the **latest Odoo version** with the JSON-2 API, on **any hosting**. Your company's instance with a private API key is the first user, but nothing is specific to it.

- **Config** (one connection per instance and user):

  | Field         | Kind     | Notes                                                                                                     |
  | ------------- | -------- | --------------------------------------------------------------------------------------------------------- |
  | `url`         | Url      | The instance, such as `https://acme.odoo.com` (Odoo Online), an Odoo.sh URL or a self-hosted URL          |
  | `database`    | String   | The database name, sent as `X-Odoo-Database`                                                              |
  | `login`       | String   | Your login or email. Checked against the user the key belongs to, and needed by a later JSON-RPC fallback |
  | `api_key`     | Password | Created in Odoo under your user's security settings                                                       |
  | `time_target` | Choice   | What time is booked on: `task` (default), `ticket` or `project`                                           |
  | `read`        | Choices  | Which time records are collected: `timesheets` (default), `attendance`                                    |

  `time_target` and `read` need a small `Choice` config value type with a fixed list (and its list shape), so the settings form renders a select or checkboxes. Today's types are string, bool, int, password, email and URL.

- **Protocol:**
  - JSON-2 only: `POST /json/2/<model>/<method>` with `Authorization: bearer <api_key>` and `X-Odoo-Database: <database>`, using `reqwest` + `serde_json`.
  - The version comes from `/web/webclient/version_info`. Older than JSON-2 → unavailable with the reason "Odoo <version> is older than this connection supports".
  - **Later option:** a JSON-RPC fallback (`POST /jsonrpc`, `common.authenticate`, then `object.execute_kw`) for older instances. There the key travels in the body, which is one reason plans and previews never hold request bodies.
- **Hosting is only help text:**
  - Self-hosted, Odoo.sh and Odoo Online all work through `url` + `database` + `login` + `api_key`.
  - The settings pane's help says that Odoo Online allows external API access only on the Custom plan.
  - A refusal that looks like that plan limit gets that hint in its reason.
- **Keys expire:** API keys can be created with a limited lifetime. An authentication failure after it worked before is reported as "Odoo API key invalid or expired; create a new one", not "unreachable".
- **What time is booked on (`time_target`):**

  | Value     | Booked on                 | Timesheet line field                               | Needs                                        |
  | --------- | ------------------------- | -------------------------------------------------- | -------------------------------------------- |
  | `task`    | A project task (default)  | `task_id` (and its `project_id`)                   | Project + Timesheets                         |
  | `ticket`  | A helpdesk ticket         | `helpdesk_ticket_id` (and the team's `project_id`) | Helpdesk with timesheets (an Enterprise app) |
  | `project` | A project, without a task | `project_id` only                                  | Project + Timesheets                         |
  - The setting decides what Search, the Log time dialog and `log_time` look for on this connection. A key of another form (a `ticket/7` on a `task` connection) is refused with `InvalidInput`, naming the setting.
  - Collection reads all your timesheet lines whatever the setting.

- **Three kinds of time record.** Which app provides each, and in which edition, are assumptions to verify on the target version:

  | Record         | App (assumed)                                                                                   | What it is                                                                                     | Read                                                                                                                                                         | Write                                                                                                                                                                                                                                                                          |
  | -------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
  | **Timesheets** | `hr_timesheet` (Community) on tasks and projects; `helpdesk_timesheet` (Enterprise) for tickets | Bookable time on an item: `account.analytic.line`                                              | Time entries with `day` only (section 4), when `read` has `timesheets`                                                                                       | `log_time` (phase 4)                                                                                                                                                                                                                                                           |
  | **Attendance** | `hr_attendance` (Community)                                                                     | Presence: one `hr.attendance` per check-in and check-out of your employee, not work on an item | Activity entries "Checked in" and "Checked out" at their instants, like the System source's boot and shutdown, when `read` has `attendance`. No time entries | Optional: a new capability `attendance` (check in, check out), off by default under the same allow list, with preview and confirm. `create` a record with `check_in`, or `write` `check_out` on your open one. Whether a regular user may do that through the API is to verify |
  | **Timer**      | The Enterprise timesheet app (`timesheet_grid`, timer mixin) (assumed)                          | A running stopwatch on a task that becomes a timesheet line when stopped                       | Not collected; `Get` may show a running timer                                                                                                                | Later, maybe: a `timer` capability (start and stop on a task). Stopping can open a wizard in Odoo, so it needs checking first                                                                                                                                                  |

- **Startup check:**
  - Authenticate and read the version.
  - Find your employee (`hr.employee` with `user_id` = the key's user).
  - Report which apps and booking targets are available. This only asks for what the user may read, so no admin rights are needed. The sources:
    - `fields_get` on `account.analytic.line` (`task_id`, `helpdesk_ticket_id`);
    - `fields_get` on `project.task` (timer fields such as `timer_start`);
    - access rights on `project.task`, `helpdesk.ticket`, `project.project`, `account.analytic.line` and `hr.attendance`.
  - Settings show the result as information ("Timesheets ✓ · Tasks ✓ · Helpdesk tickets: not installed · Attendance ✓ · Timer: not installed").
  - When the configured `time_target` or a `read` choice isn't available, the status is degraded with that reason. The rest still collects.
- **Models:**
  - `project.project` and `project.task` (stages, `user_ids`).
  - `helpdesk.ticket`, when installed.
  - `account.analytic.line` for timesheets: `date`, `unit_amount` in hours, `name`, `project_id`, `task_id`, `helpdesk_ticket_id`, `employee_id`, `user_id`.
  - `hr.attendance`: `employee_id`, `check_in`, `check_out` (datetimes, so instants: no civil-date handling).
  - `hr.employee`.

## 7. UI (dashboard, desktop app)

- **Work items view:** in the sidebar's Work group (direction A), shown in the desktop app when some connection allows `search`.
  - Connection picker; decided (user, 2026-11-18; Q1 in `generic-read-step.md`): all connections, narrowed by the picker, as "Search Jira & GitLab…".
  - Filter chips: Assigned to me, Unassigned, Not mine, Created by me, Open/Done.
  - A text box, and a JQL toggle for Jira.
  - The results are a list of `WorkItem`s, separate from the timeline: they are current state, not events. They never mix with entries, filters, export or the timeline search.
  - Selecting a result docks its detail beside the list above 1100 px, with the flyout below that and the bottom sheet on phones.
- **Action bar** in the detail panel, for work items and for timeline entries that point to one (a Jira entry, a GitLab MR event, a commit mentioning a key):
  - **Open** in the browser.
  - **Comment…**, **Move to…**, **Assign to me**, **Log time…**.
  - Each action shows only when the connection allows it. When the token lacks the scope, it is disabled with the reason.
  - Opening a link needs a desktop command that opens only URLs core built (`WorkItem.url`): the app's only opener is `tauri-plugin-opener` called from Rust by `open_url`, and the webview gets no general one (decided, Q2 in `generic-read-step.md`).
- **New item:** a button in the Work items view and an entry action ("Create ticket from this…" prefills the summary and description). The form comes from `DescribeCreate`: connection, project, type, then the type's fields.
- **Log time:**
  - Opened from one entry, a selection of entries (multi-select in the day and week views and on the timeline), or a work item. The prefill comes from section 4.
  - In the day and week views, `timeEntries` draw as blocks, and the day header shows the day's total.
- **Confirmation, one pattern for every write:**
  - The dialog shows the preview core rendered: connection label, target key and title, each field as it will be sent, any recent-write warning, and a warning for writes others will see ("Comments notify watchers").
  - The confirm button names the effect ("Create in Work Jira", "Log 1h30m on ABC-123"); it is never a generic "OK".
  - Enter doesn't submit.
  - On success, the returned item or time entry appears at once, with a toast and a link.
  - On `OutcomeUnknown`, a sticky warning with what to check.
  - The UI never offers to undo a remote write.
- **Settings:** each connection pane gets an **Access** section (desktop only, like the other settings):
  - Toggles: "Search and read items", and one per write capability the kind supports ("Create tickets", "Comment", "Change status and assignee", "Log time").
  - Next to each toggle, the probed token state ("token lacks `api` scope").
  - Turning on the first write capability asks for confirmation.
- **Command palette (later):** `Ctrl+K` for "Search Jira…", "Log time…", "New ticket…". It is not needed for the first phases.

## 8. Security and safety

Writing is a security boundary: Activity Log starts acting **as you** in shared systems.

| Threat                                                                                                                                                 | Mitigation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Accidental writes** (wrong ticket, double booking, wrong connection)                                                                                 | Writes off by default per connection. Prepare/confirm is enforced in core, and a confirm carries only the plan id. The preview names the connection and target and lists existing time that day, and recent identical writes in the audit raise a warning. Confirm ids are used once and die with the runtime on a config change. Writes are never retried automatically.                                                                                                                                                                                                                                                                                            |
| **Changed settings between prepare and confirm** (write turned off, token or URL changed)                                                              | Pending plans live in the runtime that is replaced on save and reload. Confirm also compares the config and policy fingerprint and re-runs the allow-list and channel checks.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| **Requests to the local API server** (`api/` is an unauthenticated HTTP server on `127.0.0.1`, and any local process or web page can send it requests) | **No actions over HTTP** (phases 1–3), reads included. **No Host check** (decided against): the server binds only `127.0.0.1` (`api/src/main.rs` builds its `rocket::Config` from `Config::default()`, so `ROCKET_ADDRESS` is not read), and the DNS-rebinding exposure of the activity routes is accepted.                                                                                                                                                                                                                                                                                                                                                          |
| **Prompt injection** (an AI agent reads ticket and meeting text that can tell it to act)                                                               | Remote text is data: core never interprets it as instructions, templates or commands; actions come only from typed requests. Agent results are typed and truncated, carry no URLs except core-built ones and no credentials or config values. The agent gets only what `agent_allow` grants (none by default), its writes always wait for a human, and every agent request is in the durable audit. The closed verb set caps what an injected agent could ask for. Agent reads go through a recursive projection, so a nested GitLab event in a Git entry is shown only if the agent may see that GitLab connection. Details under "Agent surface" in section 5.     |
| **Path injection through keys**                                                                                                                        | Strict key parsing per provider in `plan` and `get`, and paths built with `Url::path_segments_mut()` (section 3).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| **Field injection on create**                                                                                                                          | `FieldValue` is a closed union, never `serde_json::Value`. `plan` fetches createmeta again and rejects unknown field keys and mismatched kinds.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **Token leakage**                                                                                                                                      | Plans hold no credentials; auth is added when the request is sent. Previews are rendered from typed fields, never from request bodies (Odoo JSON-RPC carries the key in the body). Errors are cleaned of bodies; the audit holds no content; confirm ids are never logged. Writes are refused on Jira connections with `insecure = true` (an invalid certificate makes a man-in-the-middle easy). Secrets stay write-only in settings and in the `0600` file, and keychain storage stays the planned later step.                                                                                                                                                     |
| **Untrusted remote content in the desktop webview** (ticket descriptions, comments)                                                                    | A Content Security Policy in `desktop/tauri.conf.json` (there is none today) before any remote text is shown: no remote scripts, no inline scripts, no remote frames. Render as plain text (Angular escaping; no `innerHTML`, no wiki or ADF rendering). The app's own commands take part in Tauri capabilities only once the app declares an app manifest (in `build.rs`), which it doesn't today, so `run_action` is reachable from any script in the webview. That is why the CSP and core's origin and confirm rules matter, and an app manifest is worth adding. Core builds every link (`WorkItem.url`, `http`/`https` only) and never passes on a remote URL. |
| **Query injection**                                                                                                                                    | Structured filters build JQL with quoted, escaped literals. Native JQL is the user's own input and can only read. Odoo domains are built as JSON values, never from text.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |

**Review before phase 1c ships:**

- the executor's check order: allow list and channel come before any provider code;
- `PendingActions`: random ids, expiry, once only, dropped on runtime replacement, fingerprint check, `Confirm` carrying only the id;
- strict key parsing and path building;
- the closed `FieldValue` and the createmeta re-check;
- that plans hold no credentials and exactly one mutation;
- the error cleanup function;
- the audit file's contents (metadata only);
- the desktop CSP;
- the agent socket: private folder or owner-only pipe, peer check, frame limits, origin fixed to `Agent`;
- agent requests: immutable snapshot, origin and confirmer separate, policy checked twice, handle not logged, `not_found` for anything inaccessible;
- the agent projection: recursive, typed, size-capped;
- the confirmation display: trusted chrome, the typed challenge, no carry-over;
- the audit: lock, `applying` before sending, fail closed.

## 9. Phases

Each phase ends green on `pnpm verify` (and `pnpm win:check` where Windows code changes) and updates the README and docs.

**Phase 0: groundwork, no new features**

- `JiraApi` and `GitLabApi`, shared by the check and the collector, with path building through `path_segments_mut`.
- The error cleanup function, used by the collectors.
- `allow` reserved and parsed into `Connection` (fails closed, unused yet).
- Tests:
  - Error cleanup: no body leaks.
  - Path building with odd user names.
  - `allow` parsing: missing, empty, malformed (→ `[]`), unknown names.

Phase 0, 1a and the agent policy of 1b are planned in detail in [generic-read-step.md](generic-read-step.md): Jira and GitLab together, without the Host check, with `agent_allow` and the policy intersection landing early.

**Phase 1a: search and details for Jira and GitLab (read-only)**

- `core::actions`: the executor with caller, policy, limits and `ActionCall`; `agent_allow` parsed and intersected for the `Agent` origin.
- `ActionProvider` for Jira and GitLab: `search`, `get`, `probe`.
- DTOs and generated TypeScript; `run_action` and `open_work_item` in Tauri; CLI `<source> search|show` (checks the targeted connection only).
- Dashboard: Work items view (desktop), detail panel with Open through a core-built URL.
- Settings: Access section showing the probe; the desktop CSP.
- Tests:
  - JQL building for each filter (EMPTY handling, quoting, escaping).
  - Mapping recorded search and issue responses.
  - Executor refusals in order: unsupported, not allowed, unavailable; awaiting a running check.
  - Strict Jira key parsing.
  - `WorkItem.url` built only from the connection URL.

**Phase 1b: agent reads over MCP**

- The desktop's agent socket or pipe (private folder or owner-only DACL, peer check, frame limits) and single instance.
- `activity-log mcp` (stdio, `rmcp` pinned) as a thin adapter: `search`, `get_item`, `activity`, `time_report`.
- Agent response types and the recursive projection (required before the transport ships). `agent_allow` and the policy intersection already land in 1a.
- Limits at the socket.
- The audit file with its lock (no agent writes yet).
- Docs: adding the server to an agent host, and the boundary (shell, PTY, file access and UI automation defeat it).
- Tests:
  - `agent_allow` missing → nothing visible; the intersection with `allow`.
  - Allowed Git and denied GitLab: a merged git/lab entry shows no GitLab part, no nested `raw`, no GitLab count in the summary, no GitLab name in `pending` or errors, and nothing from diagnostics.
  - Truncation and response caps; no `raw`, remote URL or token in any tool result.
  - Frames over the limit, too many connections and read budgets are refused.
  - A frame that claims another origin can't (there is no field); a symlinked socket path is refused at start; the adapter without a running desktop answers "not running".
  - Tool schemas generated from the same types.

**Phase 1c: the first write (Jira create), from you and from the agent**

- `describe_create`, `plan`/`apply` for `Create`.
- The write path from section 3: `PendingActions` in the runtime with fingerprint and channel (S1, S2), random confirm and correlation ids (S6), the `confirm_action` command, `actions.jsonl` audit with the recent-write warning (S7), typed plan with one mutation and core-rendered preview (S8), closed `FieldValue` with the createmeta re-check (S9).
- New item dialog with preview and confirm; CLI `jira create` with `--dry-run`/`--yes`.
- Agent writes: `request_write`, `request_status`, the desktop inbox and confirmation display, and the audit states with `applying` before sending.
- The returned item is inserted by the client.
- Tests:
  - Prepare → confirm → second confirm returns the stored result.
  - Expired confirm, and confirm after a runtime swap → `Expired`.
  - Confirm from another channel refused; confirm after `allow` removed refused.
  - Plan has one mutation and no secret.
  - Unknown field and wrong kind rejected.
  - Audit line contents; recent-write warning.
  - CLI `--dry-run` sends nothing, and no terminal without `--yes` fails.
  - Dashboard confirm flow.
  - Agent request:
    - refused at planning when `agent_allow` lacks `create`; refused at confirmation after `agent_allow` (or `allow`) is narrowed;
    - expired after a settings change;
    - origin stays `Agent` whatever the confirmer;
    - the approved snapshot is what is sent;
    - no confirm call on the socket;
    - write-request limits (per connection, total, per hour, input length).
  - `request_status`: unknown, expired and no-longer-visible handles all get `not_found`; results use the agent projection; the handle is in no log.
  - Crash between `applying` and the outcome → `outcome_unknown` in the inbox after restart, no replay. A failed audit append → nothing sent.
  - Display: control and bidi characters made visible; Confirm locked until the target is typed; no focus on the next request.

**Phase 2: time tracking with Jira**

- `ActivityLog.timeEntries` and `collect_time` outside the day cache; Jira worklog collection (plain worklogs).
- `CollectRequest.days` (used by Odoo later).
- Tempo detection in the Jira probe, shown in settings as information.
- Before building: check the Tempo assumption from section 6 on a Tempo site if one is at hand.
- `LogTime` with existing time that day in the preview.
- Dashboard: Log time dialog from entries and items, time blocks and day totals in the dashboard.
- CLI `time log`, `time report`.
- The AGENTS.md rule on civil dates.
- Tests:
  - Worklog filtering by author and `[start, end)`, including a worklog added later for an old, settled day.
  - Jira's `started` format.
  - Tempo probe: `404` → not installed; `200`, `401` or `403` → installed; no effect on status.
  - Suggestion: duration rounding, key extraction from commits, branches and meetings.
  - Dashboard and CLI totals by local day.

**Phase 3: GitLab, and the rest of Jira's writes**

- Jira: `comment`, `transition`, `assign`.
- GitLab: `create`, `comment`, `log_time` (GraphQL), timelog collection, and the write scopes in the probe. Search, get, the scope probe and strict `#`/`!` refs moved to phase 1a.
- Tests: mapping fixtures, `read_api` against `api` probe results, ref parsing, the transition `Conflict` when the transition list changed.

**Phase 4: the Odoo provider**

- New kind with `url`, `database`, `login`, `api_key`, `time_target` and `read`, plus the `Choice` config value type.
- `OdooApi` on JSON-2 only; older versions are reported as unavailable.
- Startup check: version, employee, installed apps and booking targets, expired-key reason, Online plan hint.
- Collection: timesheet lines by `days`, and attendance entries when `read` has `attendance`.
- Actions: `search` and `get` on the `time_target` model, `create` task, `log_time`.
- Dashboard adapter and icon; it leaves "coming soon".
- Tests:
  - JSON-2 request shapes, the version check, the key's user against `login`.
  - App and target detection from recorded `fields_get` and access answers (helpdesk and attendance missing, timer present).
  - `time_target`: the key form refused on another target; the right line field per target.
  - Employee missing.
  - Hours ↔ seconds; day-only time entries; attendance entries at their instants, including an open check-in.
  - The key never appears in a plan, preview or error.

**Phase 4b (optional): Odoo attendance writes, later the timer**

- The `attendance` capability (check in, check out) once the rights question in section 6 is checked on a real instance.
- The timer only if wanted, after checking how Odoo stops it.
- A JSON-RPC fallback for older Odoo versions, only if an instance needs it.

**Phase 5: polish**

Command palette, saved searches (where they live is undecided: `config.toml` per connection or browser storage), GitLab `update`, entries marked as covered by a time entry.

Each later phase adds its new actions to the MCP tools under the same rules.

### Refactor this needs, and why

- **One API client per remote provider** (`JiraApi`, `GitLabApi`, later `OdooApi`). The check, the collector and the actions would otherwise build clients, auth, paths and error handling three times.
- **`Connection` gains `allow`**, and `core::settings` reserves and reads it. Permissions must be part of the connection that core resolves, not a UI setting, or the CLI would skip them.
- **`CollectionRuntime` gains `PendingActions`, and hands out a `&ProviderRuntime` per connection** (awaiting its check), so plans die with the runtime they were made in.
- **`ActivityLog.timeEntries`, `CollectRequest.days` and `collect_time`**, the time model that keeps core free of date↔instant conversion.
- **Error cleanup in the collectors**, which today echo response bodies.
- **The desktop CSP**, before any remote text is shown (phase 1a). The Host-check fairing is dropped by decision.
- **A `Choice` config value type** (single and list) for Odoo's `time_target` and `read`.
- **The agent surface:** the desktop's agent socket and inbox, a thin MCP subcommand in `cli`, `agent_allow` in settings, and agent response types with their projection.

### Risks

- **Jira variety:** custom required fields, screens, workflows with transition screens. Phase 1c refuses what it can't fill rather than guessing.
- **Tempo:** if the assumption in section 6 is wrong, or Tempo requires attributes, plain worklogs may be refused or incomplete. Tempo's own API is then the fallback, with its own token.
- **Double bookings and unknown outcomes:** reduced by the preview, the once-only confirm and the recent-write warning, but not ruled out without remote idempotency keys.
- **Contract growth:** every verb is a contract item. Adding verbs needs a design note, which is intended.
- **Odoo variety:** apps differ by edition and version (helpdesk timesheets, the timer). The startup check reports what is there instead of assuming it. Instances older than JSON-2 wait for the optional fallback.

## 10. Decisions and open questions

### Decided

1. **Jira: Data Center only**, the Jira you use today. Cloud is not planned (section 6).
2. **Jira time: plain Jira worklogs** through the worklog REST API. Whether your company uses Tempo is unknown. Tempo on Data Center is assumed to store its time as Jira worklogs, which is to be checked. The probe shows whether Tempo is installed. Tempo's own API comes later and only if needed.
3. **Odoo: the latest version, JSON-2 first, any hosting.**
   - Configured by URL, database, login and API key; the Odoo Online Custom-plan requirement is help text only. JSON-RPC for older versions is a later option.
   - What time is booked on (`time_target`: task by default, ticket or project) and which time records are read (`read`: timesheets by default, attendance) are per-connection settings.
   - The startup check reports which apps are installed.
4. **An AI agent is a near-term goal.**
   - The agent works through a thin MCP adapter (stdio) that talks to the running desktop app. The desktop is the only authority for policy, requests, confirmation and audit.
   - The agent has its own `Agent` origin and its own `agent_allow` list, which grants nothing by default.
   - It gets reads in phase 1b. Writes come in phase 1c, only as requests a human confirms in the desktop inbox. CLI confirmation of agent requests is deferred.
   - The durable audit applies to it from the start.

5. **No Host check** on the local API server; the DNS-rebinding exposure of today's activity routes is accepted.
6. **Further decisions** (UI direction A with C's calendar look, saved agent drafts, generic read step for Jira and GitLab together, AI access select per connection, naming, shortcuts, approved `rmcp` and `tauri-plugin-single-instance`) are in [open-decisions-design.md](../open-decisions-design.md).

### Still open

Nothing.

### Defaults adopted unless you say otherwise

- **No actions over the HTTP API server**, reads included, in phases 1–3. The desktop app and the CLI only.
- **Search and read on by default** for every connection in the desktop app and the CLI (confirmed); writes (including Odoo `attendance`) always need explicit opt-in.
- **Civil dates are data:** clients send local days (`LogTime.day`, `CollectRequest.days`), remote systems supply their dates, and core never converts between dates and instants.

## Not verified while drafting

Public documentation could not be fetched from this environment (the doc hosts were blocked), so these come from memory or the review and need a check before building:

- **Jira:** the `mypermissions` parameter rules on Data Center, and the createmeta endpoint versions.
- **Tempo:** that Tempo Timesheets on Data Center stores time as Jira worklogs (both directions), and which read-only Tempo REST resource tells reliably whether it is installed.
- **GitLab:** `GET /personal_access_tokens/self` (its version and the `granular_scopes` field), and the GraphQL `timelogCreate` arguments.
- **Odoo, API:** the JSON-2 details (path, headers, how access checks are called), the Custom-plan requirement on Odoo Online, and API key lifetimes.
- **Odoo, apps:** which edition and version provide helpdesk timesheets, attendance and the timer; whether a regular user may create and close their own `hr.attendance` through the API; and how the timer is stopped.
- **Tauri:** that app-defined commands take part in capabilities only with an app manifest.
- **MCP:** the `rmcp` release to pin (the reviewer saw 3.10.0 with `transport-io` in its docs) and its minimal features.
- **Desktop agent socket:** the tokio named-pipe options (`first_pipe_instance`, `reject_remote_clients`, an owner-only DACL through raw security attributes), peer-credential calls per OS, `tauri-plugin-single-instance` for Tauri 2, and `std::fs::File::lock` (needs Rust 1.89 or later).

These were checked in the code: the runtime is replaced on save (`desktop/src/commands/mod.rs`, `update_config_at`) and on reload (`api/src/reload`); the dev proxy has no `changeOrigin`; `api/src/main.rs` configures Rocket from `Config::default()`; there is no CSP or opener plugin; there is no per-connection re-collect; `entry_values` warns on unknown fields; and the GitLab collector builds paths with `format!`.
