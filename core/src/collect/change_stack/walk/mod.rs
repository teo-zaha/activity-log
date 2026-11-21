//! One connection's change stacks: which event changes are open, and from each the way down
//! (the change whose source branch it targets, and so on) and the tree above (the changes that
//! target its source branch, breadth-first).

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    hash::Hash,
    sync::{Arc, Mutex, PoisonError},
};

use chrono::{DateTime, Utc};
use futures::{
    FutureExt, StreamExt,
    future::{BoxFuture, Shared},
    stream,
};

use super::{
    budget::{Breaker, Budget},
    lookup::{
        ChangeLookup, ChangeRef, FailureKind, LookupCause, LookupFailure, OpenChange, OpenPage,
    },
};
use crate::{
    diagnostics,
    model::{
        ChangeStack, ChangeStackAbove, ChangeStackBottom, ChangeStackEnd, ChangeStackNode, StackCut,
    },
};

/// Changes per direction from the current one. The last one's further end is `cut{limit}`
/// without asking, also where a page could answer, so a stack never depends on how it was
/// looked up.
pub(crate) const STACK_DEPTH: usize = 12;
/// Changes one stack shows below and above its change, counted in walk order.
pub(crate) const STACK_MEMBERS: usize = 30;

// The way down is never cut for its members, only for its depth.
const _: () = assert!(STACK_DEPTH < STACK_MEMBERS);

/// What one connection's lookups found.
#[derive(Debug)]
pub(crate) struct StackReport<R> {
    /// The stack of each event change that is open and in one.
    pub stacks: HashMap<ChangeRef<R>, ChangeStack>,
    /// Repositories whose open changes all came on one page, answered from it.
    pub read_whole: usize,
    /// Repositories with more than a page of open changes, asked per branch.
    pub read_in_part: usize,
    /// What failed, by kind: event changes whose openness could not be told (they have no
    /// stack), questions about a branch whose answer is a cut, and the first request failure
    /// that explains them.
    pub failures: BTreeMap<FailureKind, Failures>,
    /// Requests sent.
    pub requests: usize,
    /// Why lookups stopped before they were done, if they did.
    pub stopped: Option<Breaker>,
}

/// What failed for one kind of reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Failures {
    /// Repositories with event changes whose openness is unknown, and those changes.
    pub repositories: usize,
    pub changes: usize,
    /// Branch questions that failed, each counted once however many stacks show its cut.
    pub questions: usize,
    /// What explains the first failure that has an explanation: a request's status and the
    /// server's cleaned message, a repeated page, an ignored filter.
    pub cause: Option<LookupCause>,
}

impl Failures {
    fn add(&mut self, failure: &LookupFailure) {
        if self.cause.is_none() {
            self.cause = failure.cause().cloned();
        }
    }
}

/// The failures of one collection's lookups as they come, from all walks at once.
#[derive(Default)]
struct Recorder(Mutex<BTreeMap<FailureKind, Failures>>);

impl Recorder {
    fn record(&self, failure: &LookupFailure, count: impl FnOnce(&mut Failures)) {
        let mut failures = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = failures.entry(failure.kind()).or_default();
        entry.add(failure);
        count(entry);
    }

    fn unknown(&self, failure: &LookupFailure, changes: usize) {
        self.record(failure, |entry| {
            entry.repositories += 1;
            entry.changes += changes;
        });
    }

    fn question(&self, failure: &LookupFailure) {
        self.record(failure, |entry| entry.questions += 1);
    }

