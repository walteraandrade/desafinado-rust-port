use std::io;
use std::path::Path;

use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::classify::WatchEventKind;
use super::{DirectoryWatch, RawWatchEvent, RawWatchMessage, WatchHandle, WatchSink};

#[derive(Debug, Clone, Copy, Default)]
pub struct NotifyDirectoryWatch;

struct NotifyHandle {
    watcher: Option<RecommendedWatcher>,
}

impl WatchHandle for NotifyHandle {
    fn close(&mut self) {
        self.watcher = None;
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

impl DirectoryWatch for NotifyDirectoryWatch {
    fn watch(&self, dir: &Path, sink: WatchSink) -> io::Result<Box<dyn WatchHandle>> {
        let watched = dir.to_path_buf();
        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    let Some(kind) = kind_of(event.kind) else {
                        return;
                    };
                    for path in &event.paths {
                        let filename = if path == &watched {
                            None
                        } else {
                            path.file_name().map(|n| n.to_string_lossy().into_owned())
                        };
                        sink(RawWatchMessage::Event(RawWatchEvent { kind, filename }));
                    }
                    if event.paths.is_empty() {
                        sink(RawWatchMessage::Event(RawWatchEvent {
                            kind,
                            filename: None,
                        }));
                    }
                }
                Err(error) => {
                    sink(RawWatchMessage::Failed(error.to_string()));
                }
            },
        )
        .map_err(io::Error::other)?;
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(io::Error::other)?;
        Ok(Box::new(NotifyHandle {
            watcher: Some(watcher),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_core::log_err::LogErr;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    fn wait_for(
        rx: &std::sync::mpsc::Receiver<RawWatchMessage>,
        want: impl Fn(&RawWatchEvent) -> bool,
    ) -> bool {
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

    #[test]
    fn reports_a_created_and_a_modified_file_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let mut handle = NotifyDirectoryWatch
            .watch(
                dir.path(),
                Box::new(move |m| {
                    tx.send(m).log_warn();
                }),
            )
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
            NotifyDirectoryWatch
                .watch(Path::new("/definitely/not/here"), Box::new(|_| {}))
                .is_err()
        );
    }
}
