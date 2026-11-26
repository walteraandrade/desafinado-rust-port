//! A check of a whole Document in chunks: the Blocks to check are taken in a snapshot, checked
//! a few at a time without holding the Document's results, and the marks found so far are
//! published as the work goes.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::document::{self, Checked, Delta};
use super::{Issue, Speller};

/// How many Blocks a worker takes at a time: small enough to stop soon when a newer check
/// starts, large enough that taking the next chunk costs nothing.
const CHUNK: usize = 4;

/// What a check sends as it goes: everything known so far at first and at the end, and in
/// between only what each chunk added.
#[derive(Debug)]
pub enum Update {
    Snapshot(Progress),
    Delta(Delta),
}

/// What a check knows so far.
#[derive(Debug)]
pub struct Progress {
    /// The issues of every Block checked, in Document order.
    pub issues: Vec<Issue>,
    /// Where the Blocks still unchecked sit; empty once the check is complete.
    pub pending: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    /// A newer check began; the Blocks checked so far are kept for it. Forgetting the Blocks
    /// that left the Document (`keep_only`) waits for the next complete pass.
    Superseded,
}

/// How a pass is run.
pub struct Plan<'a> {
    /// Byte offset to check from, read before each chunk: Blocks from there on are checked
    /// first, so scrolling during a long check moves the work to what the writer sees.
    pub anchor: &'a (dyn Fn() -> usize + Sync),
    /// The least time between two publishes; the first and the last are always sent.
    pub throttle: Duration,
    /// Whether this check is still the newest one.
    pub live: &'a (dyn Fn() -> bool + Sync),
    pub publish: &'a (dyn Fn(Update) + Sync),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Checks one chunk of Blocks: each Block's text with the issues found in it.
pub type CheckChunk = dyn Fn(Vec<String>) -> Vec<(String, Vec<Issue>)> + Sync;

/// Checks `chunk` with `speller`, Block by Block.
pub fn check_chunk(speller: &mut Speller, chunk: Vec<String>) -> Vec<(String, Vec<Issue>)> {
    chunk
        .into_iter()
        .map(|text| {
            let issues = speller.check(&text);
            (text, issues)
        })
        .collect()
}

/// Checks the Blocks of the Document that `checked` has no result for. `checked` is held only
/// to read a snapshot and to add each chunk's results, never while a Block is being checked.
pub fn run(
    blocks: &[(usize, String)],
    checked: &Mutex<Checked>,
    check: &CheckChunk,
    plan: &Plan<'_>,
) -> Outcome {
    let mut todo = document::unchecked_at(blocks, &lock(checked));
    let places = places_of(blocks, &todo);
    let mut last_publish = None::<Instant>;
    let mut fresh: Vec<(String, Vec<Issue>)> = Vec::new();
    while plan.live() {
        let chunk = document::take_chunk(&mut todo, (plan.anchor)(), CHUNK);
        if chunk.is_empty() {
            break;
        }
        let found = check(chunk);
        let mut checked = lock(checked);
        checked.extend(found.iter().cloned());
        fresh.extend(found);
        if due(plan, last_publish) {
            let update = match last_publish {
                None => {
                    fresh.clear();
                    Update::Snapshot(progress(blocks, &checked))
                }
                Some(_) => Update::Delta(delta(&places, std::mem::take(&mut fresh))),
            };
            last_publish = Some(Instant::now());
            (plan.publish)(update);
        }
    }
    let mut checked = lock(checked);
    if !plan.live() {
        return Outcome::Superseded;
    }
    document::keep_only(&mut checked, blocks);
    (plan.publish)(Update::Snapshot(progress(blocks, &checked)));
    Outcome::Complete
}

impl Plan<'_> {
    fn live(&self) -> bool {
        (self.live)()
    }
}

fn progress(blocks: &[(usize, String)], checked: &Checked) -> Progress {
    Progress {
        issues: document::place(blocks, checked),
        pending: document::pending_ranges(blocks, checked),
    }
}

/// Where each Block text in `todo` sits in the Document.
fn places_of<'a>(
    blocks: &'a [(usize, String)],
    todo: &[(usize, String)],
) -> HashMap<&'a str, Vec<usize>> {
    let wanted: std::collections::HashSet<&str> =
        todo.iter().map(|(_, text)| text.as_str()).collect();
    let mut places = HashMap::<&str, Vec<usize>>::new();
    for (from, text) in blocks {
        if wanted.contains(text.as_str()) {
            places.entry(text.as_str()).or_default().push(*from);
        }
    }
    places
}

