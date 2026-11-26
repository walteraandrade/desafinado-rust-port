//! The one thread every Block is spell checked on. Harper keeps the tables of its fuzzy search
//! per thread, and the widest one takes about a second to build: a long unknown word with no
//! close match ("Antigravity") needs it. On a pool of threads each one built it again.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

type Job = Box<dyn FnOnce() + Send>;

/// The job did not finish: it panicked, or the worker thread is gone.
#[derive(Debug, thiserror::Error)]
#[error("the spelling thread did not finish the job")]
pub struct Stopped;

pub struct Worker {
    jobs: Sender<Job>,
}

impl Worker {
    /// Starts the thread. `warm_up` runs once, after the first job, when no job has come for
    /// `idle`: a check is done by then, and the writer is most likely reading or typing.
    pub fn spawn(
        name: &str,
        idle: Duration,
        warm_up: impl FnOnce() + Send + 'static,
    ) -> io::Result<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name(name.into())
            .spawn(move || serve(&queue, idle, warm_up))?;
        Ok(Self { jobs })
    }

    /// Runs `job` on the worker thread, behind the jobs already sent, and waits for it.
    pub fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Stopped> {
        let (reply, result) = mpsc::sync_channel(1);
        self.jobs
            .send(Box::new(move || {
                if reply.send(job()).is_err() {
                    log::debug!("A spell check finished after its caller stopped waiting");
                }
            }))
            .map_err(|_gone| Stopped)?;
        result.recv().map_err(|_dropped| Stopped)
    }
}

fn serve(queue: &Receiver<Job>, idle: Duration, warm_up: impl FnOnce()) {
    let mut warm_up = Some(warm_up);
    let Ok(mut job) = queue.recv() else {
        return;
    };
    loop {
        guarded(job);
        let next = match warm_up.take() {
            None => queue.recv().ok(),
            Some(warm) => match queue.recv_timeout(idle) {
                Ok(next) => {
                    warm_up = Some(warm);
                    Some(next)
                }
                Err(RecvTimeoutError::Timeout) => {
                    guarded(warm);
                    queue.recv().ok()
                }
                Err(RecvTimeoutError::Disconnected) => None,
            },
        };
        // `None` means every sender is gone: the app is quitting.
        let Some(next) = next else {
            return;
        };
        job = next;
    }
}

/// A job that panics loses its own result, not the thread every later check needs.
fn guarded(job: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(job)).is_err() {
        log::error!("A spell check panicked");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread::{self, ThreadId};
    use std::time::Duration;

    use super::Worker;

    const IDLE: Duration = Duration::from_secs(1);
    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn every_job_runs_on_the_same_other_thread_and_returns_its_result() {
        let worker = Worker::spawn("test-spelling", IDLE, || {}).unwrap();
        let ids: Vec<ThreadId> = (0..3)
            .map(|_| worker.run(|| thread::current().id()).unwrap())
            .collect();
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_ne!(ids[0], thread::current().id());
        assert_eq!(worker.run(|| 2 + 2).unwrap(), 4);
    }

    #[test]
    fn jobs_from_many_threads_run_one_at_a_time() {
        let worker = Worker::spawn("test-spelling", IDLE, || {}).unwrap();
        let (sender, events) = mpsc::channel::<&str>();
        thread::scope(|scope| {
            for _ in 0..4 {
                let sender = sender.clone();
                let worker = &worker;
                scope.spawn(move || {
                    worker
                        .run(move || {
                            sender.send("start").unwrap();
                            thread::sleep(Duration::from_millis(10));
                            sender.send("end").unwrap();
                        })
                        .unwrap();
                });
            }
        });
        let seen: Vec<_> = events.try_iter().collect();
        assert_eq!(seen, ["start", "end"].repeat(4));
    }

    #[test]
    fn warms_up_once_on_the_same_thread_after_a_job_and_a_pause() {
        let (sender, warmed) = mpsc::channel::<ThreadId>();
        let open = sender.clone();
        let worker = Worker::spawn("test-spelling", IDLE, move || {
            sender.send(thread::current().id()).unwrap();
        })
        .unwrap();
        let quiet = |wait| warmed.recv_timeout(wait) == Err(RecvTimeoutError::Timeout);
        assert!(quiet(IDLE / 2));
        let worker_thread = worker.run(|| thread::current().id()).unwrap();
        assert!(quiet(IDLE / 4));
        assert_eq!(warmed.recv_timeout(WAIT).unwrap(), worker_thread);
        worker.run(|| ()).unwrap();
        assert!(quiet(IDLE + IDLE / 2));
        drop(open);
    }

    #[test]
    fn a_job_that_panics_fails_alone() {
        let worker = Worker::spawn("test-spelling", IDLE, || {}).unwrap();
        assert!(worker.run(|| panic!("boom")).is_err());
        assert_eq!(worker.run(|| "still here").unwrap(), "still here");
    }
}
