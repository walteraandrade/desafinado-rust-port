use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use desafinado_core::log_err::LogErr;
use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::classify::WatchEventKind;
use super::{DirectoryWatch, RawWatchEvent, RawWatchMessage, WatchHandle, WatchSink};

type Routes = HashMap<PathBuf, Vec<(u64, WatchSink)>>;

struct Shared {
    watcher: Mutex<RecommendedWatcher>,
    routes: Arc<Mutex<Routes>>,
    next_id: Mutex<u64>,
}

type SharedCell = Arc<Mutex<Option<Arc<Shared>>>>;

/// One notify watcher (one thread, one inotify instance) serves every
/// directory watched through clones of the same value.
#[derive(Clone, Default)]
pub struct NotifyDirectoryWatch {
    shared: SharedCell,
}

impl std::fmt::Debug for NotifyDirectoryWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotifyDirectoryWatch")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct NotifyHandle {
    shared: Option<Arc<Shared>>,
    dir: PathBuf,
    id: u64,
}

impl WatchHandle for NotifyHandle {
    fn close(&mut self) {
        let Some(shared) = self.shared.take() else {
            return;
        };
        let mut watcher = lock(&shared.watcher);
        let last = {
            let mut routes = lock(&shared.routes);
            let Some(sinks) = routes.get_mut(&self.dir) else {
                return;
            };
            sinks.retain(|(id, _)| *id != self.id);
            let empty = sinks.is_empty();
            if empty {
                routes.remove(&self.dir);
            }
            empty
        };
        if last {
            watcher.unwatch(&self.dir).log_warn();
        }
    }
}

impl Drop for NotifyHandle {
    fn drop(&mut self) {
        self.close();
    }
}

fn kind_of(kind: EventKind) -> Option<WatchEventKind> {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => {
            Some(WatchEventKind::Rename)
        }
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => Some(WatchEventKind::Change),
        EventKind::Access(_) => None,
    }
}

fn send(routes: &Routes, dir: &Path, make: &dyn Fn() -> RawWatchMessage) {
    for (_, sink) in routes.get(dir).into_iter().flatten() {
        sink(make());
    }
}

fn dispatch_event(routes: &Routes, event: &notify::Event) {
    let Some(kind) = kind_of(event.kind) else {
        return;
    };
    let to = |dir: &Path, filename: Option<String>| {
        send(routes, dir, &|| {
            RawWatchMessage::Event(RawWatchEvent {
                kind,
                filename: filename.clone(),
            })
        });
    };
    for path in &event.paths {
        to(path, None);
        if let Some(parent) = path.parent() {
            to(
                parent,
                path.file_name().map(|n| n.to_string_lossy().into_owned()),
            );
        }
    }
    if event.paths.is_empty() {
        for dir in routes.keys() {
            to(dir, None);
        }
    }
}

fn dispatch_error(routes: &Routes, error: &notify::Error) {
    let text = error.to_string();
    let failed = || RawWatchMessage::Failed(text.clone());
    let mut targets: Vec<&Path> = error
        .paths
        .iter()
        .flat_map(|p| [Some(p.as_path()), p.parent()])
        .flatten()
        .filter(|p| routes.contains_key(*p))
        .collect();
    targets.dedup();
    if targets.is_empty() {
        targets = routes.keys().map(PathBuf::as_path).collect();
    }
    for dir in targets {
        send(routes, dir, &failed);
    }
}

fn start_shared() -> io::Result<Arc<Shared>> {
    let routes: Arc<Mutex<Routes>> = Arc::default();
    let callback_routes = Arc::clone(&routes);
    let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let routes = lock(&callback_routes);
        match &result {
            Ok(event) => dispatch_event(&routes, event),
            Err(error) => dispatch_error(&routes, error),
        }
    })
    .map_err(io::Error::other)?;
    Ok(Arc::new(Shared {
        watcher: Mutex::new(watcher),
        routes,
        next_id: Mutex::new(0),
    }))
}

impl NotifyDirectoryWatch {
    fn shared(&self) -> io::Result<Arc<Shared>> {
        let mut cell = lock(&self.shared);
        if let Some(shared) = cell.as_ref() {
            return Ok(Arc::clone(shared));
        }
        let started = start_shared()?;
        *cell = Some(Arc::clone(&started));
        Ok(started)
    }
}

