use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use chrono::{TimeZone, Utc};
use futures::{FutureExt, future::BoxFuture};
use tokio::sync::{Semaphore, mpsc, watch};

use super::{
    BadConfig, CheckStarter, Host, HostInner, HostWork, Published, RecheckRefused, RunSpawner,
    Shown, load, planned_collector,
};
use crate::{
    model::{
        ActivityEntry, ActivityState, CollectOptions, ConnectionId, IdentityEvidence, ProviderInfo,
        ProviderProblem, ProviderStatus, RemoteIdentity, RunEnd, RunId, RunOutcome, Source,
        SupportedOs, SystemEntry, SystemEntryType,
    },
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_context::ProviderContext,
        provider_runtime::start_check_with,
    },
    runtime::report::{Outcome, RunReport, Slot, Stored},
    settings::{EditError, Precondition},
};

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn april() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
    }
}

fn jira() -> ConnectionId {
    ConnectionId::new(Source::Jira, "work")
}

fn gitlab() -> ConnectionId {
    ConnectionId::new(Source::Gitlab, "work")
}

/// Sources the tests control. A connection's check names the account `identities` gives it,
/// its collection gives one entry labelled `<name> as <account>` (or fails, for one in
/// `failing`, or is refused by its server, for one in `refusing`), and either waits while a
/// gate is set for it.
#[derive(Default)]
struct World {
    identities: Mutex<BTreeMap<ConnectionId, IdentityEvidence>>,
    failing: Mutex<BTreeSet<ConnectionId>>,
    refusing: Mutex<BTreeSet<ConnectionId>>,
    check_gates: Mutex<BTreeMap<ConnectionId, Arc<Semaphore>>>,
    collect_gates: Mutex<BTreeMap<ConnectionId, Arc<Semaphore>>>,
    /// Each collection of a connection, with its range.
    collected: Mutex<Vec<(ConnectionId, CollectOptions)>>,
    checks: AtomicUsize,
    /// What a connection's collection gives instead of its labelled entry.
    entries: Mutex<BTreeMap<ConnectionId, Vec<ActivityEntry>>>,
}

impl World {
    fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn name(&self, id: &ConnectionId, account: &str) {
        self.identities.lock().unwrap().insert(
            id.clone(),
            IdentityEvidence::Verified(RemoteIdentity {
                id: account.to_string(),
                username: account.to_string(),
            }),
        );
    }

    fn hold_checks(&self, id: &ConnectionId) -> Arc<Semaphore> {
        let gate = Arc::new(Semaphore::new(0));
        self.check_gates
            .lock()
            .unwrap()
            .insert(id.clone(), gate.clone());
        gate
    }

    fn hold_collections(&self, id: &ConnectionId) -> Arc<Semaphore> {
        let gate = Arc::new(Semaphore::new(0));
        self.collect_gates
            .lock()
            .unwrap()
            .insert(id.clone(), gate.clone());
        gate
    }

    fn collected(&self) -> Vec<(ConnectionId, CollectOptions)> {
        self.collected.lock().unwrap().clone()
    }

    fn starter(self: &Arc<Self>) -> CheckStarter {
        let world = self.clone();
        Arc::new(move |connection: &Connection| {
            let adapter = Arc::new(Fake {
                source: connection.source().clone(),
                world: world.clone(),
            });
            Some(start_check_with(adapter, connection))
        })
    }
}

async fn pass(gate: Option<Arc<Semaphore>>) {
    if let Some(gate) = gate {
        gate.acquire().await.unwrap().forget();
    }
}

struct Fake {
    source: Source,
    world: Arc<World>,
}

impl ProviderAdapter for Fake {
    fn id(&self) -> Source {
        self.source.clone()
    }

    fn description(&self) -> &'static str {
        "A source for tests"
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux]
    }

    fn startup_check(
        &self,
        _current_os: SupportedOs,
        context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult> {
        let id = context.connection().unwrap().id.clone();
        Box::pin(async move {
            self.world.checks.fetch_add(1, Ordering::SeqCst);
            let gate = self.world.check_gates.lock().unwrap().get(&id).cloned();
            pass(gate).await;
            let identity = self
                .world
                .identities
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .unwrap_or_default();
            ProviderCheckResult::available().with_identity(identity)
        })
    }

    fn collect(
        &self,
        _current_os: SupportedOs,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>> {
        let id = context.connection().unwrap().id.clone();
        Box::pin(async move {
            self.world
                .collected
                .lock()
                .unwrap()
                .push((id.clone(), options));
            let gate = self.world.collect_gates.lock().unwrap().get(&id).cloned();
            pass(gate).await;
            if self.world.failing.lock().unwrap().contains(&id) {
                anyhow::bail!("unreachable host");
            }
            if self.world.refusing.lock().unwrap().contains(&id) {
                anyhow::bail!(crate::provider::remote_error::RemoteFailure {
                    service: "Jira".into(),
                    error: crate::provider::remote_error::ApiError::Status {
                        status: reqwest::StatusCode::UNAUTHORIZED,
                        message: String::new(),
                        retry_after: None,
                    },
                });
            }
            if let Some(entries) = self.world.entries.lock().unwrap().get(&id) {
                return Ok(entries.clone());
            }
            let account = match self.world.identities.lock().unwrap().get(&id) {
                Some(IdentityEvidence::Verified(identity)) => identity.id.clone(),
                _ => "nobody".to_string(),
            };
            Ok(vec![entry(&format!("{} as {account}", id.name))])
        })
    }
}

fn entry(label: &str) -> ActivityEntry {
    ActivityEntry::System(SystemEntry {
        source: crate::model::SystemTag,
        connection: crate::model::ConnectionName::unstamped(),
        id: label.to_string(),
        entry_type: SystemEntryType::Unlock,
        timestamp: "2026-03-05T09:00:00+00:00".into(),
        label: label.to_string(),
    })
}

fn labels(state: &ActivityState) -> Vec<String> {
    let log = match state {
        ActivityState::Running { log } | ActivityState::Ready { log } => log,
        other => panic!("expected a log, got {other:?}"),
    };
    let mut labels = log
        .entries
        .iter()
        .map(|entry| match entry {
            ActivityEntry::System(entry) => entry.label.clone(),
            other => panic!("unexpected entry {other:?}"),
        })
        .collect::<Vec<_>>();
    labels.sort();
    labels
}

fn connection(id: &ConnectionId, values: &[(&str, &str)]) -> Connection {
    Connection::new(
        id.clone(),
        id.name.clone(),
        ConnectionLocation::Local,
        ConnectionValues::new(
            values
                .iter()
                .map(|(key, value)| (key.to_string(), Some(value.to_string())))
                .collect(),
        ),
    )
}

/// Where spawned work went: each `RunEnd`, and the published revision when the work was
/// handed over.
struct Spawned {
    ends: mpsc::UnboundedReceiver<RunEnd>,
    revisions: Arc<Mutex<Vec<u64>>>,
}

impl Spawned {
    async fn end(&mut self) -> RunEnd {
        self.ends.recv().await.expect("a run end")
    }

    fn no_more_ends(&mut self) {
        assert!(self.ends.try_recv().is_err(), "an unexpected run end");
    }
}

/// A spawner that polls each work once at once, as a run that completes on its first poll
/// would be, and spawns what is left. `published` is the host's, set once it is built.
fn eager_spawner(
    published: Arc<OnceLock<watch::Receiver<Arc<Published>>>>,
) -> (RunSpawner, Spawned) {
    let (sender, ends) = mpsc::unbounded_channel();
    let revisions = Arc::new(Mutex::new(Vec::new()));
    let recorded = revisions.clone();
    let spawner: RunSpawner = Arc::new(move |mut work: HostWork| {
        if let Some(published) = published.get() {
            recorded.lock().unwrap().push(published.borrow().revision());
        }
        let waker = futures::task::noop_waker();
        match work.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(end) => {
                if let Some(end) = end {
                    let _ = sender.send(end);
                }
            }
            Poll::Pending => {
                let sender = sender.clone();
                tokio::spawn(async move {
                    if let Some(end) = work.await {
                        let _ = sender.send(end);
                    }
                });
            }
        }
    });
    (spawner, Spawned { ends, revisions })
}

