use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use futures::{FutureExt, future::BoxFuture};
use reqwest::Url;
use tokio::{sync::Semaphore, time::Instant};

use super::{Failures, STACK_DEPTH, STACK_MEMBERS, StackReport, change_stacks};
use crate::{
    collect::change_stack::{
        budget::{Breaker, Budget, STACK_BUDGET, STACK_REQUESTS},
        lookup::{
            ChangeLookup, ChangeRef, FailureKind, LookupCause, LookupFailure, OpenChange, OpenPage,
        },
        web_link::{WebLink, web_link},
    },
    model::{
        ChangeStack, ChangeStackAbove, ChangeStackBottom, ChangeStackEnd, ChangeStackNode,
        LogLevel, StackCut, StackedChange,
    },
    provider::{remote_api::RequestBudget, remote_error::ApiError},
};

const WIDGETS: u64 = 42;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Call<R> {
    Page(R),
    Numbers(R, Vec<u64>),
    From(R, String),
    Into(R, String),
}

fn from<R>(repo: R, branch: &str) -> Call<R> {
    Call::From(repo, branch.to_string())
}

fn into<R>(repo: R, branch: &str) -> Call<R> {
    Call::Into(repo, branch.to_string())
}

/// A host that answers from tables: each repository's open changes in the order it lists them,
/// at most `page_size` on its first page. Every call spends one request, is recorded, and then
/// waits for `gate` (only the `gated` calls, when it names some), never answers (`hang`) or
/// fails (`fail`) as the test says.
struct Fake<R> {
    repos: HashMap<R, Vec<OpenChange>>,
    page_size: usize,
    concurrency: usize,
    fail: HashMap<Call<R>, LookupFailure>,
    hang: HashSet<Call<R>>,
    gate: Option<Arc<Semaphore>>,
    gated: Option<HashSet<Call<R>>>,
    calls: Mutex<Vec<Call<R>>>,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
}

impl<R: Clone + Eq + Hash> Fake<R> {
    fn new(repos: impl IntoIterator<Item = (R, Vec<OpenChange>)>) -> Self {
        Self {
            repos: repos.into_iter().collect(),
            page_size: 100,
            concurrency: 4,
            fail: HashMap::new(),
            hang: HashSet::new(),
            gate: None,
            gated: None,
            calls: Mutex::new(Vec::new()),
            in_flight: AtomicUsize::new(0),
            most_in_flight: AtomicUsize::new(0),
        }
    }

    /// Only part of each repository on its first page, so branches are asked of the host.
    fn paged(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    fn failing(mut self, call: Call<R>, failure: impl Into<LookupFailure>) -> Self {
        self.fail.insert(call, failure.into());
        self
    }

    fn calls(&self) -> Vec<Call<R>> {
        self.calls.lock().unwrap().clone()
    }

    fn changes(&self, repo: &R) -> Vec<OpenChange> {
        self.repos.get(repo).cloned().unwrap_or_default()
    }

    async fn answer<T>(
        &self,
        call: Call<R>,
        budget: &Budget,
        answer: impl FnOnce() -> T,
    ) -> Result<T, LookupFailure> {
        budget.try_spend()?;
        self.calls.lock().unwrap().push(call.clone());
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_in_flight.fetch_max(now, Ordering::SeqCst);
        if let Some(gate) = &self.gate
            && self
                .gated
                .as_ref()
                .is_none_or(|gated| gated.contains(&call))
        {
            drop(gate.acquire().await.unwrap());
        }
        if self.hang.contains(&call) {
            std::future::pending::<()>().await;
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        match self.fail.get(&call) {
            Some(failure) => Err(failure.clone()),
            None => Ok(answer()),
        }
    }

    fn matching(&self, repo: &R, keep: impl Fn(&OpenChange) -> bool) -> Vec<OpenChange> {
        self.changes(repo)
            .into_iter()
            .filter(|change| keep(change))
            .collect()
    }
}

impl<R: Clone + Eq + Hash + Send + Sync> ChangeLookup<R> for Fake<R> {
    fn concurrency(&self) -> usize {
        self.concurrency
    }

    fn open_page<'a>(
        &'a self,
        repo: &'a R,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<OpenPage, LookupFailure>> {
        self.answer(Call::Page(repo.clone()), budget, || {
            let all = self.changes(repo);
            if all.len() <= self.page_size {
                OpenPage::Complete(all)
            } else {
                OpenPage::Partial(all[..self.page_size].to_vec())
            }
        })
        .boxed()
    }

    fn open_numbers<'a>(
        &'a self,
        repo: &'a R,
        numbers: &'a [u64],
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        self.answer(
            Call::Numbers(repo.clone(), numbers.to_vec()),
            budget,
            || self.matching(repo, |change| numbers.contains(&change.number())),
        )
        .boxed()
    }

    fn open_from<'a>(
        &'a self,
        repo: &'a R,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        self.answer(from(repo.clone(), branch), budget, move || {
            self.matching(repo, |change| change.source_branch() == branch)
        })
        .boxed()
    }

    fn open_into<'a>(
        &'a self,
        repo: &'a R,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        self.answer(into(repo.clone(), branch), budget, move || {
            self.matching(repo, |change| change.target_branch() == branch)
        })
        .boxed()
    }
}