/// The Blocks `fresh` finished, at every place they occur, with the issues found there.
fn delta(places: &HashMap<&str, Vec<usize>>, fresh: Vec<(String, Vec<Issue>)>) -> Delta {
    let mut found = Delta::default();
    for (text, issues) in &fresh {
        for from in places.get(text.as_str()).into_iter().flatten() {
            found.blocks.push(*from..*from + text.len());
            found
                .issues
                .extend(issues.iter().map(|issue| issue.shifted(*from)));
        }
    }
    found.blocks.sort_by_key(|range| range.start);
    found
        .issues
        .sort_by_key(|issue| (issue.range.start, issue.range.end));
    found
}

/// Whether a publish is due: the first always is, the next ones after the throttle.
fn due(plan: &Plan<'_>, last_publish: Option<Instant>) -> bool {
    plan.live() && last_publish.is_none_or(|at| at.elapsed() >= plan.throttle)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    static SPELLER: std::sync::LazyLock<Mutex<Speller>> =
        std::sync::LazyLock::new(|| Mutex::new(Speller::new().expect("dictionaries load")));

    const ENGLISH: [&str; 4] = [
        "I belive that the comming years will show he was a genious.",
        "This idea could of worked, and they has the money, teh whole lot.",
        "Rode `cargo fmtt` and then the writter reads every paragrah slowly.",
        "# Heading number",
    ];
    const PORTUGUESE: [&str; 3] = [
        "Ele não sabia que a excessão era tão grande, mas concerteza vai dar certo.",
        "Ação e emoção nao faltam, e o app quebrou no login de novo.",
        "Os meninos foram para a escola e encontraram o professor.",
    ];

    fn corpus(count: usize) -> Vec<(usize, String)> {
        let mut from = 0;
        (0..count)
            .map(|index| {
                let base = if index % 3 == 2 {
                    PORTUGUESE[index % PORTUGUESE.len()]
                } else {
                    ENGLISH[index % ENGLISH.len()]
                };
                let text = format!("{base} Item {}.", "x".repeat(index % 7));
                let block = (from, text);
                from += block.1.len() + 2;
                block
            })
            .collect()
    }

    fn local(chunk: Vec<String>) -> Vec<(String, Vec<Issue>)> {
        check_chunk(&mut lock(&SPELLER), chunk)
    }

    fn plan<'a>(
        live: &'a (dyn Fn() -> bool + Sync),
        publish: &'a (dyn Fn(Update) + Sync),
    ) -> Plan<'a> {
        Plan {
            anchor: &|| 0,
            throttle: Duration::ZERO,
            live,
            publish,
        }
    }

    fn reference(blocks: &[(usize, String)]) -> Vec<Issue> {
        let mut checked = Checked::new();
        let mut speller = lock(&SPELLER);
        for text in document::unchecked(blocks, &Checked::new()) {
            checked.insert(text.to_string(), speller.check(text));
        }
        document::place(blocks, &checked)
    }

    fn sorted(mut issues: Vec<Issue>) -> Vec<Issue> {
        issues.sort_by_key(|issue| (issue.range.start, issue.range.end));
        issues
    }

    /// What the window shows after each update, the way the app applies them.
    fn shown_after_each(updates: &[Update], mut shown: Vec<Issue>) -> Vec<Vec<Issue>> {
        updates
            .iter()
            .map(|update| {
                match update {
                    Update::Snapshot(progress) => {
                        shown = document::merge_progress(
                            &shown,
                            &progress.pending,
                            progress.issues.clone(),
                        );
                    }
                    Update::Delta(delta) => {
                        document::apply_delta(&mut shown, delta.clone());
                    }
                }
                shown.clone()
            })
            .collect()
    }

    fn collect(blocks: &[(usize, String)], anchor: usize, throttle: Duration) -> Vec<Update> {
        let updates = Mutex::new(Vec::<Update>::new());
        let publish = |update: Update| lock(&updates).push(update);
        let checked = Mutex::new(Checked::new());
        let at = || anchor;
        let mut plan = plan(&|| true, &publish);
        plan.anchor = &at;
        plan.throttle = throttle;
        run(blocks, &checked, &local, &plan);
        updates.into_inner().expect("updates")
    }

    #[test]
    fn chunked_checks_find_what_a_single_pass_finds() {
        let blocks = corpus(40);
        let expected = reference(&blocks);
        assert!(expected.len() > 30, "{}", expected.len());
        let updates = collect(&blocks, 0, Duration::ZERO);
        let Some(Update::Snapshot(last)) = updates.last() else {
            panic!("the last update is a snapshot");
        };
        assert!(last.pending.is_empty());
        assert_eq!(last.issues, expected);
    }

    #[test]
    fn applying_the_steps_one_by_one_ends_at_the_whole_document_check() {
        let blocks = corpus(40);
        let updates = collect(&blocks, 0, Duration::ZERO);
        let Some(Update::Snapshot(first)) = updates.first() else {
            panic!("the first update is a snapshot");
        };
        let deltas_only = &updates[..updates.len() - 1];
        assert!(!first.pending.is_empty());
        let shown = shown_after_each(deltas_only, Vec::new());
        assert_eq!(shown.last(), Some(&reference(&blocks)));
    }

    #[test]
    fn marks_come_in_steps_and_none_goes_missing_once_shown() {
        let blocks = corpus(24);
        let updates = collect(&blocks, 0, Duration::ZERO);
        assert!(updates.len() > 2, "{} updates", updates.len());
        assert!(
            updates[1..updates.len() - 1]
                .iter()
                .all(|update| matches!(update, Update::Delta(_)))
        );
        let shown = shown_after_each(&updates, Vec::new());
        for pair in shown.windows(2) {
            assert!(
                pair[0].iter().all(|issue| pair[1].contains(issue)),
                "an issue shown earlier went missing"
            );
        }
    }

    #[test]
    fn steps_send_only_what_a_chunk_added_not_everything_so_far() {
        let blocks = corpus(40);
        let updates = collect(&blocks, 0, Duration::ZERO);
        let sizes: Vec<usize> = updates
            .iter()
            .filter_map(|update| match update {
                Update::Delta(delta) => Some(delta.blocks.len()),
                Update::Snapshot(_) => None,
            })
            .collect();
        assert!(sizes.len() > 3);
        assert!(sizes.iter().sum::<usize>() <= blocks.len(), "{sizes:?}");
    }

    #[test]
    fn marks_shown_before_the_check_stay_until_their_block_is_checked() {
        let blocks = corpus(24);
        let truth = reference(&blocks);
        let stale = |at: usize| Issue {
            range: at..at + 1,
            kind: crate::spelling::IssueKind::Grammar,
            message: "stale".into(),
            replacements: vec![],
        };
        let mut previous = truth.clone();
        previous.extend(blocks.iter().map(|(from, _)| stale(*from)));
        let previous = sorted(previous);
        let updates = collect(&blocks, 0, Duration::ZERO);
        let shown = shown_after_each(&updates, previous.clone());
        let mut done = Vec::<Range<usize>>::new();
        for (update, now) in updates.iter().zip(&shown) {
            match update {
                Update::Delta(delta) => done.extend(delta.blocks.clone()),
                Update::Snapshot(progress) => {
                    done = blocks
                        .iter()
                        .map(|(from, text)| *from..*from + text.len())
                        .filter(|range| !progress.pending.contains(range))
                        .collect();
                }
            }
            let inside = |issue: &Issue| {
                done.iter()
                    .any(|range| range.start <= issue.range.start && issue.range.end <= range.end)
            };
            let expected = sorted(
                previous
                    .iter()
                    .filter(|issue| !inside(issue))
                    .chain(truth.iter().filter(|issue| inside(issue)))
                    .cloned()
                    .collect(),
            );
            assert_eq!(now, &expected);
        }
    }

    #[test]
    fn a_repeated_block_text_is_marked_at_every_place_by_one_check() {
        let blocks: Vec<(usize, String)> = vec![
            (0, "I belive it.".into()),
            (20, "Fine words.".into()),
            (40, "I belive it.".into()),
        ];
        let updates = collect(&blocks, 0, Duration::ZERO);
        let shown = shown_after_each(&updates, Vec::new());
        let last = shown.last().expect("shown");
        assert_eq!(last, &reference(&blocks));
        assert!(last.iter().any(|issue| issue.range.start >= 40));
    }

    #[test]
    fn blocks_from_the_anchor_are_checked_first() {
        let blocks = corpus(24);
        let anchor = blocks[18].0;
        let updates = collect(&blocks, anchor, Duration::ZERO);
        let Some(Update::Snapshot(first)) = updates.first() else {
            panic!("the first update is a snapshot");
        };
        assert!(first.issues.iter().all(|issue| issue.range.start >= anchor));
        assert!(first.pending.iter().any(|range| range.start < anchor));
    }

    #[test]
    fn a_newer_check_stops_this_one_between_chunks_and_keeps_the_results() {
        let blocks = corpus(40);
        let stale = AtomicBool::new(false);
        let publish = |_: Update| stale.store(true, Ordering::SeqCst);
        let live = || !stale.load(Ordering::SeqCst);
        let checked = Mutex::new(Checked::new());
        let outcome = run(&blocks, &checked, &local, &plan(&live, &publish));
        assert_eq!(outcome, Outcome::Superseded);
        let kept = lock(&checked).len();
        assert!(kept >= CHUNK && kept < blocks.len(), "{kept} kept");
        let resumed = run(&blocks, &checked, &local, &plan(&|| true, &|_| {}));
        assert_eq!(resumed, Outcome::Complete);
        assert_eq!(
            document::place(&blocks, &lock(&checked)),
            reference(&blocks)
        );
    }

    #[test]
    fn a_check_with_nothing_new_still_publishes_the_issues_once() {
        let blocks = corpus(6);
        let checked = Mutex::new(Checked::new());
        run(&blocks, &checked, &local, &plan(&|| true, &|_| {}));
        let count = AtomicUsize::new(0);
        let publish = |update: Update| {
            let Update::Snapshot(progress) = update else {
                panic!("nothing to send but a snapshot");
            };
            assert!(progress.pending.is_empty());
            count.fetch_add(1, Ordering::SeqCst);
        };
        run(&blocks, &checked, &local, &plan(&|| true, &publish));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn scrolling_during_a_check_moves_the_work_to_what_is_on_screen() {
        let blocks = corpus(40);
        let anchor = AtomicUsize::new(0);
        let updates = Mutex::new(Vec::<Update>::new());
        let publish = |update: Update| {
            let mut updates = lock(&updates);
            if updates.is_empty() {
                anchor.store(blocks[30].0, Ordering::SeqCst);
            }
            updates.push(update);
        };
        let checked = Mutex::new(Checked::new());
        let load = || anchor.load(Ordering::SeqCst);
        let mut plan = plan(&|| true, &publish);
        plan.anchor = &load;
        run(&blocks, &checked, &local, &plan);
        let updates = lock(&updates);
        let Update::Delta(second) = &updates[1] else {
            panic!("the second update is a delta");
        };
        assert!(
            second
                .issues
                .iter()
                .any(|issue| issue.range.start >= blocks[30].0)
        );
        assert!(
            second
                .blocks
                .iter()
                .all(|range| range.start >= blocks[30].0)
        );
    }

    #[test]
    fn a_pass_superseded_before_it_cleans_up_leaves_the_results_alone() {
        let blocks = corpus(6);
        let calls = AtomicUsize::new(0);
        let cleanup_lock_held = AtomicBool::new(false);
        let checked = Mutex::new(Checked::new());
        let counting = || {
            calls.fetch_add(1, Ordering::SeqCst);
            true
        };
        run(&blocks, &checked, &local, &plan(&counting, &|_| {}));
        let total = calls.load(Ordering::SeqCst);

        calls.store(0, Ordering::SeqCst);
        let checked = Mutex::new(Checked::new());
        lock(&checked).insert("a newer pass's Block".into(), vec![]);
        let live = || {
            cleanup_lock_held.store(checked.try_lock().is_err(), Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst) + 1 < total
        };
        let outcome = run(&blocks, &checked, &local, &plan(&live, &|_| {}));
        assert_eq!(outcome, Outcome::Superseded);
        assert!(
            cleanup_lock_held.load(Ordering::SeqCst),
            "liveness was read before the results were locked"
        );
        assert!(lock(&checked).contains_key("a newer pass's Block"));
    }

    #[test]
    fn publishes_are_spaced_but_the_last_one_always_goes() {
        let blocks = corpus(24);
        let updates = collect(&blocks, 0, Duration::from_hours(1));
        assert_eq!(updates.len(), 2);
        let (Update::Snapshot(first), Update::Snapshot(last)) = (&updates[0], &updates[1]) else {
            panic!("both are snapshots");
        };
        assert!(!first.pending.is_empty());
        assert!(last.pending.is_empty());
    }
}