fn spawner() -> (RunSpawner, Spawned) {
    let (sender, ends) = mpsc::unbounded_channel();
    let spawner: RunSpawner = Arc::new(move |work: HostWork| {
        let sender = sender.clone();
        tokio::spawn(async move {
            if let Some(end) = work.await {
                let _ = sender.send(end);
            }
        });
    });
    (
        spawner,
        Spawned {
            ends,
            revisions: Arc::default(),
        },
    )
}

fn host_of(world: &Arc<World>, connections: &[Connection]) -> (Host, Spawned) {
    let (spawner, spawned) = spawner();
    let host = Host::build(None, world.starter(), planned_collector(), spawner);
    host.install(connections.to_vec(), None);
    (host, spawned)
}

/// A host of the config file at `path` whose sources are `world`'s.
fn host_at(path: &Path, world: &Arc<World>) -> (Host, Spawned) {
    let (spawner, spawned) = spawner();
    let host = Host::build(
        Some(path.to_path_buf()),
        world.starter(),
        planned_collector(),
        spawner,
    );
    let (connections, content) = load(path).ok().unwrap();
    host.install(connections, Some(content));
    (host, spawned)
}

async fn until(host: &Host, done: impl Fn(&Published) -> bool) -> Arc<Published> {
    host.subscribe()
        .wait_for(|published| done(published))
        .await
        .unwrap()
        .clone()
}

fn ready(published: &Published) -> bool {
    matches!(*published.activity.value, ActivityState::Ready { .. })
}

/// Every connection's check is recorded.
fn checked(published: &Published) -> bool {
    published
        .providers
        .value
        .providers
        .iter()
        .all(|info| info.status != ProviderStatus::Checking)
}

fn running(published: &Published) -> bool {
    matches!(*published.activity.value, ActivityState::Running { .. })
}

fn status(host: &Host, id: &ConnectionId) -> ProviderStatus {
    host.providers_response()
        .providers
        .into_iter()
        .find(|info| info.connection == *id)
        .map(|info| info.status)
        .unwrap_or_else(|| panic!("no provider {id}"))
}

fn shown_run(host: &Host) -> Option<RunId> {
    host.lock().shown.run()
}

fn running_range(host: &Host) -> Option<CollectOptions> {
    match &host.lock().shown {
        Shown::Running { options, .. } => Some(*options),
        _ => None,
    }
}

/// A Calendar connection without its URL: its real check fails at once, without I/O.
fn unconfigured_calendar() -> Connection {
    Connection::new(
        ConnectionId::new(Source::Calendar, "team"),
        "Team",
        ConnectionLocation::Local,
        ConnectionValues::default(),
    )
}

fn fixed_git() -> Vec<ProviderInfo> {
    vec![ProviderInfo {
        connection: ConnectionId::new(Source::Git, "default"),
        label: "Git".to_string(),
        wsl_distro: None,
        status: ProviderStatus::Available,
        usable: true,
        supported_os: vec![SupportedOs::Macos],
        reason: None,
        problem: None,
        recheckable: false,
        checked_at: "2026-03-25T10:00:00Z".to_string(),
    }]
}

// B66: each command answers with the state it installed, even when its work completes on its
// first poll, and that state's revision is below the work's own.

#[tokio::test]
async fn collect_answers_running_even_when_the_run_completes_at_once() {
    let published = Arc::new(OnceLock::new());
    let (spawner, mut spawned) = eager_spawner(published.clone());
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    published.set(host.subscribe()).unwrap();

    let answer = host.collect(march()).value;

    let ActivityState::Running { log } = answer else {
        panic!("expected running, got {answer:?}");
    };
    assert_eq!(
        log.meta.pending,
        [ConnectionId::new(Source::Git, "default")]
    );
    let end = spawned.end().await;
    assert_eq!(end.outcome, RunOutcome::Ready { entries: 0 });
    let handed_over_at = spawned.revisions.lock().unwrap()[0];
    let now = host.subscribe().borrow().activity.revision;
    assert!(handed_over_at < now, "{handed_over_at} < {now}");
    assert!(matches!(host.activity(), ActivityState::Ready { .. }));
}

#[tokio::test]
async fn recheck_answers_checking_even_when_the_check_ends_at_once() {
    let published = Arc::new(OnceLock::new());
    let (spawner, spawned) = eager_spawner(published.clone());
    let host = Host::fixed(&[unconfigured_calendar()], spawner);
    published.set(host.subscribe()).unwrap();
    let calendar = ConnectionId::new(Source::Calendar, "team");
    assert_eq!(status(&host, &calendar), ProviderStatus::Unavailable);

    let answer = host
        .recheck(&calendar)
        .expect("a connection of this host")
        .value;

    assert_eq!(answer.providers[0].status, ProviderStatus::Checking);
    assert_eq!(status(&host, &calendar), ProviderStatus::Unavailable);
    let handed_over_at = last_handed_over(&spawned);
    assert!(handed_over_at < host.subscribe().borrow().providers.revision);
    assert!(
        host.recheck(&ConnectionId::new(Source::Calendar, "other"))
            .err()
            == Some(RecheckRefused::UnknownConnection)
    );
}

/// A connection the host still lists is not missing once the host shut down: it only cannot
/// be checked any more.
#[tokio::test]
async fn a_recheck_after_shutdown_is_refused_as_such_not_as_unknown() {
    let host = Host::fixed(
        &[unconfigured_calendar()],
        eager_spawner(Default::default()).0,
    );
    let calendar = ConnectionId::new(Source::Calendar, "team");
    host.shutdown();

    assert!(host.recheck(&calendar).err() == Some(RecheckRefused::ShutDown));
    assert!(
        host.providers_response()
            .providers
            .iter()
            .any(|info| info.connection == calendar)
    );
}

fn last_handed_over(spawned: &Spawned) -> u64 {
    *spawned.revisions.lock().unwrap().last().unwrap()
}

#[tokio::test]
async fn an_edit_answers_its_new_connections_checking_even_when_their_checks_end_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let published = Arc::new(OnceLock::new());
    let (spawner, spawned) = eager_spawner(published.clone());
    let host = Host::start(Some(path), spawner, BadConfig::Fail).unwrap();
    published.set(host.subscribe()).unwrap();

    let edit = host
        .edit(Precondition::None, |file| {
            file.add(&Source::Calendar, Some("Team"), ConnectionLocation::Local)
        })
        .await
        .unwrap();

    assert_eq!(edit.providers.providers.len(), 1);
    assert_eq!(edit.providers.providers[0].status, ProviderStatus::Checking);
    assert_eq!(status(&host, &edit.value), ProviderStatus::Unavailable);
    let handed_over_at = last_handed_over(&spawned);
    let installed = edit.installed.expect("installed");
    assert_eq!(handed_over_at, installed.revision);
    assert!(installed.revision < host.subscribe().borrow().providers.revision);
}