fn link(number: u64) -> Option<WebLink> {
    let origin = Url::parse("https://gitlab.example.org").unwrap().origin();
    web_link(
        &format!("https://gitlab.example.org/acme/widgets/-/merge_requests/{number}"),
        &origin,
    )
    .ok()
}

fn open(number: u64, source: &str, target: &str, same_repo: bool) -> OpenChange {
    OpenChange::new(
        number,
        format!("Parse {number}"),
        source.to_string(),
        target.to_string(),
        same_repo,
        link(number),
    )
    .unwrap()
}

/// A merge request of the repository's own branches.
fn mr(number: u64, source: &str, target: &str) -> OpenChange {
    open(number, source, target, true)
}

/// One from a fork, or a deleted fork.
fn fork(number: u64, source: &str, target: &str) -> OpenChange {
    open(number, source, target, false)
}

fn shown(number: u64) -> StackedChange {
    StackedChange {
        number,
        title: format!("Parse {number}"),
        web_url: link(number).map(|link| link.as_str().to_string()),
    }
}

fn node(number: u64, above: ChangeStackAbove) -> ChangeStackNode {
    ChangeStackNode {
        change: shown(number),
        above,
    }
}

fn leaf(number: u64) -> ChangeStackNode {
    node(number, complete(vec![]))
}

fn complete(changes: Vec<ChangeStackNode>) -> ChangeStackAbove {
    ChangeStackAbove {
        changes,
        end: ChangeStackEnd::Complete,
    }
}

fn cut(changes: Vec<ChangeStackNode>, reason: StackCut) -> ChangeStackAbove {
    ChangeStackAbove {
        changes,
        end: ChangeStackEnd::Cut { reason },
    }
}

fn reached(branch: &str) -> ChangeStackBottom {
    ChangeStackBottom::Reached {
        branch: branch.to_string(),
    }
}

fn cut_bottom(reason: StackCut) -> ChangeStackBottom {
    ChangeStackBottom::Cut { reason }
}

fn stack(
    current: u64,
    below: &[u64],
    bottom: ChangeStackBottom,
    above: ChangeStackAbove,
) -> ChangeStack {
    ChangeStack::new(
        shown(current),
        below.iter().copied().map(shown).collect(),
        bottom,
        above,
    )
    .expect("a valid stack")
}

/// `numbers` of `repo`, the first the newest.
fn wanted<R: Clone>(repo: R, numbers: &[u64]) -> Vec<(ChangeRef<R>, DateTime<Utc>)> {
    numbers
        .iter()
        .enumerate()
        .map(|(age, number)| {
            (
                ChangeRef {
                    repo: repo.clone(),
                    number: *number,
                },
                at(59 - age as u32),
            )
        })
        .collect()
}

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 17, 9, minute, 0).unwrap()
}

fn budget() -> Budget {
    Budget::for_collection()
}

async fn stacks_of<R>(fake: &Fake<R>, wanted: Vec<(ChangeRef<R>, DateTime<Utc>)>) -> StackReport<R>
where
    R: Clone + Eq + Hash + Ord + Send + Sync,
{
    change_stacks(wanted, fake, &budget()).await
}

/// [`stacks_of`], and why lookups stopped for good, if they did.
async fn stopped_by<R>(
    fake: &Fake<R>,
    wanted: Vec<(ChangeRef<R>, DateTime<Utc>)>,
) -> (StackReport<R>, Option<Breaker>)
where
    R: Clone + Eq + Hash + Ord + Send + Sync,
{
    let budget = budget();
    let report = change_stacks(wanted, fake, &budget).await;
    (report, budget.breaker())
}

fn stack_of<R: Eq + Hash>(report: &StackReport<R>, repo: R, number: u64) -> Option<&ChangeStack> {
    report.stacks.get(&ChangeRef { repo, number })
}

/// `main` ← 1 ← 2 ← 3 ← 4, each from `feature/parse-{a..d}`.
fn four() -> Vec<OpenChange> {
    vec![
        mr(1, "feature/parse-a", "main"),
        mr(2, "feature/parse-b", "feature/parse-a"),
        mr(3, "feature/parse-c", "feature/parse-b"),
        mr(4, "feature/parse-d", "feature/parse-c"),
    ]
}

fn four_seen_from_each() -> [ChangeStack; 4] {
    [
        stack(
            1,
            &[],
            reached("main"),
            complete(vec![node(
                2,
                complete(vec![node(3, complete(vec![leaf(4)]))]),
            )]),
        ),
        stack(
            2,
            &[1],
            reached("main"),
            complete(vec![node(3, complete(vec![leaf(4)]))]),
        ),
        stack(3, &[1, 2], reached("main"), complete(vec![leaf(4)])),
        stack(4, &[1, 2, 3], reached("main"), complete(vec![])),
    ]
}

#[tokio::test]
async fn a_linear_stack_is_seen_from_each_of_its_changes() {
    for fake in [
        Fake::new([(WIDGETS, four())]),
        Fake::new([(WIDGETS, four())]).paged(0),
    ] {
        let report = stacks_of(&fake, wanted(WIDGETS, &[1, 2, 3, 4])).await;
        for (number, expected) in (1..=4).zip(four_seen_from_each()) {
            assert_eq!(
                stack_of(&report, WIDGETS, number),
                Some(&expected),
                "!{number}"
            );
        }
        assert_eq!(report.stacks.len(), 4);
    }
}

