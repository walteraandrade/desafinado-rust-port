use serde::{Deserialize, Serialize};

use super::library::LibraryRoot;
use super::scan_skills::{SKILL_FILE_NAME, Skill};
use crate::text::{is_js_space, js_trim};

pub const MAX_SEARCH_RESULTS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Skill,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub kind: SearchKind,
    pub path: String,
    pub label: String,
    pub label_match: Option<MatchRange>,
    pub detail: Option<String>,
    pub detail_match: Option<MatchRange>,
    pub context: Option<String>,
}

struct Ranked {
    result: SearchResult,
    rank: u8,
}

const SEPARATOR_MARKS: &str = "-_/.()[]{},;:!?'\"`“”‘’";

fn is_separator(c: char) -> bool {
    is_js_space(c) || SEPARATOR_MARKS.contains(c)
}

pub fn is_word_start(text: &str, index: usize) -> bool {
    index == 0
        || text
            .get(..index)
            .and_then(|before| before.chars().next_back())
            .is_some_and(is_separator)
}

struct Folded {
    text: String,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

fn fold(text: &str) -> Folded {
    let mut folded = Folded {
        text: String::new(),
        starts: Vec::new(),
        ends: Vec::new(),
    };
    for (start, original) in text.char_indices() {
        let end = start + original.len_utf8();
        for lower in original.to_lowercase() {
            folded.text.push(lower);
            for _ in 0..lower.len_utf8() {
                folded.starts.push(start);
                folded.ends.push(end);
            }
        }
    }
    folded
}

fn range_in(folded: &Folded, start: usize, length: usize) -> MatchRange {
    MatchRange {
        start: folded.starts.get(start).copied().unwrap_or(start),
        end: folded
            .ends
            .get(start + length - 1)
            .copied()
            .unwrap_or(start + length),
    }
}

fn match_at(text: &str, needle: &str, accept: &dyn Fn(&str, usize) -> bool) -> Option<MatchRange> {
    let folded = fold(text);
    let mut from = 0;
    while let Some(found) = folded.text.get(from..).and_then(|rest| rest.find(needle)) {
        let at = from + found;
        let original = folded.starts.get(at).copied().unwrap_or(at);
        let begins_character = at == 0 || folded.starts.get(at - 1).copied() != Some(original);
        if begins_character && accept(text, original) {
            return Some(range_in(&folded, at, needle.len()));
        }
        from = at + folded.text[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

fn prefix_match(text: &str, needle: &str) -> Option<MatchRange> {
    let folded = fold(text);
    folded
        .text
        .starts_with(needle)
        .then(|| range_in(&folded, 0, needle.len()))
}

fn word_match(text: &str, needle: &str) -> Option<MatchRange> {
    match_at(text, needle, &is_word_start)
}

fn one_line(text: &str) -> String {
    text.split(['\r', '\n'])
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn skill_row(
    skill: &Skill,
    path: &str,
    rank: u8,
    label_match: Option<MatchRange>,
    detail_match: Option<MatchRange>,
    detail: Option<String>,
) -> Ranked {
    Ranked {
        result: SearchResult {
            id: format!("skill:{}", skill.real_path),
            kind: SearchKind::Skill,
            path: path.to_string(),
            label: skill.name.clone(),
            label_match,
            detail,
            detail_match,
            context: None,
        },
        rank,
    }
}

fn skill_result(skill: &Skill, needle: &str) -> Option<Ranked> {
    let path = skill
        .files
        .iter()
        .find(|file| file.name == SKILL_FILE_NAME)
        .map(|file| file.path.as_str())?;
    let detail = skill.description.as_deref().map(one_line);
    if let Some(by_name) = prefix_match(&skill.name, needle) {
        return Some(skill_row(skill, path, 0, Some(by_name), None, detail));
    }
    if let Some(by_word) = word_match(&skill.name, needle) {
        return Some(skill_row(skill, path, 1, Some(by_word), None, detail));
    }
    let by_description = detail
        .as_deref()
        .and_then(|text| word_match(text, needle))?;
    Some(skill_row(
        skill,
        path,
        3,
        None,
        Some(by_description),
        detail,
    ))
}

fn file_results(skill: &Skill, needle: &str) -> Vec<Ranked> {
    skill
        .files
        .iter()
        .filter(|file| file.is_markdown && file.name != SKILL_FILE_NAME)
        .filter_map(|file| {
            word_match(&file.name, needle).map(|found| Ranked {
                result: SearchResult {
                    id: format!("file:{}", file.path),
                    kind: SearchKind::File,
                    path: file.path.clone(),
                    label: file.name.clone(),
                    label_match: Some(found),
                    detail: None,
                    detail_match: None,
                    context: Some(skill.name.clone()),
                },
                rank: 2,
            })
        })
        .collect()
}

pub fn search_library(roots: &[LibraryRoot], text: &str) -> Vec<SearchResult> {
    let needle = js_trim(text).to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut candidates: Vec<Ranked> = Vec::new();
    for skill in roots.iter().flat_map(|root| root.skills.iter()) {
        candidates.extend(skill_result(skill, &needle));
        candidates.extend(file_results(skill, &needle));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut unique: Vec<Ranked> = candidates
        .into_iter()
        .filter(|candidate| {
            if seen.contains(&candidate.result.id) {
                false
            } else {
                seen.push(candidate.result.id.clone());
                true
            }
        })
        .collect();
    unique.sort_by_key(|candidate| candidate.rank);
    unique
        .into_iter()
        .take(MAX_SEARCH_RESULTS)
        .map(|candidate| candidate.result)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::scan_skills::SkillFile;

    fn skill(name: &str, files: &[&str], description: Option<&str>, folder: &str) -> Skill {
        Skill {
            real_path: format!("/skills/{folder}"),
            folder_name: folder.to_string(),
            root: "/skills".to_string(),
            name: name.to_string(),
            description: description.map(str::to_string),
            problem: None,
            files: std::iter::once("SKILL.md")
                .chain(files.iter().copied())
                .map(|file| SkillFile {
                    path: format!("/skills/{folder}/{file}"),
                    name: file.to_string(),
                    is_markdown: file.to_lowercase().ends_with(".md"),
                })
                .collect(),
        }
    }

    fn simple(name: &str) -> Skill {
        skill(name, &[], None, name)
    }

    fn with_files(name: &str, files: &[&str]) -> Skill {
        skill(name, files, None, name)
    }

    fn described(name: &str, description: &str) -> Skill {
        skill(name, &[], Some(description), name)
    }

    fn root(skills: Vec<Skill>) -> LibraryRoot {
        LibraryRoot {
            root: "/skills".to_string(),
            label: "~/.claude/skills".to_string(),
            removable: false,
            readable: true,
            skills,
        }
    }

    fn listing(skills: Vec<Skill>) -> Vec<LibraryRoot> {
        vec![root(skills)]
    }

    fn labels(results: &[SearchResult]) -> Vec<String> {
        results
            .iter()
            .map(|result| {
                let kind = match result.kind {
                    SearchKind::Skill => "skill",
                    SearchKind::File => "file",
                };
                format!("{kind}:{}", result.label)
            })
            .collect()
    }

    fn range(start: usize, end: usize) -> Option<MatchRange> {
        Some(MatchRange { start, end })
    }

    #[test]
    fn is_word_start_starts_a_word_at_the_beginning_of_the_text() {
        assert!(is_word_start("pr-format", 0));
    }

    #[test]
    fn is_word_start_starts_a_word_after_a_separator() {
        for separator in ["-", "_", "/", " ", ".", "(", ",", "\""] {
            assert!(is_word_start(&format!("a{separator}b"), 2), "{separator}");
        }
    }

    #[test]
    fn is_word_start_does_not_start_a_word_inside_a_word() {
        assert!(!is_word_start("improve", 2));
    }

    #[test]
    fn returns_nothing_for_empty_or_blank_text() {
        let roots = listing(vec![simple("pr-format")]);
        assert_eq!(search_library(&roots, ""), vec![]);
        assert_eq!(search_library(&roots, "   "), vec![]);
    }

    #[test]
    fn puts_a_name_that_starts_with_the_text_before_a_word_that_starts_with_it() {
        let roots = listing(vec![simple("git-pr"), simple("pr-format"), simple("print")]);
        assert_eq!(
            labels(&search_library(&roots, "pr")),
            vec!["skill:pr-format", "skill:print", "skill:git-pr"]
        );
    }

    #[test]
    fn puts_name_matches_before_file_paths_and_file_paths_before_descriptions() {
        let roots = listing(vec![
            described("review", "Checks a pr body"),
            with_files("docs", &["evals/pr-case/prompt.md"]),
            simple("pr-format"),
        ]);
        assert_eq!(
            labels(&search_library(&roots, "pr")),
            vec![
                "skill:pr-format",
                "file:evals/pr-case/prompt.md",
                "skill:review"
            ]
        );
    }

    #[test]
    fn matches_case_insensitively() {
        let roots = listing(vec![simple("PR-Format")]);
        assert_eq!(
            labels(&search_library(&roots, "pr-F")),
            vec!["skill:PR-Format"]
        );
    }

    #[test]
    fn keeps_the_library_order_among_equal_matches() {
        let roots = listing(vec![simple("prb"), simple("pra"), simple("prc")]);
        assert_eq!(
            labels(&search_library(&roots, "pr")),
            vec!["skill:prb", "skill:pra", "skill:prc"]
        );
    }

    #[test]
    fn keeps_the_library_order_across_roots() {
        let mut first = root(vec![skill("prx", &[], None, "a")]);
        first.root = "/a".to_string();
        let mut second = root(vec![skill("pry", &[], None, "b")]);
        second.root = "/b".to_string();
        assert_eq!(
            labels(&search_library(&[first, second], "pr")),
            vec!["skill:prx", "skill:pry"]
        );
    }

    #[test]
    fn does_not_match_inside_a_word_of_a_name() {
        assert_eq!(
            search_library(&listing(vec![simple("improve")]), "pr"),
            vec![]
        );
    }

    #[test]
    fn matches_a_file_path_by_the_start_of_a_word_in_it() {
        let roots = listing(vec![with_files("docs", &["evals/a/graders/before.md"])]);
        assert_eq!(
            labels(&search_library(&roots, "graders")),
            vec!["file:evals/a/graders/before.md"]
        );
        assert_eq!(
            labels(&search_library(&roots, "graders/bef")),
            vec!["file:evals/a/graders/before.md"]
        );
        assert_eq!(search_library(&roots, "raders"), vec![]);
    }

    #[test]
    fn does_not_match_inside_a_word_of_a_file_path() {
        let roots = listing(vec![with_files("docs", &["evals/improve.md"])]);
        assert_eq!(search_library(&roots, "pr"), vec![]);
    }

    #[test]
    fn matches_a_description_by_the_start_of_a_word_only() {
        let roots = listing(vec![described("a", "Improve the pr text")]);
        assert_eq!(labels(&search_library(&roots, "pr")), vec!["skill:a"]);
        assert_eq!(search_library(&roots, "prove"), vec![]);
    }

    #[test]
    fn skips_skill_md_and_files_desafinado_cannot_open_as_file_rows() {
        let roots = listing(vec![with_files(
            "docs",
            &["notes.txt", "run.sh", "extra.md"],
        )]);
        assert_eq!(search_library(&roots, "skill"), vec![]);
        assert_eq!(search_library(&roots, "notes"), vec![]);
        assert_eq!(
            labels(&search_library(&roots, "extra")),
            vec!["file:extra.md"]
        );
    }

    #[test]
    fn opens_a_skill_through_its_skill_md_and_a_file_through_itself() {
        let roots = listing(vec![with_files("pr-format", &["evals/pr/prompt.md"])]);
        let results = search_library(&roots, "pr");
        assert_eq!(results[0].kind, SearchKind::Skill);
        assert_eq!(results[0].path, "/skills/pr-format/SKILL.md");
        assert_eq!(results[1].kind, SearchKind::File);
        assert_eq!(results[1].path, "/skills/pr-format/evals/pr/prompt.md");
    }

    #[test]
    fn names_the_skill_a_file_row_belongs_to() {
        let roots = listing(vec![with_files("docs", &["extra.md"])]);
        assert_eq!(
            search_library(&roots, "extra")[0].context.as_deref(),
            Some("docs")
        );
    }

    #[test]
    fn gives_the_range_to_underline_in_the_name() {
        let roots = listing(vec![simple("git-pr")]);
        let first = &search_library(&roots, "PR")[0];
        assert_eq!(first.label, "git-pr");
        assert_eq!(first.label_match, range(4, 6));
        assert_eq!(first.detail_match, None);
    }

    #[test]
    fn gives_the_range_to_underline_in_a_file_path() {
        let roots = listing(vec![with_files("docs", &["evals/a/prompt.md"])]);
        let first = &search_library(&roots, "prom")[0];
        assert_eq!(first.label, "evals/a/prompt.md");
        assert_eq!(first.label_match, range(8, 12));
    }

    #[test]
    fn gives_the_range_to_underline_in_a_description() {
        let roots = listing(vec![described("a", "Writes the PR body")]);
        let first = &search_library(&roots, "pr")[0];
        assert_eq!(first.label, "a");
        assert_eq!(first.label_match, None);
        assert_eq!(first.detail.as_deref(), Some("Writes the PR body"));
        assert_eq!(first.detail_match, range(11, 13));
    }

    #[test]
    fn shows_the_description_under_a_skill_found_by_name_without_underline() {
        let roots = listing(vec![described("pr-format", "Formats")]);
        let first = &search_library(&roots, "pr")[0];
        assert_eq!(first.detail.as_deref(), Some("Formats"));
        assert_eq!(first.detail_match, None);
    }

    #[test]
    fn collapses_line_breaks_in_a_description() {
        let roots = listing(vec![described("a", "one\ntwo pr")]);
        let first = &search_library(&roots, "pr")[0];
        assert_eq!(first.detail.as_deref(), Some("one two pr"));
        assert_eq!(first.detail_match, range(8, 10));
    }

    #[test]
    fn collapses_mixed_whitespace_and_blank_lines_around_line_breaks() {
        let roots = listing(vec![described("a", "  one \r\n \n\t two  three\n\n pr\n ")]);
        assert_eq!(
            search_library(&roots, "pr")[0].detail.as_deref(),
            Some("one two  three pr")
        );
    }

    #[test]
    fn returns_at_most_12_rows() {
        let many: Vec<Skill> = (0..30)
            .map(|index| simple(&format!("pr-{index}")))
            .collect();
        assert_eq!(
            search_library(&listing(many), "pr").len(),
            MAX_SEARCH_RESULTS
        );
        assert_eq!(MAX_SEARCH_RESULTS, 12);
    }

    #[test]
    fn keeps_the_best_matches_when_it_cuts_at_12() {
        let mut skills: Vec<Skill> = (0..20)
            .map(|index| described(&format!("x-{index}"), "a pr thing"))
            .collect();
        skills.push(simple("pr-last"));
        assert_eq!(search_library(&listing(skills), "pr")[0].label, "pr-last");
    }

    #[test]
    fn lists_a_skill_once_even_if_two_roots_reach_it() {
        let one = root(vec![simple("pr")]);
        let mut two = root(vec![simple("pr")]);
        two.root = "/b".to_string();
        assert_eq!(search_library(&[one, two], "pr").len(), 1);
    }

    #[test]
    fn starts_a_description_word_after_a_mark() {
        for mark in ["(", ")", ",", "\"", "'"] {
            let roots = listing(vec![described("a", &format!("see{mark}pr text"))]);
            assert_eq!(
                search_library(&roots, "pr")[0].detail_match,
                range(4, 6),
                "{mark}"
            );
        }
    }

    #[test]
    fn keeps_ranges_on_the_original_text_when_lower_casing_changes_its_length() {
        let roots = listing(vec![simple("İpr-x")]);
        assert_eq!(search_library(&roots, "pr-"), vec![]);
        let named = listing(vec![simple("İstanbul-pr")]);
        let first = &search_library(&named, "pr")[0];
        assert_eq!(first.label, "İstanbul-pr");
        assert_eq!(first.label_match, range(10, 12));
    }

    #[test]
    fn trims_the_text() {
        assert_eq!(
            labels(&search_library(&listing(vec![simple("pr")]), "  pr ")),
            vec!["skill:pr"]
        );
    }
}