// B14

#[tokio::test]
async fn a_recheck_while_another_range_runs_restarts_that_range() {
    let world = World::shared();
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    host.collect(march());
    spawned.end().await;
    let gate = world.hold_collections(&jira());
    host.collect(april());

    host.recheck(&jira()).unwrap();

    assert_eq!(running_range(&host), Some(april()));
    gate.add_permits(2);
    let end = spawned.end().await;
    assert_eq!(end.outcome, RunOutcome::Stopped, "the first April run");
    let end = spawned.end().await;
    assert!(matches!(end.outcome, RunOutcome::Ready { .. }));
    let ranges = world
        .collected()
        .into_iter()
        .map(|(_, range)| range)
        .collect::<Vec<_>>();
    assert!(
        ranges[1..].iter().all(|range| *range == april()),
        "{ranges:?}"
    );
}

/// The collect and the recheck race from two threads; whichever is installed first, April is
/// what runs afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recheck_racing_a_collect_of_another_range_never_restarts_the_old_one() {
    let world = World::shared();
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    for _ in 0..50 {
        // March is on screen, done.
        world.collect_gates.lock().unwrap().clear();
        host.collect(march());
        until(&host, ready).await;
        world.hold_collections(&jira());

        let barrier = Arc::new(std::sync::Barrier::new(2));
        let collect = std::thread::spawn({
            let (host, barrier) = (host.clone(), barrier.clone());
            let runtime = tokio::runtime::Handle::current();
            move || {
                let _entered = runtime.enter();
                barrier.wait();
                host.collect(april());
            }
        });
        let runtime = tokio::runtime::Handle::current();
        let recheck = std::thread::spawn({
            let host = host.clone();
            move || {
                let _entered = runtime.enter();
                barrier.wait();
                assert!(host.recheck(&jira()).is_ok());
            }
        });
        collect.join().unwrap();
        recheck.join().unwrap();

        assert_eq!(running_range(&host), Some(april()));
    }
    while spawned.ends.try_recv().is_ok() {}
}

// B15

#[tokio::test]
async fn a_superseded_run_marks_nothing_degraded_and_shows_nothing() {
    let world = World::shared();
    world.failing.lock().unwrap().insert(jira());
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    until(&host, checked).await;
    let gate = world.hold_collections(&jira());
    host.collect(march());
    let superseded = shown_run(&host).unwrap();
    host.collect(april());
    let shown = host.subscribe().borrow().clone();

    // What the first run would install if it finished after all (it was polled on another
    // thread while it was stopped).
    let key = host.lock().keys.key(&jira()).unwrap();
    let failed = RunReport {
        slots: vec![(
            jira(),
            Slot::Settled(Outcome::Failed {
                key,
                failure: crate::runtime::CollectFailure::of(&anyhow::anyhow!("unreachable host")),
            }),
        )],
    };
    host.progress(superseded, march(), failed.clone());
    let outcome = host.finish(superseded, march(), Ok(failed));

    assert_eq!(outcome, RunOutcome::Stopped);
    assert_eq!(status(&host, &jira()), ProviderStatus::Available);
    assert_eq!(host.subscribe().borrow().revision(), shown.revision());
    assert!(!host.is_current(superseded));

    // The run on screen still marks its own failure.
    gate.add_permits(2);
    loop {
        if matches!(spawned.end().await.outcome, RunOutcome::Ready { .. }) {
            break;
        }
    }
    assert_eq!(status(&host, &jira()), ProviderStatus::Degraded);
}

// Run ids and run ends

#[tokio::test]
async fn every_run_ends_once_and_only_runs_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[[jira]]\nlabel = \"Work\"\nurl = \"https://jira.example.org\"\n",
    )
    .unwrap();
    let world = World::shared();
    let (host, mut spawned) = host_at(&path, &world);
    // Startup checks end no run.
    until(&host, |published| {
        published.providers.value.providers[0].status == ProviderStatus::Available
    })
    .await;
    spawned.no_more_ends();
    // A recheck with no collection on screen ends none either.
    host.recheck(&jira()).unwrap();
    until(&host, |published| {
        published.providers.value.providers[0].status == ProviderStatus::Available
    })
    .await;
    spawned.no_more_ends();

    let mut runs = Vec::new();
    host.collect(march());
    // A join hands over nothing.
    host.collect(march());
    runs.push(spawned.end().await);
    host.recheck(&jira()).unwrap();
    runs.push(spawned.end().await);
    host.edit(Precondition::None, |file| {
        file.set(
            &jira(),
            crate::provider::jira::JIRA_URL,
            Some("https://jira2.example.org"),
        )
    })
    .await
    .unwrap();
    runs.push(spawned.end().await);
    std::fs::write(
        &path,
        "[[jira]]\nlabel = \"Work\"\nurl = \"https://jira3.example.org\"\n",
    )
    .unwrap();
    host.reload_if_changed().await;
    runs.push(spawned.end().await);

    spawned.no_more_ends();
    assert!(
        runs.iter()
            .all(|end| matches!(end.outcome, RunOutcome::Ready { entries: 1 }))
    );
    let ids = runs.iter().map(|end| end.run).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 4, "{runs:?}");
    assert!(host.is_current(runs[3].run));
    assert!(!host.is_current(runs[2].run));
}

#[tokio::test]
async fn a_stopped_run_ends_stopped() {
    let world = World::shared();
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    let gate = world.hold_collections(&jira());
    host.collect(march());
    host.collect(april());
    gate.add_permits(2);

    let first = spawned.end().await;
    let second = spawned.end().await;

    assert_eq!(first.outcome, RunOutcome::Stopped);
    assert!(matches!(second.outcome, RunOutcome::Ready { .. }));
    assert!(first.run < second.run);
}

/// The work of a transition reaches the spawner once both locks are released: a spawner that
/// takes them does not wait. Each run's end arrives once it shows.
#[tokio::test]
async fn work_is_handed_over_with_no_lock_held_and_ends_after_it_shows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let inner = Arc::new(OnceLock::<Weak<HostInner>>::new());
    let (sender, mut ends) = mpsc::unbounded_channel();
    let spawner: RunSpawner = {
        let inner = inner.clone();
        Arc::new(move |work: HostWork| {
            if let Some(inner) = inner.get().and_then(Weak::upgrade) {
                assert!(inner.state.try_lock().is_ok(), "the state lock is held");
                assert!(inner.config.try_lock().is_ok(), "the config lock is held");
            }
            let (sender, inner) = (sender.clone(), inner.clone());
            tokio::spawn(async move {
                if let Some(end) = work.await {
                    let inner = inner.get().and_then(Weak::upgrade).unwrap();
                    let host = Host { inner };
                    assert!(host.is_current(end.run));
                    assert!(matches!(host.activity(), ActivityState::Ready { .. }));
                    let _ = sender.send(end);
                }
            });
        })
    };
    let world = World::shared();
    let host = Host::build(
        Some(path.clone()),
        world.starter(),
        planned_collector(),
        spawner,
    );
    inner.set(Arc::downgrade(&host.inner)).unwrap();
    host.install(Vec::new(), None);

    host.edit(Precondition::None, |file| {
        file.add(&Source::Jira, Some("Work"), ConnectionLocation::Local)
    })
    .await
    .unwrap();
    host.collect(march());
    ends.recv().await.unwrap();
    std::fs::write(
        &path,
        "[[jira]]\nlabel = \"Work\"\nurl = \"https://jira.example.org\"\n",
    )
    .unwrap();
    host.reload_if_changed().await;
    ends.recv().await.unwrap();
}