    fn into_inner(self) -> BTreeMap<FailureKind, Failures> {
        self.0.into_inner().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<R> StackReport<R> {
    /// One summary line, and one warning for each reason something was not found, with the
    /// first request failure that explains it. `scope` is the kind's log scope (`gitlab`), so
    /// the lines show under the connection; `service` names the host (`GitLab`).
    pub(crate) fn log(&self, scope: &str, service: &str) {
        diagnostics::info(
            scope,
            format!(
                "Change stacks: {} found with {}; {} read whole, {} in part",
                self.stacks.len(),
                counted(self.requests, "request", "requests"),
                counted(self.read_whole, "repository", "repositories"),
                self.read_in_part
            ),
        );
        for (kind, failures) in &self.failures {
            let what = match kind {
                FailureKind::TokenInvalid => {
                    format!(
                        "{service} refused the token as invalid or expired; change stacks stopped"
                    )
                }
                FailureKind::ScopeMissing => format!(
                    "{service} refused the token: it lacks the scope to read the API; change stacks stopped"
                ),
                FailureKind::Throttled => {
                    format!("{service} asked to slow down; change stacks stopped")
                }
                FailureKind::Unreadable => {
                    "Change stacks: repositories or branches are hidden from the token".to_string()
                }
                FailureKind::Limit => {
                    "Change stacks: the time or request limit ran out".to_string()
                }
                FailureKind::Failed => "Change stacks: requests failed".to_string(),
            };
            let mut parts = Vec::new();
            if failures.changes > 0 {
                parts.push(format!(
                    "whether {} in {} {} open is unknown",
                    counted(failures.changes, "change", "changes"),
                    counted(failures.repositories, "repository", "repositories"),
                    if failures.changes == 1 { "is" } else { "are" }
                ));
            }
            if failures.questions > 0 {
                parts.push(format!(
                    "{} cut short",
                    counted(failures.questions, "branch lookup", "branch lookups")
                ));
            }
            let cause = failures
                .cause
                .as_ref()
                .map(|cause| format!(" ({})", cause.message(service)))
                .unwrap_or_default();
            diagnostics::warn(scope, format!("{what}: {}{cause}", parts.join(", ")));
        }
    }
}

/// `count` with its noun, singular for one.
fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The stacks of `wanted`'s changes, each with the instant of its latest event. Infallible: what
/// cannot be read is a cut, or absent. Two phases share the budget: first each repository's
/// page (and the numbers it did not show), then the walks. Newest first holds within each
/// phase, so many repositories' pages can use the budget before the newest change is walked.
pub(crate) async fn change_stacks<R, L>(
    wanted: impl IntoIterator<Item = (ChangeRef<R>, DateTime<Utc>)>,
    lookup: &L,
    budget: &Budget,
) -> StackReport<R>
where
    R: Clone + Eq + Hash + Ord + Send + Sync,
    L: ChangeLookup<R>,
{
    let order = newest_first(wanted);
    let concurrency = lookup.concurrency().max(1);
    let recorder = Recorder::default();
    let runner = Runner {
        lookup,
        budget,
        recorder: &recorder,
        memo: Mutex::new(HashMap::new()),
    };

    let mut repos: Vec<(R, Vec<u64>)> = Vec::new();
    for change in &order {
        match repos.iter_mut().find(|(repo, _)| *repo == change.repo) {
            Some((_, numbers)) => numbers.push(change.number),
            None => repos.push((change.repo.clone(), vec![change.number])),
        }
    }
    // The futures are made before they are driven, so no closure over borrowed items is held
    // across an await: that keeps the whole future `Send` for any `R`.
    let resolving: Vec<_> = repos
        .into_iter()
        .map(|(repo, numbers)| runner.resolve(repo, numbers))
        .collect();
    let resolved: Vec<Resolved<R>> = stream::iter(resolving)
        .buffered(concurrency)
        .collect()
        .await;

    let by_repo: HashMap<&R, &Repo<R>> = resolved
        .iter()
        .filter_map(|resolved| match resolved {
            Resolved::Read(repo) => Some((&repo.repo, repo)),
            Resolved::Unread => None,
        })
        .collect();
    let mut walks = Vec::new();
    for change in &order {
        if let Some(repo) = by_repo.get(&change.repo)
            && let Some(open) = repo.open.get(&change.number)
        {
            walks.push(runner.walk(change, repo, open));
        }
    }
    let stacks = stream::iter(walks)
        .buffered(concurrency)
        .filter_map(std::future::ready)
        .collect()
        .await;

    let (mut read_whole, mut read_in_part) = (0, 0);
    for resolved in &resolved {
        if let Resolved::Read(repo) = resolved {
            match repo.branches {
                Branches::Index(_) => read_whole += 1,
                Branches::Remote => read_in_part += 1,
            }
        }
    }
    drop(runner);
    StackReport {
        stacks,
        read_whole,
        read_in_part,
        failures: recorder.into_inner(),
        requests: budget.spent(),
        stopped: budget.breaker(),
    }
}

/// Each change once, by its latest event, newest first; ties by repository and number.
fn newest_first<R: Clone + Eq + Hash + Ord>(
    wanted: impl IntoIterator<Item = (ChangeRef<R>, DateTime<Utc>)>,
) -> Vec<ChangeRef<R>> {
    let mut latest: HashMap<ChangeRef<R>, DateTime<Utc>> = HashMap::new();
    for (change, at) in wanted {
        latest
            .entry(change)
            .and_modify(|seen| *seen = (*seen).max(at))
            .or_insert(at);
    }
    let mut order: Vec<_> = latest.into_iter().collect();
    order.sort_by(|(a, a_at), (b, b_at)| b_at.cmp(a_at).then_with(|| a.cmp(b)));
    order.into_iter().map(|(change, _)| change).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Direction {
    /// Open changes whose source branch is the branch: below.
    From,
    /// Open changes that target the branch: above.
    Into,
}

/// One answer about a branch, its changes by number.
type Answer = Result<Arc<[OpenChange]>, LookupFailure>;

/// One repository, once its first page was asked for.
enum Resolved<R> {
    Read(Repo<R>),
    /// Its page could not be read, so whether its event changes are open is unknown.
    Unread,
}

/// What is known of one repository once its page was read.
struct Repo<R> {
    repo: R,
    branches: Branches,
    /// The event changes found open.
    open: HashMap<u64, OpenChange>,
}

/// Where a repository's branch questions are answered.
enum Branches {
    /// From its one page, which held every open change.
    Index(RepoIndex),
    /// By the host, each question once.
    Remote,
}

/// A repository's open changes by the branches they come from and go into.
struct RepoIndex {
    from: HashMap<String, Arc<[OpenChange]>>,
    into: HashMap<String, Arc<[OpenChange]>>,
}

impl RepoIndex {
    fn new(changes: &[OpenChange]) -> Self {
        let group = |branch: fn(&OpenChange) -> &str| {
            let mut groups: HashMap<String, Vec<OpenChange>> = HashMap::new();
            for change in changes {
                groups
                    .entry(branch(change).to_string())
                    .or_default()
                    .push(change.clone());
            }
            groups
                .into_iter()
                .map(|(branch, changes)| (branch, by_number(changes)))
                .collect()
        };
        Self {
            from: group(OpenChange::source_branch),
            into: group(OpenChange::target_branch),
        }
    }

    fn get(&self, direction: Direction, branch: &str) -> Arc<[OpenChange]> {
        let groups = match direction {
            Direction::From => &self.from,
            Direction::Into => &self.into,
        };
        groups.get(branch).cloned().unwrap_or_else(|| Arc::from([]))
    }
}

fn by_number(mut changes: Vec<OpenChange>) -> Arc<[OpenChange]> {
    changes.sort_by_key(OpenChange::number);
    changes.dedup_by_key(|change| change.number());
    changes.into()
}

type MemoKey<R> = (R, Direction, String);

struct Runner<'a, R, L> {
    lookup: &'a L,
    budget: &'a Budget,
    recorder: &'a Recorder,
    /// Every branch question asked of the host, answered or still running: walks that reach
    /// the same branch at once share one request, and a failure is not asked again.
    memo: Mutex<HashMap<MemoKey<R>, Shared<BoxFuture<'a, Answer>>>>,
}

impl<'a, R, L> Runner<'a, R, L>
where
    R: Clone + Eq + Hash + Send + Sync + 'a,
    L: ChangeLookup<R>,
{
    /// Which of `numbers` are open in `repo`, and how its branches are answered: from its page
    /// when the page held every open change, else by the host, after asking it for the
    /// numbers the page did not show.
    async fn resolve(&self, repo: R, numbers: Vec<u64>) -> Resolved<R> {
        let page = self
            .budget
            .run(self.lookup.open_page(&repo, self.budget))
            .await;
        let pick = |known: &HashMap<u64, OpenChange>| -> HashMap<u64, OpenChange> {
            numbers
                .iter()
                .filter_map(|number| Some((*number, known.get(number)?.clone())))
                .collect()
        };
        match page {
            Err(failure) => {
                self.recorder.unknown(&failure, numbers.len());
                Resolved::Unread
            }
            Ok(OpenPage::Complete(changes)) => {
                let index = RepoIndex::new(&changes);
                let known = numbered(changes);
                Resolved::Read(Repo {
                    open: pick(&known),
                    repo,
                    branches: Branches::Index(index),
                })
            }
            Ok(OpenPage::Partial(changes)) => {
                let mut known = numbered(changes);
                let missing: Vec<u64> = numbers
                    .iter()
                    .copied()
                    .filter(|number| !known.contains_key(number))
                    .collect();
                if !missing.is_empty() {
                    match self
                        .budget
                        .run(self.lookup.open_numbers(&repo, &missing, self.budget))
                        .await
                    {
                        Ok(found) => known.extend(
                            numbered(found)
                                .into_iter()
                                .filter(|(number, _)| missing.contains(number)),
                        ),
                        Err(failure) => self.recorder.unknown(&failure, missing.len()),
                    }
                }
                Resolved::Read(Repo {
                    open: pick(&known),
                    repo,
                    branches: Branches::Remote,
                })
            }
        }
    }

    async fn branch(&self, repo: &Repo<R>, direction: Direction, branch: &str) -> Answer {
        match &repo.branches {
            Branches::Index(index) => Ok(index.get(direction, branch)),
            Branches::Remote => self.ask(&repo.repo, direction, branch).await,
        }
    }

    /// The host's answer about `branch`, asked once however many walks wait for it.
    fn ask(&self, repo: &R, direction: Direction, branch: &str) -> Shared<BoxFuture<'a, Answer>> {
        let (lookup, budget, recorder) = (self.lookup, self.budget, self.recorder);
        let mut memo = self.memo.lock().unwrap_or_else(PoisonError::into_inner);
        memo.entry((repo.clone(), direction, branch.to_string()))
            .or_insert_with_key(|(repo, direction, branch)| {
                let (repo, direction, branch) = (repo.clone(), *direction, branch.clone());
                async move {
                    let changes = match direction {
                        Direction::From => {
                            budget.run(lookup.open_from(&repo, &branch, budget)).await
                        }
                        Direction::Into => {
                            budget.run(lookup.open_into(&repo, &branch, budget)).await
                        }
                    };
                    match changes {
                        Ok(changes) => Ok(by_number(changes)),
                        Err(failure) => {
                            recorder.question(&failure);
                            Err(failure)
                        }
                    }
                }
                .boxed()
                .shared()
            })
            .clone()
    }

    /// `current`'s stack by its reference, or `None` when it is in none.
    async fn walk(
        &self,
        change: &ChangeRef<R>,
        repo: &Repo<R>,
        current: &OpenChange,
    ) -> Option<(ChangeRef<R>, ChangeStack)> {
        let mut path: HashSet<u64> = HashSet::from([current.number()]);
        // Nearest first.
        let mut below: Vec<OpenChange> = Vec::new();
        let bottom = loop {
            if below.len() == STACK_DEPTH {
                break cut_bottom(StackCut::Limit);
            }
            let target = below.last().unwrap_or(current).target_branch().to_string();
            let found = match self.branch(repo, Direction::From, &target).await {
                Ok(found) => found,
                Err(failure) => break cut_bottom(failure.kind().cut()),
            };
            // A change from a fork comes from the fork's branch of that name, not this one.
            let parents: Vec<&OpenChange> =
                found.iter().filter(|change| change.same_repo()).collect();
            match parents.as_slice() {
                [] => break ChangeStackBottom::Reached { branch: target },
                [parent] => {
                    if !path.insert(parent.number()) {
                        break cut_bottom(StackCut::Cycle);
                    }
                    below.push((*parent).clone());
                }
                _ => break ChangeStackBottom::Ambiguous,
            }
        };
        let above = self
            .above(repo, current, &path, STACK_MEMBERS - below.len())
            .await;
        let stack = ChangeStack::new(
            current.stacked(),
            below.iter().rev().map(OpenChange::stacked).collect(),
            bottom,
            above,
        )?;
        Some((change.clone(), stack))
    }

    /// The tree above `current`, breadth-first and children by number, at most `room` changes:
    /// a deep branch never takes the room of a direct child. A change on its own path (`below`
    /// with `current`, and its ancestors above) is not shown again.
    async fn above(
        &self,
        repo: &Repo<R>,
        current: &OpenChange,
        below: &HashSet<u64>,
        room: usize,
    ) -> ChangeStackAbove {
        let mut tree = vec![Built::new(current.clone())];
        // A node, its depth and its path above `current`.
        let mut queue = VecDeque::from([(0, 0, Vec::<u64>::new())]);
        let mut shown = 0;
        while let Some((at, depth, path)) = queue.pop_front() {
            let node = &tree[at].change;
            let end = if !node.same_repo() {
                // A fork change's source branch is in the fork: what targets it is not here.
                ChangeStackEnd::Complete
            } else if depth == STACK_DEPTH || shown == room {
                cut_end(StackCut::Limit)
            } else {
                let source = node.source_branch().to_string();
                match self.branch(repo, Direction::Into, &source).await {
                    Err(failure) => cut_end(failure.kind().cut()),
                    Ok(children) => {
                        let (mut cycle, mut full) = (false, false);
                        for child in children.iter() {
                            let number = child.number();
                            if below.contains(&number) || path.contains(&number) {
                                cycle = true;
                                continue;
                            }
                            if shown == room {
                                full = true;
                                break;
                            }
                            shown += 1;
                            tree.push(Built::new(child.clone()));
                            let id = tree.len() - 1;
                            tree[at].children.push(id);
                            let mut path = path.clone();
                            path.push(number);
                            queue.push_back((id, depth + 1, path));
                        }
                        if full {
                            cut_end(StackCut::Limit)
                        } else if cycle {
                            cut_end(StackCut::Cycle)
                        } else {
                            ChangeStackEnd::Complete
                        }
                    }
                }
            };
            tree[at].end = end;
        }
        above_of(&tree, 0)
    }
}

fn numbered(changes: Vec<OpenChange>) -> HashMap<u64, OpenChange> {
    let mut known = HashMap::new();
    for change in changes {
        known.entry(change.number()).or_insert(change);
    }
    known
}

fn cut_bottom(reason: StackCut) -> ChangeStackBottom {
    ChangeStackBottom::Cut { reason }
}

fn cut_end(reason: StackCut) -> ChangeStackEnd {
    ChangeStackEnd::Cut { reason }
}

/// A change of the tree above, while it is built.
struct Built {
    change: OpenChange,
    children: Vec<usize>,
    end: ChangeStackEnd,
}

impl Built {
    fn new(change: OpenChange) -> Self {
        Self {
            change,
            children: Vec::new(),
            end: ChangeStackEnd::Complete,
        }
    }
}

fn above_of(tree: &[Built], at: usize) -> ChangeStackAbove {
    ChangeStackAbove {
        changes: tree[at]
            .children
            .iter()
            .map(|&child| ChangeStackNode {
                change: tree[child].change.stacked(),
                above: above_of(tree, child),
            })
            .collect(),
        end: tree[at].end,
    }
}

#[cfg(test)]
mod tests;
