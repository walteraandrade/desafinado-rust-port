use super::store::{HistoryStore, MarkSavedInput, RecordInput};
use super::types::VersionCause;
use crate::text::count_words;

const KEY: &str = "document";
const OTHER_KEY: &str = "other";

#[derive(Clone, Copy, Debug)]
pub struct StoreContractOptions {
    pub cap: Option<usize>,
    pub per_file: bool,
}

type Case<'a> = (&'static str, Box<dyn Fn(&dyn HistoryStore) + 'a>);

fn append(store: &dyn HistoryStore, texts: &[&str], key: &str) {
    for (index, text) in texts.iter().enumerate() {
        store
            .record(
                key,
                RecordInput {
                    at: 100 + index as i64,
                    cause: VersionCause::Edited,
                    text: (*text).to_string(),
                },
            )
            .expect("record");
    }
}

fn counts(store: &dyn HistoryStore, key: &str) -> Vec<(usize, usize)> {
    store
        .list(key)
        .expect("list")
        .iter()
        .map(|v| (v.words_added, v.words_removed))
        .collect()
}

fn mark(store: &dyn HistoryStore, key: &str, text: &str, name: &str, at: i64) {
    store
        .mark_saved(
            key,
            MarkSavedInput {
                text: text.into(),
                name: name.into(),
                at,
            },
        )
        .expect("mark_saved");
}

fn saved_marks(store: &dyn HistoryStore, key: &str) -> Vec<Option<String>> {
    store
        .list(key)
        .expect("list")
        .into_iter()
        .map(|v| v.saved)
        .collect()
}

