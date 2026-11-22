use desafinado_core::log_err::LogErr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvError, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::classify::{
    AwaitedNames, ExternalChanges, MAX_REPORTED_PATHS, WatchEvent, classify_events,
};
use super::fs_probe::{RootEntries, directory_identity, root_entries};
use super::plan::{diff_watches, watch_directories};
use super::{DirectoryWatch, NotifyDirectoryWatch, RawWatchMessage, WatchHandle};
use crate::library::LibrarySnapshot;

pub const DEFAULT_DELAY_MS: u64 = 250;
pub const DEFAULT_MAX_WAIT_MS: u64 = 1000;
const OWN_WRITE_WINDOW: Duration = Duration::from_secs(2);

type PathLookup = Box<dyn Fn(&str) -> Option<String> + Send>;
type EntriesLookup = Box<dyn Fn(&str) -> Option<RootEntries> + Send>;

pub struct WatchConfig {
    pub list: Box<dyn Fn() -> LibrarySnapshot + Send>,
    pub emit: Box<dyn Fn(ExternalChanges) + Send>,
    pub on_problem: Box<dyn Fn(&str) + Send>,
    pub watch: Box<dyn DirectoryWatch>,
    pub real_path: PathLookup,
    pub identity_of: PathLookup,
    pub root_entries: EntriesLookup,
    pub delay: Duration,
    pub max_wait: Duration,
}

impl WatchConfig {
    pub fn new(
        list: impl Fn() -> LibrarySnapshot + Send + 'static,
        emit: impl Fn(ExternalChanges) + Send + 'static,
    ) -> Self {
        Self {
            list: Box::new(list),
            emit: Box::new(emit),
            on_problem: Box::new(|_| {}),
            watch: Box::new(NotifyDirectoryWatch),
            real_path: Box::new(|path| {
                std::fs::canonicalize(path)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            }),
            identity_of: Box::new(directory_identity),
            root_entries: Box::new(|root| root_entries(root).ok()),
            delay: Duration::from_millis(DEFAULT_DELAY_MS),
            max_wait: Duration::from_millis(DEFAULT_MAX_WAIT_MS),
        }
    }
}

enum Command {
    Raw {
        dir: String,
        id: u64,
        message: RawWatchMessage,
    },
    WatchFile(Option<String>),
    RootsChanged,
    OwnWrite(String),
    Barrier(Sender<()>),
    Close,
}

struct Active {
    identity: String,
    id: u64,
    handle: Box<dyn WatchHandle>,
}

struct Anchor {
    dir: String,
    waiting_for: String,
}

struct Worker {
    config: WatchConfig,
    commands: Sender<Command>,
    active: HashMap<String, Active>,
    next_id: u64,
    open_file: Option<String>,
    awaited: AwaitedNames,
    batch: Vec<WatchEvent>,
    batch_started: Option<Instant>,
    deadline: Option<Instant>,
    own_writes: Vec<(String, Instant)>,
}

