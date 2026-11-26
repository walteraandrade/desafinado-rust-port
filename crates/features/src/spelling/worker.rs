//! The one thread every spell check runs on. Harper keeps the tables of its fuzzy search per
//! thread, and the widest one takes about a second to build: a long unknown word with no close
//! match ("Antigravity") needs it. On a pool of threads each one built it again.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

pub type Job = Box<dyn FnOnce() + Send>;

/// The worker thread is gone, so the job was dropped without running.
#[derive(Debug, thiserror::Error)]
#[error("the spelling thread has stopped")]
pub struct Stopped;

pub struct Worker {
    jobs: Sender<Job>,
}

impl Worker {
    /// Starts the thread. `warm_up` runs once, the first time the queue is empty after a job,
    /// so it never delays the first check.
    pub fn spawn(name: &str, warm_up: impl FnOnce() + Send + 'static) -> io::Result<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name(name.into())
            .spawn(move || serve(&queue, warm_up))?;
        Ok(Self { jobs })
    }

    /// Queues `job` behind the ones already sent.
    pub fn submit(&self, job: Job) -> Result<(), Stopped> {
        self.jobs.send(job).map_err(|_dropped| Stopped)
    }
}

fn serve(queue: &Receiver<Job>, warm_up: impl FnOnce()) {
    let mut warm_up = Some(warm_up);
    let mut ran_a_job = false;
    loop {
        let job = match queue.try_recv() {
            Ok(job) => job,
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {
                if ran_a_job && let Some(warm_up) = warm_up.take() {
                    run(warm_up);
                    continue;
                }
                match queue.recv() {
                    Ok(job) => job,
                    Err(_) => return,
                }
            }
        };
        run(job);
        ran_a_job = true;
    }
}

/// A job that panics loses its own result, not the thread every later check needs.
fn run(job: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(job)).is_err() {
        log::error!("A spell check panicked");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread::{self, ThreadId};
    use std::time::Duration;

    use super::Worker;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn every_job_runs_in_order_on_the_same_thread() {
        let worker = Worker::spawn("test-spelling", || {}).unwrap();
        let (sender, results) = mpsc::channel::<(usize, ThreadId)>();
        for index in 0..5 {
            let sender = sender.clone();
            worker
                .submit(Box::new(move || {
                    sender.send((index, thread::current().id())).unwrap();
                }))
                .unwrap();
        }
        let seen: Vec<_> = (0..5)
            .map(|_| results.recv_timeout(WAIT).unwrap())
            .collect();
        assert_eq!(
            seen.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert!(seen.iter().all(|(_, id)| *id == seen[0].1));
        assert_ne!(seen[0].1, thread::current().id());
    }

    #[test]
    fn warms_up_once_after_the_first_job_and_on_the_same_thread() {
        let (sender, events) = mpsc::channel::<(&str, ThreadId)>();
        let warm = sender.clone();
        let worker = Worker::spawn("test-spelling", move || {
            warm.send(("warm", thread::current().id())).unwrap();
        })
        .unwrap();
        assert!(events.recv_timeout(Duration::from_millis(100)).is_err());
        let job = |sender: mpsc::Sender<(&'static str, ThreadId)>| -> super::Job {
            Box::new(move || sender.send(("job", thread::current().id())).unwrap())
        };
        worker.submit(job(sender.clone())).unwrap();
        let mut seen = vec![
            events.recv_timeout(WAIT).unwrap(),
            events.recv_timeout(WAIT).unwrap(),
        ];
        worker.submit(job(sender)).unwrap();
        seen.push(events.recv_timeout(WAIT).unwrap());
        assert!(events.recv_timeout(Duration::from_millis(100)).is_err());
        assert_eq!(
            seen.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["job", "warm", "job"]
        );
        assert!(seen.iter().all(|(_, id)| *id == seen[0].1));
    }

    #[test]
    fn a_job_that_panics_leaves_the_thread_running() {
        let worker = Worker::spawn("test-spelling", || {}).unwrap();
        worker.submit(Box::new(|| panic!("boom"))).unwrap();
        let (sender, results) = mpsc::channel();
        worker
            .submit(Box::new(move || sender.send(()).unwrap()))
            .unwrap();
        assert!(results.recv_timeout(WAIT).is_ok());
    }
}
