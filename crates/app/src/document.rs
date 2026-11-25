use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::base::input::Rope;

use desafinado_core::blocks::{BlockKind, extract_blocks, heading_text};

pub const UNTITLED_NAME: &str = "Untitled";
pub const SCRATCH_KEY: &str = "scratch:document";
pub const DEFAULT_FILE_NAME: &str = "document.md";
const MAX_SLUG_LENGTH: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSession {
    pub name: String,
    pub path: Option<PathBuf>,
    pub saved_text: Arc<str>,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub name: String,
    pub path: PathBuf,
}

pub fn opened_session(name: &str, path: &Path, text: &str, saved_text: &str) -> FileSession {
    FileSession {
        name: name.to_string(),
        path: Some(path.to_path_buf()),
        saved_text: Arc::from(saved_text),
        dirty: text != saved_text,
    }
}

/// Marks the open file as edited or not, comparing the Rope with the saved text in place.
pub fn mark_edited(session: Option<&mut FileSession>, text: &Rope) {
    if let Some(session) = session {
        session.dirty = *text != *session.saved_text;
    }
}

pub fn has_unsaved_changes(session: Option<&FileSession>, text: &str, baseline: &str) -> bool {
    match session {
        None => !text.trim().is_empty() && text != baseline,
        Some(session) => session.dirty,
    }
}

pub fn history_key(path: Option<&Path>) -> String {
    path.map_or_else(
        || SCRATCH_KEY.to_string(),
        |path| path.to_string_lossy().into_owned(),
    )
}

pub fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalVerdict {
    Nothing,
    Settled,
    Reload,
    Notice,
}

/// What the disk held for the open file: its text, or nothing because the file is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskState<T> {
    Text(T),
    Missing,
}

impl<T> DiskState<T> {
    pub fn of(disk: Option<T>) -> Self {
        disk.map_or(Self::Missing, Self::Text)
    }
}

impl DiskState<String> {
    pub fn as_deref(&self) -> DiskState<&str> {
        match self {
            Self::Text(text) => DiskState::Text(text),
            Self::Missing => DiskState::Missing,
        }
    }
}

pub struct ExternalFacts<'a> {
    pub disk: Option<&'a str>,
    pub saved_text: &'a str,
    pub document: &'a str,
    pub writing: Option<&'a str>,
    pub noticed: Option<DiskState<&'a str>>,
}