// Edits

#[tokio::test]
async fn an_edit_whose_caller_went_away_is_still_installed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path), spawner, BadConfig::Fail).unwrap();
    let (started, has_started) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();

    let mut edit = Box::pin(host.edit(Precondition::None, move |file| {
        let _ = started.send(());
        released.recv().unwrap();
        file.add(&Source::Calendar, Some("Team"), ConnectionLocation::Local)
    }));
    assert!(futures::poll!(&mut edit).is_pending());
    has_started.await.unwrap();
    drop(edit);
    release.send(()).unwrap();

    until(&host, |published| {
        published.providers.value.providers.len() == 1
    })
    .await;
}

#[tokio::test]
async fn a_save_that_writes_nothing_installs_what_another_writer_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();

    let edit = host.edit(Precondition::None, |_| Ok(())).await.unwrap();

    assert_eq!(edit.providers.providers.len(), 1);
    let installed = edit.installed.expect("installed");
    assert_eq!(
        installed.revision,
        host.subscribe()
            .borrow()
            .providers
            .revision
            .min(installed.revision)
    );

    let current = host.subscribe().borrow().providers.revision;
    let unchanged = host.edit(Precondition::None, |_| Ok(())).await.unwrap();
    assert_eq!(unchanged.installed.expect("current").revision, current);
    assert_eq!(host.subscribe().borrow().providers.revision, current);
}

#[tokio::test]
async fn an_edit_without_a_settings_location_fails() {
    let (spawner, _spawned) = spawner();
    let host = Host::start(None, spawner, BadConfig::Fail).unwrap();

    let refused = host.edit(Precondition::None, |_| Ok(())).await;

    assert!(matches!(refused, Err(EditError::NoLocation)));
}

// B20: affected kinds

const TWO_SOURCES: &str = "[[jira]]\nlabel = \"Work\"\nurl = \"https://jira.example.org\"\ntoken = \"a\"\n\n[[gitlab]]\nlabel = \"Work\"\nurl = \"https://gitlab.example.org\"\ntoken = \"a\"\n";

async fn ready_at(text: &str, world: &Arc<World>) -> (tempfile::TempDir, PathBuf, Host, Spawned) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).unwrap();
    let (host, mut spawned) = host_at(&path, world);
    host.collect(march());
    spawned.end().await;
    (dir, path, host, spawned)
}

fn collected_since(world: &World, from: usize) -> Vec<ConnectionId> {
    world.collected()[from..]
        .iter()
        .map(|(id, _)| id.clone())
        .collect()
}

#[tokio::test]
async fn a_label_rename_starts_no_run_and_keeps_a_degraded_source_degraded() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.name(&gitlab(), "a");
    world.failing.lock().unwrap().insert(gitlab());
    let (_dir, _path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;
    assert_eq!(status(&host, &gitlab()), ProviderStatus::Degraded);
    let shown = host.subscribe().borrow().activity.clone();
    let checks = world.checks.load(Ordering::SeqCst);

    host.edit(Precondition::None, |file| file.rename(&jira(), "Renamed"))
        .await
        .unwrap();
    // The fresh checks are recorded with the same accounts.
    until(&host, checked).await;

    assert_eq!(world.checks.load(Ordering::SeqCst), checks + 2);
    spawned.no_more_ends();
    assert!(Arc::ptr_eq(
        &host.subscribe().borrow().activity.value,
        &shown.value
    ));
    assert_eq!(status(&host, &gitlab()), ProviderStatus::Degraded);
    let renamed = host.providers_response().providers;
    assert!(
        renamed
            .iter()
            .any(|info| info.connection == jira() && info.label == "Renamed")
    );
}

#[tokio::test]
async fn a_gitlab_url_change_collects_gitlab_alone_again() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.name(&gitlab(), "a");
    let (_dir, _path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;
    let before = world.collected().len();

    host.edit(Precondition::None, |file| {
        file.set(
            &gitlab(),
            crate::provider::gitlab::GITLAB_URL,
            Some("https://gitlab2.example.org"),
        )
    })
    .await
    .unwrap();
    spawned.end().await;

    assert_eq!(collected_since(&world, before), [gitlab()]);
    assert_eq!(labels(&host.activity()), ["work as a", "work as a"]);
}

#[tokio::test]
async fn an_outside_edit_of_jira_and_gitlab_collects_both_again() {
    let world = World::shared();
    let (_dir, path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;
    let before = world.collected().len();

    std::fs::write(&path, TWO_SOURCES.replace("token = \"a\"", "token = \"b\"")).unwrap();
    host.reload_if_changed().await;
    spawned.end().await;

    let collected = collected_since(&world, before)
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(collected, BTreeSet::from([jira(), gitlab()]));
}

#[tokio::test]
async fn a_label_edit_while_a_run_waits_for_a_check_runs_whole_on_the_new_config() {
    let world = World::shared();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, TWO_SOURCES).unwrap();
    let gate = world.hold_checks(&jira());
    let (host, mut spawned) = host_at(&path, &world);
    host.collect(march());
    let waiting = shown_run(&host).unwrap();

    host.edit(Precondition::None, |file| file.rename(&jira(), "Renamed"))
        .await
        .unwrap();
    assert!(!host.is_current(waiting));
    gate.add_permits(2);

    let stopped = spawned.end().await;
    assert_eq!(
        (stopped.run, stopped.outcome),
        (waiting, RunOutcome::Stopped)
    );
    let whole = spawned.end().await;
    assert_eq!(whole.outcome, RunOutcome::Ready { entries: 2 });
    let ids = world
        .collected()
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(ids.iter().filter(|id| **id == jira()).count(), 1, "{ids:?}");
}

#[tokio::test]
async fn a_label_edit_after_a_failed_run_runs_whole() {
    let collector: super::Collector = Arc::new(|_, _, _| async { panic!("no way") }.boxed());
    let world = World::shared();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, TWO_SOURCES).unwrap();
    let (spawner, mut spawned) = spawner();
    let host = Host::build(Some(path.clone()), world.starter(), collector, spawner);
    let (connections, content) = load(&path).ok().unwrap();
    host.install(connections, Some(content));
    host.collect(march());
    assert_eq!(spawned.end().await.outcome, RunOutcome::Failed);

    host.edit(Precondition::None, |file| file.rename(&jira(), "Renamed"))
        .await
        .unwrap();

    assert_eq!(spawned.end().await.outcome, RunOutcome::Failed);
}

