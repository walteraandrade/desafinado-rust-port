use std::cell::RefCell;

use super::fake_file_system::{
    DENIED_DIRECTORY, DENIED_FILE, EMPTY_DIRECTORY, FakeEntry, FakeTree, SPECIAL,
    create_fake_file_system, file, link,
};
use super::frontmatter_fields::FrontmatterProblemRule;
use super::scan_skills::{
    DEFAULT_MAX_DEPTH, DirectoryEntry, FsError, RootScan, ScanOptions, Skill, SkillFile,
    SkillFileSystem, is_skill_file, scan_skill_roots,
};

const CLAUDE: &str = "/home/u/.claude/skills";
const AGENTS: &str = "/home/u/.agents/skills";
const REPO: &str = "/home/u/code/repo/skills";

fn skill_text(name: &str) -> String {
    skill_text_with(name, "Does a thing.")
}

fn skill_text_with(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}

fn tree(entries: Vec<(String, FakeEntry)>) -> FakeTree {
    entries
}

fn entry(path: impl Into<String>, value: FakeEntry) -> (String, FakeEntry) {
    (path.into(), value)
}

fn skill_file(path: impl Into<String>, text: &str) -> (String, FakeEntry) {
    entry(path, file(text))
}

fn scan(entries: FakeTree, roots: &[&str], options: ScanOptions) -> Vec<RootScan> {
    let roots: Vec<String> = roots.iter().map(|root| (*root).to_string()).collect();
    scan_skill_roots(&create_fake_file_system(&entries), &roots, options)
}

fn scan_default(entries: FakeTree, roots: &[&str]) -> Vec<RootScan> {
    scan(entries, roots, ScanOptions::default())
}

fn folders_of(root_scan: Option<&RootScan>) -> Vec<String> {
    root_scan
        .map(|scan| {
            scan.skills
                .iter()
                .map(|skill| skill.folder_name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn only(entries: FakeTree) -> Skill {
    let scans = scan_default(entries, &[CLAUDE]);
    scans[0].skills.first().cloned().expect("no skill found")
}

#[test]
fn a_skill_is_a_folder_under_a_root_with_a_skill_md() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/pdf/SKILL.md"),
        &skill_text_with("pdf-processing", "Reads PDFs."),
    )]));
    assert_eq!(skill.real_path, format!("{CLAUDE}/pdf"));
    assert_eq!(skill.folder_name, "pdf");
    assert_eq!(skill.root, CLAUDE);
    assert_eq!(skill.name, "pdf-processing");
    assert_eq!(skill.description.as_deref(), Some("Reads PDFs."));
    assert_eq!(skill.problem, None);
}

#[test]
fn a_skill_is_shown_by_its_frontmatter_name_not_its_folder_name() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        &skill_text("other-name"),
    )]));
    assert_eq!(skill.name, "other-name");
    assert_eq!(skill.folder_name, "folder");
}

#[test]
fn a_skill_falls_back_to_the_folder_name_when_the_name_is_missing() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "---\ndescription: Only this.\n---\n",
    )]));
    assert_eq!(skill.name, "folder");
    assert_eq!(skill.description.as_deref(), Some("Only this."));
}

#[test]
fn a_skill_falls_back_to_the_folder_name_when_the_name_is_empty_or_not_text() {
    for line in [
        "name: \"\"",
        "name:",
        "name: 12",
        "name: [a]",
        "name: \"  \"",
    ] {
        let skill = only(tree(vec![skill_file(
            format!("{CLAUDE}/folder/SKILL.md"),
            &format!("---\n{line}\n---\n"),
        )]));
        assert_eq!(skill.name, "folder", "{line}");
    }
}

#[test]
fn a_skill_trims_the_name_and_the_description() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "---\nname: \"  spaced  \"\ndescription: >\n  Folded\n  text\n---\n",
    )]));
    assert_eq!(skill.name, "spaced");
    assert_eq!(skill.description.as_deref(), Some("Folded text"));
}