/// A repository whose open changes fit on one page costs one request, whatever is walked.
#[tokio::test]
async fn one_page_answers_every_walk_in_its_repository() {
    let fake = Fake::new([(WIDGETS, four())]);
    let report = stacks_of(&fake, wanted(WIDGETS, &[1, 2, 3, 4])).await;

    assert_eq!(fake.calls(), [Call::Page(WIDGETS)]);
    assert_eq!(
        (report.requests, report.read_whole, report.read_in_part),
        (1, 1, 0)
    );
}

/// Past one page, each branch is asked once for all walks: the page, the numbers it did not
/// show, and each branch in each direction.
#[tokio::test]
async fn past_one_page_each_question_is_asked_once() {
    let fake = Fake::new([(WIDGETS, four())]).paged(0);
    let report = stacks_of(&fake, wanted(WIDGETS, &[1, 2, 3, 4])).await;

    let calls = fake.calls();
    let distinct: HashSet<_> = calls.iter().cloned().collect();
    assert_eq!(calls.len(), distinct.len(), "{calls:?}");
    let expected: HashSet<_> = [
        Call::Page(WIDGETS),
        Call::Numbers(WIDGETS, vec![1, 2, 3, 4]),
        from(WIDGETS, "main"),
        from(WIDGETS, "feature/parse-a"),
        from(WIDGETS, "feature/parse-b"),
        from(WIDGETS, "feature/parse-c"),
        into(WIDGETS, "feature/parse-a"),
        into(WIDGETS, "feature/parse-b"),
        into(WIDGETS, "feature/parse-c"),
        into(WIDGETS, "feature/parse-d"),
    ]
    .into();
    assert_eq!(distinct, expected);
    assert_eq!(
        (report.requests, report.read_whole, report.read_in_part),
        (10, 0, 1)
    );
}

/// One stack's walk asks exactly its own branches.
#[tokio::test]
async fn one_walk_asks_its_own_branches() {
    let fake = Fake::new([(WIDGETS, four())]).paged(0);
    stacks_of(&fake, wanted(WIDGETS, &[2])).await;

    assert_eq!(
        fake.calls(),
        [
            Call::Page(WIDGETS),
            Call::Numbers(WIDGETS, vec![2]),
            from(WIDGETS, "feature/parse-a"),
            from(WIDGETS, "main"),
            into(WIDGETS, "feature/parse-b"),
            into(WIDGETS, "feature/parse-c"),
            into(WIDGETS, "feature/parse-d"),
        ]
    );
}

/// A partial page tells which event changes are open as far as it goes; only the others are
/// asked for by number.
#[tokio::test]
async fn only_event_changes_off_the_page_are_asked_for() {
    let fake = Fake::new([(WIDGETS, four())]).paged(2);
    let report = stacks_of(&fake, wanted(WIDGETS, &[4, 1, 2, 9])).await;

    assert!(fake.calls().contains(&Call::Numbers(WIDGETS, vec![4, 9])));
    assert_eq!(
        fake.calls()
            .iter()
            .filter(|call| matches!(call, Call::Numbers(..)))
            .count(),
        1
    );
    assert_eq!(report.stacks.len(), 3);
    assert_eq!(stack_of(&report, WIDGETS, 9), None);
}

#[tokio::test]
async fn a_lone_or_closed_change_has_no_stack() {
    for fake in [
        Fake::new([(WIDGETS, vec![mr(5, "feature/parse-e", "main")])]),
        Fake::new([(WIDGETS, vec![mr(5, "feature/parse-e", "main")])]).paged(0),
    ] {
        let report = stacks_of(&fake, wanted(WIDGETS, &[5, 6])).await;
        assert!(report.stacks.is_empty(), "{:?}", report.stacks);
        assert!(report.failures.is_empty());
    }
}

/// Children by number, breadth-first: a child's own children come after its siblings.
#[tokio::test]
async fn children_are_shown_breadth_first_by_number() {
    let changes = vec![
        mr(1, "a", "main"),
        mr(4, "d", "b"),
        mr(3, "c", "a"),
        mr(2, "b", "a"),
    ];
    let fake = Fake::new([(WIDGETS, changes)]);
    let report = stacks_of(&fake, wanted(WIDGETS, &[1])).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 1),
        Some(&stack(
            1,
            &[],
            reached("main"),
            complete(vec![node(2, complete(vec![leaf(4)])), leaf(3)])
        ))
    );
}

/// Several open changes from the branch below are no single way down.
#[tokio::test]
async fn several_changes_below_are_ambiguous() {
    let changes = vec![mr(1, "x", "main"), mr(2, "x", "dev"), mr(3, "c", "x")];
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[3])).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 3),
        Some(&stack(
            3,
            &[],
            ChangeStackBottom::Ambiguous,
            complete(vec![])
        ))
    );
}

