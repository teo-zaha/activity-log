# Shared contracts: design review for the merged plan (step 0)

Status: design, not built. Revision 5 closes the three points of Astra's check of revision 4 (`scratchpad/review/step0-d1d7.md`; rows "Revision 5" in [Final check resolutions](#final-check-resolutions-revision-4)). Revision 4 followed Astra's final check of revision 3 (`scratchpad/review/step0-final.md`); its defects D1–D7 are mapped in the same table. Revision 3 followed Astra's re-review of revision 2 (`scratchpad/review/step0-rereview.md`); each re-review point is mapped to the section that closes it in [Re-review 2 resolutions](#re-review-2-resolutions). Revision 2 followed Astra's independent review (`scratchpad/review/step0-review.md`, verdict "rework before implementation", 19 findings and a position on each of the 23 recorded deviations). Every finding was checked against the code; the dispositions are in [Review response](#review-response-revision-2), and Astra's positions on the deviations are answered in [Changes to the merged plan](#changes-to-the-merged-plan). The user decided the five open points on 2026-11-19; each is marked **decided (user, 2026-11-19)** where it applies ([list](#user-decisions-2026-11-19)).

This is step 0 of the merged whole-app plan (revision 5 with the two user notes after it: splash step 1a dropped, B67 flaky tests). AGENTS.md asks for a design review of every new or changed shared contract before it is implemented; steps 1–14 code against this document.

It is a plan document rather than an ADR because the repository's ADRs (`docs/adr/0001`–`0005`) are short records of one decision each, while this file specifies types, wire formats, ordering rules and tests that several steps implement. Decisions in it that deserve a record of their own (push instead of polling, the write protocol) can get ADRs in the docs step (R-X1, step 13).

## Contents

| §   | Contract                                                        | Plan items       | Implemented in                                      | Wire change                                           |
| --- | --------------------------------------------------------------- | ---------------- | --------------------------------------------------- | ----------------------------------------------------- |
| 1   | Config write protocol, conflicts, `config edit`                 | R-C1             | step 1 (desktop parts in step 7)                    | additive in step 7: `revision`, `basedOn`, remove DTO |
| 2   | Runtime host, run generation, `RunSpawner`, `RunEnd`            | R-C2, B66, NF-5  | step 5 (spawner used by NF-5 in step 12)            | none                                                  |
| 3   | Run report and result store                                     | R-C3             | step 5                                              | none                                                  |
| 4   | Identity, collection key, access token, cache scope and writer  | R-C5, B4, R-B4   | identity step 2, cache step 3, keys step 5          | none                                                  |
| 5   | Push: publishing, `GET /api/events`, Tauri events, save receipt | R-P1, §3.1       | core in step 5, wire and dashboard in step 6a       | additive: route, events, command, DTOs, receipt field |
| 6   | Additive DTO fields: field issues, `recheckable`, constants     | R-B2, R-F5, R-D2 | constants step 6, the rest step 7                   | additive: command, 3 DTOs, field, generated constants |
| 7   | Dashboard URL module, `?item=`, Work availability               | R-F4, N-1, N-15  | step 6 (`item` and Work routing in generic slice 9) | none (URL vocabulary gains `item`)                    |
| 8   | Shortcut registry ownership                                     | R-F14, NF-6      | step 10, menu in step 10a, `work.search` in slice 9 | none (dashboard-internal)                             |
| 9   | Not covered here; requirements on generic slices                | slices 3, 9      | —                                                   | —                                                     |

## Versions relied on

Checked in the installed crates and `dashboard/node_modules`:

- **Rocket 0.5.2**: `rocket::response::stream::{EventStream, Event}`; `Event::data(impl Into<Cow<'static, str>>)` (`sse.rs:189`), `Event::retry(Duration)` (`:221`), `.event(name)` (`:242`); `EventStream` sends a heartbeat comment every 30 s by default (`:573`); `data` must not hold raw carriage returns. `rocket::Shutdown` is a request guard (`shutdown.rs:68`). The local async client can read a streaming body incrementally (`local/asynchronous/response.rs:193`).
- **tokio 1.54.1**: `watch::Sender::send` drops the value when there is no receiver (`watch.rs:1082`); `send_replace` always stores it and returns the old one (`:1247`); `Sender::subscribe` (`:1405`). A `spawn_blocking` task keeps running when its `JoinHandle` is dropped (`task/blocking.rs:106`).
- **Tauri 2.12.5**: `Emitter::emit_str_to(target, event, String)` (`lib.rs:1020`). The payload string is inlined into a script as `payload: {payload}` (`event/mod.rs:199`), so it must be serializer-produced JSON, never assembled from untrusted text. Wry enqueues each script on the UI event loop (`tauri-runtime-wry` 2.12.3 `lib.rs:1740`); the mock runtime records scripts without running them (`test/mock_runtime.rs:598`). `tauri::ipc::Response::new(String)` returns a pre-serialized JSON body from a command (`ipc/mod.rs:200`). **@tauri-apps/api 2.12.4**: `listen` resolves to an `UnlistenFn` once registered.
- **ts-rs 12.0.1**: `u64` maps to `bigint` unless overridden (`lib.rs:596`); `#[ts(type = "number")]` overrides one field. `Arc<T>` is transparent (`lib.rs:1073`).
- **serde 1.0.229**: `Serialize` for `Arc<T>` needs the `rc` feature, not enabled yet (`Cargo.toml:22`).
- **toml_edit 0.25.17**: `TomlError::message()` and `span()`; its `Display` prints the offending source line (`error.rs:104`), which is how B2 leaks a token.
- **tempfile 3.27.0**: creates the Windows file with default security and only then returns it (`file/imp/windows.rs:26`), so it is not used for private files; `persist` is `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` (`:92`), and a moved file keeps its own security descriptor, which the private-file helper relies on through `std::fs::rename`.
- **windows 0.62.2** is already a Windows dependency of `core` with `Win32_Security`; the private-file helper adds the `Win32_Storage_FileSystem` and `Win32_Security_Authorization` features (no new crate).
- **Rust 1.99.1** (the Nix shell): `std::fs::File::try_lock` (stable since 1.89), `std::path::absolute`.
- **Angular router 22.2.4**: query values are encoded with `encodeURIComponent` except `@ : $ , ;` (`_router-chunk.mjs:417–422`), so `/` becomes `%2F` and `#` becomes `%23`; parsing decodes them and turns `+` into a space (`:432`). The app router uses `defaultQueryParamsHandling: 'merge'` (`app.routes.ts:118`). `Router.lastSuccessfulNavigation` is a signal.
- **@signality/core 0.5.0**: no `EventSource` composable; `routeData()` reads the injected `ActivatedRoute`, so it is not usable from a root service.

## Conventions used below

- **One model per file** (AGENTS.md): every new struct or enum shared outside its module gets its own file under `core/src/model/<domain>/`. New domain folder: `core/src/model/runtime/`. Types only one module uses stay in that module.
- **Generated DTOs**: `#[derive(Serialize, Deserialize, TS)]`, `#[serde(rename_all = "camelCase")]`, `#[ts(export, rename_all = "camelCase")]`, listed in `core/src/ts.rs`, generated by `pnpm types:generate` into `dashboard/src/app/generated/activity.ts`.
- **Additive** means: existing routes, commands and events stay; an existing response may gain fields, which changes its bytes but not the meaning of any existing field, so a client written against the old contract keeps working by ignoring them; new request fields are optional and their absence means today's behaviour. Fixtures pin the old fields, statuses and wrappers separately from the new ones.
- **Opaque string**: a client may only compare two values for equality, and only within one `epoch` (§5).

---

## 1. Config write protocol (R-C1, step 1)

### Purpose

Every writer of `config.toml` (the CLI `sources` and `config edit` commands, the desktop settings commands, slice 2's `set_access` and slice 10's access edits) goes through one protocol: an interprocess lock around re-read, precondition check, change, check, and a publish that is private from creation and atomic. Fixes B3, B6, B2 and, through the reader changes, B9 and B10.

### Location and reading

`core/src/settings/mod.rs` keeps `ConfigFile`; reading is split so a reload can tell an I/O error from bad content (finding 8):

```rust
/// The bytes of the config file, `None` when it does not exist.
pub fn read_config_bytes(path: &Path) -> std::io::Result<Option<Vec<u8>>>;

/// A stable FNV-1a hash of config bytes (`utils::stable_id`); a missing file has the id of empty
/// bytes. Used only inside one process, never sent to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(u64);
impl ContentId { pub fn of(bytes: &[u8]) -> Self; }

impl ConfigFile {
    /// Parses `bytes` (invalid UTF-8 and invalid TOML both fail). The error names the path, line,
    /// column and the parser's short message only, and has no `source()`: no part of the file's
    /// text can reach a log through the error chain (B2).
    pub fn parse(path: impl Into<PathBuf>, bytes: &[u8]) -> Result<ConfigFile, ConfigError>;
    /// `read_config_bytes` then `parse`; missing is empty. For readers that do not reload.
    pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<ConfigFile>;
}
```

- `ConfigFile::resolve_path(explicit) -> Option<PathBuf>` keeps its order (`--config`, `ACTIVITY_LOG_CONFIG`, the OS config folder) and returns an absolute path (`std::path::absolute`), so a relative single-file override has a real parent folder. It returns `None` when there is no explicit path and no OS config folder (no `HOME`, `APPDATA` or `XDG_CONFIG_HOME`).
- **No config folder: start empty and refuse to save (decided, user, 2026-11-19).** `config_dir()` no longer falls back to `env::temp_dir()`, and there is no other fallback location, so tokens are never written to a temporary or shared folder. With `resolve_path` returning `None`:
  - reading gives no sources, with one warning per process (`NoConfigLocation`'s message below);
  - every write fails before touching the disk: `edit_config` is not reachable without a path (callers get `EditError::NoLocation`), and the desktop settings commands, the CLI `sources` commands and `config edit` report the message;
  - `activity-log config path` prints the message instead of a path and exits with status 1;
  - the host starts with no sources and watches nothing (`Host::start` takes `Option<PathBuf>`, §2).

  The message, one text everywhere: "Activity Log has no folder for its settings. Set ACTIVITY_LOG_CONFIG to a file path, or start it with --config <path>." This replaces the signed R-C1 "private fallback folder" by the user's decision (change 21).

- Readers never take the lock and never write.

### Private files (shared helper)

`core/src/platform/private_file/mod.rs` + `tests.rs`, with `windows`, `macos` and `linux` child modules behind the parent's dispatch (AGENTS.md OS rules). Used for every file that holds settings or collected data: the config writer, `config edit`'s session folder and the cache writer (§4). Lock files hold nothing and need no protection.

```rust
/// A folder beside `target` (same file system) in which only the current user can open or create
/// anything, verified before it is returned. It holds open handles to the target's parent folder
/// and to itself; every later operation goes through those handles, never through a path.
/// Removed on drop.
pub fn private_staging(target: &Path) -> Result<Staging, PrivateFileError>;
impl Staging {
    /// A new file inside, private from its creation (fails if the name exists).
    pub fn create(&self, name: &str) -> std::io::Result<File>;
    /// Opens a file inside by name, relative to the staging handle, without following links.
    pub fn open(&self, name: &str) -> std::io::Result<File>;
    /// Moves a file of this folder over the target atomically, relative to the two verified
    /// handles. The result keeps its private protection.
    pub fn publish(&self, name: &str) -> std::io::Result<()>;
    /// For handing the session copy to an editor (`config edit`); not used by the app itself.
    pub fn path(&self) -> &Path;
}
/// Whether an existing file can be opened by anyone but its owner (mode bits, ACL entries).
pub fn readable_by_others(path: &Path) -> std::io::Result<bool>;

pub enum PrivateFileError {
    /// The location cannot hold private files: a file system without permissions or ACLs that
    /// can be set (FAT, exFAT, some network shares), or protection that did not stick.
    Unprotectable { location: PathBuf },
    Io(std::io::Error),
}
```

**One recipe per system** (D2, D3). Secrets are written only into a file created inside a verified staging folder, so no other account can ever have opened it.

**The parent folder and handle-bound publication** (revision 5): first the target's parent folder is opened and checked, on every system, before anything is created:

- it must be owned by the current user (or by root/SYSTEM), and **no other account may create, rename or delete entries in it**: on Unix no group or other write bit and no ACL entry granting another principal `write`, `add_file`, `add_subdirectory` or `delete_child`; on Windows no ACE for a SID other than the user, `SYSTEM` or `Administrators` granting `FILE_ADD_FILE`, `FILE_ADD_SUBDIRECTORY`, `FILE_DELETE_CHILD`, `DELETE`, `WRITE_DAC` or `WRITE_OWNER` (read with `GetSecurityInfo` on the handle);
- otherwise `Unprotectable`. The users' normal folders (`~/.config`, `~/Library/Application Support`, `~/Library/Caches`, `%APPDATA%`, `%LOCALAPPDATA%`) pass; a world-writable folder such as `/tmp` does not.

Every later step uses handles, so a name another account could rename is never followed:

- **Unix**: the parent is opened with `O_DIRECTORY | O_NOFOLLOW` (the path's own links were resolved once in step 1 of Writing); the staging folder is made with `mkdirat` on that descriptor and opened with `openat(…, O_DIRECTORY | O_NOFOLLOW)`; files with `openat(staging, name, O_CREAT | O_EXCL | O_NOFOLLOW, 0600)`; publication is `renameat(staging, name, parent, target_name)`; the editor's copy is read back with `openat(staging, "config.toml", O_NOFOLLOW)`.
- **Windows**: the parent is opened with `CreateFileW(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)`; the staged file is published with `SetFileInformationByHandle(FileRenameInfoEx)` on its own handle, with `RootDirectory` set to the parent's handle and `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`; the editor's copy is read back by its path inside the staging folder, which no other account can rename or replace (the folder's protected DACL and the parent check above).

Per system:

- **Windows**: the staging folder is created with `CreateDirectoryW` and a `SECURITY_ATTRIBUTES` built from exactly one descriptor, `ConvertStringSecurityDescriptorToSecurityDescriptorW("D:P(A;OICI;FA;;;{owner SID})")`: `P` sets `SE_DACL_PROTECTED` (no inherited ACEs), the single ACE grants full access to the current user's SID (from `GetTokenInformation(TokenUser)`), and `OICI` makes every file and folder created inside, including an editor's backups, inherit only that ACE. Files are created with `CreateFileW(CREATE_NEW)` and the file form `D:P(A;;FA;;;{owner SID})`. Before returning, the folder is checked with `GetVolumeInformationByHandleW`: without `FILE_PERSISTENT_ACLS` the result is `Unprotectable`, and nothing is written. **Cleanup** (step 1 review): the folder is held open with `DELETE` access and without delete sharing; on drop, each entry is opened by name and must be a plain file whose real path is in the held folder, and is deleted through its handle (`FileDispositionInfoEx`, POSIX semantics), then the folder through its own handle. If anything else is inside (a folder, a link, an entry that cannot be checked), the whole staging or session folder is kept with one warning naming it; nothing is ever removed by path. New `windows` crate features: `Win32_Storage_FileSystem`, `Win32_Security_Authorization` (no new crate).
- **macOS**: the same rule by name: `fstatfs` must report `f_fstypename` `apfs` or `hfs`, and the volume must not be mounted with `MNT_IGNORE_OWNERSHIP` ("Ignore ownership on this volume"); SMB, NFS, `macfuse`, `msdos` and `exfat` are `Unprotectable`. The folder is created with mode `0700`, then its ACL is set to an empty ACL (`acl_init(0)` and `acl_set_fd` on the folder's descriptor), which removes any entries inherited from the parent; then `fstat` must show `0700`, the owner must be the current user, and `acl_get_fd` must return no entries; otherwise `Unprotectable`. Files inside are created with `O_CREAT | O_EXCL` and mode `0600`; with an empty folder ACL they inherit no entries, and each file's ACL is checked empty too before writing. The ACL functions come from libSystem through a small `extern "C"` block (`acl_init`, `acl_set_fd`, `acl_get_fd`, `acl_get_entry`, `acl_free`).
- **Linux**: the parent folder's file system must be one known to enforce permissions locally: `fstatfs` on the parent's descriptor must report `f_type` ext2/3/4 (`0xEF53`), XFS (`0x58465342`), Btrfs (`0x9123683E`), F2FS (`0xF2F52010`), ZFS (`0x2FC12FC1`), bcachefs (`0xCA451A4E`) or tmpfs (`0x01021994`). Anything else, including overlayfs, FUSE (`sshfs`, `rclone`), NFS, CIFS/SMB, 9p, `vboxsf` and `virtiofs`, is `Unprotectable` before anything is created, because such mounts can report owner-only modes without enforcing them for other local accounts. overlayfs is refused because the file system of its upper layer cannot be proven: mount ids are reused and a later mount can cover the configured upper path (step 1 review, decided by the coordinator). The folder is created with mode `0700`; any POSIX ACL it received from a parent's default ACL is removed (`fremovexattr` of `system.posix_acl_access` and `system.posix_acl_default`); then `fstat` must show `0700` and the owner, and `fgetxattr` must report both attributes absent (`ENODATA`) or unsupported by the file system (`ENOTSUP`, where the mode is authoritative); otherwise `Unprotectable`. Files are created `0600` with `O_EXCL`. `libc` becomes a Linux dependency of `core` too (it already is one on macOS; same crate and version).
- **Between creation and verification** the folder is empty: nothing secret exists yet, and a file another account managed to create in that moment is found by the emptiness check, which then discards the folder and starts over.

**What the user sees on such a location**: saving settings fails with "{folder} cannot keep the settings private; choose another location with ACTIVITY_LOG_CONFIG or --config" (desktop: inline save error with the draft kept; CLI: exit status 1). Reading the settings still works. The cache writes nothing there and logs one warning ("{folder} cannot keep cached activity private; collecting without a cache"); collection continues uncached.

**Existing files** (decision b of the final check): an unchanged file is **never** rewritten, on any system, and its permissions are never changed. A file the app writes is always owner-only, because it is a new file from the staging folder that replaces the old one (its old mode or ACL is gone with it). When `readable_by_others` is true for an existing settings file, the app logs one warning per process ("config.toml can be read by other accounts; Activity Log makes it private the next time it saves it"). README states both rules (R-X1, step 13).

`tempfile` is not used in production: it creates the Windows file with default security first (`file/imp/windows.rs:26`). It stays a dev-dependency.

### Writing

`core/src/settings/write/mod.rs` + `tests.rs`:

```rust
/// Edits the config file at `path` under the config lock. Callers without a location get
/// `EditError::NoLocation` before this is reached (decided, user, 2026-11-19).
pub fn edit_config<T>(
    path: &Path,
    precondition: Precondition,
    change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T>,
) -> Result<Edited<T>, EditError>;

pub struct Edited<T> {
    pub value: T,
    /// The file as written (or as found, when nothing had to be written).
    pub file: ConfigFile,
    pub content: ContentId,
    /// Problems in single connections of the result, for the caller to log after the write.
    pub warnings: Vec<String>,
}

pub enum EditError {
    /// No settings location (decided, user, 2026-11-19): nothing was read or written.
    NoLocation,
    /// Another process held the lock for the whole 5 s wait (decided, user, 2026-11-19).
    Busy,
    /// The precondition failed: the connection was changed, replaced or removed by another writer.
    Conflict(Conflict),
    /// `change` refused, or its result could not be read again; nothing was written.
    Rejected(anyhow::Error),
    /// The location cannot hold private files; nothing was written ("{folder} cannot keep the
    /// settings private; choose another location with ACTIVITY_LOG_CONFIG or --config").
    Unprotectable { location: PathBuf },
    /// Reading, locking or writing failed; the file is as it was.
    Io(anyhow::Error),
}
```

Sequence:

1. **Path**: if `path` is a symbolic link, resolve it with `fs::canonicalize` so the link stays and its target is replaced; a link to a missing file fails with "{path} links to a file that does not exist" (`Io`). A missing parent folder is created with `DirBuilder` mode `0700` (Unix) or the protected folder descriptor (Windows).
2. **Lock**: open or create the sidecar `config.toml.lock` beside the resolved path, then `File::try_lock()`. Only `TryLockError::WouldBlock` is retried, every 50 ms for up to 5 s, then `Busy` ("The settings file is busy; try again."). Any other lock error, such as a file system without locks, is `Io` with its own message, never reported as another process. The lock is released when the handle drops; the lock file is never deleted.
3. **Read** the bytes (missing is empty) and parse them.
4. **Precondition** (below), against the file as read, before anything else changes it: on a mismatch, `Conflict`; nothing is written.
5. **Change**: an error is `Rejected`. The change records which tables it adds or modifies (`add`, `set_value`, `rename`, `set_access`).
6. **Instances**: each table the change added or modified, and only those, gets a fresh `instance` if it has none (below). No other table is touched.
7. **Check**: `file.connections()` must succeed on the result, so an edit never writes a file the app cannot read.
8. **Skip**: if the file exists and the serialized bytes equal the bytes read, nothing is written and nothing about the file changes, on every system.
9. **Write**: `private_staging(path)` (which checks the parent folder and the file system first), create the new content inside, write, `sync_all`, `publish` (handle-bound, over the target); on Unix `fsync` on the parent folder's descriptor, best effort. `Unprotectable` stops before any byte is written. A failed write or publish leaves the original intact; the staging folder is removed.
10. Drop the lock and return. The caller logs "Settings saved" and the warnings **after** this returns.

Guarantee: other readers see the old file or the new one, never part of one. Durability across a power loss is best effort (folder sync on Unix only; `MoveFileExW` without write-through on Windows).

### Connection instances and preconditions (findings 6, R1, D1; decision e)

**Instance**: a reserved field `instance` in a `[[kind]]` table, a random 16-hex-digit id (not a secret, not a config key, not in any fingerprint, like `name`, `label` and `wsl`). It is written **only into a table the app is writing**: a connection it adds, or a connection it edits that has none yet (user decision, 2026-11-19: "written when the app first edits that connection"). Unrelated tables are never backfilled, so a hand-maintained or version-controlled file changes only where the app edits it. `ConnectionId` stays `kind/name`; the instance tells two lifetimes of the same id apart: a connection removed and added again by any cooperating writer gets a new instance, even when every value is byte-identical.

**Reading instances**: a value that is not 16 lowercase hex digits, or that an earlier table in the file (any kind) already uses, is read as **absent** for that table, with one warning; that table is then treated as legacy and gets a fresh instance when the app next edits it.

```rust
/// A field a writer can change: the label, a config value, or an access list (slice 2).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldId {
    Label,
    Value(&'static str),  // config key wire name, `GITLAB_TOKEN`
    Access(AccessList),   // `Allow` | `AgentAllow` (slice 2)
}

/// What a writer saw of one connection. Held in this process only: it includes secrets.
#[derive(Clone)]
pub struct EditBase {
    connection: ConnectionId,
    /// `None` for a legacy table that had no instance when the writer read it.
    instance: Option<String>,
    /// The touched fields and the kind's account fields, as seen; `None` for unset.
    fields: BTreeMap<FieldId, Option<String>>,
}

impl ConfigFile {
    /// The base for changing `touched` fields of `id`.
    pub fn edit_base(&self, id: &ConnectionId, touched: &[FieldId]) -> Option<EditBase>;
}

pub enum Precondition {
    None,
    Unchanged(EditBase),
}

/// What failed, without any value. `fields` is empty when the connection itself is gone or replaced.
pub struct Conflict {
    pub connection: ConnectionId,
    pub kind: ConflictKind, // Removed | Replaced | Changed | UnknownBase
    pub fields: Vec<FieldId>,
}
```

**Precondition check under the lock**, on the file as read (step 4), with `current` the table now named `connection`:

| Base instance | Current table              | Result                                                                                                                                                       |
| ------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| any           | none                       | `Removed`                                                                                                                                                    |
| `Some(x)`     | instance `x`               | compare the base's fields: equal → proceed; else `Changed` with the differing `FieldId`s                                                                     |
| `Some(x)`     | another instance, or none  | `Replaced` (re-added, or a writer outside the protocol rewrote the table)                                                                                    |
| `None`        | no instance (still legacy) | compare the base's fields (the pre-image of the touched and account fields): equal → proceed, and step 6 gives this table its first instance; else `Changed` |
| `None`        | an instance                | `Replaced`: another cooperating writer edited or re-created the table since it was read, and the two cannot be told apart; the safe answer is a conflict     |

The last row costs at most one extra conflict, only when two writers race on a table that had no instance yet; after reloading, the draft's base carries the instance and the next save proceeds. No row can repeat: a reload always yields a base that matches the current table.

- **Compared fields**: every touched field (label, values, access lists) and the kind's **account fields**, declared per kind in `provider!` (a new `account` flag on `ProviderConfigKey`: Jira and GitLab URL and token). A changed account field is a conflict even when untouched, because it means another account. Untouched other fields merge.
- **Limit**: a writer outside the protocol (a text editor on the real file, a script, `config edit`'s own editor session when the user keeps the `instance` line) that removes a table and writes an identical one with the same `instance` cannot be told apart from the original.
- **CLI**: a command that prompts (`sources add`'s questions, `sources set` without values) takes its base before the first prompt and calls `edit_config` once with the answers. `sources set` with values on the command line, `rename`, `unset` and `remove` use `Precondition::None`: they showed nothing.
- **Desktop settings (step 7)**: `ProviderConfigSnapshotDto` gains `revision: string` (additive). The desktop keeps, in process memory, every connection's instance (or its absence) and field values for the 16 latest snapshot revisions it handed out (`provider_config::SnapshotRegistry`). Requests carry the revision the draft was built from:
  - `ProviderConfigSaveRequestDto` gains `basedOn?: string`. The save's base is that revision's entry for the connection, for the fields the request touches.
  - `remove_connection` takes a new `RemoveConnectionRequestDto` that flattens `ConnectionId` and adds `basedOn?: string`; the Tauri argument stays named `request`, so today's `{ request: { source, name } }` still parses (additive). Its base is the instance and the account fields: removing a connection another program replaced under the same id is a `Replaced` conflict.
  - Without `basedOn` (an older dashboard), no precondition, as today. An unknown or evicted revision is `UnknownBase`.
  - The pane keeps `basedOn` in the connection's draft (`SourceDrafts`), so section switches, a resize across the phone breakpoint and Retry reuse it; a webview reload drops drafts, as today.
- **Recovery (decided, user, 2026-11-19)**:
  - Edits to different fields of a connection go through silently.
  - **Busy**: rejected with "The settings file is busy; try again." The pane keeps the draft and offers Retry, which sends the same request again.
  - **Conflict**: nothing is saved. The save's error string stays the envelope ("{label} was changed by another program while you were editing it. Nothing was saved." for `Changed`; "{label} was removed or replaced by another program. Nothing was saved." for the others). The pane keeps the draft, loads a new snapshot, and calls `check_provider_config` with the same request (§6.1), which returns the conflict as issues naming each `FieldId` (never a value), compared in process against the draft's revision, so an unchanged set token and a token changed to another set token are told apart. The pane shows the current values of those fields next to the draft's (secrets only as set or not set), and the user re-applies and saves with the new revision. `Removed`, `Replaced` and `UnknownBase` keep the whole draft and show it beside the current connection, or beside nothing if it is gone; the user can add it again (secrets they did not re-enter must be typed again).
  - **Unprotectable**: rejected with its message; the draft stays.
  - The CLI prints the same messages and exits with status 1; an interactive command can be run again.

### `config edit` (external editor)

`activity-log config edit` today opens the real file in `$VISUAL`/`$EDITOR` and creates it first if missing (`cli/src/main.rs:194`). It becomes a writer inside the protocol (findings R4, D1):

1. Create a session folder with `private_staging` beside the file (`Unprotectable` stops here) and copy the file's bytes (none if it is missing) into `config.toml` inside it. Editors' backup, swap and rename files land in that folder and inherit its owner-only protection. The real file is not touched: no creation, no permission change, no instance.
2. Run the editor on the copy, holding no lock.
3. If the copy's bytes equal the original's, remove the session folder and stop. **Opening and closing the editor without a change never modifies, creates or re-permissions the file.**
4. Otherwise, under the lock: re-read the file. If its bytes differ from those copied in step 1, stop with "config.toml changed while you were editing; your version is in {copy}" and keep the session folder. If the copy does not parse or `connections()` fails, keep it and print its path and the redacted error.
5. **Reconcile instances** against the original document, table by table (matched by kind and name):
   - a table **new** in the edited document gets a fresh instance (any instance it carries is replaced);
   - a table that **had an instance** keeps it only if the edited table still has that same instance and no other edited table uses it; if the instance was changed, copied elsewhere or **removed by hand**, the table gets a fresh one, so it counts as a new lifetime and any base that referenced the old id is `Replaced`;
   - a table that **had no instance** stays without one only if its content (its keys and values, ignoring formatting and comments) is unchanged; a **modified** legacy table gets a fresh instance, as any table the app writes does;
   - a malformed or duplicate instance in the edited document is treated as absent before these rules apply.

   Only those `instance` lines change in the user's text; everything else is kept byte for byte.

6. Publish through steps 7–10 (a new file in a staging folder, never the editor's own file) and remove the session folder.

### Reader changes in the same step

- **B2**: every TOML error goes through `ConfigFile::parse`'s redacted `ConfigError`, including the desktop and the API reload messages.
- **B9, B10**: `entries_with_warnings` skips a table it cannot read (a `wsl` key outside `[[git]]`, a second table of a single-instance kind) and a kind whose value is no list of tables, each with a warning. Each `Entry` keeps its **original index** in the TOML array, and `pin_names`, `index_of` and `table_mut` address tables by it. `instance` joins the reserved fields that raise no "unknown setting" warning; a malformed or duplicate instance is read as absent with a warning (above).
- `readable_by_others` is checked when the settings file is first read in a process, for the warning above.

### Tests (`core/src/settings/write/tests.rs`, `core/src/platform/private_file/tests.rs`, `core/src/settings/tests.rs`, `cli/src/tests.rs`, `desktop/src/provider_config/tests.rs`)

- Two threads editing through `edit_config`, one adding and one renaming: both changes land.
- A **second process** (the built `activity-log` binary running `sources set`) while the test holds the lock: busy after the wait (shortened through an environment seam); with the lock free, two processes setting different fields both land.
- Instances:
  - an edit of one legacy connection writes an `instance` into that table only; every other table is byte-identical, including other legacy tables;
  - a malformed and a duplicate instance are read as absent with one warning each, and the edited one gets a fresh id.
- Preconditions:
  - **first conditional save of a legacy table**, no competing writer: proceeds and assigns that table's first instance;
  - **cooperating same-value replacement before the first save** (legacy base, then another writer removes and re-adds the connection with identical values, which gives it an instance): `Replaced`; after reloading, the save proceeds;
  - a legacy base whose touched field another writer changed: `Changed` naming the field;
  - a marked base, then a same-value re-add: `Replaced`;
  - a token changed to another token after the base: `Changed` naming the token field; the check reports it, and an unchanged set token is not reported;
  - two concurrent label edits from two bases: the second is `Changed` (label); two edits of `agent_allow` the same (slice 2);
  - a stale remove from an old snapshot after a re-add: `Replaced`, nothing removed;
  - an evicted revision: `UnknownBase`;
  - an untouched non-account field changed after the base: the save merges;
  - a Git connection (no account fields) removed and re-added: `Replaced`.
- Private files (**effective access, not only mode bits**):
  - macOS (runs in `pnpm verify` on the developer's Mac): a parent folder given an inheritable allow entry for `everyone` (`chmod +a "everyone allow read,file_inherit,directory_inherit"`) → the staging folder, the published config, a cache file and an editor-created file in the session folder all have **no ACL entries** (`acl_get_file`) and modes `0700`/`0600`; a parent on a volume where the ACL cannot be cleared gives `Unprotectable` with nothing written (simulated by a failing ACL seam);
  - Linux (CI): a parent with a default ACL granting `other` read (`setfacl -d -m o::r`, skipped with a message where `setfacl` or ACL support is missing) → staging folder and published file have neither ACL attribute and modes `0700`/`0600`;
  - both: an unchanged edit leaves an existing `0644` file's bytes, mode and modification time untouched and logs the warning once; the next real write makes it `0600` with no ACL entries;
  - a failed publish leaves the original intact and removes the staging folder.
- **Step 1 device check, Windows** (`infra/windows-vm`; CI has no Windows before step 13): with a second, non-administrator local account `alw-other` created by the check script, and a parent folder whose ACL grants Everyone read with inheritance:
  - `alw-other` cannot open, for reading, the staging folder, a file in it while it is written, the published config, a cache file or an editor backup created in the session folder (`CreateFileW` as `alw-other` through `runas` or `LogonUserW`, each `ERROR_ACCESS_DENIED`); the owner can open each;
  - `GetEffectiveRightsFromAclW` for `alw-other` and for Everyone returns no read right on each of them, and the DACL is protected with a single owner ACE;
  - a config on a FAT32 or exFAT volume (a small VHD) gives `Unprotectable` before any byte is written;
  - an unchanged edit leaves an existing loose-ACL file untouched; the next real write replaces it with the owner-only one;
  - a parent folder that grants `alw-other` `FILE_ADD_FILE` or `FILE_DELETE_CHILD` gives `Unprotectable` before any byte is written; in an allowed folder, `alw-other` cannot rename the staging folder or the target while a save runs, and the published file is the one written through the handle.
- **Step 1 device check, Linux** (a VM or the CI runner where it can mount): an `sshfs` (FUSE) mount and a `vboxsf` or 9p share report `Unprotectable` for settings and no cache file, with nothing written there; ext4 and tmpfs pass. Automated in `pnpm verify`/CI: the `f_type` allow-list through a fake `fstatfs` seam, and a world-writable parent (`/tmp`-like, mode `1777`) refused on macOS and Linux.
- Handle-bound publication (macOS and Linux, automated): a test that renames the staging folder's path to another folder between `create` and `publish` (a seam between the steps) still publishes the bytes written through the handle, and a symlink planted at the staging folder's old path is never followed.
- Paths: a relative `--config config.toml` works; a symlinked config keeps the link; a dangling link fails without creating a file.
- No location: reading gives no sources with the warning; `sources set`, `config edit` and a desktop save fail with the message and create no file anywhere (the temp folder is checked); `config path` prints the message and exits 1.
- A lock error other than contention surfaces as `Io` with its message (a fake lock function).
- `config edit`:
  - open and close without a change, on an existing file and on a missing one: the file's bytes, mode, ACL and modification time are unchanged, and a missing file is still missing;
  - a table added in the editor gets a fresh instance; a table pasted with another table's instance gets a fresh one; a **modified legacy table** gets a fresh one while an unchanged legacy table stays without one; a marked table whose `instance` line was **deleted by hand** gets a fresh one, and a desktop save based on the old id is then `Replaced`; nothing else in the text changes;
  - a change by another writer during the edit keeps the session folder and publishes nothing; an invalid copy is kept; an editor that saves by rename does not change the published file's protection.
- B2: a parse error with `token = "glpat-SECRET` on the bad line has the line and column, not `SECRET`, in its message or any `source()`.
- B9/B10: a broken table between two good ones; renaming the second good one changes that table by its original index.

---

## 2. Runtime host and run generation (R-C2, B66, NF-5; step 5)

### Purpose

One `core::runtime::Host` serves the API and the desktop app. It owns the current `CollectionRuntime`, the run shown to clients, config reloads and edits, the identity and access keys (§4) and publishing (§5). Every transition is decided, installed and published under one short state lock, so a stale run, a superseded recheck or a replaced runtime can no longer publish or restart anything (B14, B15). `api/src/reload/` (`ApiRuntime`) and the desktop's `AppState` (`RwLock<CollectionRuntime>`) become thin adapters.

### Structure

- `CollectionRuntime` keeps what belongs to one config: its connections and their checks. Slice 3's `run_action` and `ActionLimits` hang off it; an in-flight read keeps the clone it started on.
- `Host` keeps what outlives a config: the shown run with its result store (§3), the failure map, the identity and key tables (§4), the revision counter and the publisher (§5), and the config's content ids.

`take_over` disappears: installing a runtime starts a run that carries from the host's own store. `ActivitySnapshot` is replaced by the wire type `ActivityState` (§5); the existing commands map it to their old shapes exactly as `activity_response` does today (202 running, 200 ready, 200 `CollectionFailedResponse` with range and message, 503 "No data yet"; on desktop the same `Ok`/`Err` values).

### Rust

`core/src/runtime/host/mod.rs` + `tests.rs`; run types in `core/src/model/runtime/` (Rust only, not exported to TypeScript):

```rust
// run_id.rs
/// One collection run. Unique for the whole life of a host, across runtime replacements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(pub(crate) u64);

// run_outcome.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// `entries` is the length of the run's final log `entries`.
    Ready { entries: usize },
    /// The run failed as a whole; the message stays in the published state and the Logs pane.
    Failed,
    /// Stopped by a newer run, a runtime replacement or shutdown.
    Stopped,
}

// run_end.rs
#[derive(Debug, Clone, Copy)]
pub struct RunEnd {
    pub run: RunId,
    /// From the moment the run's future was first polled.
    pub elapsed: std::time::Duration,
    pub outcome: RunOutcome,
}
```

```rust
// core/src/runtime/host/mod.rs
/// Work the host hands to its adapter: a collection run resolves to its `RunEnd`; startup
/// checks and a recheck without a collection resolve to `None`. A run is handed over exactly
/// once; a join of a running collection hands over nothing.
pub type HostWork = BoxFuture<'static, Option<RunEnd>>;

/// Spawns host work on the adapter's runtime. Must not block, and must not hold the host or
/// anything that owns it.
pub type RunSpawner = Arc<dyn Fn(HostWork) + Send + Sync>;

/// What `start` does with a config file it cannot read or parse.
pub enum BadConfig {
    /// Fail: the API stops with the redacted error, as today (decided, user, 2026-11-19).
    Fail,
    /// Log it and start with no sources, still watching the file: the desktop, as today.
    StartEmpty,
}

#[derive(Clone)]
pub struct Host { inner: Arc<HostInner> }

impl Host {
    /// `path` is `None` when there is no settings location (§1): no sources, nothing watched,
    /// and `edit` fails with `EditError::NoLocation`.
    pub fn start(path: Option<PathBuf>, spawner: RunSpawner, bad_config: BadConfig) -> anyhow::Result<Host>;
    /// Tests: fixed connections, never reloads.
    pub fn fixed(connections: &[Connection], spawner: RunSpawner) -> Host;
    /// Tests: a fixed collector (§3) and provider list (today's `CollectionRuntime::new`).
    pub fn with_collector(collector: Collector, providers: Vec<ProviderInfo>, spawner: RunSpawner) -> Host;

    pub fn start_initial_collection(&self);
    /// Starts collecting `options` or joins a running collection of the same range; answers
    /// with the state installed by this call.
    pub fn collect(&self, options: CollectOptions) -> ActivityState;
    /// Runs the check of `id` again; answers with the providers, `id` checking. `None` for an
    /// unknown connection.
    pub fn recheck(&self, id: &ConnectionId) -> Option<ProvidersResponse>;
    /// Edits the config (§1) and installs the result. The write and the install run in a task
    /// the host owns, so dropping this future never leaves a written file uninstalled.
    pub async fn edit<T: Send + 'static>(
        &self,
        precondition: Precondition,
        change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<Edit<T>, EditError>;
    pub async fn reload_if_changed(&self);
    /// Calls `reload_if_changed` every 2 s until shutdown; holds the host weakly.
    pub fn watch_config(&self) -> impl Future<Output = ()> + Send + 'static;
    /// Stops the watcher and forwarders, aborts the shown run and drops the spawner.
    pub fn shutdown(&self);

    pub fn activity(&self) -> ActivityState;
    pub fn providers_response(&self) -> ProvidersResponse;
    pub fn is_current(&self, run: RunId) -> bool;
    /// What a Work read runs against: waits for the connection's current check to be recorded
    /// (following a newer check if this one is superseded), then takes the checked provider,
    /// the connection's access policy and its access context together under the state lock
    /// (§9). A known connection always gets a target, usable or not, so slice 3's executor
    /// applies its own refusal order to every caller alike; `None` only for an unknown one.
    pub async fn action_target(&self, id: &ConnectionId) -> Option<ActionTarget>;
    pub fn subscribe(&self) -> watch::Receiver<Arc<Published>>; // §5
}

/// Immutable for the whole read: the answer is tagged with `context`, which names exactly the
/// check and policy the read used.
pub struct ActionTarget {
    pub provider: ProviderRuntime, // as recorded by the check: identity, usability and reason included
    pub access: Access,            // slice 2: allow, agent_allow at that moment
    pub limits: Arc<ActionLimits>, // the runtime's, slice 3
    /// Epoch + this connection's access token, read under the same lock; `None` for the CLI,
    /// which builds its target from the one runtime it checked itself and has no host.
    pub context: Option<AccessContext>,
}

pub struct Edit<T> {
    pub value: T,
    pub connections: Vec<Connection>,
    pub providers: ProvidersResponse,
    /// §5: the published state that holds this edit.
    pub receipt: InstallReceipt,
}
```

### Locks and lifetimes

- **State lock** (`std::sync::Mutex<HostState>`): every read or change of the shown run, store, failures, keys, revision and publisher. Never held across an `.await`, file or network I/O, a spawner or adapter call, or JSON serialization. A poisoned lock is recovered (`into_inner`), as today.
- **Config lock** (`Arc<tokio::sync::Mutex<()>>`, taken with `lock_owned`): held by the edit task and by `reload_if_changed` from reading or writing the file until the new runtime is installed, so installs happen in the order the files were written. The state lock may be taken briefly inside it, never the other way round. It is released **before** the spawner is called.
- **Edit ownership** (finding 12): `edit` spawns one task that takes the config lock, runs `edit_config` on `spawn_blocking`, installs and builds the receipt, and sends the result back through a oneshot. A requester that goes away drops only the receiver; the file is never left written but not installed.
- **Old values**: the `Arc<Published>` returned by `send_replace` and a replaced store are dropped after the state lock is released, so freeing a large log never happens under it.
- **Shutdown**: `watch_config`, the SSE and Tauri forwarders and the cache writers hold `Weak` references or a shutdown signal and end on `shutdown()`. The adapters call `shutdown()` on Rocket's shutdown and on Tauri's `RunEvent::Exit`.

### Transition rule

```rust
let (answer, work, old) = {
    let mut state = self.lock();             // state lock
    let change = decide(&state, input);      // pure
    let work = install(&mut state, change);  // futures built, not polled
    let old = publish(&mut state);           // stamp revision, send_replace (§5)
    (answer_of(&state), work, old)           // B66: answer = what was just installed
};                                           // lock released
drop(old);
for work in work { (self.spawner)(work) }    // only now can a run make progress
answer
```

- **Answer before run (B66)**: the answers of `collect` (state `running`), `recheck` (the connection `checking`) and `edit` (providers as installed, new connections `checking`, and the receipt) are captured in the critical section that installs the transition; work reaches the spawner only after the lock is released.
- **Generation guard**: an update from a run (progress, end) applies only while its `RunId` is the shown run; an update from a check only while it is the connection's current check in the current runtime. Stale updates are dropped.
- **Terminal state first**: a run's last act in its future is: lock, install `Done` or `Failed`, publish, unlock. Only then does it resolve to `Some(RunEnd)`. A stopped run installs nothing and resolves to `Stopped`.
- **Run ids**: a host-wide counter under the state lock (today's `next_run` restarts per runtime, `core/src/runtime/mod.rs:182`).
- **Joins**: `collect` with the range of a running collection returns its state and hands over nothing (`core/src/runtime/mod.rs:360–369`).
- **Checks are recorded before use** (finding 1, deviation 22, R2): when a check ends, its wrapper takes the state lock, records the `ProviderRuntime`, updates the identity state and keys (§4), evicts entries that no longer match (§3), publishes, and only then wakes its waiters. Collections and Work reads never await the raw shared check:
  - a collection plan awaits `checked(id, check_generation)`, which yields the recorded `ProviderRuntime` with its `CollectionKey`;
  - a Work read awaits `action_target(id)`, which yields provider, policy and `AccessContext` read together **after** recording (§9), so the context always names the principal the read runs as;
  - a superseded check (a recheck or an install replaced it): a collection waiter belongs to a run that the superseding transition stopped or replaced in the same critical section, so it is dropped with that run (§3 Supersession); an `action_target` waiter follows the connection's newer check, or resolves to `None` if the connection is gone. No waiter is left pending.

  The providers part, `GET /api/providers` and `collectable()` read recorded results, not `Shared::peek`.

### Installing a config (edit and reload)

1. Under the config lock: `edit` writes (§1); its result is installed whenever its bytes differ from the **host's installed** content, even when `edit_config` itself wrote nothing (another writer may have changed the file since the last reload), and a save that changes nothing anywhere installs nothing and answers with the current receipt. A reload reads the bytes:
   - an I/O error (permission, a vanished folder) is logged once per distinct message and **not** recorded, so the next tick tries again even if nothing changes;
   - bytes whose `ContentId` equals the installed one, or the last **rejected** one, stop here;
   - otherwise, for a change this host did not write, read again after 200 ms and continue only if the bytes are the same. This is a debounce, not a guarantee (deviation 28): cooperating writers replace the file atomically, so their documents are always whole; a writer outside the protocol that pauses longer than 200 ms with a valid intermediate document (for example an empty one) can have that document installed, which may remove connections and close an open Work view until its final document arrives on a later tick;
   - bytes that do not parse are logged with the redacted error, their id recorded as rejected, and the last good runtime stays.
2. Build the new `CollectionRuntime` (no I/O).
3. Under the state lock: install it; update the key tables (§4); choose the affected kinds; start the run; publish; capture the answer and receipt.
4. Release both locks, then hand the checks and the run to the spawner.

**Affected kinds** (findings 5 and R5): a kind is affected when one of its connections is added or **removed**, has changed **collection inputs** (values, location; not the label, not `allow`/`agent_allow`), when the **order** of its connections changed (`finish_kind` gives a commit two clones hold to the first one, `core/src/provider/git/mod.rs:82`, and the producer keeps connection order, `core/src/lib.rs:235`), or when one of its connections has no `Collected` or `Failed` outcome in the store (it was skipped or never collected, and its new check may make it usable). Affected kinds are collected whole; connections of other kinds keep their store slots (`Kept`, §3). The scope is a set of kinds: `Scope::Kinds(BTreeSet<Source>)` replaces `Scope::Kind`.

One outcome per case of the shown run:

| Shown run      | Affected kinds | Result                                                                                                                                                                                             |
| -------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| running        | any, even none | the run is stopped and a new one starts whole on the new runtime, carrying from the store; the old run's waiters on the old runtime's checks end with it (`Superseded`)                            |
| failed         | any, even none | a new run starts whole, as today                                                                                                                                                                   |
| done           | some           | a new run collects the affected kinds; the others are kept                                                                                                                                         |
| done           | none           | no run starts; the store and the shown activity stay, only the providers part changes; the new runtime's fresh checks are recorded as they end, and a key change still evicts and re-collects (§3) |
| none (startup) | —              | the initial collection, as today                                                                                                                                                                   |

**Failures**: the failure map (degraded connections) is per connection. An install removes the entries of removed connections and of connections in affected kinds; unaffected connections keep theirs, so a label edit never makes a failed connection look healthy. A recheck clears its connection; a run's end updates the connections it collected (today's rule).

**Reload trigger (B21)**: `watch_config` every 2 s in both adapters; the API also calls `reload_if_changed` before each request, as `ApiRuntime::current` does today. The file is a few kilobytes and is read and hashed each time.

### Adapters

- **API**: manages a `Host` built with `BadConfig::Fail` (startup on an invalid file stops with the error's line and column and no file content, as today; decided, user, 2026-11-19); spawner `Arc::new(|work| { tokio::spawn(work); })`; `watch_config` and the initial collection start on liftoff; `shutdown()` on Rocket's shutdown. `api/src/reload/` is deleted; its tests move to the host or stay as route tests.
- **Desktop**: `AppState { host }` with `BadConfig::StartEmpty`. The spawner (NF-5's `spawn_collection` in step 12; a plain `tauri::async_runtime::spawn` until then) spawns the work and, on `Some(end)`, sends it into an unbounded `tokio::sync::mpsc` channel; it captures only the sender. A notice task started in `setup` owns the receiver and an `AppHandle`, looks up the host at dispatch, and evaluates NF-5's gate with focus and `is_current` checked **after** any asynchronous permission request. On `RunEvent::Exit` the desktop calls `host.shutdown()`, which drops the spawner and with it the sender, so the notice task ends.

### Errors and edge cases

- A panicking run is `Failed` with "Collection stopped unexpectedly: …" (as today) and resolves to `RunOutcome::Failed`.
- `recheck` of an unknown connection: `None` (404, "No source …" on desktop).
- An edit whose `change` fails or whose precondition fails: nothing written or installed; the `EditError` is returned.
- A Work read that already has its `ActionTarget` when an edit installs keeps that target and finishes; the edit does not wait for it, and its answer carries the context of the target it used, which the dashboard then drops if the boundary changed that connection's access (§9).

### Compatibility

No wire change: the existing routes and commands answer as today (fixtures for statuses, bodies and the desktop `Ok`/`Err` wrappers). Behaviour changes: the desktop picks up CLI edits within about 2 s (B21), and a save re-collects only affected kinds (B20).

### Tests (`core/src/runtime/host/tests.rs` unless noted)

- B14: a recheck while another range runs never restarts the old range. B15: a failed run of a superseded generation marks nothing degraded.
- B66, one per command, with a fake run or check that completes on its first poll: `collect` answers `running`, `recheck` answers `checking`, `edit` answers every new connection `checking`; those states' revisions are below the run's own.
- `RunId` unique across an edit and a reload. `RunEnd` exactly once for collect, a recheck that re-collects, an edit's re-collect, a reload's run and the startup collection; none for a join, a startup check or a recheck without a shown collection; `Stopped` for a stopped run.
- The spawner is called with neither lock held (a fake spawner that takes both locks does not deadlock), and a `RunEnd` arrives after `is_current` and `activity()` show the terminal state.
- Edit ownership: dropping the `edit` future while its blocking write is held by a barrier still installs the written file.
- Affected kinds: a label rename starts no run and keeps a degraded provider degraded; a GitLab URL change re-collects GitLab only; one external edit changing Jira and GitLab re-collects both; removing the Git clone that won a commit re-collects Git, and the commit appears under the remaining clone; **reversing two Git tables** whose clones share a commit re-collects Git, and the commit's connection matches a fresh collection; an access-list-only edit starts no run and keeps the activity; a label-only install **while a run waits for a check** stops that run, ends its waiters and runs whole on the new runtime; a label-only install after a failed run runs whole.
- Edit no-op: a save that writes nothing while the file differs from the host's installed content (changed by another writer since the last tick) installs it and answers with that install's receipt; a save that changes nothing anywhere answers at once with the current receipt.
- `action_target`: a read that starts while a recheck is pending gets the context of the principal the recheck records; a superseded check moves its waiter to the newer check; a removed connection ends it with `None`; an unusable known connection still gets a target; removing or replacing a connection while the collector awaits its check drops the run's check futures with the run, and no report of it is installed.
- Reload: rewriting the file with no request installs it within one tick (driven directly); a permission error is retried on the next tick without a rewrite and installs once fixed; repeated invalid UTF-8 and invalid TOML are logged once each; a truncate-then-write sequence 50 ms apart installs only the complete file (the debounce; no test claims more for outside writers).
- Startup: an invalid file fails `BadConfig::Fail` and starts empty with `StartEmpty` (api and desktop tests).
- Shutdown: after `shutdown()` and dropping the last `Host` clone, the spawner, watcher and a forwarder are gone (`Weak` probes); the desktop notice task ends on exit (mock runtime).
- api: existing route tests and fixtures pass unchanged on the host.

---

## 3. Run report and result store (R-C3, step 5)

### Purpose

Today the collector gives the runtime a folded log (`Collector`, `Progress`, `core/src/runtime/mod.rs:37–44`), and carry-over and a scoped recheck reverse the fold, which loses GitLab pushes of one SHA (B19) and cannot tell whose entries they are (B4). The collection now reports each connection's own slot, the host keeps those slots as the store, and the folded log is a projection of it (finding 4).

### Producer contract

`core/src/runtime/report/mod.rs` + `tests.rs` (crate-public: `lib.rs`'s `collect_planned` produces it, the host and the fake collectors in tests consume it):

```rust
/// Entries a connection produced under one collection key (§4).
#[derive(Clone)]
pub struct Stored {
    pub key: CollectionKey,
    /// After the kind's `finish_kind` and `make_ids_unique`, before the Git ⇄ GitLab fold.
    pub entries: Arc<[ActivityEntry]>,
}

/// How a connection's last collection ended.
#[derive(Clone)]
pub enum Outcome {
    Collected(Stored),
    /// Listed with no entries and marked degraded.
    Failed { key: CollectionKey, message: String },
}

/// One connection's place in a run.
#[derive(Clone)]
pub enum Slot {
    /// Its check or collection is still running. `carried` is what the log shown when the run
    /// started had for it, with the key it was collected under.
    Pending { carried: Option<Stored> },
    /// Its check found it unusable: not listed (today's `Skipped`).
    Skipped,
    /// Settled in this run. While siblings of its kind are pending, `Collected` holds the
    /// kind's provisional `finish_kind` output, which a later report may replace.
    Settled(Outcome),
    /// Not collected by this run: the outcome from the store the run started from.
    Kept(Outcome),
}

/// Every connection in the run's plan, in connection order. Each report is complete
/// (level-triggered), never a delta.
#[derive(Clone)]
pub struct RunReport {
    pub slots: Vec<(ConnectionId, Slot)>,
}

/// What a run does with one connection.
pub enum Plan {
    /// Collect it once its recorded check (§2) yields a usable runtime.
    Collect { check: BoxFuture<'static, Result<Checked, Superseded>>, carried: Option<Stored> },
    Keep(Outcome),
}

/// A recorded check: the runtime with the collection key it was recorded under.
pub struct Checked { pub runtime: ProviderRuntime, pub key: CollectionKey }
/// The check this plan waited for was replaced or its connection removed.
pub struct Superseded;

pub type Progress = Arc<dyn Fn(RunReport) + Send + Sync>;
/// Collects a run; the final report is the result. Production: `collect_planned`; tests: fakes.
pub type Collector = Arc<
    dyn Fn(CollectOptions, Vec<(ConnectionId, Plan)>, Progress) -> BoxFuture<'static, anyhow::Result<RunReport>>
        + Send + Sync,
>;
```

**Supersession** (D7): every transition that supersedes a check a run waits for (a recheck, an install, a removal) also stops or replaces that run in the same critical section (§2's outcome table), so its collector future, with all its pending check futures, is aborted and dropped. `Superseded` exists so the type can say what happens if a check future resolves anyway: `collect_planned` treats it as `Skipped` for that connection, and the generation guard drops any later report of the stopped run. Only `action_target` follows a newer check.

`collect_planned` (`core/src/lib.rs`) keeps its algorithm and slot states and builds a `RunReport` instead of a log at each settle and at the end; the entries it collects for a connection are stamped with the key its plan's check yielded. `collect_progressively` (the CLI) projects the final report itself.

### The store

`core/src/runtime/result_store/mod.rs` + `tests.rs`, crate-private:

```rust
pub(crate) struct ResultStore {
    range: CollectOptions,
    run: RunId,
    report: RunReport,
}

impl ResultStore {
    /// The log clients see: finished slots (`Settled`, `Kept`) in connection order, `Failed`
    /// ones with no entries; `Pending` ids in `meta.pending` with their carried entries (shown,
    /// not counted, `carried_for`'s commit dedupe); `Skipped` omitted.
    pub(crate) fn project(&self) -> ActivityLog;
    /// The plan of a new run of `options` over `affected` kinds: affected or pending connections
    /// get `Collect` with what this store shows for them as `carried`; the others `Keep` their
    /// outcome. Carried and kept data whose key differs from `keys` is left out.
    pub(crate) fn plan(&self, options: CollectOptions, affected: &BTreeSet<Source>, keys: &KeyTable) -> Vec<(ConnectionId, PlanSeed)>;
    /// Drops every carried, settled or kept outcome whose key differs from `keys`; true if
    /// anything was dropped.
    pub(crate) fn evict(&mut self, keys: &KeyTable) -> bool;
}
```

- **Who is in a plan** (R-finding 4): exactly the connections of the new runtime, in its order, minus unaffected connections the store has as `Skipped` or does not have at all, which are left out as today (`core/src/runtime/mod.rs:486`); with §2's affected-kinds rule such a connection only stays out while its kind is unaffected, and any kind holding one is affected when the store has no outcome for it. A **new** connection is in an affected kind (its addition makes it so) and gets `Collect` with no carry. A **removed** connection is not in the new runtime, so it is in no plan, and its slot leaves the store when the plan is built. `Skipped` is never kept: an affected connection is checked again; an unaffected skipped one is not listed, as today.
- **What a new run inherits**: for each connection, what the store **shows**: a settled or kept outcome, or a pending slot's own `carried` entries with their key. So when run B still shows A's carried entries for a pending connection and run C replaces B, C carries A's entries with A's key, and they are dropped if the key no longer matches. Carry across another range filters by range and overlap, as `carry_over` does today.
- **Failed vs skipped vs empty**: `Failed` is listed in the summary with zero entries and degraded; `Skipped` is not listed; `Collected` with no entries is listed with zero and healthy. Kept outcomes keep their kind.
- **Eviction (finding 1)**: whenever a connection's collection key changes (an edit, or a check that records a different identity state, §4), the same critical section calls `evict` and reprojects before publishing, so the boundary pair, a plain `GET /api/activity` and every later progress report already lack the old entries. A Git commit's nested GitLab push disappears with them, because the fold is redone from the store. If the shown run will not collect that connection (a label-only install whose fresh check finds another principal), the host starts a run for its kind, by the same path as a recheck's collection.
- **Stale progress**: the host checks every report's keys against the key table before installing it and evicts mismatches. Reports are projected outside the state lock against a key-table version; if the version changed before the install, the host evicts and reprojects under the lock (rare: only after a key change).

### Compatibility

No wire change: projecting a report equals today's log for the same inputs (fixtures compared with `meta.generated` fixed through the existing seam, `core/src/tests.rs:264`, and against step 4's B19 fold).

### Tests

- Projection equals today's `collect_activity_from_results` output for the existing aggregator fixtures.
- B19: two GitLab pushes of one SHA survive a recheck of GitLab and of Git, before and after.
- A→B→C: C started while B still shows A's carried entries for a pending connection carries them with A's key; an access-only edit between B and C keeps them; a token edit drops them.
- Failed, skipped and empty-collected connections project and carry differently, and kept outcomes keep their kind.
- Finding 1 / R3: a recheck of a ready **GitLab** connection (so its push is nested in a Git commit) whose check, held by a barrier before B's fetch, records principal B with unchanged settings. Before the recheck, the log and providers parts have been **serialized** (a stream subscriber received them). Assertions: the pushed providers part of the boundary `state` carries B's access token for that connection, and no message after the boundary carries A's; the boundary's activity part, `GET /api/activity` and the projection of a delayed progress report of the same run lack A's entries, including the nested push; `GET /api/providers` (which has no access vector and keeps its shape) equals the `providers` field of the published providers part.
- Plans: a connection added while the shown run is done joins an affected kind with no carry; a removed one leaves the store; an unaffected skipped one is not listed; a skipped one whose kind is affected is checked and collected.
- Provisional `finish_kind` output replaced by a later report of the same kind.

---

## 4. Identity, collection key, access token and the cache (R-C5, B4, R-B4)

### Purpose

Know who a remote token belongs to, keep caches of different principals apart on disk, keep a connection's activity apart from the next principal's, and tell clients when what a connection may read, or as whom, changed. Four keys, each for one job:

| Key                       | Built from                                                              | Lives              | Used for                                      |
| ------------------------- | ----------------------------------------------------------------------- | ------------------ | --------------------------------------------- |
| `RemoteIdentity`          | the check's answer (Jira `/myself`, GitLab `/user`)                     | `ProviderRuntime`  | slice 5's "me", the identity state below      |
| cache scope               | safe config fields, canonical endpoint, verified identity id            | cache file names   | separating day caches of different principals |
| collection key (internal) | values (secrets included, compared in memory), location, identity state | host memory, store | carry, keep and eviction of activity (§3)     |
| access token (wire)       | collection key, `allow`, `agent_allow`                                  | host memory, push  | Work invalidation (§5), action context (§9)   |

### `RemoteIdentity` and identity state (step 2 sets the identity; step 5 the state)

`core/src/model/common/remote_identity.rs`, Rust only:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteIdentity {
    /// Stable account id: GitLab's numeric `id` as text, Jira Data Center's `key`. Never empty.
    pub id: String,
    /// GitLab `username`, Jira `name`.
    pub username: String,
}
```

`ProviderCheckResult` gains `identity: IdentityEvidence`, produced in **step 2** (generic slice 1) where the check parses its identity request; nothing later produces it:

```rust
pub enum IdentityEvidence {
    /// The identity request succeeded and named the principal (non-empty id).
    Verified(RemoteIdentity),
    /// The identity request succeeded (2xx) but named no one: the token works, whose it is is unknown.
    Unnamed,
    /// The identity request was refused with 401 or 403.
    Denied,
    /// No usable answer: timeout, unreachable, 5xx, or a kind without an identity request.
    None,
}
```

For GitLab the identity is the token's owner, apart from the configured `user` whose events are collected.

The host keeps per connection an **identity state** (finding 19):

```rust
enum IdentityState {
    /// No check of these values has said anything yet.
    Unknown,
    Verified(RemoteIdentity),
    /// The remote accepted the token without naming its owner. The number is new for every
    /// such check (a host counter), so two unnamed checks are never the same state.
    Unnamed(u64),
    /// The last check was refused (401/403): the credentials no longer work.
    Denied,
}
```

- `Verified`, `Unnamed` and `Denied` evidence set the state to that value; `Unnamed` evidence always sets a **new** `Unnamed(n)` (D4), so every usable check without an id rotates both keys: nothing collected or read after one unnamed check is ever accepted as belonging to the next, because there is no evidence that it is the same account. A check that **read** something (any of these three) always replaces the historical state, so new data is never labelled with an earlier principal (R2): a `Verified(A)` connection whose next check answers 2xx without an id becomes `Unnamed`, gets a new collection key, and collects uncached (no verified id, §4 cache scope).
- `None` evidence (no usable answer) keeps the state: no read happened, the values are unchanged, so the token and its owner are the same; whether the connection is collected follows its usability as today. Previously shown data stays; nothing new is labelled by this check.
- Changing the values resets the state to `Unknown`.
- Advisory probes (slice 3) never change it.

### Collection key and access token (step 5)

```rust
// core/src/runtime/host/keys.rs (host-private)
struct CollectionInputs { values: Arc<ConnectionValues>, location: ConnectionLocation, identity: IdentityState }
struct AccessInputs { collection: CollectionKey, allow: Access /* slice 2: allow, agent_allow */ }
/// Opaque, decimal text of a host-wide counter; meaningful only with the host's epoch.
pub(crate) struct CollectionKey(Arc<str>);
pub(crate) struct AccessToken(Arc<str>);
```

- Inputs are compared by value in memory (`ConnectionValues: PartialEq`); a change takes the next counter value. Counters never derive from a secret.
- A check that changes the identity state (`Unknown → Verified`, `Verified(a) → Verified(b)`, `Verified → Unnamed`, `Unnamed(n) → Unnamed(m)`, anything → `Denied`) changes the collection key and therefore the access token. Ordinary progress changes neither.
- **User-visible cost of the safe rule**: a connection whose server never names the token's owner loses its shown activity (it is re-collected, uncached) and its Work results and open detail on every check: at startup, after each settings save and on Check again. GitLab "me" filters report it unavailable. Jira Data Center's `/myself` and GitLab's `/user` always name the owner, so this is expected only from unusual proxies or servers.
- An `allow`/`agent_allow` edit changes the access token only: Work state is invalidated, collected activity stays (finding 5).
- Both are scoped to the host's `epoch` (§5): `"3"` in one host and `"3"` in the next mean nothing to each other (finding 2).

### Cache scope (step 3)

- `ProviderRuntime::cache_scope() -> Option<u64>`: `Some(fnv(safe_fingerprint ‖ "\n" ‖ identity.id))` only when the check returned a verified identity; `None` otherwise.
- **Safe fingerprint** (finding 9): the existing fingerprint (`resolved_config.rs:57`) leaves out `Password` values but hashes URL values as written. For `Url` values it now uses the canonical endpoint: scheme, lowercase host, port, and the path without trailing slashes (the context path, such as `/jira`, kept). A URL value with a query or a fragment makes the scope `None`, with one warning that names the setting, never its value; this applies to hand-written TOML too, since the scope is computed at collection time. A URL value with a **user name or password** never reaches collection: the connection's check reports it unavailable (`notConfigured`, "Remove the user name and password from the address; the token goes in the token field.") until the URL is changed (user decision, 2026-11-19), and the settings show the field error of §6.1.
- **Bypass**: with no scope, the collection fetches the range directly and neither reads nor writes a cache file, with one info line ("Not cached: the check did not say whose token this is" or "Not cached: the URL holds credentials"). Only usable connections are collected at all (`core/src/lib.rs:144`), so a failed check never triggers an uncached fetch.
- **Guarantee**: the disk cache is separated per principal, not per token. Two tokens of the same account with different reach share a cache, so a narrower token can show days fetched by a broader one of the same person. This is accepted: it is the same person's activity in their own cache folder, and the in-memory collection key still separates the two tokens for carry and Work.
- File names keep the pattern `{source}-{name}-{scope:016x}.json` (`cache/mod.rs:82`); files under old scopes are no longer read.

### Cache writer (R-C5, step 3; finding 10)

- `collect_with_day_cache` takes a **repeatable** fetch (`Fn(CollectOptions) -> Fut`): B8 splits the stale days into contiguous runs, fetched one after another **without** any lock.
- Each fetched day's `fetched_at` is the instant its run's fetch **started** (not when the lock was taken), so a slow fetch is not treated as newer than one that started later.
- After fetching, under the file's lock (`{file}.lock`, the same wait): re-read the file, merge **only the fetched days** into it, and publish through `private_staging` (§1 Private files); a cache folder that is `Unprotectable` gets no cache files at all (one warning, collection uncached). A fetched day replaces a stored one only if its `fetched_at` is newer; ties keep the stored day.
- A failed run of days fails the collection, as today, but the runs fetched before it are still merged and written.
- Read failures (R6):
  - the **first** read (before fetching) failing with anything but NotFound is logged and treated as a cache miss for this request: every requested day is fetched;
  - the **locked re-read** of an existing file failing means the file is not published at all (an unreadable file is never replaced by a subset, which would drop other days and other processes' writes); the request still answers;
  - a busy or failing lock or write is logged and nothing is published.
- **What the request returns** is always the full requested range: days fresh in the first read plus the fetched days, cut to the range, whether or not the write happened. Cache problems never fail a collection.
- All cache file I/O runs on `spawn_blocking`, never on the async path.

### Tests

- Step 2: the Jira and GitLab checks parse `/myself` and `/user` into `Verified`; a complete JSON 2xx without an id is `Unnamed`; 401 and 403 are `Denied`, whatever becomes of their body; a timeout, a body that breaks off, and a complete 2xx that is not the API's JSON are `None`. The last one (an SSO sign-in page) also leaves the connection unavailable: "Jira answered with a page that is not its API (a sign-in page?); check the URL and token" (user decision, 2026-11-19).
- Step 3: two identities behind the same URL use two files; no identity and a URL with `user:pass@` or `?private_token=` read and write no file and log no credential; a context path changes the scope, trailing slashes do not; a pinned file name for a fixed fingerprint and id; **two tokens of the same principal share one file** (the stated guarantee) while two principals do not; two processes writing disjoint and overlapping days both keep their days and the later-started fetch wins; a fresh middle day is not refetched or rewritten; a failed later run still writes the earlier one; the locked re-read failing after another writer added an unrelated day publishes nothing, keeps that day on disk, and still answers the whole range.
- Step 5: a token edit changes both keys; the same identity found again keeps them; `Verified(a) → Verified(b)` with byte-identical `ProviderInfo` changes both; `Verified(a) → Unnamed` changes both and the next collection is uncached; two consecutive unnamed checks give two different keys, and an action answer tagged with the first check's context is dropped once the second is installed; `Denied` changes both; a timeout keeps both; an `agent_allow` edit changes only the access token.

---

## 5. Push contract (R-P1, user decision 4; core in step 5, wire and dashboard in step 6a)

### Purpose

The host publishes its collection state and provider state; the API streams them as server-sent events, the desktop forwards them as Tauri events, and the dashboard subscribes once through `BackendTransport`. Timed polling of activity and providers is removed, with no polling fallback (merged plan §3.1). Logs stay polled while the Logs pane is open.

### Identity, generations and revisions

| Field       | Wire type             | Changes                                                                    |
| ----------- | --------------------- | -------------------------------------------------------------------------- |
| `epoch`     | opaque string         | never during a host's life; differs between hosts                          |
| `access`    | opaque string, per id | when the connection's access token changes (§4)                            |
| `authority` | opaque string         | at every config install and whenever any connection's access token changes |
| `revision`  | `number` (safe int)   | each installed transition; every part the transition changes carries it    |

- `epoch`: 16 hex digits of FNV-1a over the host's start time in nanoseconds, the process id and a process-wide host counter.
- `access` and `authority` are host counters (decimal text), compared only within one epoch.
- `revision`: one host-wide `u64`, first published value 1. Each transition takes the next value for every part it changes. At an `authority` change both parts are stamped. Core checks the **next** value against 2^53 − 1 before using it (`assert!`); generated as `number` (`#[ts(type = "number")]`).
- The signed plan's `configGeneration` is not on the wire (deviation 24): its one client use, activation after a save, is served by the install receipt below, and config installs still change `authority`.

### DTOs (`core/src/model/runtime/`, generated)

```rust
// activity_state.rs: replaces `runtime::ActivitySnapshot`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ActivityState {
    Empty,
    /// `log.meta.pending` names the connections still to come.
    Running { log: Arc<ActivityLog> },
    Ready { log: Arc<ActivityLog> },
    Failed { failure: CollectionFailedResponse },
}

// activity_update.rs
pub struct ActivityUpdate {
    pub epoch: String,
    pub authority: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub state: Arc<ActivityState>,
}

// connection_access.rs
#[ts(export)]
pub struct ConnectionAccess { pub connection: ConnectionId, pub access: String }

// providers_update.rs
pub struct ProvidersUpdate {
    pub epoch: String,
    pub authority: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub providers: Arc<ProvidersResponse>,
    /// One per connection in `providers`, same order.
    pub access: Vec<ConnectionAccess>,
}

// runtime_state.rs
/// Both parts: sent first on every subscription and at every `authority` change.
#[ts(export)]
pub struct RuntimeState { pub activity: ActivityUpdate, pub providers: ProvidersUpdate }

// install_receipt.rs
/// Where a settings change became visible: the providers part's revision in this epoch.
pub struct InstallReceipt {
    pub epoch: String,
    #[ts(type = "number")]
    pub revision: u64,
}

// access_context.rs (used by slice 3, §9)
pub struct AccessContext { pub epoch: String, pub access: String }
```

(Derives and serde/ts attributes as in Conventions.) The workspace `serde` gains the `rc` feature (no new dependency). Invariant asserted in core and relied on by the dashboard: in every `RuntimeState`, both parts have the same `epoch` and `authority`.

Generated TypeScript (shape):

```ts
export type ActivityState =
  | { kind: 'empty' }
  | { kind: 'running'; log: ActivityLog }
  | { kind: 'ready'; log: ActivityLog }
  | { kind: 'failed'; failure: CollectionFailedResponse }
export type ActivityUpdate = {
  epoch: string
  authority: string
  revision: number
  state: ActivityState
}
export type ConnectionAccess = { connection: ConnectionId; access: string }
export type ProvidersUpdate = {
  epoch: string
  authority: string
  revision: number
  providers: ProvidersResponse
  access: Array<ConnectionAccess>
}
export type RuntimeState = { activity: ActivityUpdate; providers: ProvidersUpdate }
export type InstallReceipt = { epoch: string; revision: number }
export type AccessContext = { epoch: string; access: string }
```

**Save receipt** (finding 11): `ProviderConfigSaveResponseDto` gains `installed: InstallReceipt` (additive; `AddConnectionResponseDto` flattens it in). It names the providers revision that first holds the save's install, or, for a save that changed nothing, the providers revision current when it answered. The legacy fields (`snapshot`, `providersResponse`, `created`) stay.

### Publishing in core

`core/src/runtime/publish/mod.rs` + `tests.rs`:

```rust
/// A payload with its JSON, built at most once.
pub struct Payload<T> { pub value: T, json: OnceLock<Arc<str>> }

/// One stamped part: the envelope (epoch, authority, revision) around a shared payload, with the
/// envelope's own JSON cache.
pub struct Part<T> { pub epoch: Arc<str>, pub authority: Arc<str>, pub revision: u64, pub payload: Arc<Payload<T>>, json: OnceLock<Arc<str>> }

pub struct Published {
    pub activity: Arc<Part<ActivityState>>,
    pub providers: Arc<Part<ProvidersPayload>>, // providers + access
}

pub enum RuntimeMessage { State(Arc<str>), Activity(Arc<str>), Providers(Arc<str>) }

/// What a subscriber still needs, part by part.
pub enum Due { Pair, Parts { activity: bool, providers: bool }, Nothing }

/// Per-subscriber forwarding, shared by the SSE route and the Tauri task. It keeps one cursor
/// per part plus the authority last delivered.
pub struct Forwarder { authority: Option<Arc<str>>, activity: u64, providers: u64 }
impl Forwarder {
    pub fn due(&self, value: &Published) -> Due;
    pub fn pair(value: &Published) -> RuntimeMessage;
    pub fn activity(value: &Published) -> RuntimeMessage;
    pub fn providers(value: &Published) -> RuntimeMessage;
    /// After a `State` was accepted: both cursors and the authority advance.
    pub fn delivered_pair(&mut self, value: &Published);
    /// After one part message was accepted: only that part's cursor advances.
    pub fn delivered_activity(&mut self, value: &Published);
    pub fn delivered_providers(&mut self, value: &Published);
    /// After a failed delivery: the next `due` is `Pair`.
    pub fn reset(&mut self);
}
```

- **One watch**: `watch::Sender<Arc<Published>>`, written only under the state lock with `send_replace`. Revision order is install order; every value is a coherent pair.
- **Published at**: run start, each connection settling, run done, run failed, a check ending (providers; with a key change also a new `authority` and the eviction of §3), recheck start, each config install. A transition that changes neither part publishes nothing.
- **Answer before run (B66)**: published inside the same critical section as the install, before work reaches the spawner (§2).
- **Caches and when they are invalid** (R3). There are exactly three serializations, each attached to an immutable value, so a cache can only be stale if a value were mutated, which never happens:
  - **Activity payload** (`Payload<ActivityState>`, the large one): a new `Payload` (empty cache) whenever the shown activity changes: run start, progress, end, failure, and **every eviction or reprojection** (§3). A transition that changed nothing in the activity keeps the same `Arc<Payload>`.
  - **Providers payload** (`Payload<ProvidersPayload>`: the providers list **and** the access vector): a new `Payload` whenever either changes, which includes every `authority` change (an `authority` change is by definition a changed access vector or a config install, and an install restarts checks, so its list changes).
  - **Part envelope** (`Part<_>`): a new `Part` (empty cache) at every new revision of that part, and for both parts at an `authority` change.

  So at a principal change the host first evicts and reprojects (§3), creating a new activity payload if anything was evicted, creates the new providers payload with the new access vector, and only then stamps both parts; only an activity payload that the transition left untouched is wrapped again (the restamp then builds only the small envelope `{"epoch":…,"authority":…,"revision":…,"state":<payload JSON>}`, keys in the DTO's field order). No cached JSON from before the transition can carry an evicted entry or an old access token. A `State` message is `{"activity":<a>,"providers":<p>}` from the two part strings. Every message is serializer output and fixed keys only. Plain reads (`GET /api/activity`, `GET /api/providers`, `get_runtime_state`) read the same published value.

- **Off the lock**: the `Arc<Published>` is cloned out of the watch and the borrow dropped before any JSON is built; projection and sorting of logs happen outside the state lock (§3); replaced values are dropped after unlock (§2).
- **Forwarder rule** (R8): `due` is `Pair` for the first value, after `reset`, or when the value's `authority` differs from the last delivered; otherwise it lists each part whose revision is above its own cursor. The caller sends what is due in whatever order its rate rule allows and acknowledges each accepted message on its own (`delivered_pair`, `delivered_activity`, `delivered_providers`), so sending providers while activity waits never advances the activity cursor. SSE sends everything due at once (activity first). The Tauri rule is below.

### HTTP: `GET /api/events` (step 6a)

- `200`, `text/event-stream`, Rocket's `EventStream` with the default 30 s heartbeat; it takes `rocket::Shutdown` and ends on shutdown or `Host::shutdown()`. A config reload keeps the stream (it subscribes to the host, not a runtime). The route does not reload; the timer does.
- First `retry: 1000`, then the forwarder's messages: subscribe, take the current value with `borrow_and_update`, send, then loop on `changed()`. Each message is `Event::data(String::from(&*json)).event(name)` with `name` `state`, `activity` or `providers`; compact JSON, one `data:` line. A send counts as delivered when the stream yields it.
- No `id:` field and no replay: `Last-Event-ID` is not read; every connection starts with a `state`.
- Before any collection the first `state` has `activity.state.kind == "empty"`; the route never answers 503.

```text
retry: 1000

event: state
data: {"activity":{"epoch":"9f2c4e01a7b3d5e8","authority":"1","revision":3,"state":{"kind":"running","log":{"meta":{"generated":"2026-11-19T08:00:01Z","start":"2026-10-20T00:00:00Z","end":"2026-11-19T08:00:00Z","pending":[{"source":"jira","name":"work"}]},"summary":{"connections":[],"mergedCount":0},"entries":[]}}},"providers":{"epoch":"9f2c4e01a7b3d5e8","authority":"1","revision":3,"providers":{"currentOs":"macos","providers":[…]},"access":[{"connection":{"source":"jira","name":"work"},"access":"1"}]}}

event: activity
data: {"epoch":"9f2c4e01a7b3d5e8","authority":"1","revision":4,"state":{"kind":"running","log":{…}}}

:
```

### Desktop: Tauri events and `get_runtime_state` (step 6a)

- Events `state`, `activity` and `providers`, same JSON as the SSE `data`, sent with `emit_str_to("main", name, json)` by one forwarding task over `host.subscribe()`.
- **Delivery accounting** (finding 18): a message is acknowledged only after its `emit_str_to` returned `Ok`. On an error the forwarder calls `reset`, logs once, and tries again after 1 s with the then-current value, so a lost last event is resent as a full pair.
- **Rate** (R8): Wry queues each event as a script on the UI thread, and `watch` bounds only the host's snapshots, not those queued scripts. The forwarder's loop, on each change and each timer:
  - `Pair` due: send `state` at once, `delivered_pair`, and cancel any pending activity timer (the pair covers it);
  - `providers` due: send it at once and `delivered_providers`;
  - `activity` due: send it if at least 250 ms passed since the last activity or pair send, then `delivered_activity`; otherwise arm one timer for the end of the quiet period, and when it fires, call `due` again on the **then-current** value (which may now need a pair, or nothing).

  A terminal activity state with no later publication is therefore still sent when its timer fires. The 250 ms is provisional: step 6a measures queue growth and latency with a suspended window against a one-year log and adjusts it; no byte bound is promised.

- Command `get_runtime_state` (no request) returns the current pair as `tauri::ipc::Response::new(json)`, the same string a `State` message carries; the TypeScript operation types it `RuntimeState`.

### Dashboard: `BackendTransport.subscribeRuntime` (step 6a)

```ts
export type RuntimeConnectionStatus = 'connecting' | 'open' | 'reconnecting' | 'failed'

export interface RuntimeSink {
  state(pair: RuntimeState): void
  activity(update: ActivityUpdate): void
  providers(update: ProvidersUpdate): void
  connection(status: RuntimeConnectionStatus): void
}

export interface RuntimeSubscription {
  /** Starts a new attempt: a new handshake on desktop, a new `EventSource` in the browser. Also Retry after `failed`. */
  resync(): void
  close(): void
}

export abstract class BackendTransport {
  // existing members unchanged
  abstract subscribeRuntime(sink: RuntimeSink): RuntimeSubscription
}
```

`BackendOperations` gains `runtimeState: read<RuntimeState>({ label: 'Runtime state', tauri: 'get_runtime_state' })`.

**Attempts** (finding 3): each subscription runs one **attempt** at a time, numbered. `resync()` and Retry retire the current attempt and start the next. A retired attempt's callbacks (a late invoke reply, a buffered event, a retry timer, a listener that resolves late) never reach the sink and never change the status; its timers are cancelled and its listeners, resolved or still resolving, are unlistened. The coordinator passes `{ attempt }` nowhere: fencing is the transport's job, ordering the coordinator's.

Promise of every transport: within an attempt, the first message is a `state` pair, and that pair is the attempt's **authoritative** pair: the handshake reply on Tauri; on SSE the first `state` of each HTTP connection, including after the browser's own reconnect. Only an authoritative pair may change the epoch; the status becomes `open` once it is delivered. Later messages arrive in the order received; the coordinator decides which apply. A message that does not parse as the expected type is a fault: the transport starts a new attempt (status `reconnecting`).

- **`HttpBackendTransport`**: one hand-written `EventSource('/api/events')` per attempt (Signality 0.5.0 has no composable; the comment says so). `error` in `CONNECTING`: `reconnecting`, and the browser's own reconnect is the same attempt (a pair comes first again). `CLOSED`: `failed`, until `resync()`.
- **`TauriBackendTransport`**, per attempt:
  1. Register `state`, `activity` and `providers` listeners and await all three. Until step 3, keep the latest pair and the latest update of each part.
  2. Invoke `get_runtime_state`; deliver it as `state`; status `open`.
  3. Deliver the kept pair, then the kept parts, then pass later events through, **dropping** any kept or later message whose epoch differs from the handshake pair's (a buffered message from a previous host). The handshake reply is this attempt's **authoritative** pair. Draining runs to completion before the attempt can be retired by a resync it triggered, and a new attempt has its own, empty buffer.
  - A failed `listen` or invoke unlistens every listener of the attempt (including ones that resolve later) and retries the whole attempt after 0.5, 1 and 2 s (simulated time in tests); then `failed`.
  - The listener lifecycle helper (await, cancel, unlisten) is shared with `listenForDeepLinks` (B58); deep links keep their one-shot semantics.

### Dashboard: coordinator rules (R-F2 on push, step 6a)

The coordinator owns the one subscription and keeps `installed = { epoch, authority, activityRevision, providersRevision }` (null before the first pair); `watermark` is the higher revision.

On a **pair** `p` from the current attempt:

1. `installed` is null, or `p.epoch` differs **and `p` is the attempt's authoritative pair** (R7): install `p` whole. An epoch change means another host: **all** Work state and outstanding reads are cleared, pending evidence is reset, and pending save receipts of the old epoch are retired, before the pair renders (finding 2). A non-authoritative pair of another epoch is dropped (and on Tauri never reaches the coordinator, see the drain rule).
2. Same epoch, same `authority`: apply each part whose revision is above its cursor; cursors never decrease; status `open` even if nothing applies.
3. Same epoch, other `authority`: install `p` whole only if its watermark is above `installed`'s; otherwise discard it.

Installing is one synchronous application: providers, activity, cursors, and Work invalidation over the **union** of the old and new connection ids, so a removed connection is cleared too (slice 9: clear results and details whose `(epoch, connection, access)` no longer matches, drop their outstanding reads, re-run the search only where still authorized).

On a **part update** `u`:

- Other epoch: drop.
- Same `authority`: apply if its revision is above that part's cursor.
- Other `authority`, revision above the watermark: a pair was missed. Record `u.revision` as **pending evidence** (the highest seen in this epoch; reset when the epoch changes) and request a resync if none is outstanding. When a resync's pair has been applied, if the evidence is still above the watermark (a newer boundary happened during the request), request one more; otherwise clear the evidence. So a C part arriving while a resync samples B is never lost.
- Other `authority`, at or below the watermark: drop.

**Work keys** (slice 9): every Work result, detail, cursor and outstanding read is keyed by `(epoch, connection, access)`; an action answer is applied only if its context (§9) equals the installed one for its connection.

**Save activation and replies** (R7):

- A receipt whose epoch differs from the installed epoch is **retired**: it never activates, and its save's `snapshot` is never applied; the pane reloads the snapshot instead. A pending receipt is also retired when an epoch change installs.
- Otherwise the save activates all usable sources once `providersRevision >= receipt.revision` (at once if already true).
- The save's `snapshot` is applied to the pane only if the epoch matches and no later providers revision than the receipt's is installed; otherwise the pane reloads the snapshot.
- Every reply, and every follow-up snapshot reload, is applied only to the draft it was sent for (the connection and its draft counter, R-F7) and only if the epoch it was sent under is still installed when it returns; otherwise it is dropped and, if the draft is still open, the snapshot is reloaded under the live epoch.

Everything else of §3.1 stays: activation of all usable sources on the first pair and after an epoch change; a same-epoch reconnect keeps hidden sources; the explicit URL range captured before the bootstrap pair wins; send-pending (command acknowledgement, operation token) apart from collection-running (pushed state only); "Reconnecting…" over the last state, stale after 5 s; `failed` offers Retry.

### Command answers

Unchanged shapes. The coordinator does not apply their runtime slices (`collect`'s `ActivityResponse`, `recheck`'s `ProvidersResponse`, the save envelope's `providersResponse`); `snapshot`, `created`, `installed`, field issues and errors are used as above. `POST /api/connections/recheck` still answers with the connection `checking`.

### Compatibility

Additive: a route, three events, a command, seven DTOs and one response field. The existing reads and commands are unchanged and fixture-tested. An older dashboard polls the retained reads and works. A new dashboard against an older API gets no `/api/events` (404): the HTTP transport reports `failed` with Retry and the README says the dashboard and the API are deployed together (no polling fallback, by decision). Each browser tab holds one HTTP/1.1 connection for its stream (README states the limit).

### Tests

- core: each transition publishes once; part revisions rise strictly across reloads and edits; stale runs and replaced runtimes publish nothing; no subscriber still leaves the latest value; a new subscriber's first value is a coherent pair; at an `authority` change both parts carry it and the watermark rises; two hosts differ in epoch; finding 1's sequence (§3 tests) publishes no pair with A's entries under the new authority; the `Forwarder` makes a `Pair` due at a boundary and after `reset`, and only risen parts otherwise; acknowledging providers alone leaves activity due; every message equals `serde_json::to_string` of its DTO byte for byte, restamps included; the log payload's JSON is built once across a restamp (a counting serializer); building messages for a one-year log while another thread installs transitions makes neither wait (barriers, not sleeps).
- api (incremental reads of the local streaming response, never an unbounded body read): first `retry`, then `state` with equal epoch and authority in both parts; a principal-changing edit reaches an open stream as one `state`; collect reaches `ready` with rising revisions, `running` asserted only behind a barrier; the stream survives a reload; shutdown ends it; existing routes keep their fixtures (statuses, old fields and wrappers pinned; `ProviderInfo` gains only `recheckable`, which has its own fixture); a slow consumer against a one-year log is benchmarked.
- desktop (mock runtime, which records scripts): the target is `main` and each recorded payload equals the message; an `emit` error resets and resends a full pair after 1 s (paused time); `get_runtime_state` equals the `state` payload **at the same revision**; with paused time: a providers change during the activity quiet period is emitted at once and acknowledged alone, the activity follows when the timer fires; a terminal activity with no later publication is still emitted; an `authority` change during the quiet period emits one `state` and cancels the pending activity. Delivery and queue growth in a real webview are step 6a's manual check.
- dashboard (fake `EventSource`, fake Tauri `listen`/`invoke`, fake timers): pair then open; CONNECTING → reconnecting → pair; CLOSED → failed → Retry; a reply, event, timer or listener of a retired attempt reaches nothing; a listener that resolves after a failed attempt is unlistened; an unparsable message starts a new attempt; same-authority reconnect (r12/r8 then r12/r11 applies activity r11); C@30 then delayed B@20 discarded; activity B@21 before pair B@20 resyncs once and ends at B@21; a second boundary C arriving while a resync samples B leads to exactly one more resync and ends at C; a delayed B@20 pair after B@21 applied changes nothing; epoch change clears all Work state, including a removed connection's pending detail and two hosts with identical ids and counters; a buffered pair of the previous host drained after the handshake installed the new one is dropped and pending evidence from the old epoch causes no resync; a receipt from the old epoch is retired and its snapshot not applied; a no-op save activates at once; a save followed by an external reload activates on the receipt's revision; a stale `snapshot` reply after another install reloads instead of applying; plus the remaining merged plan §3.1 cases.

---

## 6. Additive DTO fields

### 6.1 Field issues for settings (R-B2, B64; step 7)

**Purpose**: one normalization and validation of setting values in core, used by the desktop save, a pre-save check, the CLI and slice 10's access edits; the dashboard keeps hints only (finding 13).

Core (`core/src/settings/view/mod.rs`, R-B3's module):

```rust
/// A value as a writer submitted it.
pub enum Submitted<'a> {
    /// A single value, or a list in today's text form (JSON array or comma-separated).
    Text(&'a str),
    /// A list as items; an empty list means "none", which differs from clearing.
    List(&'a [String]),
    /// Remove the key so its default applies.
    Clear,
}

/// Where an issue is: a `FieldId` (§1: label, config value or access list), or one list item.
pub enum FieldRef { Field(FieldId), Item(FieldId, usize), Connection }

pub struct FieldIssue { pub at: FieldRef, pub problem: IssueKind, pub message: &'static str }

/// The stored form of one submitted value for `kind`, or its issues. Pure: no file. Used by the
/// pre-save check and, under the lock, by `ConfigFile::set_value`.
pub fn normalize(kind: &Source, field: &FieldId, value: Submitted<'_>) -> Result<Option<String>, Vec<FieldIssue>>;

/// Every issue in a save request, in request order (label, then values in order, items by
/// index, then access lists). With `based_on`, also the precondition's conflicts against the
/// file as it is now (§1), compared in process: one issue per changed `FieldId`, or one
/// `Connection` issue when the connection was removed or replaced or the base is unknown.
pub fn check_save(request: &ProviderConfigSaveRequestDto, file: &ConfigFile, base: Option<&EditBase>) -> Vec<ProviderConfigFieldIssueDto>;
```

The CLI's several arguments for a list (`sources set git/home dirs a b`) are `Submitted::List`; one argument stays `Text` (today's text-form rule). Storage problems (the connection is gone, a conflict, busy, I/O) are not field issues: they are the save's error (§1).

DTO `core/src/model/common/provider_config_field_issue_dto.rs`, generated:

```rust
// provider_config_field_ref_dto.rs
/// A field of a connection, as `FieldId` names it.
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ProviderConfigFieldRefDto {
    Label,
    Value { key: String },              // `GITLAB_URL`
    Item { key: String, index: u32 },   // one item of a list value
    Access { list: AccessListDto },     // `allow` | `agentAllow` (slice 2 adds the type and variant)
    Connection,                         // the connection as a whole
}

// provider_config_issue_kind_dto.rs
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum ProviderConfigIssueKindDto { Invalid, Changed, Removed, Replaced, UnknownBase }

// provider_config_field_issue_dto.rs
pub struct ProviderConfigFieldIssueDto {
    pub field: ProviderConfigFieldRefDto,
    pub problem: ProviderConfigIssueKindDto,
    pub message: String,
}
```

```ts
export type ProviderConfigFieldRefDto =
  | { kind: 'label' }
  | { kind: 'value'; key: string }
  | { kind: 'item'; key: string; index: number }
  | { kind: 'connection' } // slice 2 adds { kind: 'access'; list: AccessListDto }
export type ProviderConfigIssueKindDto =
  'invalid' | 'changed' | 'removed' | 'replaced' | 'unknownBase'
export type ProviderConfigFieldIssueDto = {
  field: ProviderConfigFieldRefDto
  problem: ProviderConfigIssueKindDto
  message: string
}
```

The `access` variant is added by slice 2 together with `AccessList`, so this step ships no unused variant. No issue ever carries a value: not the submitted one, not the current one, not a secret.

Desktop command `check_provider_config(request: ProviderConfigSaveRequestDto) -> Vec<ProviderConfigFieldIssueDto>`; operation `checkProviderConfig` (desktop only). It validates values against the request's kind (`invalid`) and, when the request carries `basedOn`, reads the file (no lock) and reports the precondition's conflicts (`changed`, `removed`, `replaced`, `unknownBase`, §1). The pane uses it on blur and after a rejected save for the conflict view. It cannot promise that the save succeeds (the file may change again; the save's own precondition decides). The pane ignores a reply for a draft that changed since the check was sent (its own counter per connection). No HTTP route.

Rules (core owns the texts; messages never repeat the submitted value):

| Value type   | Accepted                                                      | Message                                              |
| ------------ | ------------------------------------------------------------- | ---------------------------------------------------- |
| `bool`       | `true` or `false`, any case, trimmed                          | "Enter true or false."                               |
| `int`        | an `i64` in decimal, trimmed                                  | "Enter a whole number."                              |
| `url`        | `http` or `https` with a host (B64: Rust accepted any scheme) | "Enter an http or https address."                    |
| `url`        | no user name or password in it                                | "Enter the address without a user name or password." |
| `email`      | today's `looks_like_email`                                    | "Enter an email address."                            |
| list item    | each item as its item type, located by `item`                 | the item type's message                              |
| single value | a list for a single value is refused                          | "Enter one value, not a list."                       |
| label        | not empty after trimming, when the request carries a label    | "A connection needs a label."                        |
| unknown key  | —                                                             | "This kind has no such setting."                     |

Kept from today: a blank text for a single value clears it; an explicit empty list stays "none"; the pane omits a blank label from its delta, so it never sees the label issue (the CLI's `rename` does).

**Save envelope**: unchanged except the additive `installed` receipt (§5). A rejected save is one string built from the first issue as `"{field label}: {message}"`.

**Field descriptors**: labels are declared with each key in `provider!` (user decision 1) and replace `provider_config_key_label`; `ProviderConfigFieldStateDto.label` keeps its shape. No input-hint fields before a caller needs one; slice 10 adds its access descriptors.

**Tests**: table tests per row (including `ftp://x`, `https://` without a host, `https://user:pass@host` refused with the user-info message; a hand-written URL with user info still loads, its check reports the connection unavailable with the §4 message until the URL is changed, and an edit of another field does not have to resubmit the URL); `normalize` and `set_value` agree; text-form and multi-argument lists give the same stored value; clear vs empty list; issue order and item indexes; the save rejects with the first issue's text; a blur check for draft A that resolves after edit B is ignored; a file change between a valid check and the save is reported by the save; `provider-config-validation.ts` is deleted.

### 6.2 `ProviderInfo.recheckable` (R-F5; step 7)

```rust
/// Whether the UI offers to check this connection again.
#[serde(default)]
pub recheckable: bool,
```

**Rule (decided, user, 2026-11-19: keep both controls as today)**: today's effective rule, unchanged: `status` is `unavailable` or `degraded`, and `problem` is not `unsupported`. It equals the settings pane's predicate (`settings-source-pane.component.ts:227`) and, on the sidebar's list of unusable connections (`activity-data.service.ts:100`), its predicate (`sidebar.component.ts:85`). Both controls keep showing "Check again" for not-configured connections. The rule is computed in `provider_info()` and again where the host marks a connection degraded.

**Compatibility**: new answers carry the field; older consumers ignore it. `#[serde(default)]` lets new Rust read an older `ProviderInfo`. Every `ProviderInfo` literal in Rust and TypeScript fixtures gains it.

**Tests**: a core table over status × problem; the sidebar and pane render the control from the field alone, with today's outcomes.

**Meeting-ness (R-F5, N-18b)** needs no DTO change: in the day and week views `e` is an `EntryUiAdapter` whose `meetingWeight` is `null` for entries that are no meetings (`providers/types.ts:102`), so the check is `e.meetingWeight !== null` (a meeting weighted `none` is still a meeting), combined with the existing calendar-day comparison (`day-view.component.ts:154`, `week-view.component.ts:171`).

### 6.3 Generated constants (R-D2; step 6)

`core/src/ts.rs` appends after the type declarations:

```ts
export const COLLECT_MAX_DAYS = 3653
export const SOURCES = [
  'git',
  'gitlab',
  'jira',
  'outlook',
  'system',
  'calendar',
] as const satisfies readonly Source[]
```

Built from `CollectOptions::MAX_DAYS` and `Source::iter()` in enum order. They land in **step 6** with their first consumer (B57, §7) and replace `MAX_COLLECT_DAYS` (`deep-link.service.ts:54`); step 7 replaces the `SOURCES` record (`providers/connection.ts:13`) and the `no-sources` list (N-18a). **Tests**: strum names equal serde names; the generator output holds both; R-D3's drift check covers them.

R-D2's maximize-button payload (step 7) is pinned to today's exact shape, `{ hovered: boolean, pressed: boolean }` (`desktop/src/window_chrome/windows.rs:63`), as a generated type with a payload fixture.

---

## 7. Dashboard URL module (R-F4, step 6; `item` and Work routing in generic slice 9)

### Purpose

One typed module (`utils/app-url.ts`) reads and builds every dashboard URL, declares dated calendar views explicitly so the undated `work-items` view is never routed as a calendar, and carries the open work item as `?item=` with Back, share, deep link and reload behaving like `?event=`.

### Types

```ts
export const CALENDAR_VIEWS = ['day', 'workweek', 'week'] as const
export type CalendarView = (typeof CALENDAR_VIEWS)[number]
/** Views without a day in their path. Slice 9 adds 'work-items'. */
export const UNDATED_VIEWS = ['list'] as const
export type UndatedView = (typeof UNDATED_VIEWS)[number]
export type ViewMode = UndatedView | CalendarView

interface SharedQuery {
  readonly from: string | null
  readonly to: string | null
  readonly settings: SettingsSelection | null
}
interface ActivityDetail {
  readonly q: string
  readonly event: EntryKey | null
  readonly events: readonly EntryKey[]
}
/** Slice 9. */
interface WorkDetail {
  readonly item: WorkItemKey | null
}

export type AppLocation =
  | ({ readonly view: 'list' } & SharedQuery & ActivityDetail)
  | ({ readonly view: CalendarView; readonly day: string | null } & SharedQuery & ActivityDetail)
  | ({ readonly view: 'work-items' } & SharedQuery & WorkDetail) // slice 9

/** Every query parameter the app owns, with null for those `location` does not use. */
export function queryParamsOf(location: AppLocation): Required<AppQueryParams>
```

`ViewMode` moves here from `services/types.ts`; `isCalendarView(view): view is CalendarView` replaces `Exclude<ViewMode, 'list'>`.

### `?item=` (slice 9)

- **Key**: ``WorkItemKey = `${Source}/${string}/${string}` ``, spelled `<source>/<connection name>/<item key>`. `parseWorkItemKey(value): WorkItemRef | null` splits at the first two `/` (connection names are slugs and never hold `/`), needs a known source, a name and a key, and keeps the rest, `/` and `#` included, as the key; no source-specific validation (core's `run_action` answers `InvalidInput` or `NotFound`, which the detail shows).
- **Families**: `item` is valid only on `/work-items`; `q`, `event` and `events` only on the activity views. Because the router merges query parameters by default (`app.routes.ts:118`), dropping a parameter in the parsed model is not enough (finding 15): every navigation the app builds passes `queryParamsOf(location)`, which sets the other family's parameters to `null`, and a guard on the activity and Work routes that finds a parameter of the other family (from a link or a hand-typed URL) replaces the URL with it removed (`replaceUrl`, history depths unchanged).
- **History**: opening an item when none is open pushes one entry with `detailDepth` + 1; selecting another while one is open replaces it; closing goes back past `detailDepth`, or replaces the URL after a link or a reload (`activity-view-state.service.ts:268–367`). `navigation-motion` counts `item` like `event`. `historyDepth` reads `history.state` through `in` narrowing.
- **Reading**: a link only navigates; the detail reads through `run_action` under the connection's `allow`, as a click does.
- **Share**: the desktop header's link is built from the parsed location through `queryParamsOf` and the existing whitelist plus `item` (`header.component.ts:85`), never from the raw URL tree, so a parameter of the other family can never be shared. The browser has no Work items view; its Share gives the page's whole URL (B63).

### Work availability (slice 9)

The Work route depends on the pushed providers (finding 15; transport failure keeps the URL, decided, user, 2026-11-19):

| Situation                                                                | Result                                                                                                                                                   |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Browser build                                                            | `canMatch` false; `**` redirects to `/timeline` and the guard above removes `item` (replace)                                                             |
| Desktop, no pair yet, status `connecting` (the first attempt is running) | `canMatch` waits; it settles on the first of: a pair (then the rows below), or the status leaving `connecting`                                           |
| Desktop, no pair yet, status `reconnecting` or `failed`                  | `canMatch` matches; the view shows the transport state with Retry; the URL and `item` stay                                                               |
| A pair where no connection's `search` capability is allowed              | `canMatch` false (redirect as above); while on the route, a replace navigation to `/timeline` with `item: null`                                          |
| A pair where it is allowed but the connection's check is pending         | the route matches: whether `search` is allowed comes from the configuration and is in the checking `ProviderInfo` (§9)                                   |
| Transport failure while viewing                                          | the view and URL stay, showing the transport state; Work invalidation happens only through pairs (deep links are never redirected for a transport fault) |

### Round trip with `/` and `#` as data

| Stage                                 | Value                                                                             |
| ------------------------------------- | --------------------------------------------------------------------------------- |
| built by the router                   | `/work-items?item=gitlab%2Fwork%2Fgroup%2Fsub%2Fproject%2312`                     |
| desktop share                         | `activity-log://work-items?item=gitlab%2Fwork%2Fgroup%2Fsub%2Fproject%2312`       |
| OS → Rust (`url::Url`, `app_link`)    | unchanged: the `url` crate keeps `%2F` and `%23` in a query                       |
| `deepLinkPath`                        | `/work-items?item=gitlab%2Fwork%2Fgroup%2Fsub%2Fproject%2312`                     |
| `UrlSerializer.parse` → `queryParams` | `item = "gitlab/work/group/sub/project#12"`                                       |
| `parseWorkItemKey`                    | `{ connection: { source: 'gitlab', name: 'work' }, key: 'group/sub/project#12' }` |
| reload (asset protocol serves index)  | the same URL, parsed again to the same ref                                        |

A hand-written link with a raw `#` loses the part after it to the fragment; a raw `+` decodes as a space. Links the app builds encode both; README's links table says hand-written links must percent-encode them. Entry ids already hold `#` (`make_ids_unique`'s `id#2`) and `/`, so step 6 tests the round trip on `?event=` first; slice 9 repeats it for `?item=`.

### Range normalization (B57, step 6)

`collectableRange(from, to): boolean` in this module, used by `DeepLinkService` and `ActivityViewState`:

- No range, or only one bound: true (today's rule: nothing is collected for a partial range, and the link still opens).
- Both bounds must be valid day keys (`isDayKey`); otherwise false.
- `localDayRange(from, to)` gives the RFC 3339 `[start, end)` the dashboard sends; parse both with `Date.parse` and require `end > start` and `end − start ≤ COLLECT_MAX_DAYS × 86 400 000 ms`, exactly `CollectOptions::check` (`collect_options.rs:18`).

### Tests

- `app-url.spec.ts`: round trips for the key above and for `git/work/1f3a#2` and `gitlab/work/a/b/c`; `queryParamsOf` nulls the other family; raw `#` and `+`; `isCalendarView('work-items')` is false.
- `deep-link.service.spec.ts`: the `activity-log://` form resolves to the same `UrlTree` as the in-app URL; a link carrying `item` on `/timeline` lands without it in the URL.
- B57 in a fixed zone (`TZ=Europe/Berlin` for the spec run), at the actual limit across a DST change: `from=2020-03-30`, `to=2030-03-30` (inclusive) has the exclusive end 2030-03-31 00:00 local and lasts 3653 days plus one hour, so it is **refused**, as core refuses it; `from=2020-03-30`, `to=2030-03-29` lasts 3652 days plus one hour and is accepted; partial bounds pass (nothing is collected); invalid day keys are refused.
- Slice 9: a cold deep link during checks waits and opens the item; losing the last allowed connection while on the route replaces to `/timeline` without `item`; a failed first handshake keeps the URL and Retry recovers; Work → Timeline → Work with Share in between never carries `item` on the timeline link; history depths stay right through each replace.
- desktop `deep_link/tests.rs`: `app_link` returns the link above byte for byte.

---

## 8. Shortcut registry ownership (R-F14, step 10; NF-6 in step 10a)

### Purpose

One table of app shortcuts, one document listener and one resolution rule decide which command a key or a menu item runs. Scopes come from the active route and the open layers, so a view kept alive while detached (`TimelineReuseStrategy`) or hidden under the phone settings page holds no keys. The macOS menu is generated from the same table and resolves its accelerators through the same rule.

### Types (`models/shortcuts.ts`)

```ts
/** `global`, or the scope a route declares in `data.shortcutScope`. Slice 9 adds 'work-items'. */
export type ShortcutScope = 'global' | 'timeline' | 'calendar'
export type MenuSubmenu = 'app' | 'file' | 'edit' | 'view' | 'window' | 'help'

export interface Chord {
  /** Normalized by `utils/key-chord.ts`: 'mod+r', 'f5', '/', '?', 'escape', 'mod+[', 'alt+arrowleft'. */
  readonly keys: ChordKeys
  /** Absent: every OS. 'mac' includes the browser on macOS. */
  readonly os?: 'mac' | 'other'
  /** Absent: wherever the entry is. 'desktop': not in the browser. */
  readonly where?: 'desktop'
  /** Fires while focus is in an editable field. Never for a chord that types a character. */
  readonly whileTyping: boolean
  /** On macOS Tauri, the menu owns this chord once installed. */
  readonly menuAccelerator: boolean
}

export interface ShortcutEntry {
  readonly id: ShortcutId
  /** Verbatim in the key tag's host, the `?` help and the macOS menu. */
  readonly label: string
  readonly scope: ShortcutScope
  readonly chords: readonly Chord[]
  readonly menu?: MenuSubmenu
}
```

`ShortcutService` (`services/shortcut.service.ts`):

```ts
/** Binds `run` to `id` until the returned function is called. `run` returning false declines. */
bind(id: ShortcutId, run: () => void | false, enabled?: Signal<boolean>): () => void
/** Opens a layer (a detail page or panel, the phone settings page) until the returned function is called. */
openLayer(layer: { readonly hidesRoute: boolean }): () => void
/** Runs `id` as a key would, with the same guards; for buttons without a host and menu items. */
trigger(id: ShortcutId): boolean
resolve(keys: ChordKeys): Signal<ShortcutEntry | null>
triggerChord(keys: ChordKeys): boolean
/** Set by the native menu once `setAsAppMenu()` resolved (NF-6). */
readonly menuOwnsAccelerators: WritableSignal<boolean>
```

### Resolution rule

On a `keydown` at the document (the only key listener for app shortcuts), dispatched in the bubbling phase so focused widgets see the key first:

1. Ignore it when `defaultPrevented` (a field or widget consumed it, which also leaves every layer open), during IME composition (`isComposing`, key 229), on auto-repeat, or when the target is inside an `inert` subtree.
2. Normalize it to `ChordKeys` (`mod` = ⌘ on macOS, Ctrl elsewhere).
3. Candidates: entries with that chord for this OS and build (`where` per chord).
4. **Active scope**: if an open layer has `hidesRoute` (the phone settings page, a phone detail page), the route scope is inactive and only `global` entries apply; otherwise the `shortcutScope` of the deepest route in `router.routerState.snapshot` that declares one, recomputed on `router.lastSuccessfulNavigation()`, else `global`.
5. **View scope wins**: a candidate of the active scope beats a `global` one with the same chord.
6. **Typing guard**: in an editable target (a text-like `input`, `textarea`, `select`, `isContentEditable`), only chords with `whileTyping`.
7. **Dialogs**: while a Material dialog other than the `?` help is open, nothing fires; the dialog owns its keys, Escape included (generic slice 8's rule).
8. **Menu ownership**: on macOS Tauri, when `menuOwnsAccelerators()` and the chord is `menuAccelerator`, stop; the menu dispatches it. Per chord. Before step 10a, after a failed installation, on Windows, Linux and in the browser, the renderer handles every chord.
9. Run the top binding: bindings of an entry form a stack in bind order; the top one that is enabled and whose host (for the directive) is connected and not inert runs; if it returns false, the next one down is tried. Entries with a default command (for example `app.filter`) run it when no binding takes the key.
10. `preventDefault()` only when a command ran.

**Escape** keeps today's behaviour (`event-detail.component.ts:135`, `settings-page-host.component.ts:60`): `layer.close` has `escape` with `whileTyping: true`; a field or a dialog that handles Escape marks it handled and step 1 stops it; otherwise the top layer's binding closes that layer only. Each layer binds `layer.close` when it opens; inner layers open later, so they are on top. A layer that wants Escape to reach the layer below returns false.

**Menu dispatch**: a menu item's action runs `trigger`/`triggerChord` with the same guards; for an entry whose renderer chord is not `whileTyping`, the action declines while `document.activeElement` is editable. Because a native accelerator consumes the keystroke before the page sees it, a chord may be `menuAccelerator` only if it has no text-editing meaning on that OS: ⌘[ on macOS qualifies; Alt+← (word navigation) is never a menu accelerator and stays renderer-only, not while typing.

### Menu items (NF-6)

- Entries with `menu` are grouped per submenu and OS by their `menuAccelerator` chord. A chord shared by several entries becomes one item whose title is `resolve(keys)().label` and whose action is `triggerChord(keys)`; the title follows scope changes with `setText`. ⌘F is the case: `app.filter` ("Filter Activity…", global) and, with slice 9, `work.search` ("Search work items…", `work-items`) share `mod+f` in `edit`.
- An entry with `menu` and no accelerator chord is one item without an accelerator whose action is `trigger(id)` (Export…, Copy Link, Help › Keyboard Shortcuts, whose `?` stays in the renderer).
- Accelerators come only from `menuAccelerator` chords, through `toAccelerator()`. All menu operations go through the signed plan's single serial queue tagged with a generation (NF-6).

### Invariants (a registry spec checks them)

- No two entries of one scope share a chord on one OS.
- An unmodified character chord (`/`, `?`, letters) is neither `whileTyping` nor `menuAccelerator`. Modified chords, function keys (F5) and Escape may be `whileTyping` (R-F14 g).
- `menuAccelerator` only on modified chords with no text-editing meaning on their OS.
- Entries sharing a `menuAccelerator` chord on one OS sit in the same submenu.
- In the browser build the active global chords are exactly `/` and `?` (native 12; Escape layers apart); `app.filter`'s Mod+F is `where: 'desktop'`, so the browser keeps its own find.
- Labels are unique within a submenu.
- **One listener**: a spec reads the dashboard's TypeScript and templates and fails on `(document:keydown`, `(window:keydown`, `@HostListener('document:keydown` or `('window:keydown`, and `onKey(` or `listener(` calls with `target: document` or `window`, outside `shortcut.service.ts`. It catches today's three handlers (`search-bar.component.ts:24`'s `onKey('/', …, { target: document })`, the two Escape handlers), which the step migrates. Widget keys on their own elements stay local.
- A host inside a view the router can detach binds only entries of that view's scope.

### Tests

The slice 8 and R-F14 tests, plus: a detached Timeline's `/` does not fire on a calendar view or under the phone settings page; ⌘F resolves to `app.filter` on the Timeline and to `work.search` on Work items, by key and by `triggerChord`; F5 fires in an input on Windows and Linux; Escape in an input inside a detail closes the detail unless the input handled it; a dialog over a detail takes Escape and both stay as they should; the menu's ⌘F title ends on the last scope after out-of-order changes; with `menuOwnsAccelerators` false the renderer handles ⌘R, with it true (macOS only) it does not; Back from the menu does nothing while typing. Device (macOS, step 10a): the focused control after ⌘F, Escape and ⌘[ and that typing `a?b/c` keeps its text, not only handler counts.

---

## 9. Not covered here; requirements on generic slices

- **`detail-panel`** (R-F13, step 11): a component API from the layout design (CHANGE 3 and 4). Its shared parts are here: the URL and history rules (§7) and Escape as a stacked `layer.close` binding with `openLayer` (§8).
- **Slices 3 and 6** keep their own design review in `generic-read-step.md`; they must meet this document at these points (R2):
  - **Where the context is bound**: the desktop's `run_action` calls `Host::action_target(id)`, which first awaits the connection's **recorded** check and then reads the checked provider, the access policy and the `AccessContext` together under one state lock (§2). The read runs only against that `ActionTarget`, and the answer is tagged with that `context`. The context therefore always names the principal and policy the read actually used: a read started under A that waits for a check recording B runs as B and is tagged B.
  - **One executor, one refusal order** (D5): slice 3's `actions::run` takes the target instead of a bare runtime, `run(caller, &ActionTarget, ActionRequest) -> ActionResponse`, and keeps its refusal order for every caller: (1) the kind implements the capability → `Unsupported`; (2) `allow` → `NotAllowed`; (3) the origin's policy → `NotAllowed`; (4) the recorded check left it usable → `Unavailable` with the check's reason; (5) limits, then the provider. Because `action_target` returns a target for every known connection, usable or not, a known Jira connection with search off and an unusable check answers `NotAllowed` on the desktop and in the CLI alike, and an unsupported action on an unusable kind answers `Unsupported` in both.
  - **Answer shape**: the common `ActionResponse` (`Done`/`Failed`) stays as slice 3 defines it; the desktop command answers with an envelope `HostActionResponseDto { context?: AccessContext, response: ActionResponse }`, with `context` from the target. `context` is absent only when `action_target` returned `None` (the connection is not in the current runtime); `response` is then `Failed` with `Unavailable`, "No connection gitlab/x". The CLI builds its target from the one runtime it checked (`context: None`) and prints `ActionResponse` unchanged.
  - **Discarding a mismatch**: the dashboard applies an answer only if its `context.epoch` and `context.access` equal what it has installed for that connection; otherwise it drops it. If the answer is newer (its boundary has not arrived yet), the boundary pair re-runs the read under the new access (§5); if it is older, the installed state already superseded it. An answer without `context` is applied only as an error for the request that sent it, never as data.
  - **CLI**: has no host and no push; no context exists or is needed, because nothing displays a previous principal.
  - `generic-read-step.md` slices 3 and 6 carry a note pointing here for these replacements (`run`'s signature, `run_action`'s call and its envelope).
  - Tests: a refused (search off) and unusable known connection gives the same code from the desktop command and the CLI; an unsupported action on an unusable kind the same; a context-changing push arriving before and after an action reply ends with the reply applied exactly when its context matches the installed one.
  - The checking `ProviderInfo` already lists `capabilities` with `allowed` from the configuration and `grant: unknown`, so the Work group can be decided before checks end (§7).
  - Identity evidence (`Verified`/`Unnamed`/`Denied`/`None`) is produced by step 2 (generic slice 1) only (§4); slice 3 reads it, it does not map it again.
- **Slice 9** (docs edit in step 9, R-X1): its Work service re-runs a search only when a connection's `access` changes, never on an ordinary providers update (§5 replaces "a changed providers response re-runs the current search", `generic-read-step.md:256`); its "falls back to Timeline" rule applies only to the policy rows of §7's availability table, never before a pair or on a transport fault.
- **Slice 10**'s access fields and descriptors, R-D1's entry deserialization: no new shared shape here beyond §4's access token.
- **Time, Review, writes, the agent transport, ⌘K**: out of scope. Nothing here reserves a field, event or slot for them; the push stream carries collection and provider state only (N-17).

---

## User decisions (2026-11-19)

The five points the review raised, as the user decided them. Each is applied in the section named.

| Id  | Question                                     | Decision                                                                                                                                                                                                                                                                        | Where  |
| --- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| a   | "Check again" for not-configured connections | Keep both controls (sidebar and settings pane) as today.                                                                                                                                                                                                                        | §6.2   |
| b   | No config folder                             | Start empty and refuse to save: reading gives no sources; every save or edit fails with a message naming `ACTIVITY_LOG_CONFIG` and `--config`; `config path` says the same; tokens are never written to a temporary or fallback location. Replaces signed R-C1's fallback.      | §1, §2 |
| c   | API startup with an invalid config           | The API stops with the error (line and column, no file content), as today; the desktop opens empty.                                                                                                                                                                             | §2     |
| d   | Work items when the transport fails          | Keep the URL and `?item=`, show the transport state and offer Retry.                                                                                                                                                                                                            | §7     |
| e   | Lock wait and conflicts                      | Wait up to 5 s, then "The settings file is busy; try again." with Retry and the draft kept. Edits to different fields go through silently. A conflict on the same field, a token, or a removed and re-added connection stops the save and shows the current values to re-apply. | §1     |

Also kept as today: blank-label and list input in the settings pane (§6.1) and Escape behaviour (§8).

---

## Final check resolutions (revision 4)

Each remaining defect of `step0-final.md` and the section of this revision that closes it.

| Defect                                                                         | Closed in                                                                                                                    | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1 · legacy drafts and editor publication bypass instances                     | §1 Writing (steps 4–6), Connection instances (precondition table), `config edit` (steps 3 and 5), §1 tests                   | The precondition runs on the file as read, before any instance is assigned; a legacy base (`instance: None`) compares its pre-image and the save assigns that table's first id; a legacy base meeting a marked table is `Replaced`, once; ids go only into tables the app writes; malformed or duplicate ids read as absent; the editor never touches an unchanged file, and reconciliation gives added or copied tables fresh ids; the observational limit is stated |
| D2 · Windows recipes not equivalent; folder ACE does not propagate             | §1 Private files (Windows); §1 step 1 device check                                                                           | One SDDL recipe with `P` (protected) and `OICI` for the staging and session folders; files with the protected file form; `FILE_PERSISTENT_ACLS` checked before writing; a second non-owner account and `GetEffectiveRightsFromAclW` in the VM check, owner access verified too                                                                                                                                                                                        |
| D3 · Unix mode test can pass for an ACL-readable macOS file                    | §1 Private files (macOS, Linux); §1 tests                                                                                    | Secrets written only inside a staging folder whose ACL is emptied (macOS) or whose POSIX ACLs are removed (Linux) and verified; `Unprotectable` otherwise; tests check `acl_get_file` and the xattrs with an inheriting parent, for config, cache and editor files                                                                                                                                                                                                    |
| D4 · two unnamed checks share keys                                             | §4 identity state (`Unnamed(u64)`), key rules, tests                                                                         | Each unnamed check is a new state, rotating both keys; user-visible cost stated                                                                                                                                                                                                                                                                                                                                                                                       |
| D5 · desktop and CLI refuse differently                                        | §2 `action_target` (always a target for a known connection); §9 one executor; `generic-read-step.md` notes in slices 3 and 6 | `actions::run(caller, &ActionTarget, request)` applies slice 3's refusal order for both callers; the envelope only adds the context; tests across callers and push orderings                                                                                                                                                                                                                                                                                          |
| D6 · fixture asks legacy reads for access tokens                               | §3 tests (Finding 1 / R3)                                                                                                    | Access asserted on the pushed providers part; activity and delayed projection checked for evicted entries; `GET /api/providers` compared with the published providers field                                                                                                                                                                                                                                                                                           |
| D7 · collection check future cannot be superseded                              | §3 `Plan::Collect` (`Result<Checked, Superseded>`), Supersession; §2 recorded checks; §2 tests                               | Superseding transitions stop the waiting run and drop its check futures; the type can still express `Superseded`, treated as `Skipped`; only `action_target` follows a newer check                                                                                                                                                                                                                                                                                    |
| Revision 5 · editor reconciliation (D1 remainder)                              | §1 `config edit` step 5; §1 tests                                                                                            | A modified legacy table gets a fresh instance; a hand-deleted instance line gives a fresh one, so bases on the old id are `Replaced`; only unchanged legacy tables stay unmarked                                                                                                                                                                                                                                                                                      |
| Revision 5 · Linux and macOS mounts that report but do not enforce permissions | §1 Private files (Linux, macOS); "What the user sees"; §1 Linux device check                                                 | Allow-list of local file systems by `f_type` (Linux) and `f_fstypename` plus `MNT_IGNORE_OWNERSHIP` (macOS); everything else `Unprotectable` before writing, with the stated message; cache runs uncached there                                                                                                                                                                                                                                                       |
| Revision 5 · publication through a path another account could rename           | §1 Private files (parent folder check, handle-bound publication); §1 tests and Windows device check                          | Parent must not let other accounts add, rename or delete entries, else `Unprotectable`; staging, file creation, editor read-back and publication use `openat`/`renameat` on verified descriptors (Unix) and `SetFileInformationByHandle(FileRenameInfoEx)` relative to the parent handle (Windows)                                                                                                                                                                    |

Also applied from the user's record (`user-decisions.md`, 2026-11-19): a Jira or GitLab URL with a user name or password makes the connection unavailable until it is changed (§4 cache scope, §6.1), and `instance` is written when the app first edits a connection (§1).

## Re-review 2 resolutions

Each point of `step0-rereview.md` and the section of this revision that closes it, so the final check can be narrow.

| Re-review point                                                           | Closed in                                                           | Change                                                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| R1 · a stale draft overwrites a removed and re-added connection           | §1 Connection instances and preconditions; §6.1; §1 tests           | A reserved `instance` per table makes any cooperating re-add a `Replaced` conflict, values or not; `FieldId` covers label, values and access lists; remove gets an optional base; conflicts are reported per field without values; draft keeps `basedOn`; whole-draft recovery |
| R2 · a Work read tagged with the wrong principal                          | §2 `action_target` and recorded checks; §9 Slices 3 and 6           | The context is read with the checked provider and policy after the recorded check; superseded checks release waiters; answers carry that context in a desktop envelope and mismatches are dropped; CLI has none                                                                |
| R2 · usable check without an id keeps the old principal                   | §4 `IdentityEvidence` and identity state                            | `Unnamed` replaces `Verified(A)`, rotating the key and bypassing the cache; `None` (no read) keeps the history                                                                                                                                                                 |
| R2 · generic interface alignment (unknown id, CLI, Work invalidation)     | §9                                                                  | Envelope with optional context for unknown connections; CLI prints `ActionResponse`; slice 9 re-runs searches only on an `access` change, recorded for step 9's doc edit                                                                                                       |
| R3 · restamping reuses a payload that holds evicted entries or old tokens | §5 Publishing in core (caches); §3 tests                            | Exactly three caches, each on an immutable value; eviction creates a new activity payload, any access or list change a new providers payload, every revision a new envelope; test with previously serialized parts and a GitLab nested push                                    |
| R4 · Windows temporary file readable before tightening                    | §1 Private files                                                    | `CreateFileW` with a protected owner-only descriptor at creation (`CREATE_NEW`); the result replaces a loose target ACL; `tempfile` not used in production; cache and lock files use the same helper                                                                           |
| R4 · editor copy, retained copy, unchanged path                           | §1 `config edit`                                                    | Private session folder; first creation and mode repair run before the editor opens                                                                                                                                                                                             |
| R5 · reordering Git connections; running vs zero-kind contradiction       | §2 Installing a config (affected kinds and the outcome table)       | Per-kind order is a collection input; one outcome per shown-run case; a label-only install during a pending check replaces the run and ends its waiters                                                                                                                        |
| R6 · unreadable cache replaced by a subset                                | §4 Cache writer                                                     | A failed locked re-read publishes nothing; the request still answers the whole range; `fetched_at` is the fetch start                                                                                                                                                          |
| R7 · epoch from buffered messages; old receipts                           | §5 Attempts, Tauri drain, coordinator pair rule 1, save activation  | Only an attempt's authoritative pair changes the epoch; buffered other-epoch messages dropped; evidence reset per epoch; old-epoch receipts retired and their snapshots not applied; replies pinned to epoch and draft                                                         |
| R8 · per-part acknowledgement with the Tauri rate                         | §5 Publishing (`Due`, per-part `delivered_*`) and Desktop rate rule | One cursor per part; a pair advances both and cancels a pending activity send; the timer recomputes from the current value                                                                                                                                                     |
| Finding 4 · skipped connections and plan membership                       | §3 Who is in a plan                                                 | Membership defined for new, removed, skipped and unaffected connections                                                                                                                                                                                                        |
| Finding 8 / deviation 28 · outside writer overclaim                       | §2 Installing a config; deviation 28                                | The settle re-read is a debounce; guarantees only for cooperating writers                                                                                                                                                                                                      |
| Finding 9 · pin the same-principal behaviour                              | §4 Tests (step 3)                                                   | Two tokens of one principal share a file; two principals do not                                                                                                                                                                                                                |
| Finding 13 · URL rule vs test; access field identity                      | §6.1                                                                | User info refused for `url` values (row and test agree); `FieldId`/`ProviderConfigFieldRefDto` include access lists (variant added by slice 2)                                                                                                                                 |
| Finding 15 · overlapping pre-pair rows; generic slice 9 fallback          | §7 Work availability; §9 Slice 9                                    | `connecting` waits, `reconnecting`/`failed` match with Retry; slice 9's fallback limited to policy rows                                                                                                                                                                        |
| Finding 17 · impossible DST fixture                                       | §7 Tests                                                            | Berlin 2020-03-30 to 2030-03-30 refused (3653 days + 1 h); to 2030-03-29 accepted                                                                                                                                                                                              |
| Deviation 24 · no-op compared with the installed content                  | §2 Installing a config step 1                                       | A save that wrote nothing still installs when the file differs from the host's installed content                                                                                                                                                                               |
| Scope audit · `denied` produced twice; fixture wording; (e) message       | §4, §9; §5 Tests; User decisions                                    | Evidence produced only in step 2; route fixtures pin old fields plus the additive field; the busy message quoted exactly                                                                                                                                                       |
| Scope audit · stale Q text                                                | `generic-read-step.md` Q2; `provider-actions.md` Q1–Q3 lines        | Q2 no longer says there is no opener; provider-actions marks Q1 and Q3 decided and Q2's opener as present                                                                                                                                                                      |

## Review response (revision 2)

Each of Astra's findings was checked against the code and this document. None is rejected; three are taken with a narrower change than proposed, with the reason.

| #   | Disposition                                                                                                                                        | Change                                                                                                                                                                         |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Confirmed. Revision 1 filtered carry only at run start; a check that records a new principal mid-run had no eviction.                              | Checks are recorded before collection or actions use them; collection key per entry; eviction and reprojection in the boundary transition; stale reports checked (§2, §3, §4). |
| 2   | Confirmed in the contract. Practical impact is small (Work is desktop-only and a desktop epoch changes only with the app), but the rule must hold. | Keys scoped by epoch; all Work state cleared on an epoch change; invalidation over the union of ids (§4, §5).                                                                  |
| 3   | Confirmed.                                                                                                                                         | Numbered attempts with fencing, listener disposal, parse faults, pending-evidence resync, drain rule (§5).                                                                     |
| 4   | Confirmed: `Collector` and `Progress` carry a folded log (`core/src/runtime/mod.rs:37–44`).                                                        | `RunReport`/`Slot`/`Outcome`/`Plan`; carry with provenance; failed, skipped and empty kept apart; fixtures with fixed `generated` (§3).                                        |
| 5   | Confirmed: removal changes Git's `finish_kind` result for siblings; clearing the failure map hid failures.                                         | Affected kinds from additions, removals and changed collection inputs; `Scope::Kinds`; zero-kind installs; per-connection failures; access-only edits keep activity (§2, §4).  |
| 6   | Confirmed, including `config edit` opening the real file (`cli/src/main.rs:194`).                                                                  | `Precondition`/`EditBase` with account fields; desktop snapshot revisions; `config edit` through a private copy and compare-and-publish (§1).                                  |
| 7   | Confirmed.                                                                                                                                         | Absolute paths, dangling links refused, mode repair, owner-only DACL before the first byte on Windows, lock errors apart from contention, durability stated separately (§1).   |
| 8   | Confirmed.                                                                                                                                         | Bytes, id and parse separated; rejected bytes cached, I/O retried; settle re-read for outside writers; `BadConfig` startup policy; no error `source()` (§1, §2).               |
| 9   | Confirmed for URL credentials. The same-principal case is answered with a stated guarantee instead of a change: one person's own cache.            | Canonical endpoint in the fingerprint; credential-bearing URLs bypass the cache without echo; guarantee written down (§4).                                                     |
| 10  | Confirmed as missing (step 0's scope named only the identity part, but the writer is a shared on-disk contract).                                   | Cache writer contract: repeatable fetch, lock-merge-replace of fetched days, freshness rule, failure policy, blocking I/O off the async path (§4).                             |
| 11  | Confirmed.                                                                                                                                         | Additive `installed: InstallReceipt`; activation by revision; stale snapshot replies reload; `configGeneration` leaves the wire (§5).                                          |
| 12  | Confirmed. (The content-id reload would install an orphaned write within 2 s, but the receipt would be lost.)                                      | Host-owned edit task; locks released before spawning; old values dropped after unlock; `shutdown()` and weak watchers; notice task ends on exit (§2).                          |
| 13  | Confirmed.                                                                                                                                         | `Submitted`, `normalize`, `FieldRef` with item index, request order, no echo of values, storage errors apart, draft counter in the pane (§6.1).                                |
| 14  | Confirmed: F5 and Escape rules contradicted R-F14 (g); the source scan missed `onKey(..., { target: document })` (`search-bar.component.ts:24`).   | `whileTyping` rule corrected; per-chord `where`; layers with `hidesRoute`; Escape as today; menu-accelerator restriction; platform grouping; scan patterns (§8).               |
| 15  | Confirmed: the router merges query parameters by default (`app.routes.ts:118`), and Share reads the raw URL.                                       | `queryParamsOf` with explicit nulls, a replace guard, Share from the parsed location, the Work availability table (§7).                                                        |
| 16  | Confirmed: both surfaces show "Check again" for not-configured connections today.                                                                  | Rule reverted to today's; the user decided to keep it (a) (§6.2).                                                                                                              |
| 17  | Confirmed: `COLLECT_MAX_DAYS` was scheduled after its consumer; the views read the adapter, whose `meetingWeight` is nullable.                     | Constants in step 6; B57 on parsed instants with bounds and day-key checks in a fixed zone; `meetingWeight !== null`; maximize payload pinned (§6.2, §6.3, §7).                |
| 18  | Confirmed.                                                                                                                                         | `Payload`/`Part` ownership with both caches; delivery accounting and reset on Tauri; activity rate limit and a measured bound; test methods corrected (§5).                    |
| 19  | Confirmed.                                                                                                                                         | Identity state (`Unknown`/`Verified`/`Denied`); `denied` from 401/403; action answers carry their access context (§4, §9).                                                     |

---

## Changes to the merged plan

Each change is a tightening or a simplification; none changes a user decision. "Astra" is the reviewer's position on revision 1; "Response" is this revision's answer.

| #   | Change                                                                                                             | Astra                   | Response                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------ | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Parts carry `epoch` and `authority`; no `{ epoch, update }` envelope; `RuntimeState` is `{ activity, providers }`. | Agree                   | Agree. Equality of both parts' epoch and authority is asserted in core and relied on by the coordinator; cursors stay per part.                                                                                                                                                                                                                                                                                                              |
| 2   | No SSE `id:` field.                                                                                                | Agree                   | Agree; unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 3   | A future-authority part triggers a coalesced resync instead of a 1 s hold.                                         | Agree with changes      | Agree. Pending evidence survives a newer part during the sample and triggers one more resync (§5).                                                                                                                                                                                                                                                                                                                                           |
| 4   | Transports do not filter by revision; the coordinator orders.                                                      | Agree with changes      | Agree. Attempt fencing, listener disposal and the first-pair promise belong to each transport (§5).                                                                                                                                                                                                                                                                                                                                          |
| 5   | `access` and `authority` (and, in revision 1, `configGeneration`) are host counters.                               | Agree with changes      | Agree. Counters are scoped by epoch; Work keys include the epoch; no content id is emitted; `configGeneration` leaves the wire (change 24).                                                                                                                                                                                                                                                                                                  |
| 6   | The store keeps a key instead of `RemoteIdentity`; carry is keyed by it.                                           | Agree with changes      | Agree, refined: the store keeps the **collection key** (values, location, identity state), not the access token, so access-only edits keep activity; eviction happens in the boundary transition (§3, §4).                                                                                                                                                                                                                                   |
| 7   | A cached kind without an identity bypasses the day cache.                                                          | Agree with changes      | Agree. Only usable connections are collected; the identity state separates verified, historical and denied (§4).                                                                                                                                                                                                                                                                                                                             |
| 8   | No client revision token for config edits.                                                                         | Object                  | Objection accepted. Writers that showed values pass a precondition on the touched and account fields, compared in process; the desktop uses snapshot revisions; `config edit` compares bytes (§1).                                                                                                                                                                                                                                           |
| 9   | State lock and config lock.                                                                                        | Agree with changes      | Agree. The edit task is owned by the host, locks are released before spawning, and shutdown is explicit (§2).                                                                                                                                                                                                                                                                                                                                |
| 10  | Run state moves into the host; `ActivitySnapshot` replaced by `ActivityState`.                                     | Agree with changes      | Agree. Legacy mapping to 202/200/503 and the failed range and message is pinned; the collector supplies the report (§2, §3).                                                                                                                                                                                                                                                                                                                 |
| 11  | `RunSpawner` receives all host work; checks resolve to `None`; desktop notice through a channel.                   | Agree with changes      | Agree. Exactly once per run, never for a join; the notice task ends on exit through `shutdown()` (§2).                                                                                                                                                                                                                                                                                                                                       |
| 12  | `RunOutcome::Failed` carries no message.                                                                           | Agree                   | Agree; the message stays in the published state and the log.                                                                                                                                                                                                                                                                                                                                                                                 |
| 13  | Field issues through `check_provider_config`; save envelope kept.                                                  | Agree with changes      | Agree. Shared `normalize`, `FieldRef` with item index, request order, draft counter; the check cannot promise the save (§6.1).                                                                                                                                                                                                                                                                                                               |
| 14  | `recheckable` false for `notConfigured`.                                                                           | Object without approval | Objection accepted: withdrawn. Today's rule is kept (user decision a, 2026-11-19).                                                                                                                                                                                                                                                                                                                                                           |
| 15  | Meeting-ness needs no DTO field.                                                                                   | Agree with correction   | Agree. The check is on the adapter, `e.meetingWeight !== null` (§6.2).                                                                                                                                                                                                                                                                                                                                                                       |
| 16  | `item` only on `/work-items`; `q`/`event`/`events` only on activity views.                                         | Agree with changes      | Agree. Explicit nulls, a replace guard, Share from the parsed location, and the Work availability table (§7).                                                                                                                                                                                                                                                                                                                                |
| 17  | Round trip tested on `?event=` in step 6.                                                                          | Agree                   | Agree; unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 18  | B57 on the instants sent, with the backend's rule.                                                                 | Agree with sequencing   | Agree. The constant moves to step 6; bounds and day keys checked; parsed instants compared; a fixed DST zone in the test (§7).                                                                                                                                                                                                                                                                                                               |
| 19  | Menu items per accelerator chord; binding stack; dialog guard; single-listener scan.                               | Agree in part           | Agree. F5, Escape, hidden phone views, menu-accelerator restriction and scan patterns corrected (§8).                                                                                                                                                                                                                                                                                                                                        |
| 20  | serde `rc` feature.                                                                                                | Agree                   | Agree. The JSON caches are specified separately (`Payload`, `Part`); the next revision is checked before use (§5).                                                                                                                                                                                                                                                                                                                           |
| 21  | No `temp_dir` fallback for the config folder.                                                                      | User decision           | Decided by the user (b, 2026-11-19): no fallback location at all. Reading is empty, every save fails with a message naming `ACTIVITY_LOG_CONFIG` and `--config`, `config path` says the same. This deviates from signed R-C1's private fallback by user decision (§1).                                                                                                                                                                       |
| 22  | Providers part from check results recorded under the lock.                                                         | Agree with changes      | Agree. Collection and actions await the recorded check, never the raw shared future (§2).                                                                                                                                                                                                                                                                                                                                                    |
| 23  | Wording fixes (cursor listed twice, "both watches", three Tauri events, B66 committed).                            | Agree                   | Agree; unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 24  | **New**: `configGeneration` is not on the wire; the save receipt replaces its one use.                             | —                       | A generation compared for "differs" cannot tell a no-op save or a concurrent install apart; an ordered revision can (§5). Astra (re-review): sound with the receipt lifecycle fix. Done: old-epoch receipts are retired and their snapshots never applied; a no-op save compares the result with the host's installed content, not only with the file (§2, §5).                                                                              |
| 25  | **New**: a collection key apart from the access token.                                                             | —                       | Activity depends on who collected it; Work also on `allow`/`agent_allow`. One key for both would drop activity on an access-list edit (§4).                                                                                                                                                                                                                                                                                                  |
| 26  | **New**: action answers carry an `AccessContext`.                                                                  | —                       | Requirement on slice 3 so an answer computed under another principal than the one shown is dropped (§9). Astra (re-review): sound objective, unsafe binding. Done: the context is read with provider and policy after the recorded check (`action_target`), answers use a desktop envelope with an optional context, and the CLI has none (§2, §9).                                                                                          |
| 27  | **New**: on Tauri, `activity` events at most every 250 ms, to be measured in step 6a.                              | —                       | `watch` does not bound scripts already queued in a slow webview (§5). Astra (re-review): sound provisional interval. Done: per-part acknowledgement and a timer that recomputes from the then-current value; no byte bound is claimed; 6a measures (§5).                                                                                                                                                                                     |
| 28  | **New**: outside writers' changes are installed after a 200 ms settle re-read.                                     | —                       | A truncate-then-write by an editor or script outside the protocol is otherwise installed half-way (§2). Astra (re-review): sound as a debounce only. Agreed: the text now says it is a debounce, guarantees whole documents only for cooperating writers, and names what a paused outside writer can cause; later changes keep being installed (§2).                                                                                         |
| 29  | **New**: a reserved, non-secret `instance` field per connection table.                                             | —                       | Values cannot tell two lifetimes of one id apart (a same-value re-add, a Git connection without account fields); decision (e) requires that a re-added connection is always a conflict (§1). Final check (D1): written only into the table the app writes, never backfilled; legacy tables have a defined first conditional save; `config edit` reconciles instances and never touches an unchanged file. Approved by the user (2026-11-19). |
| 30  | **New**: a private-file helper replaces `tempfile` in production.                                                  | —                       | `tempfile` creates the Windows file before any protection can be set. Final check (D2, D3): one verified staging-folder recipe per system (protected owner-only DACL with `OICI` on Windows, empty ACL on macOS, no POSIX ACL on Linux), `Unprotectable` before any secret byte on locations that cannot hold private files, unchanged files never rewritten, lock files unprotected because they hold nothing (§1).                         |
| 31  | **New**: identity evidence distinguishes `Unnamed` (read without an id) from `None` (no read).                     | —                       | A usable answer without an id must not label new data with an earlier principal; a failed check that read nothing keeps the history (§4). Final check (D4): every unnamed check is its own state, so keys rotate on each; the cost is stated in §4.                                                                                                                                                                                          |
| 32  | **New**: `HostActionResponseDto` around slice 3's `ActionResponse` on desktop.                                     | —                       | Slice 3's common answer stays host-independent (the CLI uses it); only the desktop has an epoch and a pushed principal to compare with (§9).                                                                                                                                                                                                                                                                                                 |
| 33  | **Step 2**: `remote_message(body, secrets)` takes no `status`.                                                     | —                       | The status had no use in cleaning the text; the caller keeps it in `ApiError::Status`, so the slice 1 signature lost only an unused argument.                                                                                                                                                                                                                                                                                                |
| 34  | **Step 2**: `ApiError::message(service)` gives the person-facing text instead of `Display`.                        | —                       | The fixed messages name the remote ("Could not reach Jira"); the enum keeps slice 1's four variants without a service field, and slice 3 maps the same variants to `ActionError` with the same text.                                                                                                                                                                                                                                         |
| 35  | **Step 2**: one internal `RemoteClient` (`core/src/provider/remote_api/`) under `JiraApi` and `GitLabApi`.         | —                       | Origin pinning, the redirect policy, the bounded error body, the single retry and the identity mapping are written once instead of in both clients; it is crate-private, and the two clients keep slice 1's names and `endpoint` shape.                                                                                                                                                                                                      |

---

## Open risks

- **Tauri inlines each event payload into a script**: a one-year log per activity event. The 250 ms spacing bounds the rate, not the size; step 6a measures size, latency and queue growth.
- **Memory**: the store and the projected log both hold entries; R-C4's benchmark covers it.
- **Network file systems** may implement no or advisory-only locks; contention then cannot be detected, though the atomic replace still prevents torn files.
- **Writers outside the protocol** (a text editor on the real file, a script), and an editor session that keeps an `instance` line while recreating a table, can make a replacement undetectable; outside writers also bypass the lock. `config edit` is inside the protocol; README says to use it.
- **Private-file guarantees depend on the platform recipes**: macOS and Linux are tested in `pnpm verify` and CI; Windows only by the step 1 device check on `infra/windows-vm` until Windows CI exists (step 13).
- **Behaviour changes users can see**: `config edit` no longer creates an empty file before the editor opens (it is created when the user saves something); a file the app writes becomes owner-only, replacing any custom ACL or shared mode; locations that cannot hold private files (FAT, exFAT, some network shares) refuse to save settings or write cache files.
- **The desktop snapshot registry** keeps 16 revisions; a pane open across more saves elsewhere gets a conflict on save and must reload.
- **Startup produces one `authority` boundary per Jira and GitLab connection** as their identities are first verified, each sending a pair with the early log.
- **A connection whose checks keep failing without an identity collects uncached**, costing remote requests.
- **The disk cache is per principal, not per token** (stated guarantee, §4).
- **macOS menu accelerators and WKWebView**: the per-chord skip and the restriction to chords without text meaning keep dispatch single; device checks confirm focus behaviour.
- **HTTP/1.1 connection limit** for many browser tabs (documented).
- **Hand-written deep links** with a raw `#` or `+` lose data (documented).

## Steps and the tests that prove each contract

| Step    | Contract parts                                                                                         | Proving tests                                            |
| ------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| 1       | §1 private files, write protocol, instances, preconditions, `config edit`, B2, B9, B10, location       | §1 tests                                                 |
| 2       | §4 `RemoteIdentity` and `IdentityEvidence` from the checks                                             | §4 step 2 tests                                          |
| 3       | §4 cache scope, bypass and writer (R-C5)                                                               | §4 step 3 tests                                          |
| 5       | §2 host, locks, ownership, spawner, installs; §3 report and store; §4 keys; §5 core publishing         | §2, §3, §4 step 5 and §5 core tests                      |
| 6       | §6.3 constants; §7 URL module without `item`, `event` round trip, B57                                  | §6.3 and §7 tests except slice 9's                       |
| 6a      | §5 route, events, command, DTOs, receipt, transports, coordinator                                      | §5 api, desktop and dashboard tests                      |
| 7       | §6.1 field issues, §6.2 `recheckable` and meeting-ness, §1 snapshot `revision`/`basedOn`, maximize DTO | §6 tests, §1 desktop precondition tests                  |
| 10      | §8 registry, layers and service                                                                        | §8 tests                                                 |
| 10a     | §8 menu items                                                                                          | §8 menu tests, NF-6 device checks                        |
| 12      | §2 spawner used by NF-5                                                                                | NF-5 tests (merged plan §3.2)                            |
| slice 3 | §9 `AccessContext`, recorded checks, checking capabilities                                             | slice 3 tests plus the §9 requirements                   |
| slice 9 | §7 `item` and Work availability, §8 `work.search`, §5 Work keys and invalidation                       | §7 slice 9 tests, §8 ⌘F tests, §5 coordinator Work cases |

AGENTS.md changes with each step: the settings section gains the write protocol and `config edit`'s copy (step 1); the runtime section gains the host, its two locks, run reports and publishing, and `api/src/reload/` leaves the API section (step 5); the API routes gain `GET /api/events` and the Desktop section the three events and `get_runtime_state` (step 6a).