fn texts_of(store: &dyn HistoryStore, key: &str) -> Vec<Option<String>> {
    store
        .list(key)
        .expect("list")
        .iter()
        .map(|v| store.text(key, v.id).expect("text"))
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "one table of contract cases, read top to bottom"
)]
fn base_cases<'a>() -> Vec<Case<'a>> {
    vec![
        (
            "lists nothing for a History that was never written",
            Box::new(|s| {
                assert!(s.list(KEY).unwrap().is_empty());
            }),
        ),
        (
            "makes the first Version a Started one, whatever Cause came in",
            Box::new(|s| {
                s.record(
                    KEY,
                    RecordInput {
                        at: 5,
                        cause: VersionCause::Edited,
                        text: "one two three".into(),
                    },
                )
                .unwrap();
                let list = s.list(KEY).unwrap();
                assert_eq!(list.len(), 1);
                assert_eq!(
                    (list[0].at, &list[0].cause, list[0].words),
                    (5, &VersionCause::Started, 3)
                );
            }),
        ),
        (
            "appends Versions in order, with the Cause and time they came with",
            Box::new(|s| {
                s.record(
                    KEY,
                    RecordInput {
                        at: 1,
                        cause: VersionCause::Edited,
                        text: "a".into(),
                    },
                )
                .unwrap();
                s.record(
                    KEY,
                    RecordInput {
                        at: 2,
                        cause: VersionCause::Opened {
                            name: "notes.md".into(),
                        },
                        text: "b".into(),
                    },
                )
                .unwrap();
                s.record(
                    KEY,
                    RecordInput {
                        at: 3,
                        cause: VersionCause::External,
                        text: "c".into(),
                    },
                )
                .unwrap();
                let versions = s.list(KEY).unwrap();
                let kinds: Vec<_> = versions
                    .iter()
                    .map(|v| (v.at, format!("{:?}", v.cause.kind())))
                    .collect();
                assert_eq!(
                    kinds,
                    vec![
                        (1, "Started".to_string()),
                        (2, "Opened".into()),
                        (3, "External".into())
                    ]
                );
                let ids: Vec<_> = versions.iter().map(|v| v.id).collect();
                let mut sorted = ids.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted, ids);
            }),
        ),
        (
            "counts the words of a text when it is recorded",
            Box::new(|s| {
                let texts = ["", "one", "one  two\nthree four", "# Title\n\nSome body."];
                append(s, &texts, KEY);
                let words: Vec<_> = s.list(KEY).unwrap().iter().map(|v| v.words).collect();
                assert_eq!(
                    words,
                    texts.iter().map(|t| count_words(t)).collect::<Vec<_>>()
                );
            }),
        ),
        (
            "counts every word of the first Version as added",
            Box::new(|s| {
                append(s, &["one two three"], KEY);
                assert_eq!(counts(s, KEY), vec![(3, 0)]);
            }),
        ),
        (
            "counts the words each Version added and removed against the one before",
            Box::new(|s| {
                append(
                    s,
                    &[
                        "one two three",
                        "one 2 three four",
                        "one 2 three four five six",
                        "one 2",
                    ],
                    KEY,
                );
                assert_eq!(counts(s, KEY), vec![(3, 0), (2, 1), (2, 0), (0, 4)]);
            }),
        ),
        (
            "counts a change of spacing as no words",
            Box::new(|s| {
                append(s, &["one two", "one  two\n\n"], KEY);
                assert_eq!(counts(s, KEY), vec![(2, 0), (0, 0)]);
            }),
        ),
        (
            "counts a restored old text against the last Version, not its source",
            Box::new(|s| {
                append(s, &["one two three", "one two three four"], KEY);
                s.record(
                    KEY,
                    RecordInput {
                        at: 300,
                        cause: VersionCause::Restored { from: 1 },
                        text: "one two three".into(),
                    },
                )
                .unwrap();
                assert_eq!(counts(s, KEY).last(), Some(&(0, 1)));
            }),
        ),
        (
            "counts a Version recorded by a save against the last Version",
            Box::new(|s| {
                append(s, &["one two"], KEY);
                mark(s, KEY, "one two three", "a.md", 9);
                assert_eq!(counts(s, KEY).last(), Some(&(1, 0)));
            }),
        ),
        (
            "does not count a Version a save only marked",
            Box::new(|s| {
                append(s, &["one two", "one"], KEY);
                mark(s, KEY, "one two", "a.md", 9);
                assert_eq!(counts(s, KEY), vec![(2, 0), (0, 1)]);
            }),
        ),
        (
            "counts the first Version recorded by a save as all words added",
            Box::new(|s| {
                mark(s, KEY, "only these", "a.md", 1);
                assert_eq!(counts(s, KEY), vec![(2, 0)]);
            }),
        ),
        (
            "does not record a text equal to the last Version",
            Box::new(|s| {
                append(s, &["same", "same"], KEY);
                assert_eq!(s.list(KEY).unwrap().len(), 1);
            }),
        ),
        (
            "records a text equal to an older Version",
            Box::new(|s| {
                append(s, &["a", "b", "a"], KEY);
                assert_eq!(s.list(KEY).unwrap().len(), 3);
            }),
        ),
        (
            "applies writes in the order they were made",
            Box::new(|s| {
                for (index, text) in ["one", "two", "three"].iter().enumerate() {
                    s.record(
                        KEY,
                        RecordInput {
                            at: index as i64,
                            cause: VersionCause::Edited,
                            text: (*text).into(),
                        },
                    )
                    .unwrap();
                }
                mark(s, KEY, "two", "a.md", 9);
                assert_eq!(
                    texts_of(s, KEY),
                    vec![
                        Some("one".to_string()),
                        Some("two".into()),
                        Some("three".into())
                    ]
                );
                assert_eq!(
                    saved_marks(s, KEY),
                    vec![None, Some("a.md".to_string()), None]
                );
            }),
        ),
        (
            "loads the text of a Version on demand",
            Box::new(|s| {
                append(s, &["first ✓", "second\n\nwith lines"], KEY);
                assert_eq!(
                    texts_of(s, KEY),
                    vec![
                        Some("first ✓".to_string()),
                        Some("second\n\nwith lines".into())
                    ]
                );
            }),
        ),
        (
            "returns null for the text of an unknown Version",
            Box::new(|s| {
                append(s, &["a"], KEY);
                assert_eq!(s.text(KEY, 987_654).unwrap(), None);
            }),
        ),
        (
            "marks the newest Version holding the saved text",
            Box::new(|s| {
                append(s, &["a", "b", "a", "c"], KEY);
                mark(s, KEY, "a", "notes.md", 200);
                assert_eq!(
                    saved_marks(s, KEY),
                    vec![None, None, Some("notes.md".to_string()), None]
                );
            }),
        ),
        (
            "keeps the marks of earlier saves",
            Box::new(|s| {
                append(s, &["a", "b"], KEY);
                mark(s, KEY, "a", "one.md", 200);
                mark(s, KEY, "b", "two.md", 201);
                assert_eq!(
                    saved_marks(s, KEY),
                    vec![Some("one.md".to_string()), Some("two.md".into())]
                );
            }),
        ),
        (
            "records an Edited Version first when no Version holds the saved text",
            Box::new(|s| {
                append(s, &["old"], KEY);
                mark(s, KEY, "new text", "notes.md", 300);
                let versions = s.list(KEY).unwrap();
                assert_eq!(versions.len(), 2);
                assert_eq!(versions[1].at, 300);
                assert_eq!(versions[1].cause, VersionCause::Edited);
                assert_eq!(versions[1].words, 2);
                assert_eq!(versions[1].saved.as_deref(), Some("notes.md"));
            }),
        ),
        (
            "starts the History when the saved text is its first Version",
            Box::new(|s| {
                mark(s, KEY, "only", "notes.md", 1);
                let versions = s.list(KEY).unwrap();
                assert_eq!(versions.len(), 1);
                assert_eq!(versions[0].cause, VersionCause::Started);
                assert_eq!(versions[0].saved.as_deref(), Some("notes.md"));
            }),
        ),
        (
            "keeps the text of a saved Version loadable",
            Box::new(|s| {
                mark(s, KEY, "saved text", "a.md", 1);
                let id = s.list(KEY).unwrap()[0].id;
                assert_eq!(s.text(KEY, id).unwrap().as_deref(), Some("saved text"));
            }),
        ),
    ]
}

