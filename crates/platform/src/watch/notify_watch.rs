use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use desafinado_core::log_err::LogErr;
use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::classify::WatchEventKind;
use super::{DirectoryWatch, RawWatchEvent, RawWatchMessage, WatchHandle, WatchSink};

type SharedSink = Arc<Mutex<WatchSink>>;

struct Route {
    id: u64,
    sink: SharedSink,
}

type Routes = HashMap<PathBuf, Vec<Route>>;
type Delivery = (SharedSink, RawWatchMessage);

/// Lock order: `watcher`, then `routes`. The notify callback takes only
/// `routes`, and only to plan deliveries: it never holds it while a sink runs,
/// and `watch`/`close` never hold it while calling into notify (notify's own
/// `watch` blocks until the event-loop thread, which runs the callback,
/// answers). A sink must not call `watch` or `close` itself.
struct Shared {
    watcher: Mutex<RecommendedWatcher>,
    routes: Arc<Mutex<Routes>>,
    next_id: AtomicU64,
}

type SharedCell = Mutex<Option<Arc<Shared>>>;

/// Number of notify watchers (one thread and one inotify instance each).
/// notify reads inotify events until the queue is empty before it serves a
/// queued watch or unwatch request, so a write flood in one folder (a
/// `git checkout`) delays every watch call on the same instance. Spreading
/// folders over a few instances bounds that blast radius to the folders of the
/// flooded instance while the thread and fd count stay small and constant,
/// well under the inotify instance limit of 128.
const SHARDS: usize = 8;

/// Directories are spread over a fixed set of notify watchers by a hash of
/// their canonical path, created lazily. Directories are keyed by canonical
/// path, so two spellings of one folder share a watch.
#[derive(Clone)]
pub struct NotifyDirectoryWatch {
    shards: Arc<Vec<SharedCell>>,
}

impl Default for NotifyDirectoryWatch {
    fn default() -> Self {
        Self::with_shards(SHARDS)
    }
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

fn remove_route(routes: &mut Routes, dir: &Path, id: u64) -> bool {
    let Some(sinks) = routes.get_mut(dir) else {
        return false;
    };
    sinks.retain(|route| route.id != id);
    let empty = sinks.is_empty();
    if empty {
        routes.remove(dir);
    }
    empty
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
        let last = remove_route(&mut lock(&shared.routes), &self.dir, self.id);
        if last {
            let result = watcher.unwatch(&self.dir);
            let gone =
                matches!(&result, Err(e) if matches!(e.kind, notify::ErrorKind::WatchNotFound));
            if !gone {
                result.log_warn();
            }
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

/// The watched directories an event is about: a path that is the directory
/// itself or one of its direct children (what a per-directory inotify watch
/// reports).
fn concerned<'a>(routes: &'a Routes, paths: &[PathBuf]) -> BTreeSet<&'a PathBuf> {
    paths
        .iter()
        .flat_map(|p| [Some(p.as_path()), p.parent()])
        .flatten()
        .filter_map(|candidate| routes.get_key_value(candidate).map(|(dir, _)| dir))
        .collect()
}

fn deliver(routes: &Routes, dir: &Path, message: impl Fn() -> RawWatchMessage) -> Vec<Delivery> {
    routes
        .get(dir)
        .into_iter()
        .flatten()
        .map(|route| (Arc::clone(&route.sink), message()))
        .collect()
}

fn plan_event(routes: &Routes, event: &notify::Event) -> Vec<Delivery> {
    let Some(kind) = kind_of(event.kind) else {
        return Vec::new();
    };
    // An event without paths cannot be attributed: every directory gets it.
    let dirs: BTreeSet<&PathBuf> = if event.paths.is_empty() {
        routes.keys().collect()
    } else {
        concerned(routes, &event.paths)
    };
    let mut out = Vec::new();
    for dir in dirs {
        let mut names: Vec<Option<String>> = Vec::new();
        if event.paths.is_empty() {
            names.push(None);
        }
        for path in event
            .paths
            .iter()
            .filter(|p| *p == dir || p.parent() == Some(dir.as_path()))
        {
            let name = if path == dir {
                None
            } else {
                path.file_name().map(|n| n.to_string_lossy().into_owned())
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for filename in names {
            out.extend(deliver(routes, dir, || {
                RawWatchMessage::Event(RawWatchEvent {
                    kind,
                    filename: filename.clone(),
                })
            }));
        }
    }
    out
}

fn plan_error(routes: &Routes, error: &notify::Error) -> Vec<Delivery> {
    let text = error.to_string();
    let mut dirs = concerned(routes, &error.paths);
    if dirs.is_empty() {
        // An error with no path (for example an inotify queue overflow) is
        // about the one shared instance, so every directory hears it.
        dirs = routes.keys().collect();
    }
    dirs.into_iter()
        .flat_map(|dir| deliver(routes, dir, || RawWatchMessage::Failed(text.clone())))
        .collect()
}

fn start_shared() -> io::Result<Arc<Shared>> {
    let routes: Arc<Mutex<Routes>> = Arc::default();
    let callback_routes = Arc::clone(&routes);
    let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let deliveries = {
            let routes = lock(&callback_routes);
            match &result {
                Ok(event) => plan_event(&routes, event),
                Err(error) => plan_error(&routes, error),
            }
        };
        for (sink, message) in deliveries {
            (lock(&sink))(message);
        }
    })
    .map_err(io::Error::other)?;
    Ok(Arc::new(Shared {
        watcher: Mutex::new(watcher),
        routes,
        next_id: AtomicU64::new(0),
    }))
}

impl NotifyDirectoryWatch {
    pub fn with_shards(count: usize) -> Self {
        Self {
            shards: Arc::new((0..count.max(1)).map(|_| Mutex::default()).collect()),
        }
    }

