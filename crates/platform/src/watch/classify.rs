use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

pub const MAX_REPORTED_PATHS: usize = 200;
const SKILL_FILE: &str = "SKILL.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    Rename,
    Change,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    pub dir: String,
    pub kind: WatchEventKind,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalChanges {
    pub paths: Vec<String>,
    pub library: bool,
}

pub type AwaitedNames = HashMap<String, HashSet<String>>;

pub fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules"
}

fn path_of(event: &WatchEvent) -> String {
    match &event.filename {
        None => event.dir.clone(),
        Some(name) => Path::new(&event.dir)
            .join(name)
            .to_string_lossy()
            .into_owned(),
    }
}

fn is_relevant(event: &WatchEvent, open_file: Option<&str>, awaited: &AwaitedNames) -> bool {
    match &event.filename {
        None => true,
        Some(name) => {
            !is_ignored_name(name)
                || open_file == Some(path_of(event).as_str())
                || awaited
                    .get(&event.dir)
                    .is_some_and(|names| names.contains(name))
        }
    }
}

fn changes_library(event: &WatchEvent) -> bool {
    match &event.filename {
        None => true,
        Some(name) => event.kind == WatchEventKind::Rename || name == SKILL_FILE,
    }
}

fn unique(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.filter(|value| seen.insert(value.clone())).collect()
}

pub fn classify_events(
    events: &[WatchEvent],
    open_file: Option<&str>,
    awaited: &AwaitedNames,
    limit: usize,
) -> ExternalChanges {
    let relevant: Vec<&WatchEvent> = events
        .iter()
        .filter(|event| is_relevant(event, open_file, awaited))
        .collect();
    let paths = unique(relevant.iter().map(|event| path_of(event)));
    ExternalChanges {
        paths: if paths.len() > limit {
            unique(relevant.iter().map(|event| event.dir.clone()))
        } else {
            paths
        },
        library: relevant.iter().any(|event| changes_library(event)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use WatchEventKind::{Change, Rename};

    fn event(dir: &str, kind: WatchEventKind, filename: Option<&str>) -> WatchEvent {
        WatchEvent {
            dir: dir.into(),
            kind,
            filename: filename.map(str::to_owned),
        }
    }

    fn classify(events: &[WatchEvent], open: Option<&str>) -> ExternalChanges {
        classify_events(events, open, &AwaitedNames::new(), MAX_REPORTED_PATHS)
    }

    #[test]
    fn reports_the_path_of_a_changed_file() {
        let result = classify(&[event("/s/pdf", Change, Some("notes.md"))], None);
        assert_eq!(result.paths, ["/s/pdf/notes.md"]);
        assert!(!result.library);
    }

    #[test]
    fn a_change_to_skill_md_or_any_rename_changes_the_library() {
        assert!(classify(&[event("/s/pdf", Change, Some("SKILL.md"))], None).library);
        assert!(classify(&[event("/s/pdf", Rename, Some("new.md"))], None).library);
        assert!(!classify(&[event("/s/pdf", Change, Some("other.md"))], None).library);
    }

    #[test]
    fn an_event_without_a_file_name_reports_the_folder_and_the_library() {
        let result = classify(&[event("/s", Rename, None)], None);
        assert_eq!(result.paths, ["/s"]);
        assert!(result.library);
    }

    #[test]
    fn ignores_hidden_names_and_node_modules() {
        let result = classify(
            &[
                event("/s/pdf", Rename, Some(".SKILL.md.abc.tmp")),
                event("/s/pdf", Change, Some(".git")),
                event("/s/pdf", Rename, Some("node_modules")),
            ],
            None,
        );
        assert_eq!(
            result,
            ExternalChanges {
                paths: vec![],
                library: false
            }
        );
    }

    #[test]
    fn keeps_a_hidden_name_that_is_the_open_file() {
        let result = classify(
            &[event("/s/pdf", Change, Some(".hidden.md"))],
            Some("/s/pdf/.hidden.md"),
        );
        assert_eq!(result.paths, ["/s/pdf/.hidden.md"]);
    }

    #[test]
    fn keeps_a_hidden_name_a_missing_folder_is_waited_for() {
        let mut awaited = AwaitedNames::new();
        awaited.insert("/s".into(), HashSet::from([".claude".to_owned()]));
        let result = classify_events(
            &[event("/s", Rename, Some(".claude"))],
            None,
            &awaited,
            MAX_REPORTED_PATHS,
        );
        assert!(result.library);
        assert_eq!(result.paths, ["/s/.claude"]);
    }

    #[test]
    fn lists_each_path_once() {
        let result = classify(
            &[
                event("/s", Change, Some("a.md")),
                event("/s", Change, Some("a.md")),
                event("/s", Change, Some("b.md")),
            ],
            None,
        );
        assert_eq!(result.paths, ["/s/a.md", "/s/b.md"]);
    }

    #[test]
    fn reports_the_folders_when_there_are_more_paths_than_the_limit() {
        let events: Vec<WatchEvent> = (0..5)
            .map(|i| {
                event(
                    if i < 3 { "/s/a" } else { "/s/b" },
                    Change,
                    Some(&format!("f{i}.md")),
                )
            })
            .collect();
        let result = classify_events(&events, None, &AwaitedNames::new(), 4);
        assert_eq!(result.paths, ["/s/a", "/s/b"]);
        let result = classify_events(&events, None, &AwaitedNames::new(), 5);
        assert_eq!(result.paths.len(), 5);
    }
}