/// Two open changes from one branch share what builds on it: it shows under both.
#[tokio::test]
async fn shared_children_show_under_each_parent() {
    let changes = vec![
        mr(1, "p", "main"),
        mr(2, "s", "p"),
        mr(3, "s", "p"),
        mr(4, "t", "s"),
    ];
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[1])).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 1),
        Some(&stack(
            1,
            &[],
            reached("main"),
            complete(vec![
                node(2, complete(vec![leaf(4)])),
                node(3, complete(vec![leaf(4)]))
            ])
        ))
    );
}

fn members(above: &ChangeStackAbove) -> usize {
    above
        .changes
        .iter()
        .map(|node| 1 + members(&node.above))
        .sum()
}

fn ends(above: &ChangeStackAbove) -> Vec<ChangeStackEnd> {
    let mut ends = vec![above.end];
    for node in &above.changes {
        ends.extend(self::ends(&node.above));
    }
    ends
}

/// Ten diamonds in a row would double the tree at each level; the stack stops at its members.
#[tokio::test]
async fn diamonds_stay_within_the_member_limit() {
    let mut changes = vec![mr(1, "s0", "main")];
    for level in 1..=10_u64 {
        let (source, target) = (format!("s{level}"), format!("s{}", level - 1));
        changes.push(mr(level * 10, &source, &target));
        changes.push(mr(level * 10 + 1, &source, &target));
    }
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[1])).await;

    let above = stack_of(&report, WIDGETS, 1).unwrap().above();
    assert_eq!(members(above), STACK_MEMBERS);
    assert!(ends(above).contains(&ChangeStackEnd::Cut {
        reason: StackCut::Limit
    }));
}

/// A two-change cycle, from each end: the way down ends at the cycle, and the change below
/// never comes back above; a sibling of the change that would close it stays.
#[tokio::test]
async fn a_cycle_of_two_is_cut_from_both_ends() {
    let changes = vec![mr(1, "a", "b"), mr(2, "b", "a"), mr(3, "x", "a")];
    for fake in [
        Fake::new([(WIDGETS, changes.clone())]),
        Fake::new([(WIDGETS, changes.clone())]).paged(0),
    ] {
        let report = stacks_of(&fake, wanted(WIDGETS, &[1, 2])).await;

        assert_eq!(
            stack_of(&report, WIDGETS, 1),
            Some(&stack(
                1,
                &[2],
                cut_bottom(StackCut::Cycle),
                cut(vec![leaf(3)], StackCut::Cycle)
            ))
        );
        assert_eq!(
            stack_of(&report, WIDGETS, 2),
            Some(&stack(
                2,
                &[1],
                cut_bottom(StackCut::Cycle),
                cut(vec![], StackCut::Cycle)
            ))
        );
    }
}

#[tokio::test]
async fn a_cycle_of_three_is_cut_from_both_ends() {
    let changes = vec![mr(1, "a", "c"), mr(2, "b", "a"), mr(3, "c", "b")];
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[1, 2])).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 1),
        Some(&stack(
            1,
            &[2, 3],
            cut_bottom(StackCut::Cycle),
            cut(vec![], StackCut::Cycle)
        ))
    );
    assert_eq!(
        stack_of(&report, WIDGETS, 2),
        Some(&stack(
            2,
            &[3, 1],
            cut_bottom(StackCut::Cycle),
            cut(vec![], StackCut::Cycle)
        ))
    );
}

/// A change from a fork is no parent (its branch is the fork's) and has no children asked;
/// one from a deleted fork is kept as a member.
#[tokio::test]
async fn forks_are_members_but_never_parents() {
    let changes = vec![fork(1, "a", "main"), mr(2, "x", "a"), fork(3, "y", "x")];
    let fake = Fake::new([(WIDGETS, changes)]).paged(0);
    let report = stacks_of(&fake, wanted(WIDGETS, &[2, 1])).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 2),
        Some(&stack(2, &[], reached("a"), complete(vec![leaf(3)])))
    );
    assert_eq!(stack_of(&report, WIDGETS, 1), None, "a lone fork change");
    let calls = fake.calls();
    assert!(!calls.contains(&into(WIDGETS, "y")), "{calls:?}");
    assert!(!calls.contains(&into(WIDGETS, "a")), "{calls:?}");
}

/// `main` ← 1 ← … ← 15, each from `b{n}` into `b{n-1}` (`b0` is `main`).
fn fifteen() -> Vec<OpenChange> {
    (1..=15_u64)
        .map(|number| {
            let target = if number == 1 {
                "main".to_string()
            } else {
                format!("b{}", number - 1)
            };
            mr(number, &format!("b{number}"), &target)
        })
        .collect()
}