#[test]
fn a_skill_has_no_description_when_it_is_missing_empty_or_not_text() {
    for line in ["", "description:", "description: \"\"", "description: 3"] {
        let skill = only(tree(vec![skill_file(
            format!("{CLAUDE}/folder/SKILL.md"),
            &format!("---\nname: folder\n{line}\n---\n"),
        )]));
        assert_eq!(skill.description, None, "{line:?}");
    }
}

#[test]
fn a_skill_uses_the_folder_name_when_there_is_no_frontmatter() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "# Just text\n",
    )]));
    assert_eq!(skill.name, "folder");
    assert_eq!(skill.description, None);
    assert_eq!(skill.problem, None);
}

#[test]
fn a_skill_keeps_the_folder_name_and_reports_the_problem_for_a_broken_frontmatter() {
    let broken = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "---\nname: x\ndescription: Has: a colon\n---\n",
    )]));
    assert_eq!(broken.name, "folder");
    assert_eq!(broken.description, None);
    let problem = broken.problem.expect("problem");
    assert_eq!(problem.rule, FrontmatterProblemRule::YamlSyntaxError);
    assert_eq!(problem.line, Some(3));
    let unclosed = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "---\nname: x\n\n# Body\n",
    )]));
    assert_eq!(
        unclosed.problem.map(|problem| problem.rule),
        Some(FrontmatterProblemRule::FrontmatterNotClosed)
    );
}

#[test]
fn a_skill_reads_a_skill_md_with_a_bom_and_crlf_line_breaks() {
    let skill = only(tree(vec![skill_file(
        format!("{CLAUDE}/folder/SKILL.md"),
        "\u{feff}---\r\nname: crlf-skill\r\ndescription: Windows.\r\n---\r\n\r\n# Body\r\n",
    )]));
    assert_eq!(skill.name, "crlf-skill");
    assert_eq!(skill.description.as_deref(), Some("Windows."));
    assert_eq!(skill.problem, None);
}

#[test]
fn skips_a_folder_without_a_skill_md() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{CLAUDE}/notes/README.md"), "# Notes"),
            skill_file(format!("{CLAUDE}/real/SKILL.md"), &skill_text("real")),
            entry(format!("{CLAUDE}/empty"), EMPTY_DIRECTORY),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["real"]));
}

#[test]
fn skips_a_folder_whose_skill_md_is_another_name_or_a_directory() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{CLAUDE}/lower/skill.md"), &skill_text("lower")),
            skill_file(
                format!("{CLAUDE}/nested/inner/SKILL.md"),
                &skill_text("nested"),
            ),
            skill_file(format!("{CLAUDE}/dir/SKILL.md/file.md"), "x"),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), Vec::<String>::new());
}

#[test]
fn skips_files_and_special_entries_in_the_root() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{CLAUDE}/SKILL.md"), &skill_text("root")),
            skill_file(format!("{CLAUDE}/x.txt"), "x"),
            entry(format!("{CLAUDE}/pipe"), SPECIAL),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), Vec::<String>::new());
}

fn link_tree() -> Vec<(String, FakeEntry)> {
    vec![
        skill_file(format!("{AGENTS}/shared/SKILL.md"), &skill_text("shared")),
        skill_file(format!("{AGENTS}/shared/references/a.md"), "a"),
        skill_file(format!("{AGENTS}/own/SKILL.md"), &skill_text("own")),
        entry(
            format!("{CLAUDE}/shared"),
            link(&format!("{AGENTS}/shared")),
        ),
        entry(format!("{CLAUDE}/own"), link("../../.agents/skills/own")),
        skill_file(format!("{CLAUDE}/mine/SKILL.md"), &skill_text("mine")),
        entry(
            format!("{CLAUDE}/from-repo"),
            link(&format!("{REPO}/from-repo")),
        ),
        skill_file(
            format!("{REPO}/from-repo/SKILL.md"),
            &skill_text("from-repo"),
        ),
        skill_file(format!("{REPO}/from-repo/scripts/run.sh"), "#!/bin/sh"),
    ]
}

