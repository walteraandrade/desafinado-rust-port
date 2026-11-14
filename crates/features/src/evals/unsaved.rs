use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsaved {
    pub text: String,
    pub base: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    pub text: String,
    pub saved_text: String,
    pub restored: bool,
    pub disk_changed: bool,
}

pub fn restore_unsaved(disk: &str, unsaved: Option<&Unsaved>) -> Arrival {
    match unsaved {
        Some(unsaved) if unsaved.text != disk => Arrival {
            text: unsaved.text.clone(),
            saved_text: unsaved.base.clone(),
            restored: true,
            disk_changed: unsaved.base != disk,
        },
        _ => Arrival {
            text: disk.to_string(),
            saved_text: disk.to_string(),
            restored: false,
            disk_changed: false,
        },
    }
}

const SHOWN_FILES: usize = 2;

pub struct UnsavedInput<'a> {
    pub files: &'a [String],
    pub is_open: &'a dyn Fn(&str) -> bool,
    pub open_dirty: bool,
    pub stored: &'a HashSet<String>,
}

pub fn files_with_unsaved_text(input: &UnsavedInput<'_>) -> Vec<String> {
    input
        .files
        .iter()
        .filter(|file| {
            if (input.is_open)(file) {
                input.open_dirty
            } else {
                input.stored.contains(*file)
            }
        })
        .cloned()
        .collect()
}

fn list_of(names: &[String]) -> String {
    if names.len() <= SHOWN_FILES + 1 {
        match names {
            [] => String::new(),
            [only] => only.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    } else {
        format!(
            "{} and {} more",
            names[..SHOWN_FILES].join(", "),
            names.len() - SHOWN_FILES
        )
    }
}

pub fn unsaved_notice(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| {
        format!(
            "Unsaved text in {}. A run reads what is saved on disk.",
            list_of(names)
        )
    })
}

pub fn stored_is_unsaved(disk: Option<&str>, stored: Option<&Unsaved>) -> bool {
    match (stored, disk) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(stored), Some(disk)) => restore_unsaved(disk, Some(stored)).restored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn files() -> Vec<String> {
        names(&["SKILL.md", "evals/a/prompt.md", "notes.md"])
    }

    #[test]
    fn takes_the_open_file_from_its_dirty_flag_and_the_others_from_the_store() {
        let files = files();
        let stored: HashSet<String> = names(&["notes.md"]).into_iter().collect();
        let is_open = |name: &str| name == "SKILL.md";
        assert_eq!(
            files_with_unsaved_text(&UnsavedInput {
                files: &files,
                is_open: &is_open,
                open_dirty: true,
                stored: &stored
            }),
            names(&["SKILL.md", "notes.md"])
        );
    }

    #[test]
    fn ignores_a_stored_entry_for_the_open_file_when_it_is_clean() {
        let files = files();
        let stored: HashSet<String> = names(&["SKILL.md"]).into_iter().collect();
        let is_open = |name: &str| name == "SKILL.md";
        assert_eq!(
            files_with_unsaved_text(&UnsavedInput {
                files: &files,
                is_open: &is_open,
                open_dirty: false,
                stored: &stored
            }),
            Vec::<String>::new()
        );
    }

    #[test]
    fn notice_is_none_with_nothing_unsaved() {
        assert_eq!(unsaved_notice(&[]), None);
    }

    #[test]
    fn names_the_files_and_says_the_run_reads_the_disk() {
        assert_eq!(
            unsaved_notice(&names(&["SKILL.md"])).as_deref(),
            Some("Unsaved text in SKILL.md. A run reads what is saved on disk.")
        );
        assert_eq!(
            unsaved_notice(&names(&["a", "b", "c", "d"])).as_deref(),
            Some("Unsaved text in a, b and 2 more. A run reads what is saved on disk.")
        );
        assert_eq!(
            unsaved_notice(&names(&["a", "b", "c"])).as_deref(),
            Some("Unsaved text in a, b and c. A run reads what is saved on disk.")
        );
    }

    fn stored(text: &str, base: &str) -> Unsaved {
        Unsaved {
            text: text.into(),
            base: base.into(),
        }
    }

    #[test]
    fn stored_is_false_with_nothing_stored_or_text_equal_to_the_disk() {
        assert!(!stored_is_unsaved(Some("a"), None));
        assert!(!stored_is_unsaved(Some("a"), Some(&stored("a", "old"))));
    }

    #[test]
    fn stored_is_true_when_the_stored_text_differs_from_the_disk() {
        assert!(stored_is_unsaved(Some("a"), Some(&stored("b", "a"))));
    }

    #[test]
    fn stored_is_true_when_the_disk_cannot_be_read() {
        assert!(stored_is_unsaved(None, Some(&stored("b", "a"))));
    }

    #[test]
    fn restores_unsaved_text_over_the_disk() {
        let arrival = restore_unsaved("disk", Some(&stored("typed", "base")));
        assert_eq!(
            arrival,
            Arrival {
                text: "typed".into(),
                saved_text: "base".into(),
                restored: true,
                disk_changed: true
            }
        );
        assert!(!restore_unsaved("disk", None).restored);
    }
}