/// Tells which read of the file may be judged. A newer read supersedes an older one, which is
/// dropped; a save starting or ending, or the Document being replaced, makes the newest read
/// stale, and it is taken again.
#[derive(Debug, Default)]
pub struct DiskReads {
    latest_read: u64,
    epoch: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ReadTicket {
    read: u64,
    epoch: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadVerdict {
    Judge,
    Drop,
    Rerun,
}

impl DiskReads {
    pub fn start_read(&mut self) -> ReadTicket {
        self.latest_read += 1;
        ReadTicket {
            read: self.latest_read,
            epoch: self.epoch,
        }
    }

    /// A save started or ended, or the Document was replaced.
    pub fn bump_epoch(&mut self) {
        self.epoch += 1;
    }

    pub fn verdict(&self, ticket: ReadTicket) -> ReadVerdict {
        if ticket.read != self.latest_read {
            ReadVerdict::Drop
        } else if ticket.epoch != self.epoch {
            ReadVerdict::Rerun
        } else {
            ReadVerdict::Judge
        }
    }
}

pub fn decide_external_change(facts: &ExternalFacts) -> ExternalVerdict {
    let ExternalFacts {
        disk,
        saved_text,
        document,
        writing,
        noticed,
    } = *facts;
    if disk.is_some() && disk == writing {
        return ExternalVerdict::Nothing;
    }
    if disk == Some(saved_text) {
        return ExternalVerdict::Settled;
    }
    if noticed == Some(DiskState::of(disk)) {
        return ExternalVerdict::Nothing;
    }
    match disk {
        Some(disk) if document == saved_text || document == disk => ExternalVerdict::Reload,
        _ => ExternalVerdict::Notice,
    }
}

fn slug_of(title: &str) -> String {
    let mut slug = String::new();
    for character in title.chars().flat_map(char::to_lowercase) {
        let plain = fold_accent(character);
        if plain.is_ascii_alphanumeric() {
            slug.push(plain);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let cut: String = slug.chars().take(MAX_SLUG_LENGTH).collect();
    cut.trim_matches('-').to_string()
}

fn fold_accent(character: char) -> char {
    const FROM: &str = "àáâãäåèéêëìíîïòóôõöùúûüýÿñç";
    const TO: &str = "aaaaaaeeeeiiiiooooouuuuyync";
    FROM.chars()
        .position(|accented| accented == character)
        .and_then(|index| TO.chars().nth(index))
        .unwrap_or(character)
}

pub fn suggested_file_name(text: &str) -> String {
    let slug = extract_blocks(text)
        .iter()
        .find(|block| block.kind == BlockKind::Heading && block.level == Some(1))
        .map(|block| slug_of(&heading_text(block)))
        .unwrap_or_default();
    if slug.is_empty() {
        DEFAULT_FILE_NAME.to_string()
    } else {
        format!("{slug}.md")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinimalChange {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

pub fn minimal_change(before: &str, after: &str) -> MinimalChange {
    let prefix = before
        .char_indices()
        .zip(after.chars())
        .find(|((_, a), b)| a != b)
        .map_or_else(|| before.len().min(after.len()), |((index, _), _)| index);
    let prefix = floor_boundary(after, floor_boundary(before, prefix));
    let limit = (before.len() - prefix).min(after.len() - prefix);
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .scan(0, |total, size| {
            *total += size;
            Some(*total)
        })
        .take_while(|total| *total <= limit)
        .last()
        .unwrap_or(0);
    MinimalChange {
        from: prefix,
        to: before.len() - suffix,
        insert: after[prefix..after.len() - suffix].to_string(),
    }
}

fn floor_boundary(text: &str, index: usize) -> usize {
    (0..=index.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(
        disk: Option<&'a str>,
        saved: &'a str,
        document: &'a str,
        writing: Option<&'a str>,
        noticed: Option<DiskState<&'a str>>,
    ) -> ExternalVerdict {
        decide_external_change(&ExternalFacts {
            disk,
            saved_text: saved,
            document,
            writing,
            noticed,
        })
    }

    #[test]
    fn a_read_with_nothing_in_between_is_judged() {
        let mut reads = DiskReads::default();
        let ticket = reads.start_read();
        assert_eq!(reads.verdict(ticket), ReadVerdict::Judge);
    }

    #[test]
    fn a_read_superseded_by_a_newer_one_is_dropped() {
        let mut reads = DiskReads::default();
        let older = reads.start_read();
        let newer = reads.start_read();
        assert_eq!(reads.verdict(older), ReadVerdict::Drop);
        assert_eq!(reads.verdict(newer), ReadVerdict::Judge);
    }

    #[test]
    fn a_save_in_between_makes_the_newest_read_run_again() {
        let mut reads = DiskReads::default();
        let ticket = reads.start_read();
        reads.bump_epoch();
        assert_eq!(reads.verdict(ticket), ReadVerdict::Rerun);
        let again = reads.start_read();
        assert_eq!(reads.verdict(again), ReadVerdict::Judge);
    }

    #[test]
    fn overlapping_reads_judge_exactly_once_and_never_run_again_forever() {
        let mut reads = DiskReads::default();
        let mut pending = vec![reads.start_read(), reads.start_read()];
        let mut judged = 0;
        let mut reruns = 0;
        while let Some(ticket) = pending.pop() {
            match reads.verdict(ticket) {
                ReadVerdict::Judge => judged += 1,
                ReadVerdict::Drop => {}
                ReadVerdict::Rerun => {
                    reruns += 1;
                    pending.push(reads.start_read());
                }
            }
            assert!(reruns < 3);
        }
        assert_eq!((judged, reruns), (1, 0));
    }

    #[test]
    fn overlapping_reads_with_a_save_between_judge_once_after_one_rerun() {
        let mut reads = DiskReads::default();
        let first = reads.start_read();
        let second = reads.start_read();
        reads.bump_epoch();
        assert_eq!(reads.verdict(first), ReadVerdict::Drop);
        assert_eq!(reads.verdict(second), ReadVerdict::Rerun);
        let third = reads.start_read();
        assert_eq!(reads.verdict(third), ReadVerdict::Judge);
        assert_eq!(reads.verdict(second), ReadVerdict::Drop);
    }

    #[test]
    fn ignores_the_text_it_is_writing_and_settles_on_the_saved_text() {
        assert_eq!(
            facts(Some("w"), "s", "d", Some("w"), None),
            ExternalVerdict::Nothing
        );
        assert_eq!(
            facts(Some("s"), "s", "d", None, None),
            ExternalVerdict::Settled
        );
    }

    #[test]
    fn reloads_a_clean_document_and_notices_a_dirty_one() {
        assert_eq!(
            facts(Some("new"), "s", "s", None, None),
            ExternalVerdict::Reload
        );
        assert_eq!(
            facts(Some("new"), "s", "new", None, None),
            ExternalVerdict::Reload
        );
        assert_eq!(
            facts(Some("new"), "s", "mine", None, None),
            ExternalVerdict::Notice
        );
        assert_eq!(facts(None, "s", "s", None, None), ExternalVerdict::Notice);
    }

    #[test]
    fn notices_the_same_disk_text_once() {
        assert_eq!(
            facts(Some("new"), "s", "mine", None, Some(DiskState::Text("new"))),
            ExternalVerdict::Nothing
        );
        assert_eq!(
            facts(None, "s", "mine", None, Some(DiskState::Missing)),
            ExternalVerdict::Nothing
        );
        assert_eq!(
            facts(
                Some("newer"),
                "s",
                "mine",
                None,
                Some(DiskState::Text("new"))
            ),
            ExternalVerdict::Notice
        );
    }

    #[test]
    fn an_untitled_document_needs_saving_when_it_has_new_text() {
        assert!(!has_unsaved_changes(None, "  \n", "sample"));
        assert!(!has_unsaved_changes(None, "sample", "sample"));
        assert!(has_unsaved_changes(None, "typed", "sample"));
        let session = opened_session("a.md", Path::new("/a.md"), "x", "x");
        assert!(!has_unsaved_changes(Some(&session), "y", ""));
        let mut edited = session.clone();
        mark_edited(Some(&mut edited), &Rope::from_str("y"));
        assert!(has_unsaved_changes(Some(&edited), "y", ""));
        mark_edited(Some(&mut edited), &Rope::from_str("x"));
        assert!(!edited.dirty);
    }

    #[test]
    fn marks_an_edit_dirty_only_when_the_text_differs_from_the_saved_text() {
        let mut session = opened_session("a.md", Path::new("/a.md"), "same", "same");
        for (text, dirty) in [
            ("same!", true),
            ("sam", true),
            ("sane", true),
            ("same", false),
        ] {
            mark_edited(Some(&mut session), &Rope::from_str(text));
            assert_eq!(session.dirty, dirty, "{text}");
        }
        mark_edited(None, &Rope::from_str("x"));
    }

    #[test]
    fn keys_a_file_by_its_path_and_untitled_by_the_scratch_key() {
        assert_eq!(history_key(Some(Path::new("/n/a.md"))), "/n/a.md");
        assert_eq!(history_key(None), SCRATCH_KEY);
    }

    #[test]
    fn suggests_a_name_from_the_first_title() {
        assert_eq!(
            suggested_file_name("Intro\n\n# Café notes: v2!\n"),
            "cafe-notes-v2.md"
        );
        assert_eq!(suggested_file_name("No title"), DEFAULT_FILE_NAME);
        assert_eq!(suggested_file_name("## Second\n"), DEFAULT_FILE_NAME);
    }

    #[test]
    fn finds_the_smallest_change_between_two_texts() {
        let change = minimal_change("one two three", "one 2 three");
        assert_eq!(
            change,
            MinimalChange {
                from: 4,
                to: 7,
                insert: "2".into()
            }
        );
        let change = minimal_change("aaa", "aaaa");
        assert_eq!(&"aaa"[..change.from], "aaa");
        assert_eq!(change.insert, "a");
        assert_eq!(change.to, 3);
        let change = minimal_change("é", "è");
        assert_eq!(
            (change.from, change.to, change.insert.as_str()),
            (0, 2, "è")
        );
        assert_eq!(minimal_change("same", "same").insert, "");
    }
}