    fn shard_of(&self, dir: &Path) -> usize {
        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        canonical.hash(&mut hasher);
        usize::try_from(hasher.finish() % self.shards.len() as u64).unwrap_or(0)
    }

    fn shared(&self, index: usize) -> io::Result<Arc<Shared>> {
        let mut cell = lock(&self.shards[index]);
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
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let shared = self.shared(self.shard_of(&dir))?;
        let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
        let mut watcher = lock(&shared.watcher);
        lock(&shared.routes)
            .entry(dir.clone())
            .or_default()
            .push(Route {
                id,
                sink: Arc::new(Mutex::new(sink)),
            });
        // Always ask notify: if the folder was deleted and recreated the OS
        // dropped the old watch, and for a live one inotify returns the same
        // watch descriptor.
        if let Err(error) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            remove_route(&mut lock(&shared.routes), &dir, id);
            return Err(io::Error::other(error));
        }
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
    use std::sync::atomic::Ordering;
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

    fn drain(rx: &Receiver<RawWatchMessage>, ms: u64) -> Vec<(WatchEventKind, Option<String>)> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        let mut out = Vec::new();
        while Instant::now() < deadline {
            if let Ok(RawWatchMessage::Event(e)) = rx.recv_timeout(Duration::from_millis(50)) {
                out.push((e.kind, e.filename));
            }
        }
        out
    }

    fn count(
        got: &[(WatchEventKind, Option<String>)],
        kind: WatchEventKind,
        name: Option<&str>,
    ) -> usize {
        got.iter()
            .filter(|(k, n)| *k == kind && n.as_deref() == name)
            .count()
    }