#[test]
fn symlinks_show_a_skill_reached_through_a_link_and_through_its_target_once() {
    let result = scan_default(link_tree(), &[CLAUDE, AGENTS]);
    assert_eq!(
        folders_of(result.first()),
        strings(&["from-repo", "mine", "own", "shared"])
    );
    assert_eq!(folders_of(result.get(1)), Vec::<String>::new());
}

#[test]
fn symlinks_identify_a_linked_skill_by_its_real_path() {
    let result = scan_default(link_tree(), &[CLAUDE, AGENTS]);
    let shared = result[0]
        .skills
        .iter()
        .find(|skill| skill.folder_name == "shared")
        .expect("shared");
    assert_eq!(shared.real_path, format!("{AGENTS}/shared"));
    assert_eq!(shared.root, CLAUDE);
    assert_eq!(
        shared
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        vec![
            format!("{AGENTS}/shared/SKILL.md"),
            format!("{AGENTS}/shared/references/a.md")
        ]
    );
}

#[test]
fn symlinks_give_the_skill_to_the_first_root_that_reaches_it() {
    let result = scan_default(link_tree(), &[AGENTS, CLAUDE]);
    assert_eq!(folders_of(result.first()), strings(&["own", "shared"]));
    assert_eq!(folders_of(result.get(1)), strings(&["from-repo", "mine"]));
}

#[test]
fn symlinks_show_a_skill_whose_link_points_outside_every_root() {
    let result = scan_default(link_tree(), &[CLAUDE, AGENTS]);
    let from_repo = result[0]
        .skills
        .iter()
        .find(|skill| skill.folder_name == "from-repo")
        .expect("from-repo");
    assert_eq!(from_repo.real_path, format!("{REPO}/from-repo"));
    assert_eq!(from_repo.root, CLAUDE);
    assert_eq!(from_repo.name, "from-repo");
    assert_eq!(
        from_repo
            .files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>(),
        strings(&["SKILL.md", "scripts/run.sh"])
    );
}

#[test]
fn symlinks_show_a_skill_once_when_the_same_root_is_given_twice() {
    let result = scan_default(link_tree(), &[CLAUDE, CLAUDE]);
    assert_eq!(
        folders_of(result.first()),
        strings(&["from-repo", "mine", "own", "shared"])
    );
    assert_eq!(folders_of(result.get(1)), Vec::<String>::new());
}

#[test]
fn symlinks_show_a_skill_once_when_a_root_is_a_link_to_another_root() {
    let mut entries = link_tree();
    entries.push(entry("/home/u/alias-root", link(AGENTS)));
    let result = scan_default(entries, &[AGENTS, "/home/u/alias-root"]);
    assert_eq!(folders_of(result.first()), strings(&["own", "shared"]));
    assert_eq!(folders_of(result.get(1)), Vec::<String>::new());
}

#[test]
fn symlinks_show_a_skill_once_when_two_links_in_a_root_point_to_it() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{AGENTS}/one/SKILL.md"), &skill_text("one")),
            entry(format!("{CLAUDE}/b-link"), link(&format!("{AGENTS}/one"))),
            entry(format!("{CLAUDE}/a-link"), link(&format!("{AGENTS}/one"))),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["a-link"]));
}

#[test]
fn symlinks_read_a_skill_md_that_is_itself_a_link() {
    let skill = only(tree(vec![
        entry(
            format!("{CLAUDE}/linked-file/SKILL.md"),
            link(&format!("{REPO}/text/SKILL.md")),
        ),
        skill_file(
            format!("{REPO}/text/SKILL.md"),
            &skill_text("from-elsewhere"),
        ),
    ]));
    assert_eq!(skill.name, "from-elsewhere");
    assert_eq!(
        skill.files,
        vec![SkillFile {
            path: format!("{REPO}/text/SKILL.md"),
            name: "SKILL.md".to_string(),
            is_markdown: true
        }]
    );
}