/// A long chain is cut at `STACK_DEPTH` each way without asking further, and the same whether
/// a page or the host answered.
#[tokio::test]
async fn a_long_chain_is_cut_at_its_depth_the_same_on_both_backends() {
    let whole = Fake::new([(WIDGETS, fifteen())]);
    let asked = Fake::new([(WIDGETS, fifteen())]).paged(0);
    let from_whole = stacks_of(&whole, wanted(WIDGETS, &[15, 1])).await;
    let from_asked = stacks_of(&asked, wanted(WIDGETS, &[15, 1])).await;

    let top = stack_of(&from_asked, WIDGETS, 15).unwrap();
    assert_eq!(top.below().len(), STACK_DEPTH);
    assert_eq!(top.below()[0].number, 3);
    assert_eq!(top.bottom(), &cut_bottom(StackCut::Limit));
    assert!(!asked.calls().contains(&from(WIDGETS, "b2")));

    let base = stack_of(&from_asked, WIDGETS, 1).unwrap();
    assert_eq!(members(base.above()), STACK_DEPTH);
    let mut level = base.above();
    for _ in 0..STACK_DEPTH {
        level = &level.changes[0].above;
    }
    assert_eq!(
        level.end,
        ChangeStackEnd::Cut {
            reason: StackCut::Limit
        }
    );
    assert!(!asked.calls().contains(&into(WIDGETS, "b13")));

    assert_eq!(from_whole.stacks, from_asked.stacks);
}

/// The member limit counts the way down too; what was read before it stays.
#[tokio::test]
async fn the_member_limit_keeps_what_was_read() {
    let mut changes = vec![mr(50, "m", "main"), mr(1, "a", "m")];
    changes.extend((2..=41).map(|number| mr(number, &format!("x{number}"), "a")));
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[1])).await;

    let stack = stack_of(&report, WIDGETS, 1).unwrap();
    assert_eq!(stack.below(), [shown(50)]);
    let above = stack.above();
    assert_eq!(members(above), STACK_MEMBERS - 1);
    assert_eq!(
        above
            .changes
            .iter()
            .map(|node| node.change.number)
            .collect::<Vec<_>>(),
        (2..=30).collect::<Vec<_>>()
    );
    assert_eq!(
        above.end,
        ChangeStackEnd::Cut {
            reason: StackCut::Limit
        }
    );
}

/// A wide child does not take the room of its siblings: both direct children show.
#[tokio::test]
async fn a_wide_child_never_starves_its_siblings() {
    let mut changes = vec![
        mr(1, "a", "main"),
        mr(2, "b", "a"),
        mr(3, "c", "a"),
        mr(100, "d", "c"),
    ];
    changes.extend((4..=40).map(|number| mr(number, &format!("x{number}"), "b")));
    let report = stacks_of(&Fake::new([(WIDGETS, changes)]), wanted(WIDGETS, &[1])).await;

    let above = stack_of(&report, WIDGETS, 1).unwrap().above();
    assert_eq!(
        above
            .changes
            .iter()
            .map(|node| node.change.number)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(members(above), STACK_MEMBERS);
    let limit = ChangeStackEnd::Cut {
        reason: StackCut::Limit,
    };
    assert_eq!(
        (above.changes[0].above.end, above.changes[1].above.end),
        (limit, limit)
    );
}

/// Out of requests mid-walk: what was read stays, the ends not read are `limit`.
#[tokio::test]
async fn running_out_of_requests_cuts_what_was_not_read() {
    let fake = Fake::new([(WIDGETS, four())]).paged(0);
    let budget = Budget::new(4, Instant::now() + STACK_BUDGET);
    let report = change_stacks(wanted(WIDGETS, &[2]), &fake, &budget).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 2),
        Some(&stack(
            2,
            &[1],
            reached("main"),
            cut(vec![], StackCut::Limit)
        ))
    );
    assert_eq!(report.requests, 4);
}

/// An open change whose walk could not ask anything shows both ends cut, never "in no stack".
#[tokio::test]
async fn a_walk_that_never_started_is_cut_at_both_ends() {
    let fake = Fake::new([(WIDGETS, four())]).paged(2);
    let budget = Budget::new(1, Instant::now() + STACK_BUDGET);
    let report = change_stacks(wanted(WIDGETS, &[1]), &fake, &budget).await;

    assert_eq!(
        stack_of(&report, WIDGETS, 1),
        Some(&stack(
            1,
            &[],
            cut_bottom(StackCut::Limit),
            cut(vec![], StackCut::Limit)
        ))
    );
    assert_eq!(fake.calls(), [Call::Page(WIDGETS)]);
}

/// A request still unanswered at the deadline ends the walk there; stacks already found stay.
#[tokio::test(start_paused = true)]
async fn the_deadline_cuts_what_was_not_read_and_keeps_the_rest() {
    let forks = 77;
    let mut fake = Fake::new([(WIDGETS, four()), (forks, four())]).paged(2);
    fake.repos
        .insert(WIDGETS, vec![mr(1, "a", "main"), mr(2, "b", "a")]);
    fake.hang.insert(from(forks, "main"));
    let started = Instant::now();

    let mut wanted = wanted(WIDGETS, &[2]);
    wanted.extend(self::wanted(forks, &[1]));
    let report = stacks_of(&fake, wanted).await;

    assert!(started.elapsed() >= STACK_BUDGET);
    assert!(started.elapsed() < STACK_BUDGET + Duration::from_secs(1));
    assert_eq!(
        stack_of(&report, WIDGETS, 2),
        Some(&stack(2, &[1], reached("main"), complete(vec![])))
    );
    assert_eq!(
        stack_of(&report, forks, 1),
        Some(&stack(
            1,
            &[],
            cut_bottom(StackCut::Limit),
            cut(vec![], StackCut::Limit)
        ))
    );
}