/// Every Git connection has its outcome and none changed: only the order and the list tell
/// whether Git is affected.
#[test]
fn reordering_or_removing_connections_affects_their_kind() {
    let git = |name: &str| ConnectionId::new(Source::Git, name);
    let mut keys = crate::runtime::keys::KeyTable::default();
    keys.install(&[connection(&git("a"), &[]), connection(&git("b"), &[])]);
    let outcome = |id: ConnectionId| {
        let key = keys.key(&id).unwrap();
        (
            id,
            Slot::Settled(Outcome::Collected(Stored {
                key,
                entries: Vec::new().into(),
            })),
        )
    };
    let store = super::ResultStore::new(
        march(),
        RunReport {
            slots: vec![outcome(git("a")), outcome(git("b"))],
        },
    );
    let none = BTreeSet::new();
    let affected = |old: &[ConnectionId], new: &[ConnectionId]| {
        super::affected_kinds(old, new, &none, Some(&store))
    };

    assert!(affected(&[git("a"), git("b")], &[git("a"), git("b")]).is_empty());
    assert_eq!(
        affected(&[git("a"), git("b")], &[git("b"), git("a")]),
        BTreeSet::from([Source::Git])
    );
    assert_eq!(
        affected(&[git("a"), git("b")], &[git("a")]),
        BTreeSet::from([Source::Git])
    );
}

/// Two clones of one repository: the commit belongs to the first Git table, also after the
/// tables are reversed and after the first one is removed.
#[tokio::test]
async fn a_shared_commit_follows_the_first_git_table() {
    let original = crate::tests::repo(Some("me@example.com"), &[("me@example.com", "Shared")]);
    let parent = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        crate::tests::git(
            parent.path(),
            &["clone", "-q", &original.path().display().to_string(), name],
        );
        crate::tests::git(
            &parent.path().join(name),
            &["config", "user.email", "me@example.com"],
        );
    }
    let table = |name: &str| {
        let dir = parent.path().join(if name == "a" { "one" } else { "two" });
        format!(
            "[[git]]\nname = \"{name}\"\ndirs = [{}]\n",
            serde_json::to_string(&dir.display().to_string()).unwrap()
        )
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, table("a") + &table("b")).unwrap();
    let (spawner, mut spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    host.collect(march());
    spawned.end().await;
    let owner = || {
        let ActivityState::Ready { log } = host.activity() else {
            panic!("not ready: {:?}", host.activity());
        };
        assert_eq!(log.entries.len(), 1, "{:?}", log.entries);
        log.entries[0].connection().unwrap().to_owned()
    };
    assert_eq!(owner(), "a");

    for (text, first) in [(table("b") + &table("a"), "b"), (table("b"), "b")] {
        std::fs::write(&path, text).unwrap();
        host.reload_if_changed().await;
        assert!(
            running(&host.subscribe().borrow()),
            "Git is not collected again"
        );
        assert!(matches!(
            spawned.end().await.outcome,
            RunOutcome::Ready { .. }
        ));
        assert_eq!(owner(), first);
    }
}

// B4: what was collected for one account never shows for another

#[tokio::test]
async fn a_token_edit_shows_nothing_of_the_old_account() {
    let world = World::shared();
    world.name(&jira(), "a");
    let (_dir, _path, host, _spawned) = ready_at(
        "[[jira]]\nlabel = \"Work\"\nurl = \"https://jira.example.org\"\ntoken = \"a\"\n",
        &world,
    )
    .await;
    assert_eq!(labels(&host.activity()), ["work as a"]);
    world.name(&jira(), "b");
    world.hold_collections(&jira());

    host.edit(Precondition::None, |file| {
        file.set(&jira(), crate::provider::jira::JIRA_TOKEN, Some("b"))
    })
    .await
    .unwrap();

    assert!(running(&host.subscribe().borrow()));
    assert!(labels(&host.activity()).is_empty());
}

#[tokio::test]
async fn another_account_behind_the_same_settings_drops_the_old_ones_entries() {
    let world = World::shared();
    world.name(&jira(), "a");
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    host.collect(march());
    spawned.end().await;
    let check = world.hold_checks(&jira());
    let collection = world.hold_collections(&jira());
    world.name(&jira(), "b");
    let old_key = host.lock().keys.key(&jira()).unwrap();

    host.recheck(&jira()).unwrap();
    // Until the check says whose token it is, the shown entries stay.
    assert_eq!(labels(&host.activity()), ["work as a"]);
    let mut published = host.subscribe();
    check.add_permits(1);
    published
        .wait_for(|published| {
            published.providers.value.providers[0].status == ProviderStatus::Available
        })
        .await
        .unwrap();

    assert!(labels(&host.activity()).is_empty());

    // A progress report that still carries A's entry, as the run built it before the check
    // was recorded, shows none of it.
    let run = shown_run(&host).unwrap();
    let stale = Stored {
        key: old_key,
        entries: vec![entry("work as a")].into(),
    };
    host.progress(
        run,
        march(),
        RunReport {
            slots: vec![(
                jira(),
                Slot::Pending {
                    carried: Some(stale),
                },
            )],
        },
    );
    assert!(labels(&host.activity()).is_empty());

    collection.add_permits(1);
    spawned.end().await;
    assert_eq!(labels(&host.activity()), ["work as b"]);
}

/// A check that finds another account changes what the connection's entries belong to: the
/// checked provider and the eviction of the old account's entries are one boundary, a new
/// authority, so a subscriber gets them as one pair and never the provider first (§5).
#[tokio::test]
async fn another_account_reaches_a_subscriber_as_one_pair_with_the_eviction() {
    let world = World::shared();
    world.name(&jira(), "a");
    let (host, mut spawned) = host_of(&world, &[connection(&jira(), &[])]);
    host.collect(march());
    spawned.end().await;
    let check = world.hold_checks(&jira());
    let _collection = world.hold_collections(&jira());
    world.name(&jira(), "b");
    let mut forwarder = crate::runtime::Forwarder::default();
    let before = host.subscribe().borrow().clone();
    forwarder.delivered_pair(&before);

    host.recheck(&jira()).unwrap();
    let mut published = host.subscribe();
    let mut seen = vec![published.borrow_and_update().clone()];
    check.add_permits(1);
    while seen.last().unwrap().providers.value.providers[0].status != ProviderStatus::Available {
        published.changed().await.unwrap();
        seen.push(published.borrow_and_update().clone());
    }

    let checked = seen.last().unwrap();
    assert!(checked.authority > before.authority, "a new authority");
    assert!(
        labels(&checked.activity.value).is_empty(),
        "the eviction with it"
    );
    for value in &seen {
        let available = value.providers.value.providers[0].status == ProviderStatus::Available;
        let messages = forwarder.take(value);
        if available {
            let names = messages
                .iter()
                .map(|message| message.event())
                .collect::<Vec<_>>();
            assert_eq!(names, ["state"], "the checked provider comes as a pair");
        }
    }
}

#[tokio::test]
async fn a_label_edit_whose_check_finds_another_account_collects_the_kind_again() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.name(&gitlab(), "a");
    let (_dir, _path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;
    world.name(&jira(), "b");
    let collection = world.hold_collections(&jira());

    host.edit(Precondition::None, |file| file.rename(&jira(), "Renamed"))
        .await
        .unwrap();
    until(&host, running).await;

    assert_eq!(
        labels(&host.activity()),
        ["work as a"],
        "only GitLab's stays"
    );
    collection.add_permits(1);
    spawned.end().await;
    assert_eq!(labels(&host.activity()), ["work as a", "work as b"]);
}

// B21: the config file is watched

