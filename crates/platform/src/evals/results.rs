use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use desafinado_features::evals::baseline::{SummaryBaseline, parse_summary_baseline};
use desafinado_features::evals::types::ReportPart;

const RESULT_FILE: &str = "result.json";
const REPORT_DIR: &str = "report";
const REPORT_FILE: &str = "report.html";
const BASELINE_FILE: &str = "baseline.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextRun {
    pub json_file: PathBuf,
    pub output_dir: PathBuf,
}

fn part_dir(part: ReportPart) -> &'static str {
    match part {
        ReportPart::Today => "today",
        ReportPart::Version => "version",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastRun {
    pub baseline_text: Option<String>,
    pub first_text: String,
    pub second_text: Option<String>,
}

pub fn skill_key_of(real_path: &str) -> String {
    Sha256::digest(real_path.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_or_none(file: &Path) -> Option<String> {
    fs::read_to_string(file).ok()
}

fn files_in(dir: &Path) -> NextRun {
    NextRun {
        json_file: dir.join(RESULT_FILE),
        output_dir: dir.join(REPORT_DIR),
    }
}

pub struct ResultsStore {
    root: PathBuf,
}

impl ResultsStore {
    pub fn new(user_data_dir: &Path) -> Self {
        Self {
            root: user_data_dir.join("eval-runs"),
        }
    }

    fn skill_dir(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    fn next_dir(&self, key: &str) -> PathBuf {
        self.skill_dir(key).join("next")
    }

    fn last_dir(&self, key: &str) -> PathBuf {
        self.skill_dir(key).join("last")
    }

    fn retired_dir(&self, key: &str) -> PathBuf {
        self.skill_dir(key).join("retired")
    }

    pub fn discard(&self, key: &str) -> io::Result<()> {
        match fs::remove_dir_all(self.next_dir(key)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    pub fn begin(&self, key: &str) -> io::Result<NextRun> {
        self.discard(key)?;
        let dir = self.next_dir(key);
        fs::create_dir_all(&dir)?;
        Ok(files_in(&dir))
    }

    pub fn begin_part(&self, key: &str, part: ReportPart) -> io::Result<NextRun> {
        let dir = self.next_dir(key).join(part_dir(part));
        fs::create_dir_all(&dir)?;
        Ok(files_in(&dir))
    }

    pub fn read_next(&self, key: &str, part: Option<ReportPart>) -> Option<String> {
        let dir = match part {
            None => self.next_dir(key),
            Some(part) => self.next_dir(key).join(part_dir(part)),
        };
        read_or_none(&dir.join(RESULT_FILE))
    }

    pub fn write_baseline(&self, key: &str, text: &str) -> io::Result<()> {
        fs::write(self.next_dir(key).join(BASELINE_FILE), text)
    }

    pub fn promote(&self, key: &str) -> io::Result<()> {
        self.promote_with(key, &|from, to| fs::rename(from, to))
    }

    pub fn promote_with(
        &self,
        key: &str,
        rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        let retired = self.retired_dir(key);
        let _ = fs::remove_dir_all(&retired);
        let had_last = rename(&self.last_dir(key), &retired).is_ok();
        if let Err(error) = rename(&self.next_dir(key), &self.last_dir(key)) {
            if had_last {
                rename(&retired, &self.last_dir(key))?;
            }
            return Err(error);
        }
        let _ = fs::remove_dir_all(&retired);
        Ok(())
    }

    fn last_layout(&self, key: &str) -> (Option<String>, bool) {
        let baseline_text = read_or_none(&self.last_dir(key).join(BASELINE_FILE));
        let pair = baseline_text
            .as_deref()
            .and_then(parse_summary_baseline)
            .is_some_and(|baseline| matches!(baseline, SummaryBaseline::Version { .. }));
        (baseline_text, pair)
    }

    pub fn read_last(&self, key: &str) -> Option<LastRun> {
        let (baseline_text, pair) = self.last_layout(key);
        let dir = self.last_dir(key);
        let first_dir = if pair { dir.join("today") } else { dir.clone() };
        let first_text = read_or_none(&first_dir.join(RESULT_FILE))?;
        Some(LastRun {
            baseline_text,
            first_text,
            second_text: if pair {
                read_or_none(&dir.join("version").join(RESULT_FILE))
            } else {
                None
            },
        })
    }

    pub fn last_report(&self, key: &str, part: ReportPart) -> Option<PathBuf> {
        let (_, pair) = self.last_layout(key);
        if !pair && part == ReportPart::Version {
            return None;
        }
        let dir = self.last_dir(key);
        let base = if pair { dir.join(part_dir(part)) } else { dir };
        let file = base.join(REPORT_DIR).join(REPORT_FILE);
        fs::metadata(&file).ok()?.is_file().then_some(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ResultsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ResultsStore::new(dir.path());
        (dir, store)
    }

    fn finish(next: &NextRun, text: &str) {
        fs::write(&next.json_file, text).unwrap();
        fs::create_dir_all(&next.output_dir).unwrap();
        fs::write(next.output_dir.join("report.html"), "<html>").unwrap();
    }

    #[test]
    fn the_skill_key_hashes_the_real_path() {
        assert_eq!(skill_key_of("/a").len(), 64);
        assert_ne!(skill_key_of("/a"), skill_key_of("/b"));
        assert_eq!(
            skill_key_of(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn writes_a_run_under_next_of_the_skill_key() {
        let (dir, store) = store();
        let next = store.begin("k").unwrap();
        assert_eq!(
            next.json_file,
            dir.path().join("eval-runs/k/next/result.json")
        );
        assert_eq!(next.output_dir, dir.path().join("eval-runs/k/next/report"));
        finish(&next, "{}");
        assert_eq!(store.read_next("k", None).as_deref(), Some("{}"));
    }

    #[test]
    fn has_no_last_run_before_one_is_promoted_and_promotes_next_to_last() {
        let (_d, store) = store();
        assert_eq!(store.read_last("k"), None);
        finish(&store.begin("k").unwrap(), "one");
        store.promote("k").unwrap();
        assert_eq!(store.read_last("k").unwrap().first_text, "one");
        assert_eq!(store.read_next("k", None), None);
        finish(&store.begin("k").unwrap(), "two");
        store.promote("k").unwrap();
        assert_eq!(store.read_last("k").unwrap().first_text, "two");
        assert!(!store.retired_dir("k").exists());
    }

    #[test]
    fn finds_the_report_of_the_last_run() {
        let (_d, store) = store();
        assert_eq!(store.last_report("k", ReportPart::Today), None);
        finish(&store.begin("k").unwrap(), "one");
        store.promote("k").unwrap();
        let report = store.last_report("k", ReportPart::Today).unwrap();
        assert!(report.ends_with("last/report/report.html"));
        assert_eq!(store.last_report("k", ReportPart::Version), None);
    }

    #[test]
    fn leaves_last_alone_when_next_is_discarded_and_starts_each_run_empty() {
        let (_d, store) = store();
        finish(&store.begin("k").unwrap(), "one");
        store.promote("k").unwrap();
        let next = store.begin("k").unwrap();
        finish(&next, "two");
        store.discard("k").unwrap();
        assert_eq!(store.read_last("k").unwrap().first_text, "one");
        let again = store.begin("k").unwrap();
        assert!(!again.json_file.exists());
    }

    #[test]
    fn puts_the_last_run_back_when_promoting_fails() {
        let (_d, store) = store();
        finish(&store.begin("k").unwrap(), "one");
        store.promote("k").unwrap();
        finish(&store.begin("k").unwrap(), "two");
        let next = store.next_dir("k");
        let result = store.promote_with("k", &|from, to| {
            if from == next {
                Err(io::Error::other("boom"))
            } else {
                fs::rename(from, to)
            }
        });
        assert!(result.is_err());
        assert_eq!(store.read_last("k").unwrap().first_text, "one");
    }

    #[test]
    fn keeps_the_two_reports_of_a_version_run_in_separate_folders_with_the_baseline() {
        let (_d, store) = store();
        store.begin("k").unwrap();
        finish(&store.begin_part("k", ReportPart::Today).unwrap(), "today");
        finish(
            &store.begin_part("k", ReportPart::Version).unwrap(),
            "version",
        );
        store
            .write_baseline("k", r#"{"kind":"version","versionId":2,"at":10}"#)
            .unwrap();
        assert_eq!(
            store.read_next("k", Some(ReportPart::Version)).as_deref(),
            Some("version")
        );
        store.promote("k").unwrap();
        let last = store.read_last("k").unwrap();
        assert_eq!(
            (last.first_text.as_str(), last.second_text.as_deref()),
            ("today", Some("version"))
        );
        assert!(last.baseline_text.is_some());
        assert!(
            store
                .last_report("k", ReportPart::Version)
                .unwrap()
                .ends_with("last/version/report/report.html")
        );
        assert!(
            store
                .last_report("k", ReportPart::Today)
                .unwrap()
                .ends_with("last/today/report/report.html")
        );
    }

    #[test]
    fn reads_a_run_with_no_baseline_file_as_a_single_report() {
        let (_d, store) = store();
        finish(&store.begin("k").unwrap(), "single");
        store.promote("k").unwrap();
        let last = store.read_last("k").unwrap();
        assert_eq!((last.baseline_text, last.second_text), (None, None));
    }
}
