mod classify;
mod controller;
mod fs_probe;
mod notify_watch;
mod plan;
mod requests;

pub use classify::{
    AwaitedNames, ExternalChanges, MAX_REPORTED_PATHS, WatchEvent, WatchEventKind, classify_events,
    is_ignored_name,
};
pub use controller::{
    DEFAULT_DELAY_MS, DEFAULT_MAX_WAIT_MS, ExternalChangesController, WatchConfig,
};
pub use fs_probe::{RootEntries, directory_identity, root_entries};
pub use notify_watch::NotifyDirectoryWatch;
pub use plan::{WatchDiff, diff_watches, watch_directories};
pub use requests::parse_watch_request;

pub type WatchSink = Box<dyn Fn(RawWatchMessage) + Send + 'static>;

pub trait WatchHandle: Send {
    fn close(&mut self);
}

pub struct RawWatchEvent {
    pub kind: WatchEventKind,
    pub filename: Option<String>,
}

pub enum RawWatchMessage {
    Event(RawWatchEvent),
    Failed(String),
}

pub trait DirectoryWatch: Send {
    fn watch(
        &self,
        dir: &std::path::Path,
        sink: WatchSink,
    ) -> std::io::Result<Box<dyn WatchHandle>>;
}