/// A busy host stops every request: what was not read is `throttled`, and nothing more is
/// sent.
#[tokio::test]
async fn a_busy_host_stops_every_later_request() {
    let mut fake = Fake::new([(WIDGETS, four())])
        .paged(0)
        .failing(from(WIDGETS, "feature/parse-a"), FailureKind::Throttled);
    fake.concurrency = 1;
    let (report, stopped) = stopped_by(&fake, wanted(WIDGETS, &[2, 3])).await;

    assert_eq!(
        fake.calls(),
        [
            Call::Page(WIDGETS),
            Call::Numbers(WIDGETS, vec![2, 3]),
            from(WIDGETS, "feature/parse-a"),
        ]
    );
    let throttled = |number, below: &[u64]| {
        stack(
            number,
            below,
            cut_bottom(StackCut::Throttled),
            cut(vec![], StackCut::Throttled),
        )
    };
    assert_eq!(stack_of(&report, WIDGETS, 2), Some(&throttled(2, &[])));
    assert_eq!(stack_of(&report, WIDGETS, 3), Some(&throttled(3, &[])));
    assert_eq!(stopped, Some(Breaker::Throttled));
}

/// A refused token on the first page: no other repository is asked, and nothing shows a stack.
#[tokio::test]
async fn a_refused_token_stops_before_any_other_request() {
    let mut fake = Fake::new([(WIDGETS, four()), (77, four())])
        .failing(Call::Page(WIDGETS), FailureKind::ScopeMissing);
    fake.concurrency = 1;
    let mut wanted = wanted(WIDGETS, &[1, 2]);
    wanted.extend(
        self::wanted(77, &[3])
            .into_iter()
            .map(|(change, _)| (change, at(0))),
    );
    let (report, stopped) = stopped_by(&fake, wanted).await;

    assert_eq!(fake.calls(), [Call::Page(WIDGETS)]);
    assert!(report.stacks.is_empty());
    assert_eq!(stopped, Some(Breaker::ScopeMissing));
    let failures = &report.failures[&FailureKind::ScopeMissing];
    assert_eq!((failures.repositories, failures.changes), (2, 3));
}

/// A repository hidden from the token tells nothing of its changes; others are still read.
#[tokio::test]
async fn an_unreadable_repository_shows_no_stacks_and_is_counted() {
    let fake = Fake::new([(WIDGETS, four()), (77, four())])
        .failing(Call::Page(77), FailureKind::Unreadable);
    let mut wanted = wanted(WIDGETS, &[2]);
    wanted.extend(self::wanted(77, &[2]));
    let (report, stopped) = stopped_by(&fake, wanted).await;

    assert!(stack_of(&report, WIDGETS, 2).is_some());
    assert_eq!(stack_of(&report, 77, 2), None);
    let failures = &report.failures[&FailureKind::Unreadable];
    assert_eq!((failures.repositories, failures.changes), (1, 1));
    assert_eq!(stopped, None);
}

/// When the numbers off the page cannot be asked, those changes are absent (openness unknown);
/// the ones on the page are walked.
#[tokio::test]
async fn changes_whose_openness_is_unknown_have_no_stack() {
    let fake = Fake::new([(WIDGETS, four())])
        .paged(2)
        .failing(Call::Numbers(WIDGETS, vec![4]), FailureKind::Failed);
    let report = stacks_of(&fake, wanted(WIDGETS, &[4, 2])).await;

    assert_eq!(stack_of(&report, WIDGETS, 4), None);
    assert!(stack_of(&report, WIDGETS, 2).is_some());
    let failures = &report.failures[&FailureKind::Failed];
    assert_eq!((failures.repositories, failures.changes), (1, 1));
}

/// Other failures cut the end they were asked for, as their kind.
#[tokio::test]
async fn a_failed_question_cuts_its_end_by_kind() {
    for (failure, reason) in [
        (FailureKind::Failed, StackCut::Failed),
        (FailureKind::Unreadable, StackCut::Unreadable),
    ] {
        let fake = Fake::new([(WIDGETS, four())])
            .paged(0)
            .failing(into(WIDGETS, "feature/parse-c"), failure);
        let report = stacks_of(&fake, wanted(WIDGETS, &[2])).await;
        assert_eq!(
            stack_of(&report, WIDGETS, 2),
            Some(&stack(
                2,
                &[1],
                reached("main"),
                complete(vec![node(3, cut(vec![], reason))])
            )),
            "{failure:?}"
        );
    }
}

/// Two walks that reach the same branch while its question is still in flight wait for that
/// one request: the question is held until both have asked.
#[tokio::test(start_paused = true)]
async fn walks_that_meet_share_one_request() {
    let changes = vec![
        mr(1, "a", "main"),
        mr(2, "b", "main"),
        mr(3, "c", "a"),
        mr(4, "d", "b"),
    ];
    let gate = Arc::new(Semaphore::new(0));
    let mut fake = Fake::new([(WIDGETS, changes)]).paged(0);
    fake.gate = Some(gate.clone());
    fake.gated = Some([from(WIDGETS, "main")].into());
    let budget = budget();

    let (report, ()) = tokio::join!(
        change_stacks(wanted(WIDGETS, &[1, 2]), &fake, &budget),
        async {
            // Paused time moves on only once nothing else can: both walks then wait at `main`.
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert_eq!(
                fake.in_flight.load(Ordering::SeqCst),
                1,
                "{:?}",
                fake.calls()
            );
            gate.add_permits(1000);
        }
    );

    assert_eq!(
        fake.calls()
            .iter()
            .filter(|call| **call == from(WIDGETS, "main"))
            .count(),
        1
    );
    assert_eq!(report.stacks.len(), 2);
}