#[tokio::test(start_paused = true)]
async fn an_outside_edit_is_installed_within_one_look() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::StartEmpty).unwrap();
    let watcher = tokio::spawn(host.watch_config());
    let started = tokio::time::Instant::now();

    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    until(&host, |published| {
        published.providers.value.providers.len() == 1
    })
    .await;

    assert!(started.elapsed() <= super::RELOAD_INTERVAL + super::SETTLE);
    host.shutdown();
    watcher.await.unwrap();
}

/// The file is truncated, then written in full after the reload's first read: the empty
/// document is never installed, and the complete one is on the next look.
#[tokio::test(start_paused = true)]
async fn a_change_still_being_written_is_installed_once_complete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    let read = super::Pause::new();
    host.set_pauses(super::Pauses {
        after_first_read: Some(read.clone()),
        ..Default::default()
    });

    std::fs::write(&path, "").unwrap();
    let reload = tokio::spawn({
        let host = host.clone();
        async move { host.reload_if_changed().await }
    });
    read.reached.acquire().await.unwrap().forget();
    std::fs::write(
        &path,
        "[[calendar]]\nlabel = \"Team\"\n\n[[calendar]]\nlabel = \"Home\"\n",
    )
    .unwrap();
    read.resume.add_permits(1);
    reload.await.unwrap();
    assert_eq!(
        host.providers_response().providers.len(),
        1,
        "the empty file was not installed"
    );

    host.set_pauses(super::Pauses::default());
    host.reload_if_changed().await;
    assert_eq!(host.providers_response().providers.len(), 2);
}

/// Another writer saves between an edit's write and its install: the edit installs first and
/// the reload then installs the newer file, so the host ends with what is on disk.
#[tokio::test]
async fn an_edit_and_a_reload_install_in_the_order_the_file_was_written() {
    let world = World::shared();
    let (_dir, path, host, _spawned) = ready_at(TWO_SOURCES, &world).await;
    let written = super::Pause::new();
    host.set_pauses(super::Pauses {
        after_edit_write: Some(written.clone()),
        ..Default::default()
    });
    let edit = tokio::spawn({
        let host = host.clone();
        async move {
            host.edit(Precondition::None, |file| file.rename(&jira(), "Local"))
                .await
                .unwrap()
        }
    });
    written.reached.acquire().await.unwrap().forget();
    crate::settings::edit_config(&path, Precondition::None, |file| {
        file.rename(&jira(), "Outside")
    })
    .unwrap();
    let reload = tokio::spawn({
        let host = host.clone();
        async move { host.reload_if_changed().await }
    });
    // While the edit holds the config lock the reload waits for it; if it could get the lock,
    // let it finish first, the order that would install the edit's older file last.
    if host.inner.config.try_lock().is_ok() {
        reload.await.unwrap();
        written.resume.add_permits(1);
        edit.await.unwrap();
    } else {
        written.resume.add_permits(1);
        edit.await.unwrap();
        reload.await.unwrap();
    }

    let (_, on_disk) = load(&path).ok().unwrap();
    assert_eq!(host.lock().content, Some(on_disk));
    let jira_label = host
        .providers_response()
        .providers
        .into_iter()
        .find(|info| info.connection == jira())
        .unwrap()
        .label;
    assert_eq!(jira_label, "Outside");
}

fn logged(text: &str) -> usize {
    crate::diagnostics::entries()
        .iter()
        .filter(|entry| entry.message.contains(text))
        .count()
}

#[tokio::test(start_paused = true)]
async fn a_file_that_does_not_parse_is_logged_once_and_the_last_good_config_stays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    let shown = path.display().to_string();

    for bytes in [&b"[[calendar]]\nlabel = \"\xff\"\n"[..], b"[[calendar]\n"] {
        std::fs::write(&path, bytes).unwrap();
        let before = logged(&shown);
        for _ in 0..3 {
            host.reload_if_changed().await;
        }
        assert_eq!(logged(&shown), before + 1);
        assert_eq!(host.providers_response().providers.len(), 1);
    }
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn a_file_that_cannot_be_read_is_tried_again_and_installed_once_it_can() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&path).is_ok() {
        // Running as root: nothing to test.
        return;
    }
    let shown = format!("Could not read {}", path.display());

    host.reload_if_changed().await;
    host.reload_if_changed().await;
    assert_eq!(logged(&shown), 1);
    assert!(host.providers_response().providers.is_empty());

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    host.reload_if_changed().await;
    assert_eq!(host.providers_response().providers.len(), 1);
}

// Startup and shutdown

#[tokio::test]
async fn a_file_that_does_not_parse_fails_the_server_and_starts_the_app_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\ntoken = \"glpat-STARTUP-SECRET\n").unwrap();

    let (failing, _spawned) = spawner();
    let message = format!(
        "{:#}",
        Host::start(Some(path.clone()), failing, BadConfig::Fail)
            .err()
            .unwrap()
    );
    assert!(message.contains("line 2, column"), "{message}");
    assert!(!message.contains("STARTUP-SECRET"), "{message}");

    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::StartEmpty).unwrap();
    assert!(host.providers_response().providers.is_empty());
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    host.reload_if_changed().await;
    assert_eq!(host.providers_response().providers.len(), 1);
}

#[tokio::test]
async fn after_shutdown_nothing_holds_the_spawner_or_watches_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let probe = Arc::new(());
    let held = Arc::downgrade(&probe);
    let spawner: RunSpawner = Arc::new(move |work| {
        let _probe = &probe;
        tokio::spawn(work);
    });
    let host = Host::start(Some(path), spawner, BadConfig::Fail).unwrap();
    let watcher = tokio::spawn(host.watch_config());

    host.shutdown();
    watcher.await.unwrap();
    let host_held = Arc::downgrade(&host.inner);
    drop(host);

    assert!(held.upgrade().is_none());
    assert!(host_held.upgrade().is_none());
}

#[tokio::test]
async fn a_report_of_the_kept_kind_keeps_its_stored_entries() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.name(&gitlab(), "a");
    let (_dir, _path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;

    host.recheck(&gitlab()).unwrap();
    spawned.end().await;

    let kept = host
        .lock()
        .shown
        .store()
        .and_then(|store| store.outcome(&jira()).cloned());
    assert!(matches!(
        kept,
        Some(Outcome::Collected(Stored { entries, .. })) if entries.len() == 1
    ));
    assert_eq!(collected_since(&world, 2), [gitlab()]);
}

// B19 through the producer: a scoped recheck of either kind keeps both pushes of one commit.