fn dirname(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Worker {
    fn existing_ancestor(&self, path: &str) -> String {
        let parent = dirname(path);
        if (self.config.identity_of)(&parent).is_some() || dirname(&parent) == parent {
            parent
        } else {
            self.existing_ancestor(&parent)
        }
    }

    fn anchor_for(&self, missing: &str) -> Anchor {
        let dir = self.existing_ancestor(missing);
        let relative = Path::new(missing)
            .strip_prefix(&dir)
            .map(PathBuf::from)
            .unwrap_or_default();
        let waiting_for = relative
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        Anchor { dir, waiting_for }
    }

    fn plan(&self) -> (HashMap<String, String>, LibrarySnapshot, AwaitedNames) {
        let snapshot = (self.config.list)();
        let readable: Vec<_> = snapshot.roots.iter().filter(|r| r.readable).collect();
        let real: Vec<String> = readable
            .iter()
            .map(|r| (self.config.real_path)(&r.root).unwrap_or_else(|| r.root.clone()))
            .collect();
        let entries: Vec<RootEntries> = real
            .iter()
            .map(|root| (self.config.root_entries)(root).unwrap_or_default())
            .collect();
        let missing: Vec<String> = snapshot
            .roots
            .iter()
            .filter(|r| !r.readable)
            .map(|r| r.root.clone())
            .collect();
        let anchors: Vec<Anchor> = missing
            .iter()
            .chain(entries.iter().flat_map(|e| e.dangling.iter()))
            .map(|path| self.anchor_for(path))
            .collect();
        let skills: Vec<_> = readable.iter().flat_map(|r| r.skills.iter()).collect();
        let folders: Vec<String> = entries.iter().flat_map(|e| e.folders.clone()).collect();
        let anchor_dirs: Vec<String> = anchors.iter().map(|a| a.dir.clone()).collect();
        let dirs = watch_directories(
            &real,
            &skills,
            self.open_file.as_deref(),
            &folders,
            &anchor_dirs,
        );
        let wanted: HashMap<String, String> = dirs
            .into_iter()
            .filter_map(|dir| (self.config.identity_of)(&dir).map(|identity| (dir, identity)))
            .collect();
        let mut awaited: AwaitedNames = HashMap::new();
        for anchor in anchors {
            awaited
                .entry(anchor.dir)
                .or_default()
                .insert(anchor.waiting_for);
        }
        (wanted, snapshot, awaited)
    }

    fn open(&mut self, dir: &str, identity: &str) -> bool {
        self.next_id += 1;
        let id = self.next_id;
        let commands = self.commands.clone();
        let owned = dir.to_owned();
        let sink = Box::new(move |message: RawWatchMessage| {
            commands
                .send(Command::Raw {
                    dir: owned.clone(),
                    id,
                    message,
                })
                .log_warn();
        });
        match self.config.watch.watch(Path::new(dir), sink) {
            Ok(handle) => {
                self.active.insert(
                    dir.to_owned(),
                    Active {
                        identity: identity.to_owned(),
                        id,
                        handle,
                    },
                );
                true
            }
            Err(error) => {
                (self.config.on_problem)(&format!("Could not watch {dir}: {error}"));
                false
            }
        }
    }

    fn shut(&mut self, dir: &str) {
        if let Some(mut entry) = self.active.remove(dir) {
            entry.handle.close();
        }
    }

    fn sync(&mut self, wanted: &HashMap<String, String>) -> usize {
        let identities: HashMap<String, String> = self
            .active
            .iter()
            .map(|(dir, a)| (dir.clone(), a.identity.clone()))
            .collect();
        let diff = diff_watches(&identities, wanted);
        for dir in &diff.close {
            self.shut(dir);
        }
        diff.open
            .iter()
            .filter(|dir| {
                wanted
                    .get(*dir)
                    .is_some_and(|identity| self.open(dir, identity))
            })
            .count()
    }

    fn reconcile(&mut self) {
        loop {
            let (wanted, snapshot, awaited) = self.plan();
            self.awaited = awaited;
            if self.sync(&wanted) == 0 {
                return;
            }
            let again = (self.config.list)();
            if snapshot == again {
                return;
            }
            (self.config.emit)(ExternalChanges {
                paths: Vec::new(),
                library: true,
            });
        }
    }

    fn is_own_write(&mut self, event: &WatchEvent) -> bool {
        let now = Instant::now();
        self.own_writes
            .retain(|(_, at)| now.duration_since(*at) < OWN_WRITE_WINDOW);
        let Some(name) = &event.filename else {
            return false;
        };
        let path = Path::new(&event.dir)
            .join(name)
            .to_string_lossy()
            .into_owned();
        self.own_writes.iter().any(|(own, _)| *own == path)
    }

    fn add_event(&mut self, event: WatchEvent) {
        if self.is_own_write(&event) {
            return;
        }
        let now = Instant::now();
        let started = *self.batch_started.get_or_insert(now);
        self.batch.push(event);
        let waited_left = (started + self.config.max_wait).saturating_duration_since(now);
        self.deadline = Some(now + self.config.delay.min(waited_left));
    }

    fn flush(&mut self) {
        self.deadline = None;
        self.batch_started = None;
        let events = std::mem::take(&mut self.batch);
        if events.is_empty() {
            return;
        }
        let changes = classify_events(
            &events,
            self.open_file.as_deref(),
            &self.awaited,
            MAX_REPORTED_PATHS,
        );
        if changes.paths.is_empty() && !changes.library {
            return;
        }
        let library = changes.library;
        (self.config.emit)(changes);
        if library {
            self.reconcile();
        }
    }

    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Raw { dir, id, message } => {
                if self.active.get(&dir).map(|a| a.id) != Some(id) {
                    return true;
                }
                match message {
                    RawWatchMessage::Event(event) => self.add_event(WatchEvent {
                        dir,
                        kind: event.kind,
                        filename: event.filename,
                    }),
                    RawWatchMessage::Failed(error) => {
                        self.shut(&dir);
                        (self.config.on_problem)(&format!("Could not watch {dir}: {error}"));
                    }
                }
            }
            Command::WatchFile(path) => {
                self.open_file = path.map(|p| (self.config.real_path)(&p).unwrap_or(p));
                self.reconcile();
            }
            Command::RootsChanged => self.reconcile(),
            Command::OwnWrite(path) => {
                let real = (self.config.real_path)(&path).unwrap_or(path.clone());
                self.own_writes.push((path, Instant::now()));
                self.own_writes.push((real, Instant::now()));
            }
            Command::Barrier(reply) => {
                reply.send(()).log_warn();
            }
            Command::Close => return false,
        }
        true
    }

    fn run(mut self, receiver: Receiver<Command>) {
        self.reconcile();
        loop {
            let received = match self.deadline {
                None => receiver
                    .recv()
                    .map_err(|RecvError| RecvTimeoutError::Disconnected),
                Some(deadline) => {
                    receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                }
            };
            match received {
                Ok(command) => {
                    if !self.handle(command) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => self.flush(),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let dirs: Vec<String> = self.active.keys().cloned().collect();
        for dir in dirs {
            self.shut(&dir);
        }
    }
}

pub struct ExternalChangesController {
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl ExternalChangesController {
    pub fn start(config: WatchConfig) -> Self {
        let (commands, receiver) = channel();
        let worker = Worker {
            config,
            commands: commands.clone(),
            active: HashMap::new(),
            next_id: 0,
            open_file: None,
            awaited: HashMap::new(),
            batch: Vec::new(),
            batch_started: None,
            deadline: None,
            own_writes: Vec::new(),
        };
        let handle = std::thread::spawn(move || worker.run(receiver));
        Self {
            commands,
            worker: Some(handle),
        }
    }

    pub fn watch_file(&self, path: Option<&str>) {
        self.commands
            .send(Command::WatchFile(path.map(str::to_owned)))
            .log_warn();
    }

    pub fn roots_changed(&self) {
        self.commands.send(Command::RootsChanged).log_warn();
    }

    pub fn note_own_write(&self, path: &str) {
        self.commands
            .send(Command::OwnWrite(path.to_owned()))
            .log_warn();
    }

    pub fn settle(&self) {
        let (reply, done) = channel();
        if self.commands.send(Command::Barrier(reply)).is_ok() {
            done.recv_timeout(Duration::from_secs(10)).log_warn();
        }
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.commands.send(Command::Close).log_warn();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            log::error!("The file watcher thread panicked");
        }
    }
}

impl Drop for ExternalChangesController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{LibraryDeps, RealFileSystem, RootsStore, list_library};
    use crate::watch::{RawWatchEvent, WatchEventKind, WatchSink};
    use std::fs;
    use std::sync::{Arc, Mutex};

    type Sinks = Arc<Mutex<HashMap<String, WatchSink>>>;

    struct FakeWatch {
        sinks: Sinks,
        opened: Arc<Mutex<Vec<String>>>,
    }

    struct FakeHandle {
        dir: String,
        sinks: Sinks,
    }

    impl WatchHandle for FakeHandle {
        fn close(&mut self) {
            self.sinks.lock().unwrap().remove(&self.dir);
        }
    }

    impl DirectoryWatch for FakeWatch {
        fn watch(&self, dir: &Path, sink: WatchSink) -> std::io::Result<Box<dyn WatchHandle>> {
            let key = dir.to_string_lossy().into_owned();
            self.opened.lock().unwrap().push(key.clone());
            self.sinks.lock().unwrap().insert(key.clone(), sink);
            Ok(Box::new(FakeHandle {
                dir: key,
                sinks: Arc::clone(&self.sinks),
            }))
        }
    }

    struct Rig {
        _dir: tempfile::TempDir,
        base: PathBuf,
        controller: ExternalChangesController,
        sinks: Sinks,
        opened: Arc<Mutex<Vec<String>>>,
        emitted: Arc<Mutex<Vec<ExternalChanges>>>,
    }

    fn skill(base: &Path, relative: &str) {
        let path = base.join(relative);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), "---\nname: s\ndescription: d\n---\n").unwrap();
    }

    fn rig(delay_ms: u64) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let home = base.join("home");
        fs::create_dir_all(home.join(".claude/skills")).unwrap();
        skill(&base, "home/.claude/skills/pdf");
        let sinks: Sinks = Arc::default();
        let opened = Arc::new(Mutex::new(Vec::new()));
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let data = base.join("data");
        fs::create_dir_all(&data).unwrap();
        let home_text = home.to_string_lossy().into_owned();
        let list = move || {
            let store = RootsStore::new(&data);
            list_library(&LibraryDeps {
                home: &home_text,
                store: &store,
                file_system: &RealFileSystem,
            })
        };
        let sink_emitted = Arc::clone(&emitted);
        let mut config = WatchConfig::new(list, move |changes| {
            sink_emitted.lock().unwrap().push(changes);
        });
        config.watch = Box::new(FakeWatch {
            sinks: Arc::clone(&sinks),
            opened: Arc::clone(&opened),
        });
        config.delay = Duration::from_millis(delay_ms);
        config.max_wait = Duration::from_secs(1);
        let controller = ExternalChangesController::start(config);
        controller.settle();
        Rig {
            _dir: dir,
            base,
            controller,
            sinks,
            opened,
            emitted,
        }
    }

    impl Rig {
        fn send(&self, dir: &str, kind: WatchEventKind, filename: Option<&str>) {
            let sinks = self.sinks.lock().unwrap();
            if let Some(sink) = sinks.get(dir) {
                sink(RawWatchMessage::Event(RawWatchEvent {
                    kind,
                    filename: filename.map(str::to_owned),
                }));
            }
        }
        fn claude(&self) -> String {
            self.base
                .join("home/.claude/skills")
                .to_string_lossy()
                .into_owned()
        }
        fn emitted(&self) -> Vec<ExternalChanges> {
            self.emitted.lock().unwrap().clone()
        }
        fn watched(&self) -> Vec<String> {
            let mut dirs: Vec<String> = self.sinks.lock().unwrap().keys().cloned().collect();
            dirs.sort();
            dirs
        }
        fn wait_for_emits(&self, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.emitted().len() < count && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn watches_the_root_and_each_skill_folder_on_start() {
        let r = rig(40);
        let claude = r.claude();
        let home = r.base.join("home").to_string_lossy().into_owned();
        assert_eq!(r.watched(), [home, claude.clone(), format!("{claude}/pdf")]);
        assert!(r.emitted().is_empty());
    }

    #[test]
    fn watches_the_folder_of_the_open_file_and_stops_when_it_is_cleared() {
        let r = rig(40);
        let notes = r.base.join("notes");
        fs::create_dir_all(&notes).unwrap();
        let file = notes.join("a.md");
        fs::write(&file, "x").unwrap();
        r.controller.watch_file(Some(&file.to_string_lossy()));
        r.controller.settle();
        assert!(r.watched().contains(&notes.to_string_lossy().into_owned()));
        r.controller.watch_file(None);
        r.controller.settle();
        assert!(!r.watched().contains(&notes.to_string_lossy().into_owned()));
    }

    #[test]
    fn reports_an_external_change_of_the_open_file_in_one_batch() {
        let r = rig(40);
        let notes = r.base.join("notes");
        fs::create_dir_all(&notes).unwrap();
        let file = notes.join("a.md");
        fs::write(&file, "x").unwrap();
        r.controller.watch_file(Some(&file.to_string_lossy()));
        r.controller.settle();
        let dir = notes.to_string_lossy().into_owned();
        r.send(&dir, WatchEventKind::Change, Some("a.md"));
        r.send(&dir, WatchEventKind::Change, Some("a.md"));
        r.wait_for_emits(1);
        assert_eq!(
            r.emitted(),
            [ExternalChanges {
                paths: vec![file.to_string_lossy().into_owned()],
                library: false
            }]
        );
    }

    #[test]
    fn ignores_the_apps_own_write_of_a_file() {
        let r = rig(40);
        let notes = r.base.join("notes");
        fs::create_dir_all(&notes).unwrap();
        let file = notes.join("a.md");
        fs::write(&file, "x").unwrap();
        r.controller.watch_file(Some(&file.to_string_lossy()));
        r.controller.note_own_write(&file.to_string_lossy());
        r.controller.settle();
        let dir = notes.to_string_lossy().into_owned();
        r.send(&dir, WatchEventKind::Rename, Some("a.md"));
        r.send(&dir, WatchEventKind::Change, Some("a.md"));
        std::thread::sleep(Duration::from_millis(200));
        assert!(r.emitted().is_empty());
    }

    #[test]
    fn a_library_change_is_emitted_and_new_folders_are_watched_after_a_rescan() {
        let r = rig(40);
        let claude = r.claude();
        skill(&r.base, "home/.claude/skills/new");
        r.send(&claude, WatchEventKind::Rename, Some("new"));
        r.wait_for_emits(1);
        r.controller.settle();
        assert_eq!(
            r.emitted()[0],
            ExternalChanges {
                paths: vec![format!("{claude}/new")],
                library: true
            }
        );
        assert!(r.watched().contains(&format!("{claude}/new")));
    }

    #[test]
    fn a_removed_skill_is_no_longer_watched() {
        let r = rig(40);
        let claude = r.claude();
        fs::remove_dir_all(format!("{claude}/pdf")).unwrap();
        r.send(&claude, WatchEventKind::Rename, Some("pdf"));
        r.wait_for_emits(1);
        r.controller.settle();
        let home = r.base.join("home").to_string_lossy().into_owned();
        assert_eq!(r.watched(), [home, claude]);
    }

    #[test]
    fn emits_nothing_for_hidden_files() {
        let r = rig(30);
        let claude = r.claude();
        r.send(
            &format!("{claude}/pdf"),
            WatchEventKind::Rename,
            Some(".SKILL.md.x.tmp"),
        );
        std::thread::sleep(Duration::from_millis(150));
        assert!(r.emitted().is_empty());
    }

    #[test]
    fn flushes_at_the_maximum_wait_even_when_events_keep_arriving() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(base.join("home/.claude/skills")).unwrap();
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let sinks: Sinks = Arc::default();
        let home = base.join("home").to_string_lossy().into_owned();
        let data = base.join("data");
        let sink_emitted = Arc::clone(&emitted);
        let mut config = WatchConfig::new(
            move || {
                let store = RootsStore::new(&data);
                list_library(&LibraryDeps {
                    home: &home,
                    store: &store,
                    file_system: &RealFileSystem,
                })
            },
            move |c| sink_emitted.lock().unwrap().push(c),
        );
        config.watch = Box::new(FakeWatch {
            sinks: Arc::clone(&sinks),
            opened: Arc::default(),
        });
        config.delay = Duration::from_millis(150);
        config.max_wait = Duration::from_millis(300);
        let controller = ExternalChangesController::start(config);
        controller.settle();
        let root = base
            .join("home/.claude/skills")
            .to_string_lossy()
            .into_owned();
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(450) {
            if let Some(sink) = sinks.lock().unwrap().get(&root) {
                sink(RawWatchMessage::Event(RawWatchEvent {
                    kind: WatchEventKind::Change,
                    filename: Some("x.md".into()),
                }));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!emitted.lock().unwrap().is_empty());
    }

    #[test]
    fn a_watch_failure_is_reported_and_the_folder_is_dropped() {
        let r = rig(30);
        let claude = r.claude();
        {
            let sinks = r.sinks.lock().unwrap();
            sinks.get(&claude).unwrap()(RawWatchMessage::Failed("boom".into()));
        }
        r.controller.settle();
        assert!(!r.watched().contains(&claude));
    }

    #[test]
    fn closing_stops_every_watch() {
        let r = rig(30);
        let sinks = Arc::clone(&r.sinks);
        let _ = r.opened.lock().unwrap().len();
        r.controller.close();
        assert!(sinks.lock().unwrap().is_empty());
    }
}