/// A failed branch question shows as a cut in every stack that reaches it, and counts once,
/// with the request's own failure as its cause.
#[tokio::test]
async fn a_failed_question_is_counted_once_with_its_cause() {
    let cause = server_error("database is down");
    let fake = Fake::new([(WIDGETS, four())]).paged(0).failing(
        into(WIDGETS, "feature/parse-c"),
        LookupFailure::caused_by(FailureKind::Failed, cause.clone()),
    );
    let report = stacks_of(&fake, wanted(WIDGETS, &[1, 2, 3])).await;

    let failed = ChangeStackEnd::Cut {
        reason: StackCut::Failed,
    };
    for number in [1, 2, 3] {
        assert!(
            ends(stack_of(&report, WIDGETS, number).unwrap().above()).contains(&failed),
            "!{number}"
        );
    }
    assert_eq!(
        report.failures[&FailureKind::Failed],
        Failures {
            repositories: 0,
            changes: 0,
            questions: 1,
            cause: Some(LookupCause::Request(cause)),
        }
    );
}

fn server_error(message: &str) -> ApiError {
    ApiError::Status {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: message.to_string(),
        retry_after: None,
        refusal: None,
    }
}

/// No more requests are in flight than the host allows, and more than one is.
#[tokio::test(start_paused = true)]
async fn requests_in_flight_stay_within_the_hosts_concurrency() {
    let repos: Vec<(u64, Vec<OpenChange>)> = (100..110).map(|repo| (repo, four())).collect();
    let gate = Arc::new(Semaphore::new(0));
    let mut fake = Fake::new(repos.clone()).paged(0);
    fake.concurrency = 2;
    fake.gate = Some(gate.clone());
    let wanted: Vec<_> = repos
        .iter()
        .flat_map(|(repo, _)| wanted(*repo, &[2]))
        .collect();
    let budget = budget();

    let (report, ()) = tokio::join!(change_stacks(wanted, &fake, &budget), async {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            gate.add_permits(1);
        }
    });

    assert_eq!(fake.most_in_flight.load(Ordering::SeqCst), 2);
    assert_eq!(report.stacks.len(), 10);
}

/// Stacks do not depend on the order a host lists its changes in.
#[tokio::test]
async fn the_hosts_order_changes_nothing() {
    let changes = vec![
        mr(1, "a", "main"),
        mr(2, "b", "a"),
        mr(3, "c", "a"),
        mr(4, "d", "b"),
        mr(5, "b", "x"),
        mr(6, "e", "d"),
    ];
    let mut reversed = changes.clone();
    reversed.reverse();
    for page_size in [100, 0, 3] {
        let numbers = [1, 2, 3, 4, 6];
        let one = stacks_of(
            &Fake::new([(WIDGETS, changes.clone())]).paged(page_size),
            wanted(WIDGETS, &numbers),
        )
        .await;
        let other = stacks_of(
            &Fake::new([(WIDGETS, reversed.clone())]).paged(page_size),
            wanted(WIDGETS, &numbers),
        )
        .await;
        assert_eq!(one.stacks, other.stacks, "page of {page_size}");
        assert!(!one.stacks.is_empty());
    }
}

/// The newest event's change is walked first, so a short budget goes to it.
#[tokio::test]
async fn the_newest_change_is_walked_first() {
    let mut fake = Fake::new([(WIDGETS, four())]).paged(0);
    fake.concurrency = 1;
    let wanted = vec![
        (
            ChangeRef {
                repo: WIDGETS,
                number: 1,
            },
            at(1),
        ),
        (
            ChangeRef {
                repo: WIDGETS,
                number: 3,
            },
            at(30),
        ),
        (
            ChangeRef {
                repo: WIDGETS,
                number: 1,
            },
            at(50),
        ),
    ];
    stacks_of(&fake, wanted).await;

    let calls = fake.calls();
    assert_eq!(calls[1], Call::Numbers(WIDGETS, vec![1, 3]));
    assert_eq!(calls[2], from(WIDGETS, "main"), "{calls:?}");
}