    #[test]
    fn watching_while_a_writer_floods_a_watched_folder_does_not_deadlock() {
        let busy = tempfile::tempdir().unwrap();
        let others: Vec<_> = (0..200).map(|_| tempfile::tempdir().unwrap()).collect();
        let watch = NotifyDirectoryWatch::default();
        let (tx, _rx) = channel();
        let busy_handle = watch.watch(busy.path(), sink_to(tx)).unwrap();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let stop = Arc::clone(&stop);
            let path = busy.path().join("flood.md");
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::fs::write(&path, "x").log_warn();
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        };
        let (done_tx, done_rx) = channel();
        std::thread::spawn(move || {
            let handles: Vec<_> = others
                .iter()
                .map(|d| watch.watch(d.path(), Box::new(|_| {})).unwrap())
                .collect();
            drop(handles);
            done_tx.send(()).log_warn();
        });
        let finished = done_rx.recv_timeout(Duration::from_secs(20)).is_ok();
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        if !finished {
            std::mem::forget(busy_handle);
        }
        assert!(finished, "watching new folders hung while events flowed");
    }

    #[test]
    fn a_nested_watched_folder_reports_its_files_once_and_its_parent_only_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let skill = root.path().join("skill");
        std::fs::create_dir(&skill).unwrap();
        let (tx_root, rx_root) = channel();
        let (tx_skill, rx_skill) = channel();
        let watch = NotifyDirectoryWatch::default();
        let _hr = watch.watch(root.path(), sink_to(tx_root)).unwrap();
        let _hs = watch.watch(&skill, sink_to(tx_skill)).unwrap();

        std::fs::write(skill.join("SKILL.md"), "x").unwrap();
        let skill_got = drain(&rx_skill, 700);
        let root_got = drain(&rx_root, 100);
        assert_eq!(
            count(&skill_got, WatchEventKind::Rename, Some("SKILL.md")),
            1
        );
        assert!(root_got.iter().all(|(_, n)| n.as_deref() == Some("skill")));
        assert_eq!(count(&root_got, WatchEventKind::Rename, None), 0);

        std::fs::rename(&skill, root.path().join("moved")).unwrap();
        let root_got = drain(&rx_root, 700);
        assert!(count(&root_got, WatchEventKind::Rename, Some("skill")) >= 1);
        assert!(count(&root_got, WatchEventKind::Rename, Some("moved")) >= 1);
        assert_eq!(count(&root_got, WatchEventKind::Rename, None), 0);
    }

    #[test]
    fn two_spellings_of_one_folder_both_hear_it_and_one_close_keeps_the_other() {
        let real = tempfile::tempdir().unwrap();
        let holder = tempfile::tempdir().unwrap();
        let link = holder.path().join("link");
        std::os::unix::fs::symlink(real.path(), &link).unwrap();
        let (tx_1, rx_1) = channel();
        let (tx_2, rx_2) = channel();
        let watch = NotifyDirectoryWatch::default();
        let mut first = watch.watch(real.path(), sink_to(tx_1)).unwrap();
        let _second = watch.watch(&link, sink_to(tx_2)).unwrap();
        std::fs::write(real.path().join("a.md"), "x").unwrap();
        assert!(wait_for(&rx_1, named("a.md")));
        assert!(wait_for(&rx_2, named("a.md")));
        first.close();
        std::fs::write(real.path().join("b.md"), "x").unwrap();
        assert!(wait_for(&rx_2, named("b.md")));
    }

    #[test]
    fn a_folder_deleted_and_recreated_can_be_watched_again() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        let watch = NotifyDirectoryWatch::default();
        let mut old = watch.watch(&dir, Box::new(|_| {})).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        std::fs::create_dir(&dir).unwrap();
        let (tx, rx) = channel();
        let _new = watch.watch(&dir, sink_to(tx)).unwrap();
        std::fs::write(dir.join("x.md"), "x").unwrap();
        assert!(wait_for(&rx, named("x.md")));
        old.close();
    }

    fn routes_for(dirs: &[&str]) -> Routes {
        dirs.iter()
            .map(|d| {
                let route = Route {
                    id: 0,
                    sink: Arc::new(Mutex::new(Box::new(|_| {}) as WatchSink)),
                };
                (PathBuf::from(d), vec![route])
            })
            .collect()
    }

    fn event_on(kind: EventKind, paths: &[&str]) -> notify::Event {
        paths.iter().fold(notify::Event::new(kind), |e, p| {
            e.add_path(PathBuf::from(p))
        })
    }

    fn names(plan: &[Delivery]) -> Vec<Option<String>> {
        plan.iter()
            .map(|(_, m)| match m {
                RawWatchMessage::Event(e) => e.filename.clone(),
                RawWatchMessage::Failed(_) => Some("failed".into()),
            })
            .collect()
    }

    #[test]
    fn one_event_never_delivers_the_same_name_twice_to_a_route() {
        let routes = routes_for(&["/r"]);
        let event = event_on(
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            &["/r/a", "/r/a", "/r/b"],
        );
        assert_eq!(
            names(&plan_event(&routes, &event)),
            vec![Some("a".into()), Some("b".into())]
        );
    }

    #[test]
    fn a_nested_folder_is_named_to_its_parent_and_reported_as_itself_to_its_own_route() {
        let routes = routes_for(&["/r", "/r/skill"]);
        let event = event_on(
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::From)),
            &["/r/skill"],
        );
        let mut got = names(&plan_event(&routes, &event));
        got.sort();
        assert_eq!(got, vec![None, Some("skill".into())]);
    }

    #[test]
    fn an_event_inside_a_nested_folder_does_not_reach_the_parent_route() {
        let routes = routes_for(&["/r", "/r/skill"]);
        let event = event_on(
            EventKind::Create(notify::event::CreateKind::File),
            &["/r/skill/SKILL.md"],
        );
        assert_eq!(
            names(&plan_event(&routes, &event)),
            vec![Some("SKILL.md".into())]
        );
    }

    #[test]
    fn a_rename_across_two_watched_folders_names_each_folder_only_its_own_file() {
        let routes = routes_for(&["/a", "/b"]);
        let event = event_on(
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            &["/a/x", "/b/y"],
        );
        let plan = plan_event(&routes, &event);
        let mut per_dir: Vec<(bool, Option<String>)> = Vec::new();
        for (sink, message) in &plan {
            let for_a = Arc::ptr_eq(sink, &routes[Path::new("/a")][0].sink);
            if let RawWatchMessage::Event(e) = message {
                per_dir.push((for_a, e.filename.clone()));
            }
        }
        per_dir.sort();
        assert_eq!(
            per_dir,
            vec![(false, Some("y".into())), (true, Some("x".into()))]
        );
    }

    fn burst(
        dir: &Path,
        stop: &Arc<std::sync::atomic::AtomicBool>,
    ) -> Vec<std::thread::JoinHandle<()>> {
        (0..4)
            .map(|t| {
                let (dir, stop) = (dir.to_path_buf(), Arc::clone(stop));
                std::thread::spawn(move || {
                    let mut i = 0;
                    while !stop.load(Ordering::Relaxed) {
                        std::fs::write(dir.join(format!("f{t}-{}", i % 500)), "x").log_warn();
                        i += 1;
                    }
                })
            })
            .collect()
    }

    fn time_watching_another_folder_during_a_burst(watch: &NotifyDirectoryWatch) -> Duration {
        let flooded = tempfile::tempdir().unwrap();
        let (tx, _rx) = channel();
        let _flooded_handle = watch.watch(flooded.path(), sink_to(tx)).unwrap();
        let other = loop {
            let candidate = tempfile::tempdir().unwrap();
            if watch.shards.len() == 1
                || watch.shard_of(candidate.path()) != watch.shard_of(flooded.path())
            {
                break candidate;
            }
        };
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writers = burst(flooded.path(), &stop);
        std::thread::sleep(Duration::from_millis(50));
        let (done_tx, done_rx) = channel();
        let other_path = other.path().to_path_buf();
        let other_watch = watch.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let handle = other_watch.watch(&other_path, Box::new(|_| {})).unwrap();
            done_tx.send((started.elapsed(), handle)).log_warn();
        });
        let waited = done_rx
            .recv_timeout(Duration::from_mins(1))
            .map_or(Duration::from_mins(1), |(elapsed, _handle)| elapsed);
        stop.store(true, Ordering::Relaxed);
        for writer in writers {
            writer.join().unwrap();
        }
        waited
    }

    #[test]
    fn a_write_burst_in_one_folder_does_not_stall_watching_another() {
        let waited = time_watching_another_folder_during_a_burst(&NotifyDirectoryWatch::default());
        assert!(waited < Duration::from_secs(3), "waited {waited:?}");
    }
}