fn cap_case<'a>(cap: Option<usize>) -> Case<'a> {
    match cap {
        None => (
            "keeps every Version",
            Box::new(|s| {
                let texts: Vec<String> = (0..250).map(|i| format!("version {i}")).collect();
                let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
                append(s, &refs, KEY);
                let versions = s.list(KEY).unwrap();
                assert_eq!(versions.len(), 250);
                let last = versions.last().unwrap();
                assert_eq!(
                    s.text(KEY, last.id).unwrap().as_deref(),
                    Some("version 249")
                );
            }),
        ),
        Some(cap) => (
            "keeps the newest Versions up to the cap",
            Box::new(move |s| {
                let count = cap + 5;
                let texts: Vec<String> = (0..count).map(|i| format!("version {i}")).collect();
                let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
                append(s, &refs, KEY);
                let versions = s.list(KEY).unwrap();
                assert_eq!(versions.len(), cap);
                assert_eq!(
                    s.text(KEY, versions[0].id).unwrap().as_deref(),
                    Some("version 5")
                );
                let last = versions.last().unwrap();
                assert_eq!(
                    s.text(KEY, last.id).unwrap(),
                    Some(format!("version {}", count - 1))
                );
            }),
        ),
    }
}

fn per_file_cases<'a>() -> Vec<Case<'a>> {
    vec![
        (
            "keeps the Versions of each key apart",
            Box::new(|s| {
                append(s, &["a1", "a2"], KEY);
                append(s, &["b1"], OTHER_KEY);
                assert_eq!(s.list(KEY).unwrap().len(), 2);
                let other = s.list(OTHER_KEY).unwrap();
                assert_eq!(other.len(), 1);
                assert_eq!(
                    (&other[0].cause, other[0].words),
                    (&VersionCause::Started, 1)
                );
            }),
        ),
        (
            "does not dedupe a text against another key",
            Box::new(|s| {
                append(s, &["same"], KEY);
                append(s, &["same"], OTHER_KEY);
                assert_eq!(s.list(OTHER_KEY).unwrap().len(), 1);
            }),
        ),
        (
            "does not load a text through another key",
            Box::new(|s| {
                append(s, &["a1", "a2"], KEY);
                append(s, &["b1"], OTHER_KEY);
                let mine = s.list(KEY).unwrap()[1].id;
                assert_eq!(s.text(OTHER_KEY, mine).unwrap(), None);
            }),
        ),
        (
            "counts the first Version of a key from nothing",
            Box::new(|s| {
                append(s, &["one two three"], KEY);
                append(s, &["one two three four"], OTHER_KEY);
                assert_eq!(counts(s, OTHER_KEY), vec![(4, 0)]);
            }),
        ),
        (
            "marks a save only in its own key",
            Box::new(|s| {
                append(s, &["same"], KEY);
                append(s, &["same"], OTHER_KEY);
                mark(s, OTHER_KEY, "same", "b.md", 9);
                assert_eq!(s.list(KEY).unwrap()[0].saved, None);
                assert_eq!(s.list(OTHER_KEY).unwrap()[0].saved.as_deref(), Some("b.md"));
            }),
        ),
    ]
}

pub fn run_store_contract(
    name: &str,
    create: &dyn Fn() -> Box<dyn HistoryStore>,
    options: StoreContractOptions,
) {
    let mut cases = base_cases();
    cases.push(cap_case(options.cap));
    if options.per_file {
        cases.extend(per_file_cases());
    }
    for (case, run) in cases {
        let store = create();
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(store.as_ref())));
        if let Err(cause) = outcome {
            eprintln!("{name} fails the History store contract: {case}");
            std::panic::resume_unwind(cause);
        }
    }
}