/// A merged parent: retargeted, the next collection shows the new chain; not retargeted, the
/// way down ends at the branch no open change comes from any more.
#[tokio::test]
async fn a_merged_parent_shows_in_the_next_collection() {
    let before = stacks_of(&Fake::new([(WIDGETS, four())]), wanted(WIDGETS, &[3])).await;
    assert_eq!(stack_of(&before, WIDGETS, 3).unwrap().below().len(), 2);

    let retargeted = vec![
        mr(2, "feature/parse-b", "main"),
        mr(3, "feature/parse-c", "feature/parse-b"),
    ];
    let after = stacks_of(&Fake::new([(WIDGETS, retargeted)]), wanted(WIDGETS, &[3])).await;
    assert_eq!(
        stack_of(&after, WIDGETS, 3),
        Some(&stack(3, &[2], reached("main"), complete(vec![])))
    );

    let left = vec![
        mr(2, "feature/parse-b", "feature/parse-a"),
        mr(3, "feature/parse-c", "feature/parse-b"),
    ];
    let after = stacks_of(&Fake::new([(WIDGETS, left)]), wanted(WIDGETS, &[3])).await;
    assert_eq!(
        stack_of(&after, WIDGETS, 3),
        Some(&stack(
            3,
            &[2],
            reached("feature/parse-a"),
            complete(vec![])
        ))
    );
}

/// The walk is the host's whatever its repository key: here GitHub's `owner/repo`. Changes of
/// different repositories never chain, though their branches and numbers are the same.
#[tokio::test]
async fn any_repository_key_works_and_repositories_never_chain() {
    let acme = ("acme", "widgets");
    let dev = ("dev", "widgets");
    let fake = Fake::new([
        (acme, vec![mr(1, "a", "main"), mr(2, "b", "a")]),
        (dev, vec![mr(1, "a", "main")]),
    ])
    .paged(0);
    let mut wanted = wanted(acme, &[1]);
    wanted.extend(self::wanted(dev, &[1]));
    let report = stacks_of(&fake, wanted).await;

    assert_eq!(
        stack_of(&report, acme, 1),
        Some(&stack(1, &[], reached("main"), complete(vec![leaf(2)])))
    );
    assert_eq!(stack_of(&report, dev, 1), None);
}

fn log_lines(report: &StackReport<u64>, scope: &str) -> Vec<crate::model::LogEntry> {
    report.log(scope, "GitLab");
    crate::diagnostics::entries()
        .into_iter()
        .filter(|entry| entry.scope == scope)
        .collect()
}

fn levels(lines: &[crate::model::LogEntry]) -> Vec<LogLevel> {
    lines.iter().map(|line| line.level).collect()
}

/// The summary is one line, and each reason something is missing one warning, which names the
/// first request failure behind it once.
#[tokio::test]
async fn the_report_logs_one_line_per_reason_with_its_cause() {
    let hidden = ApiError::Status {
        status: reqwest::StatusCode::FORBIDDEN,
        message: "merge requests are turned off".to_string(),
        retry_after: None,
        refusal: None,
    };
    let fake = Fake::new([(WIDGETS, four()), (77, four()), (78, four())])
        .paged(0)
        .failing(
            Call::Page(77),
            LookupFailure::caused_by(FailureKind::Unreadable, hidden),
        )
        .failing(Call::Page(78), FailureKind::Unreadable)
        .failing(
            into(WIDGETS, "feature/parse-c"),
            LookupFailure::caused_by(FailureKind::Failed, server_error("database is down")),
        );
    let mut wanted = wanted(WIDGETS, &[2]);
    wanted.extend(self::wanted(77, &[2]));
    wanted.extend(self::wanted(78, &[2, 3]));
    let report = stacks_of(&fake, wanted).await;

    let lines = log_lines(&report, "change-stack-log-reasons");

    assert_eq!(
        levels(&lines),
        [LogLevel::Info, LogLevel::Warn, LogLevel::Warn]
    );
    for cause in ["merge requests are turned off", "database is down"] {
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.message.contains(cause))
                .count(),
            1,
            "{cause}: {lines:?}"
        );
    }
}

/// A refused token is one cause and one warning, however many changes it leaves unknown.
#[tokio::test]
async fn a_tripped_breaker_logs_one_warning() {
    let mut fake = Fake::new([(WIDGETS, four()), (77, four())])
        .paged(0)
        .failing(Call::Page(WIDGETS), FailureKind::TokenInvalid);
    fake.concurrency = 1;
    let mut wanted = wanted(WIDGETS, &[1, 2]);
    wanted.extend(self::wanted(77, &[3]));
    let (report, stopped) = stopped_by(&fake, wanted).await;

    assert_eq!(stopped, Some(Breaker::TokenInvalid));
    let lines = log_lines(&report, "change-stack-log-breaker");
    assert_eq!(levels(&lines), [LogLevel::Info, LogLevel::Warn]);
}

#[test]
fn counts_are_worded_for_one_and_for_many() {
    assert_eq!(super::counted(1, "change", "changes"), "1 change");
    assert_eq!(super::counted(0, "change", "changes"), "0 changes");
    assert_eq!(
        super::counted(2, "repository", "repositories"),
        "2 repositories"
    );
}

#[test]
fn the_limits_are_as_designed() {
    assert_eq!(
        (STACK_REQUESTS, STACK_BUDGET, STACK_DEPTH, STACK_MEMBERS),
        (50, Duration::from_secs(20), 12, 30)
    );
}

/// `annotate` runs in a boxed `Send` future, so the walk must be one.
#[test]
fn the_walk_is_a_send_future() {
    fn is_send<T: Send>(_: &T) {}
    let fake = Fake::<u64>::new([]);
    let budget = budget();
    let walk = change_stacks(Vec::new(), &fake, &budget);
    is_send(&walk);
}