#[tokio::test]
async fn both_pushes_of_one_commit_survive_a_recheck_of_either_kind() {
    use crate::model::{GitEntry, GitLabEntry, GitLabType};
    let git = ConnectionId::new(Source::Git, "laptop");
    let mut commit = GitEntry::new(
        "repo".into(),
        "abc123".into(),
        "work".into(),
        "me@example.com".into(),
        Some("main".into()),
        "2026-03-05T09:00:00Z".into(),
    );
    commit.connection = "laptop".into();
    let push = |id: &str, at: &str| {
        ActivityEntry::GitLab(GitLabEntry {
            source: crate::model::GitLabTag,
            connection: "work".into(),
            id: id.into(),
            entry_type: GitLabType::PushedTo,
            timestamp: at.into(),
            title: Some("repo".into()),
            target_type: Some("Project".into()),
            mr_iid: None,
            issue_iid: None,
            branch: Some("main".into()),
            commit_count: Some(1),
            note_body: None,
            note_file: None,
            note_resolved: None,
            pushed_commit: Some("abc123".into()),
        })
    };
    let world = World::shared();
    world
        .entries
        .lock()
        .unwrap()
        .insert(git.clone(), vec![ActivityEntry::Git(commit)]);
    world.entries.lock().unwrap().insert(
        gitlab(),
        vec![
            push("first", "2026-03-05T10:00:00Z"),
            push("second", "2026-03-05T11:00:00Z"),
        ],
    );
    let (host, mut spawned) = host_of(&world, &[connection(&git, &[]), connection(&gitlab(), &[])]);
    let pushes = |host: &Host| {
        let (ActivityState::Running { log } | ActivityState::Ready { log }) = host.activity()
        else {
            panic!("no log");
        };
        let mut shown = log
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ActivityEntry::Git(commit) => commit
                    .gitlab_event
                    .as_ref()
                    .map(|push| format!("folded {}", push.id)),
                ActivityEntry::GitLab(push) => Some(format!("visible {}", push.id)),
                _ => None,
            })
            .collect::<Vec<_>>();
        shown.sort();
        shown
    };
    host.collect(march());
    spawned.end().await;
    let both = ["folded first", "visible second"];
    assert_eq!(pushes(&host), both);

    for id in [gitlab(), git] {
        host.recheck(&id).unwrap();
        assert_eq!(pushes(&host), both, "while {id} is collected again");
        spawned.end().await;
        assert_eq!(pushes(&host), both, "after {id} is collected again");
    }
}

// Accounts and failures

/// A collection the server refuses shows as a refusal, not as any failure, and can be checked
/// again; another failure shows as a failed collection.
#[tokio::test]
async fn a_failed_collection_shows_how_it_failed() {
    let world = World::shared();
    world.refusing.lock().unwrap().insert(jira());
    world.failing.lock().unwrap().insert(gitlab());
    let (_dir, _path, host, _spawned) = ready_at(TWO_SOURCES, &world).await;

    let info = |id: &ConnectionId| {
        host.providers_response()
            .providers
            .into_iter()
            .find(|info| info.connection == *id)
            .unwrap()
    };
    let refused = info(&jira());
    assert_eq!(refused.status, ProviderStatus::Degraded);
    assert_eq!(refused.problem, Some(ProviderProblem::Refused));
    assert!(refused.usable && refused.recheckable);
    let failed = info(&gitlab());
    assert_eq!(failed.problem, Some(ProviderProblem::CollectionFailed));
    assert!(failed.recheckable);
}

/// A's failure is not B's: once the check finds another account, the old failure goes with
/// A's entries. A label edit that finds A again keeps it.
#[tokio::test]
async fn a_failure_belongs_to_the_account_that_failed() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.failing.lock().unwrap().insert(jira());
    let (_dir, _path, host, _spawned) = ready_at(TWO_SOURCES, &world).await;
    assert_eq!(status(&host, &jira()), ProviderStatus::Degraded);

    host.edit(Precondition::None, |file| {
        file.rename(&jira(), "Same account")
    })
    .await
    .unwrap();
    until(&host, checked).await;
    assert_eq!(status(&host, &jira()), ProviderStatus::Degraded);

    world.failing.lock().unwrap().clear();
    world.name(&jira(), "b");
    world.hold_collections(&jira());
    host.edit(Precondition::None, |file| {
        file.rename(&jira(), "Other account")
    })
    .await
    .unwrap();
    until(&host, |published| running(published) && checked(published)).await;

    assert_eq!(status(&host, &jira()), ProviderStatus::Available);
}

/// B20 with instances: a connection removed and added again between two looks at the file
/// has the same name, values and location, but it is another connection.
#[tokio::test]
async fn a_connection_removed_and_added_again_is_collected_again() {
    use crate::settings::edit_config;
    let world = World::shared();
    let text = "[[git]]\nname = \"this-computer\"\ninstance = \"0000000000000001\"\n";
    let (_dir, path, host, mut spawned) = ready_at(text, &world).await;
    let id = ConnectionId::new(Source::Git, "this-computer");
    let before = world.collected().len();

    edit_config(&path, Precondition::None, |file| file.remove(&id)).unwrap();
    edit_config(&path, Precondition::None, |file| {
        file.add(&Source::Git, None, ConnectionLocation::Local)
    })
    .unwrap();
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("0000000000000001")
    );
    host.reload_if_changed().await;

    assert!(running(&host.subscribe().borrow()), "no run started");
    spawned.end().await;
    assert_eq!(collected_since(&world, before), [id]);
}

// Shutdown is the last transition: nothing in flight shows or ends well after it.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_in_its_last_poll_at_shutdown_ends_stopped_and_shows_nothing() {
    let (entered, has_entered) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let released = Arc::new(Mutex::new(released));
    let (spawner, mut spawned) = spawner();
    let collector: super::Collector = Arc::new(move |_, _, _| {
        let (entered, released) = (entered.clone(), released.clone());
        Box::pin(async move {
            entered.send(()).unwrap();
            released.lock().unwrap().recv().unwrap();
            RunReport::default()
        })
    });
    let host = Host::with_collector(collector, Vec::new(), spawner);
    host.collect(march());
    let run = shown_run(&host).unwrap();
    has_entered
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let before = host.subscribe().borrow().revision();

    host.shutdown();
    release.send(()).unwrap();

    let end = spawned.end().await;
    assert_eq!(end.outcome, RunOutcome::Stopped);
    assert!(!host.is_current(run));
    assert!(matches!(host.activity(), ActivityState::Running { .. }));
    assert_eq!(host.subscribe().borrow().revision(), before);
}

#[tokio::test]
async fn a_check_that_ends_after_shutdown_publishes_nothing() {
    let world = World::shared();
    let gate = world.hold_checks(&jira());
    let (host, _spawned) = host_of(&world, &[connection(&jira(), &[])]);
    let mut published = host.subscribe();
    published.borrow_and_update();
    // The check runs and waits at its gate.
    while world.checks.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }

    host.shutdown();
    gate.add_permits(1);
    // Everything the check's task does after the gate runs before this yield returns.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    assert!(!published.has_changed().unwrap());
    assert_eq!(status(&host, &jira()), ProviderStatus::Checking);
}

#[tokio::test(start_paused = true)]
async fn a_watcher_waiting_for_the_config_lock_at_shutdown_installs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path.clone()), spawner, BadConfig::Fail).unwrap();
    let config = host.inner.config.clone().lock_owned().await;
    let watcher = tokio::spawn(host.watch_config());
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    // The watcher's first look is past its timer and waits for the config lock.
    tokio::time::sleep(super::RELOAD_INTERVAL).await;
    while Arc::strong_count(&host.inner) == 1 {
        tokio::task::yield_now().await;
    }

    host.shutdown();
    drop(config);
    watcher.await.unwrap();

    assert!(host.providers_response().providers.is_empty());
}

#[tokio::test]
async fn an_edit_after_shutdown_installs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path), spawner, BadConfig::Fail).unwrap();

    host.shutdown();
    let refused = host
        .edit(Precondition::None, |file| {
            file.add(&Source::Calendar, Some("Team"), ConnectionLocation::Local)
        })
        .await;

    assert!(refused.is_err());
    assert!(host.providers_response().providers.is_empty());
}

