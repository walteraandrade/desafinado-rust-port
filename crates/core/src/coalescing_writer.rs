//! One serialized writer that keeps only the newest job per key.
//!
//! A job is a write that closes over a snapshot taken when it was submitted. Submitting a
//! second job under the same key before the first has started replaces it, so a burst of
//! edits costs one write. Jobs run one batch at a time, in submission order across batches,
//! so an older write can never land after a newer one.

use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

thread_local! {
    static WRITING: Cell<bool> = const { Cell::new(false) };
}

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Pending {
    jobs: BTreeMap<String, (u64, Job)>,
    next: u64,
    draining: bool,
}

#[derive(Default)]
struct Shared {
    pending: Mutex<Pending>,
    running: Mutex<()>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn run_batch(&self) {
        if WRITING.get() {
            return;
        }
        let _running = locked(&self.running);
        WRITING.set(true);
        let mut batch: Vec<(u64, Job)> = std::mem::take(&mut locked(&self.pending).jobs)
            .into_values()
            .collect();
        batch.sort_by_key(|(order, _)| *order);
        for (_, job) in batch {
            job();
        }
        WRITING.set(false);
    }
}

#[derive(Clone, Default)]
pub struct CoalescingWriter {
    shared: Arc<Shared>,
}

/// The loop that writes what was submitted; run it off the UI thread.
pub struct Drain {
    shared: Arc<Shared>,
}

impl Drain {
    pub fn run(self) {
        loop {
            self.shared.run_batch();
            let mut pending = locked(&self.shared.pending);
            if pending.jobs.is_empty() {
                pending.draining = false;
                return;
            }
        }
    }
}

impl CoalescingWriter {
    /// Queues `job` under `key`, replacing the one waiting there. Returns the [`Drain`] to run
    /// in the background when none is running already.
    pub fn submit(
        &self,
        key: impl Into<String>,
        job: impl FnOnce() + Send + 'static,
    ) -> Option<Drain> {
        let mut pending = locked(&self.shared.pending);
        let order = pending.next;
        pending.next += 1;
        pending.jobs.insert(key.into(), (order, Box::new(job)));
        if pending.draining {
            return None;
        }
        pending.draining = true;
        Some(Drain {
            shared: Arc::clone(&self.shared),
        })
    }

    /// Writes everything submitted so far, on this thread, and waits for a write in progress.
    /// Called from inside a job it does nothing: that job is the write in progress.
    pub fn flush(&self) {
        self.shared.run_batch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> Arc<Mutex<Vec<String>>> {
        Arc::default()
    }

    fn push(log: &Arc<Mutex<Vec<String>>>, entry: &str) -> impl FnOnce() + Send + 'static + use<> {
        let log = Arc::clone(log);
        let entry = entry.to_string();
        move || locked(&log).push(entry)
    }

    #[test]
    fn keeps_only_the_newest_job_per_key() {
        let writer = CoalescingWriter::default();
        let written = log();
        let drain = writer.submit("a", push(&written, "a1"));
        writer.submit("a", push(&written, "a2"));
        writer.submit("a", push(&written, "a3"));
        drain.expect("first submit starts the drain").run();
        assert_eq!(*locked(&written), ["a3"]);
    }

    #[test]
    fn writes_keys_in_the_order_of_their_newest_submission() {
        let writer = CoalescingWriter::default();
        let written = log();
        let drain = writer.submit("a", push(&written, "a1"));
        writer.submit("b", push(&written, "b1"));
        writer.submit("a", push(&written, "a2"));
        drain.expect("drain").run();
        assert_eq!(*locked(&written), ["b1", "a2"]);
    }

    #[test]
    fn a_second_submit_while_draining_starts_no_second_drain() {
        let writer = CoalescingWriter::default();
        let written = log();
        assert!(writer.submit("a", push(&written, "a")).is_some());
        assert!(writer.submit("b", push(&written, "b")).is_none());
    }

    #[test]
    fn a_new_drain_starts_after_the_last_one_finished() {
        let writer = CoalescingWriter::default();
        let written = log();
        writer
            .submit("a", push(&written, "a"))
            .expect("drain")
            .run();
        assert!(writer.submit("a", push(&written, "a2")).is_some());
    }

    #[test]
    fn flush_writes_the_newest_text_before_returning() {
        let writer = CoalescingWriter::default();
        let written = log();
        let _drain = writer.submit("a", push(&written, "old"));
        writer.submit("a", push(&written, "newest"));
        writer.flush();
        assert_eq!(*locked(&written), ["newest"]);
    }

    #[test]
    fn an_older_write_never_lands_after_a_newer_one() {
        let writer = CoalescingWriter::default();
        let written = log();
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        let slow = {
            let inner = push(&written, "old");
            move || {
                started.send(()).expect("receiver alive");
                wait_release.recv().expect("sender alive");
                inner();
            }
        };
        let drain = writer.submit("k", slow).expect("drain");
        let background = std::thread::spawn(move || drain.run());
        wait_started.recv().expect("slow job started");
        writer.submit("k", push(&written, "new"));
        let flusher = {
            let writer = writer.clone();
            std::thread::spawn(move || writer.flush())
        };
        release.send(()).expect("slow job waiting");
        background.join().expect("drain");
        flusher.join().expect("flush");
        assert_eq!(*locked(&written), ["old", "new"]);
    }

    #[test]
    fn flushing_from_inside_a_job_does_not_wait_for_itself() {
        let writer = CoalescingWriter::default();
        let written = log();
        let inner = writer.clone();
        let push_outer = push(&written, "outer");
        let drain = writer.submit("a", move || {
            inner.flush();
            push_outer();
        });
        drain.expect("drain").run();
        assert_eq!(*locked(&written), ["outer"]);
    }
}