#[test]
fn symlinks_skip_a_broken_link_and_keep_scanning() {
    let result = scan_default(
        tree(vec![
            entry(format!("{CLAUDE}/gone"), link("/home/u/nowhere")),
            entry(
                format!("{CLAUDE}/loop-a"),
                link(&format!("{CLAUDE}/loop-b")),
            ),
            entry(
                format!("{CLAUDE}/loop-b"),
                link(&format!("{CLAUDE}/loop-a")),
            ),
            skill_file(format!("{CLAUDE}/fine/SKILL.md"), &skill_text("fine")),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["fine"]));
}

#[test]
fn symlinks_skip_a_link_to_a_file() {
    let result = scan_default(
        tree(vec![
            entry(
                format!("{CLAUDE}/to-file"),
                link(&format!("{REPO}/notes.md")),
            ),
            skill_file(format!("{REPO}/notes.md"), "# Notes"),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), Vec::<String>::new());
}

fn base() -> Vec<(String, FakeEntry)> {
    vec![skill_file(format!("{CLAUDE}/s/SKILL.md"), &skill_text("s"))]
}

fn with_base(extra: Vec<(String, FakeEntry)>) -> FakeTree {
    let mut entries = base();
    entries.extend(extra);
    entries
}

fn files(entries: FakeTree, options: ScanOptions) -> Vec<SkillFile> {
    scan(entries, &[CLAUDE], options)
        .first()
        .and_then(|scan| scan.skills.first())
        .map(|skill| skill.files.clone())
        .unwrap_or_default()
}

fn file_names(found: &[SkillFile]) -> Vec<String> {
    found.iter().map(|file| file.name.clone()).collect()
}

fn listed(path: &str, name: &str, is_markdown: bool) -> SkillFile {
    SkillFile {
        path: path.to_string(),
        name: name.to_string(),
        is_markdown,
    }
}

#[test]
fn files_list_the_folder_files_with_real_paths_and_relative_names() {
    let found = files(
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/notes.md"), "n"),
            skill_file(format!("{CLAUDE}/s/data.json"), "{}"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(
        found,
        vec![
            listed(&format!("{CLAUDE}/s/SKILL.md"), "SKILL.md", true),
            listed(&format!("{CLAUDE}/s/data.json"), "data.json", false),
            listed(&format!("{CLAUDE}/s/notes.md"), "notes.md", true),
        ]
    );
}

#[test]
fn files_list_files_in_nested_folders_with_the_folder_in_the_name() {
    let found = files(
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/references/api.md"), "api"),
            skill_file(format!("{CLAUDE}/s/references/deep/more.md"), "more"),
            skill_file(format!("{CLAUDE}/s/scripts/run.py"), "print()"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(
        file_names(&found),
        strings(&[
            "SKILL.md",
            "references/api.md",
            "references/deep/more.md",
            "scripts/run.py"
        ])
    );
    assert_eq!(
        found
            .iter()
            .map(|file| file.is_markdown)
            .collect::<Vec<_>>(),
        vec![true, true, true, false]
    );
}

#[test]
fn files_put_skill_md_first_and_order_the_rest_by_name() {
    let found = files(
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/Zeta.md"), "z"),
            skill_file(format!("{CLAUDE}/s/alpha.md"), "a"),
            skill_file(format!("{CLAUDE}/s/Beta.md"), "b"),
            skill_file(format!("{CLAUDE}/s/aardvark.txt"), "x"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(
        file_names(&found),
        strings(&["SKILL.md", "aardvark.txt", "alpha.md", "Beta.md", "Zeta.md"])
    );
}

#[test]
fn files_mark_markdown_by_extension_ignoring_case() {
    let found = files(
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/UPPER.MD"), "x"),
            skill_file(format!("{CLAUDE}/s/readme.markdown"), "x"),
            skill_file(format!("{CLAUDE}/s/md"), "x"),
            skill_file(format!("{CLAUDE}/s/notes.md.bak"), "x"),
        ]),
        ScanOptions::default(),
    );
    let marked: Vec<(String, bool)> = found
        .iter()
        .map(|file| (file.name.clone(), file.is_markdown))
        .collect();
    for (name, expected) in [
        ("SKILL.md", true),
        ("UPPER.MD", true),
        ("readme.markdown", false),
        ("md", false),
        ("notes.md.bak", false),
    ] {
        assert!(marked.contains(&(name.to_string(), expected)), "{name}");
    }
    assert_eq!(marked.len(), 5);
}

#[test]
fn files_skip_hidden_entries_and_node_modules() {
    let found = files(
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/.DS_Store"), "x"),
            skill_file(format!("{CLAUDE}/s/.git/config"), "x"),
            skill_file(format!("{CLAUDE}/s/node_modules/pkg/index.js"), "x"),
            skill_file(format!("{CLAUDE}/s/scripts/.cache/x.md"), "x"),
            skill_file(format!("{CLAUDE}/s/scripts/run.sh"), "x"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(file_names(&found), strings(&["SKILL.md", "scripts/run.sh"]));
}

#[test]
fn files_stop_at_the_maximum_depth() {
    let entries = || {
        with_base(vec![
            skill_file(format!("{CLAUDE}/s/a.md"), "x"),
            skill_file(format!("{CLAUDE}/s/one/b.md"), "x"),
            skill_file(format!("{CLAUDE}/s/one/two/c.md"), "x"),
            skill_file(format!("{CLAUDE}/s/one/two/three/d.md"), "x"),
            skill_file(format!("{CLAUDE}/s/one/two/three/four/e.md"), "x"),
        ])
    };
    assert_eq!(DEFAULT_MAX_DEPTH, 3);
    assert_eq!(
        file_names(&files(entries(), ScanOptions::default())),
        strings(&[
            "SKILL.md",
            "a.md",
            "one/b.md",
            "one/two/c.md",
            "one/two/three/d.md"
        ])
    );
    assert_eq!(
        file_names(&files(entries(), ScanOptions { max_depth: Some(0) })),
        strings(&["SKILL.md", "a.md"])
    );
    assert_eq!(
        file_names(&files(entries(), ScanOptions { max_depth: Some(1) })),
        strings(&["SKILL.md", "a.md", "one/b.md"])
    );
}

#[test]
fn files_list_a_file_reached_through_a_link_by_its_real_path() {
    let found = files(
        with_base(vec![
            entry(
                format!("{CLAUDE}/s/shared.md"),
                link(&format!("{REPO}/common/shared.md")),
            ),
            skill_file(format!("{REPO}/common/shared.md"), "x"),
        ]),
        ScanOptions::default(),
    );
    assert!(found.contains(&listed(
        &format!("{REPO}/common/shared.md"),
        "shared.md",
        true
    )));
}

#[test]
fn files_follow_a_linked_folder_inside_the_skill() {
    let found = files(
        with_base(vec![
            entry(
                format!("{CLAUDE}/s/references"),
                link(&format!("{REPO}/refs")),
            ),
            skill_file(format!("{REPO}/refs/a.md"), "a"),
        ]),
        ScanOptions::default(),
    );
    assert!(found.contains(&listed(
        &format!("{REPO}/refs/a.md"),
        "references/a.md",
        true
    )));
}

#[test]
fn files_do_not_loop_on_a_link_that_points_back_up() {
    let found = files(
        with_base(vec![
            entry(format!("{CLAUDE}/s/again"), link(&format!("{CLAUDE}/s"))),
            entry(format!("{CLAUDE}/s/refs/up"), link(&format!("{CLAUDE}/s"))),
            skill_file(format!("{CLAUDE}/s/refs/a.md"), "a"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(file_names(&found), strings(&["SKILL.md", "refs/a.md"]));
}

#[test]
fn files_list_a_file_once_when_two_links_reach_it() {
    let found = files(
        with_base(vec![
            entry(format!("{CLAUDE}/s/one.md"), link(&format!("{REPO}/x.md"))),
            entry(format!("{CLAUDE}/s/two.md"), link(&format!("{REPO}/x.md"))),
            skill_file(format!("{REPO}/x.md"), "x"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(
        found
            .iter()
            .filter(|file| file.path == format!("{REPO}/x.md"))
            .count(),
        1
    );
}

#[test]
fn files_skip_a_broken_link_an_unreadable_folder_and_a_loop_inside_the_skill() {
    let found = files(
        with_base(vec![
            entry(format!("{CLAUDE}/s/gone.md"), link("/home/u/nowhere")),
            entry(format!("{CLAUDE}/s/vault"), DENIED_DIRECTORY),
            entry(
                format!("{CLAUDE}/s/loop-a"),
                link(&format!("{CLAUDE}/s/loop-b")),
            ),
            entry(
                format!("{CLAUDE}/s/loop-b"),
                link(&format!("{CLAUDE}/s/loop-a")),
            ),
            skill_file(format!("{CLAUDE}/s/kept.md"), "k"),
        ]),
        ScanOptions::default(),
    );
    assert_eq!(file_names(&found), strings(&["SKILL.md", "kept.md"]));
}

#[test]
fn files_skip_sockets_and_other_special_entries() {
    let found = files(
        with_base(vec![entry(format!("{CLAUDE}/s/pipe"), SPECIAL)]),
        ScanOptions::default(),
    );
    assert_eq!(file_names(&found), strings(&["SKILL.md"]));
}

struct Spy<'a> {
    inner: &'a dyn SkillFileSystem,
    listed: RefCell<Vec<String>>,
}

impl SkillFileSystem for Spy<'_> {
    fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        self.listed.borrow_mut().push(path.to_string());
        self.inner.list_directory(path)
    }

    fn real_path(&self, path: &str) -> Result<String, FsError> {
        self.inner.real_path(path)
    }

    fn read_text(&self, path: &str) -> Result<String, FsError> {
        self.inner.read_text(path)
    }
}

#[test]
fn files_do_not_list_folders_below_the_maximum_depth() {
    let inner = create_fake_file_system(&with_base(vec![skill_file(
        format!("{CLAUDE}/s/one/two/three/four/e.md"),
        "x",
    )]));
    let spy = Spy {
        inner: &inner,
        listed: RefCell::new(Vec::new()),
    };
    scan_skill_roots(&spy, &[CLAUDE.to_string()], ScanOptions::default());
    assert!(
        !spy.listed
            .borrow()
            .iter()
            .any(|path| path.ends_with("/four"))
    );
}

#[test]
fn files_list_a_file_it_cannot_read_since_it_only_lists() {
    let found = files(
        with_base(vec![entry(format!("{CLAUDE}/s/secret.md"), DENIED_FILE)]),
        ScanOptions::default(),
    );
    assert_eq!(file_names(&found), strings(&["SKILL.md", "secret.md"]));
}

#[test]
fn unreadable_skips_a_skill_whose_skill_md_cannot_be_read() {
    let result = scan_default(
        tree(vec![
            entry(format!("{CLAUDE}/locked/SKILL.md"), DENIED_FILE),
            skill_file(format!("{CLAUDE}/fine/SKILL.md"), &skill_text("fine")),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["fine"]));
}

#[test]
fn unreadable_skips_a_skill_folder_it_cannot_list() {
    let result = scan_default(
        tree(vec![
            entry(format!("{CLAUDE}/locked"), DENIED_DIRECTORY),
            skill_file(format!("{CLAUDE}/fine/SKILL.md"), &skill_text("fine")),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["fine"]));
}

#[test]
fn unreadable_skips_a_skill_whose_skill_md_link_is_broken() {
    let result = scan_default(
        tree(vec![
            entry(
                format!("{CLAUDE}/dangling/SKILL.md"),
                link("/home/u/nowhere"),
            ),
            skill_file(format!("{CLAUDE}/fine/SKILL.md"), &skill_text("fine")),
        ]),
        &[CLAUDE],
    );
    assert_eq!(folders_of(result.first()), strings(&["fine"]));
}

#[test]
fn unreadable_reports_a_root_that_is_missing_a_file_or_unreadable() {
    let result = scan_default(
        tree(vec![
            skill_file("/home/u/file", "x"),
            entry("/home/u/locked", DENIED_DIRECTORY),
            skill_file(format!("{CLAUDE}/fine/SKILL.md"), &skill_text("fine")),
        ]),
        &["/home/u/missing", "/home/u/file", "/home/u/locked", CLAUDE],
    );
    assert_eq!(
        result
            .iter()
            .map(|scan| (scan.root.clone(), scan.readable))
            .collect::<Vec<_>>(),
        vec![
            ("/home/u/missing".to_string(), false),
            ("/home/u/file".to_string(), false),
            ("/home/u/locked".to_string(), false),
            (CLAUDE.to_string(), true),
        ]
    );
    assert!(result[..3].iter().all(|scan| scan.skills.is_empty()));
    assert_eq!(folders_of(result.get(3)), strings(&["fine"]));
}

#[test]
fn unreadable_reports_an_empty_root_as_readable_with_no_skills() {
    assert_eq!(
        scan_default(tree(vec![entry(CLAUDE, EMPTY_DIRECTORY)]), &[CLAUDE]),
        vec![RootScan {
            root: CLAUDE.to_string(),
            readable: true,
            skills: vec![]
        }]
    );
}

struct Failing;

impl SkillFileSystem for Failing {
    fn list_directory(&self, _path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        Err(FsError::new("EFAIL", "plain string"))
    }

    fn real_path(&self, _path: &str) -> Result<String, FsError> {
        Err(FsError::new("EFAIL", "undefined"))
    }

    fn read_text(&self, _path: &str) -> Result<String, FsError> {
        Err(FsError::new("EFAIL", "null"))
    }
}

#[test]
fn unreadable_does_not_fail_when_the_file_system_fails_in_every_call() {
    assert_eq!(
        scan_skill_roots(&Failing, &[CLAUDE.to_string()], ScanOptions::default()),
        vec![RootScan {
            root: CLAUDE.to_string(),
            readable: false,
            skills: vec![]
        }]
    );
}

#[test]
fn order_keeps_the_order_of_the_roots() {
    let result = scan_default(vec![], &["/z", "/a", "/m"]);
    assert_eq!(
        result
            .iter()
            .map(|scan| scan.root.clone())
            .collect::<Vec<_>>(),
        strings(&["/z", "/a", "/m"])
    );
}

#[test]
fn order_sorts_skills_by_name_ignoring_case_then_by_folder_name() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{CLAUDE}/z-folder/SKILL.md"), &skill_text("alpha")),
            skill_file(format!("{CLAUDE}/a-folder/SKILL.md"), &skill_text("Beta")),
            skill_file(format!("{CLAUDE}/c-folder/SKILL.md"), &skill_text("alpha")),
            skill_file(format!("{CLAUDE}/d-folder/SKILL.md"), &skill_text("Zulu")),
            skill_file(format!("{CLAUDE}/no-name/SKILL.md"), "# no frontmatter"),
        ]),
        &[CLAUDE],
    );
    assert_eq!(
        folders_of(result.first()),
        strings(&["c-folder", "z-folder", "a-folder", "no-name", "d-folder"])
    );
}

#[test]
fn order_sorts_by_code_point_not_by_locale() {
    let result = scan_default(
        tree(vec![
            skill_file(format!("{CLAUDE}/e/SKILL.md"), &skill_text("e")),
            skill_file(format!("{CLAUDE}/z/SKILL.md"), &skill_text("z")),
            skill_file(format!("{CLAUDE}/é/SKILL.md"), &skill_text("é")),
        ]),
        &[CLAUDE],
    );
    assert_eq!(
        result[0]
            .skills
            .iter()
            .map(|skill| skill.name.clone())
            .collect::<Vec<_>>(),
        strings(&["e", "z", "é"])
    );
}

struct Reversed<'a> {
    inner: &'a dyn SkillFileSystem,
}

impl SkillFileSystem for Reversed<'_> {
    fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        self.inner.list_directory(path).map(|mut entries| {
            entries.reverse();
            entries
        })
    }

    fn real_path(&self, path: &str) -> Result<String, FsError> {
        self.inner.real_path(path)
    }

    fn read_text(&self, path: &str) -> Result<String, FsError> {
        self.inner.read_text(path)
    }
}

#[test]
fn order_gives_the_same_result_whatever_order_the_file_system_lists_in() {
    let forward = create_fake_file_system(&tree(vec![
        skill_file(format!("{CLAUDE}/b/SKILL.md"), &skill_text("b")),
        skill_file(format!("{CLAUDE}/a/SKILL.md"), &skill_text("a")),
        skill_file(format!("{CLAUDE}/a/two.md"), "x"),
        skill_file(format!("{CLAUDE}/a/one.md"), "x"),
    ]));
    let reversed = Reversed { inner: &forward };
    let roots = [CLAUDE.to_string()];
    assert_eq!(
        scan_skill_roots(&reversed, &roots, ScanOptions::default()),
        scan_skill_roots(&forward, &roots, ScanOptions::default())
    );
}

#[test]
fn scans_nothing_for_no_roots() {
    assert_eq!(scan_default(vec![], &[]), vec![]);
}

#[test]
fn accepts_a_root_written_with_a_trailing_slash() {
    let root = format!("{CLAUDE}/");
    let result = scan_default(
        tree(vec![skill_file(
            format!("{CLAUDE}/s/SKILL.md"),
            &skill_text("s"),
        )]),
        &[&root],
    );
    assert_eq!(result[0].root, root);
    assert_eq!(result[0].skills[0].root, root);
    assert_eq!(result[0].skills[0].real_path, format!("{CLAUDE}/s"));
}

#[test]
fn accepts_a_root_written_with_several_trailing_slashes() {
    let root = format!("{CLAUDE}///");
    let result = scan_default(
        tree(vec![skill_file(
            format!("{CLAUDE}/s/SKILL.md"),
            &skill_text("s"),
        )]),
        &[&root],
    );
    assert_eq!(result[0].skills[0].real_path, format!("{CLAUDE}/s"));
}

#[test]
fn reads_the_machine_layout_many_skills_links_into_another_root_and_a_repo() {
    let mut entries: Vec<(String, FakeEntry)> = Vec::new();
    for index in 0..12 {
        let name = format!("skill-{index}");
        entries.push(skill_file(
            format!("{AGENTS}/{name}/SKILL.md"),
            &skill_text(&name),
        ));
        entries.push(skill_file(
            format!("{AGENTS}/{name}/references/ref.md"),
            "r",
        ));
        if index % 2 == 0 {
            entries.push(entry(
                format!("{CLAUDE}/{name}"),
                link(&format!("{AGENTS}/{name}")),
            ));
        } else {
            entries.push(skill_file(
                format!("{CLAUDE}/own-{index}/SKILL.md"),
                &skill_text(&format!("own-{index}")),
            ));
        }
    }
    let result = scan_default(entries, &[CLAUDE, AGENTS]);
    let all: Vec<&Skill> = result.iter().flat_map(|scan| scan.skills.iter()).collect();
    let mut paths: Vec<String> = all.iter().map(|skill| skill.real_path.clone()).collect();
    paths.sort();
    let mut unique = paths.clone();
    unique.dedup();
    assert_eq!(paths, unique);
    assert_eq!(all.len(), 12 + 6);
}

#[test]
fn a_skill_file_is_a_skill_md_only() {
    assert!(is_skill_file(Some("SKILL.md")));
    assert!(!is_skill_file(Some("README.md")));
    assert!(!is_skill_file(Some("skill.md")));
    assert!(!is_skill_file(Some("notes.markdown")));
    assert!(!is_skill_file(None));
}