impl DirectoryWatch for NotifyDirectoryWatch {
    fn watch(&self, dir: &Path, sink: WatchSink) -> io::Result<Box<dyn WatchHandle>> {
        let shared = self.shared()?;
        let dir = dir.to_path_buf();
        let id = {
            let mut next = lock(&shared.next_id);
            *next += 1;
            *next
        };
        let mut watcher = lock(&shared.watcher);
        let mut routes = lock(&shared.routes);
        if !routes.contains_key(&dir) {
            watcher
                .watch(&dir, RecursiveMode::NonRecursive)
                .map_err(io::Error::other)?;
        }
        routes.entry(dir.clone()).or_default().push((id, sink));
        drop(routes);
        drop(watcher);
        Ok(Box::new(NotifyHandle {
            shared: Some(shared),
            dir,
            id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::{Duration, Instant};

    fn wait_for(rx: &Receiver<RawWatchMessage>, want: impl Fn(&RawWatchEvent) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(RawWatchMessage::Event(event)) = rx.recv_timeout(Duration::from_millis(100))
                && want(&event)
            {
                return true;
            }
        }
        false
    }

    fn sink_to(tx: Sender<RawWatchMessage>) -> WatchSink {
        Box::new(move |m| {
            tx.send(m).log_warn();
        })
    }

    fn named(name: &'static str) -> impl Fn(&RawWatchEvent) -> bool {
        move |e| e.filename.as_deref() == Some(name)
    }

    fn quiet_for(rx: &Receiver<RawWatchMessage>, name: &str) -> bool {
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            if let Ok(RawWatchMessage::Event(event)) = rx.recv_timeout(Duration::from_millis(50))
                && event.filename.as_deref() == Some(name)
            {
                return false;
            }
        }
        true
    }

    fn notify_threads() -> usize {
        std::fs::read_dir("/proc/self/task")
            .unwrap()
            .filter_map(Result::ok)
            .filter(|t| {
                std::fs::read_to_string(t.path().join("comm"))
                    .is_ok_and(|c| c.starts_with("notify-rs"))
            })
            .count()
    }

    #[test]
    fn reports_a_created_and_a_modified_file_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let mut handle = NotifyDirectoryWatch::default()
            .watch(dir.path(), sink_to(tx))
            .unwrap();
        std::fs::write(dir.path().join("a.md"), "x").unwrap();
        assert!(wait_for(&rx, |e| {
            e.filename.as_deref() == Some("a.md") && e.kind == WatchEventKind::Rename
        }));
        std::fs::write(dir.path().join("a.md"), "xy").unwrap();
        assert!(wait_for(&rx, |e| {
            e.filename.as_deref() == Some("a.md") && e.kind == WatchEventKind::Change
        }));
        handle.close();
    }

    #[test]
    fn fails_for_a_folder_that_does_not_exist() {
        assert!(
            NotifyDirectoryWatch::default()
                .watch(Path::new("/definitely/not/here"), Box::new(|_| {}))
                .is_err()
        );
    }

    #[test]
    fn two_directories_get_only_their_own_events() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (tx_a, rx_a) = channel();
        let (tx_b, rx_b) = channel();
        let watch = NotifyDirectoryWatch::default();
        let _ha = watch.watch(a.path(), sink_to(tx_a)).unwrap();
        let _hb = watch.watch(b.path(), sink_to(tx_b)).unwrap();
        std::fs::write(a.path().join("only-a.md"), "x").unwrap();
        std::fs::write(b.path().join("only-b.md"), "x").unwrap();
        assert!(wait_for(&rx_a, named("only-a.md")));
        assert!(wait_for(&rx_b, named("only-b.md")));
        assert!(quiet_for(&rx_a, "only-b.md"));
        assert!(quiet_for(&rx_b, "only-a.md"));
    }

    #[test]
    fn closing_one_handle_keeps_the_other_directory_watched() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (tx_a, rx_a) = channel();
        let (tx_b, rx_b) = channel();
        let watch = NotifyDirectoryWatch::default();
        let mut ha = watch.watch(a.path(), sink_to(tx_a)).unwrap();
        let _hb = watch.watch(b.path(), sink_to(tx_b)).unwrap();
        ha.close();
        std::fs::write(a.path().join("late.md"), "x").unwrap();
        std::fs::write(b.path().join("still.md"), "x").unwrap();
        assert!(wait_for(&rx_b, named("still.md")));
        assert!(quiet_for(&rx_a, "late.md"));
    }

    #[test]
    fn watching_the_same_directory_twice_survives_closing_one_handle() {
        let dir = tempfile::tempdir().unwrap();
        let (tx_1, rx_1) = channel();
        let (tx_2, rx_2) = channel();
        let watch = NotifyDirectoryWatch::default();
        let mut first = watch.watch(dir.path(), sink_to(tx_1)).unwrap();
        let _second = watch.watch(dir.path(), sink_to(tx_2)).unwrap();
        first.close();
        std::fs::write(dir.path().join("x.md"), "x").unwrap();
        assert!(wait_for(&rx_2, named("x.md")));
        assert!(quiet_for(&rx_1, "x.md"));
    }

    #[test]
    fn watching_many_directories_does_not_grow_the_thread_count() {
        let dirs: Vec<_> = (0..50).map(|_| tempfile::tempdir().unwrap()).collect();
        let watch = NotifyDirectoryWatch::default();
        let before = notify_threads();
        let _handles: Vec<_> = dirs
            .iter()
            .map(|d| watch.watch(d.path(), Box::new(|_| {})).unwrap())
            .collect();
        assert!(notify_threads() < before + 25, "one thread per directory");
    }
}