#[tokio::test]
async fn an_edit_written_before_shutdown_installs_nothing_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[calendar]]\nlabel = \"Home\"\n").unwrap();
    let (spawner, _spawned) = spawner();
    let host = Host::start(Some(path), spawner, BadConfig::Fail).unwrap();
    let written = super::Pause::new();
    host.set_pauses(super::Pauses {
        after_edit_write: Some(written.clone()),
        ..Default::default()
    });
    let edit = tokio::spawn({
        let host = host.clone();
        async move {
            host.edit(Precondition::None, |file| {
                file.add(&Source::Calendar, Some("Team"), ConnectionLocation::Local)
            })
            .await
        }
    });
    written.reached.acquire().await.unwrap().forget();

    host.shutdown();
    written.resume.add_permits(1);
    let edit = edit.await.unwrap().expect("the file was written");

    let shown = host.providers_response().providers;
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].label, "Home");
    // B70: the save is durable, but no revision holds it, so it is no install receipt.
    assert_eq!(edit.connections.len(), 2);
    assert_eq!(edit.installed, None);
}

/// Every value a subscriber sees until `done`, the current one first.
async fn published_until(host: &Host, done: impl Fn(&Published) -> bool) -> Vec<Arc<Published>> {
    let mut updates = host.subscribe();
    let mut seen = vec![updates.borrow_and_update().clone()];
    while !done(seen.last().unwrap()) {
        updates.changed().await.unwrap();
        seen.push(updates.borrow_and_update().clone());
    }
    seen
}

#[tokio::test]
async fn the_runtime_state_is_one_host_and_one_authority_whose_revisions_only_grow() {
    let (spawner, mut spawned) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    let before = host.runtime_state();

    let answered = host.collect(march());
    let seen = published_until(&host, ready).await;
    spawned.end().await;

    for published in &seen {
        let state = published.runtime_state();
        assert_eq!(state.activity.epoch, before.activity.epoch);
        assert_eq!(state.providers.epoch, state.activity.epoch);
        assert_eq!(state.providers.authority, state.activity.authority);
    }
    for pair in seen.windows(2) {
        assert!(pair[0].activity.revision <= pair[1].activity.revision);
        assert!(pair[0].providers.revision <= pair[1].providers.revision);
    }
    // The answer is stamped with the revision it installed, under the same lock.
    assert_eq!(answered.stamp.epoch, before.activity.epoch);
    assert!(answered.stamp.revision > before.activity.revision);
    let after = host.runtime_state();
    assert!(after.activity.revision > answered.stamp.revision);
    assert!(matches!(*after.activity.state, ActivityState::Ready { .. }));
}

#[tokio::test]
async fn a_config_install_raises_the_authority_and_stamps_both_parts() {
    let world = World::shared();
    world.name(&jira(), "a");
    world.name(&gitlab(), "a");
    let (_dir, _path, host, mut spawned) = ready_at(TWO_SOURCES, &world).await;
    let before = host.subscribe().borrow().clone();

    let edit = host
        .edit(Precondition::None, |file| {
            file.set(
                &gitlab(),
                crate::provider::gitlab::GITLAB_URL,
                Some("https://gitlab2.example.org"),
            )
        })
        .await
        .unwrap();
    spawned.end().await;

    let installed = edit.installed.expect("installed");
    assert_eq!(installed.epoch, &*before.epoch);
    let state = host.runtime_state();
    assert_eq!(state.activity.authority, (before.authority + 1).to_string());
    assert_eq!(state.providers.authority, state.activity.authority);
    assert!(installed.revision > before.revision());
    assert!(state.providers.revision >= installed.revision);
    assert!(state.activity.revision >= installed.revision);
}

#[tokio::test]
async fn a_recheck_answer_is_stamped_with_the_providers_it_installed() {
    let calendar = unconfigured_calendar();
    let id = calendar.id.clone();
    let world = World::shared();
    let (host, _spawned) = host_of(&world, &[calendar]);
    until(&host, checked).await;

    let answered = host.recheck(&id).expect("a connection of this host");

    let published = host.subscribe().borrow().clone();
    assert_eq!(answered.stamp.epoch, &*published.epoch);
    assert_eq!(answered.stamp.authority, published.authority.to_string());
    assert!(answered.stamp.revision <= published.providers.revision);
    assert_eq!(answered.value.providers[0].status, ProviderStatus::Checking);
}

#[test]
fn two_hosts_have_different_epochs() {
    let (first, _) = spawner();
    let (second, _) = spawner();
    let a = Host::with_collector(planned_collector(), fixed_git(), first);
    let b = Host::with_collector(planned_collector(), fixed_git(), second);

    assert_ne!(
        a.runtime_state().activity.epoch,
        b.runtime_state().activity.epoch
    );
}

#[test]
fn the_runtime_state_serializes_as_the_push_contract_says() {
    let (spawner, _) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);

    let value = serde_json::to_value(host.runtime_state()).unwrap();

    assert_eq!(
        value["activity"]["state"],
        serde_json::json!({ "kind": "empty" })
    );
    assert!(
        value["activity"]["epoch"]
            .as_str()
            .is_some_and(|epoch| epoch.len() == 16)
    );
    assert!(value["activity"]["authority"].is_string());
    assert!(value["activity"]["revision"].is_u64());
    assert_eq!(value["providers"]["epoch"], value["activity"]["epoch"]);
    assert!(value["providers"]["providers"]["providers"].is_array());
}

// The feed each event subscriber reads (§5).

#[tokio::test]
async fn a_feed_starts_with_the_current_value_and_then_waits_for_a_change() {
    let (spawner, _spawned) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    let current = host.subscribe().borrow().clone();
    let mut feed = host.feed();

    let first = feed.next().await.expect("the current value");
    assert!(Arc::ptr_eq(&first, &current));
    let waiting = feed.next();
    futures::pin_mut!(waiting);
    assert!(futures::poll!(&mut waiting).is_pending());
}

#[tokio::test]
async fn a_feed_that_lagged_gets_the_latest_value_once() {
    let (spawner, mut spawned) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    let mut feed = host.feed();
    feed.next().await.unwrap();

    host.collect(march());
    spawned.end().await;
    host.collect(april());
    spawned.end().await;

    let latest = feed.next().await.unwrap();
    assert!(Arc::ptr_eq(&latest, &host.subscribe().borrow().clone()));
    let waiting = feed.next();
    futures::pin_mut!(waiting);
    assert!(futures::poll!(&mut waiting).is_pending());
}

#[tokio::test]
async fn two_feeds_each_get_every_latest_value() {
    let (spawner, mut spawned) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    let mut first = host.feed();
    let mut second = host.feed();
    first.next().await.unwrap();

    host.collect(march());
    spawned.end().await;

    let published = host.subscribe().borrow().clone();
    assert!(Arc::ptr_eq(&first.next().await.unwrap(), &published));
    // The second one never read: its first value is the latest too.
    assert!(Arc::ptr_eq(&second.next().await.unwrap(), &published));
}

#[tokio::test]
async fn a_feed_ends_at_shutdown_also_while_it_waits() {
    let (spawner, _spawned) = spawner();
    let host = Host::with_collector(planned_collector(), fixed_git(), spawner);
    let mut waiting = host.feed();
    waiting.next().await.unwrap();
    let next = tokio::spawn(async move { waiting.next().await.is_none() });

    host.shutdown();

    assert!(next.await.unwrap());
    assert!(host.feed().next().await.is_none());
}
