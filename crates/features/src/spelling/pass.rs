//! A check of a whole Document in chunks: the Blocks to check are taken in a snapshot, checked
//! a few at a time (on several threads when there are many) without holding the Document's
//! results, and the marks found so far are published as the work goes.

use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::document::{self, Checked};
use super::{Issue, Speller};

/// How many Blocks a worker takes at a time: small enough to stop soon when a newer check
/// starts, large enough that taking the next chunk costs nothing.
const CHUNK: usize = 4;

/// Fewer Blocks than this are checked on one thread: a helper takes about 0.2 s to start.
const PARALLEL_MIN: usize = 32;

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
    /// A newer check began; the Blocks checked so far are kept for it.
    Superseded,
}

/// How a pass is run.
pub struct Plan<'a> {
    /// Byte offset the check starts from: Blocks from there on are checked first.
    pub anchor: usize,
    /// Threads that check beside the caller; each loads a speller of its own.
    pub helpers: usize,
    /// The least time between two publishes; the first and the last are always sent.
    pub throttle: Duration,
    /// Whether this check is still the newest one.
    pub live: &'a (dyn Fn() -> bool + Sync),
    pub publish: &'a (dyn Fn(Progress) + Sync),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Checks the Blocks of the Document that `checked` has no result for. `checked` is held only
/// to read a snapshot and to add each chunk's results, never while a Block is being checked.
pub fn run(
    blocks: &[(usize, String)],
    checked: &Mutex<Checked>,
    primary: &Mutex<Speller>,
    plan: &Plan<'_>,
) -> Outcome {
    let fresh: Vec<String> = {
        let checked = lock(checked);
        document::unchecked_from(blocks, &checked, plan.anchor)
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    let next = AtomicUsize::new(0);
    let last_publish = Mutex::new(None::<Instant>);
    let work = |speller: &Mutex<Speller>| {
        while plan.live() {
            let start = next.fetch_add(CHUNK, Ordering::SeqCst);
            if start >= fresh.len() {
                return;
            }
            let chunk = &fresh[start..(start + CHUNK).min(fresh.len())];
            let found: Vec<(String, Vec<Issue>)> = {
                let mut speller = lock(speller);
                chunk
                    .iter()
                    .map(|text| (text.clone(), speller.check(text)))
                    .collect()
            };
            let mut checked = lock(checked);
            checked.extend(found);
            publish_if_due(plan, blocks, &checked, &last_publish);
        }
    };
    let helpers = if fresh.len() < PARALLEL_MIN {
        0
    } else {
        plan.helpers
    };
    std::thread::scope(|scope| {
        for _ in 0..helpers {
            scope.spawn(|| match Speller::new() {
                Ok(speller) => work(&Mutex::new(speller)),
                Err(error) => log::warn!("A spell-check helper could not start: {error}"),
            });
        }
        work(primary);
    });
    if !plan.live() {
        return Outcome::Superseded;
    }
    let mut checked = lock(checked);
    document::keep_only(&mut checked, blocks);
    (plan.publish)(progress(blocks, &checked));
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

/// Publishes while holding `checked`, so two workers' messages arrive in the order their
/// results were added: a later message always holds everything an earlier one does.
fn publish_if_due(
    plan: &Plan<'_>,
    blocks: &[(usize, String)],
    checked: &Checked,
    last_publish: &Mutex<Option<Instant>>,
) {
    if !plan.live() {
        return;
    }
    let mut last = lock(last_publish);
    if last.is_some_and(|at| at.elapsed() < plan.throttle) {
        return;
    }
    *last = Some(Instant::now());
    (plan.publish)(progress(blocks, checked));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

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

    fn plan<'a>(
        helpers: usize,
        live: &'a (dyn Fn() -> bool + Sync),
        publish: &'a (dyn Fn(Progress) + Sync),
    ) -> Plan<'a> {
        Plan {
            anchor: 0,
            helpers,
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

    #[test]
    fn chunked_checks_find_what_a_single_pass_finds() {
        let blocks = corpus(40);
        let expected = reference(&blocks);
        assert!(expected.len() > 30, "{}", expected.len());
        for helpers in [0, 3] {
            let checked = Mutex::new(Checked::new());
            let last = Mutex::new(None);
            let publish = |progress: Progress| *lock(&last) = Some(progress);
            let outcome = run(
                &blocks,
                &checked,
                &SPELLER,
                &plan(helpers, &|| true, &publish),
            );
            assert_eq!(outcome, Outcome::Complete);
            let last = lock(&last).take().expect("published");
            assert!(last.pending.is_empty());
            assert_eq!(last.issues, expected, "helpers {helpers}");
        }
    }

    #[test]
    fn marks_come_in_steps_and_none_goes_missing_once_shown() {
        let blocks = corpus(24);
        let steps = Mutex::new(Vec::<Progress>::new());
        let publish = |progress: Progress| lock(&steps).push(progress);
        let checked = Mutex::new(Checked::new());
        run(&blocks, &checked, &SPELLER, &plan(2, &|| true, &publish));
        let steps = lock(&steps);
        assert!(steps.len() > 2, "{} steps", steps.len());
        assert!(!steps[0].pending.is_empty());
        assert!(steps.last().expect("last").pending.is_empty());
        for pair in steps.windows(2) {
            assert!(
                pair[0]
                    .issues
                    .iter()
                    .all(|issue| pair[1].issues.contains(issue)),
                "an issue shown earlier went missing"
            );
        }
    }

    #[test]
    fn blocks_from_the_anchor_are_checked_first() {
        let blocks = corpus(24);
        let anchor = blocks[18].0;
        let first = Mutex::new(None::<Progress>);
        let publish = |progress: Progress| {
            lock(&first).get_or_insert(progress);
        };
        let checked = Mutex::new(Checked::new());
        let mut plan = plan(0, &|| true, &publish);
        plan.anchor = anchor;
        run(&blocks, &checked, &SPELLER, &plan);
        let first = lock(&first).take().expect("published");
        assert!(first.issues.iter().all(|issue| issue.range.start >= anchor));
        assert!(first.pending.iter().any(|range| range.start < anchor));
    }

    #[test]
    fn a_newer_check_stops_this_one_between_chunks_and_keeps_the_results() {
        let blocks = corpus(40);
        let stale = AtomicBool::new(false);
        let publish = |_: Progress| stale.store(true, Ordering::SeqCst);
        let live = || !stale.load(Ordering::SeqCst);
        let checked = Mutex::new(Checked::new());
        let outcome = run(&blocks, &checked, &SPELLER, &plan(0, &live, &publish));
        assert_eq!(outcome, Outcome::Superseded);
        let kept = lock(&checked).len();
        assert!(kept >= CHUNK && kept < blocks.len(), "{kept} kept");
        let resumed = run(&blocks, &checked, &SPELLER, &plan(0, &|| true, &|_| {}));
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
        run(&blocks, &checked, &SPELLER, &plan(0, &|| true, &|_| {}));
        let count = AtomicUsize::new(0);
        let publish = |progress: Progress| {
            assert!(progress.pending.is_empty());
            count.fetch_add(1, Ordering::SeqCst);
        };
        run(&blocks, &checked, &SPELLER, &plan(2, &|| true, &publish));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
