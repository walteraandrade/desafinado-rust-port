use serde::{Deserialize, Serialize};

use super::frontmatter_fields::FrontmatterProblem;
use super::library::LibraryRoot;
use super::scan_skills::{Skill, SkillFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeKind {
    Root,
    Skill,
    Folder,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: String,
    pub kind: TreeKind,
    pub name: String,
    pub children: Vec<TreeNode>,
    pub path: Option<String>,
    pub openable: Option<bool>,
    pub in_skill: Option<bool>,
    pub removable: Option<bool>,
    pub detail: Option<String>,
    pub note: Option<String>,
    pub description: Option<String>,
    pub problem: Option<FrontmatterProblem>,
}

impl TreeNode {
    pub fn new(id: &str, kind: TreeKind, name: &str) -> Self {
        Self {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            children: Vec::new(),
            path: None,
            openable: None,
            in_skill: None,
            removable: None,
            detail: None,
            note: None,
            description: None,
            problem: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItem {
    pub node: TreeNode,
    pub level: usize,
    pub parent_id: Option<String>,
    pub expandable: bool,
    pub expanded: bool,
}

const EVALS_FOLDER: &str = "evals";
const PROMPT_FILE: &str = "prompt.md";

fn root_id(root: &str) -> String {
    format!("root:{root}")
}

fn skill_id(real_path: &str) -> String {
    format!("skill:{real_path}")
}

struct Entry<'a> {
    file: &'a SkillFile,
    parts: Vec<&'a str>,
}

fn group_by_first<'a, 'b>(entries: Vec<Entry<'a>>) -> Vec<(String, Vec<Entry<'a>>)>
where
    'a: 'b,
{
    let mut groups: Vec<(String, Vec<Entry<'a>>)> = Vec::new();
    for entry in entries {
        let head = entry.parts.first().copied().unwrap_or("").to_string();
        match groups.iter_mut().find(|(name, _)| *name == head) {
            Some((_, group)) => group.push(entry),
            None => groups.push((head, vec![entry])),
        }
    }
    groups
}

fn eval_count(folder: &TreeNode) -> usize {
    folder
        .children
        .iter()
        .filter(|child| {
            child.kind == TreeKind::Folder
                && child.children.iter().any(|grandchild| {
                    grandchild.kind == TreeKind::File
                        && grandchild.name == PROMPT_FILE
                        && grandchild.in_skill == Some(true)
                })
        })
        .count()
}

fn eval_detail(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 Eval".to_string()),
        _ => Some(format!("{count} Evals")),
    }
}

fn build_level(
    skill_path: &str,
    owner_id: &str,
    prefix: &str,
    entries: Vec<Entry>,
    top: bool,
) -> Vec<TreeNode> {
    group_by_first(entries)
        .into_iter()
        .map(|(name, group)| {
            let id = format!("{owner_id}/{prefix}{name}");
            let leaf = group.iter().find(|entry| entry.parts.len() == 1);
            if let (Some(leaf), 1) = (leaf, group.len()) {
                return TreeNode {
                    path: Some(leaf.file.path.clone()),
                    openable: Some(leaf.file.is_markdown),
                    in_skill: Some(leaf.file.path == format!("{skill_path}/{prefix}{name}")),
                    ..TreeNode::new(&id, TreeKind::File, &name)
                };
            }
            let nested: Vec<Entry> = group
                .into_iter()
                .filter(|entry| entry.parts.len() > 1)
                .map(|entry| Entry {
                    file: entry.file,
                    parts: entry.parts[1..].to_vec(),
                })
                .collect();
            let mut folder = TreeNode::new(&id, TreeKind::Folder, &name);
            folder.children = build_level(
                skill_path,
                owner_id,
                &format!("{prefix}{name}/"),
                nested,
                false,
            );
            if top && name == EVALS_FOLDER {
                folder.detail = eval_detail(eval_count(&folder));
            }
            folder
        })
        .collect()
}

fn build_skill(skill: &Skill) -> TreeNode {
    let id = skill_id(&skill.real_path);
    let entries: Vec<Entry> = skill
        .files
        .iter()
        .map(|file| Entry {
            file,
            parts: file.name.split('/').collect(),
        })
        .collect();
    let mut node = TreeNode::new(&id, TreeKind::Skill, &skill.name);
    node.path = Some(skill.real_path.clone());
    node.description = skill.description.clone();
    node.problem = skill.problem.clone();
    node.children = build_level(&skill.real_path, &id, "", entries, true);
    node
}

fn root_note(root: &LibraryRoot) -> Option<String> {
    if !root.readable {
        Some("Folder not found or not readable".to_string())
    } else if root.skills.is_empty() {
        Some("No skills here".to_string())
    } else {
        None
    }
}

fn build_root(root: &LibraryRoot) -> TreeNode {
    let mut node = TreeNode::new(&root_id(&root.root), TreeKind::Root, &root.label);
    node.path = Some(root.root.clone());
    node.removable = Some(root.removable);
    node.detail = root.readable.then(|| root.skills.len().to_string());
    node.note = root_note(root);
    node.children = root.skills.iter().map(build_skill).collect();
    node
}

pub fn build_library_tree(roots: &[LibraryRoot]) -> Vec<TreeNode> {
    roots.iter().map(build_root).collect()
}

fn flatten(
    nodes: &[TreeNode],
    is_expanded: &dyn Fn(&TreeNode) -> bool,
    level: usize,
    parent_id: Option<&str>,
) -> Vec<TreeItem> {
    nodes
        .iter()
        .flat_map(|node| {
            let expandable = !node.children.is_empty();
            let expanded = expandable && is_expanded(node);
            let mut items = vec![TreeItem {
                node: node.clone(),
                level,
                parent_id: parent_id.map(str::to_string),
                expandable,
                expanded,
            }];
            if expanded {
                items.extend(flatten(
                    &node.children,
                    is_expanded,
                    level + 1,
                    Some(&node.id),
                ));
            }
            items
        })
        .collect()
}

pub fn visible_items(tree: &[TreeNode], is_expanded: &dyn Fn(&TreeNode) -> bool) -> Vec<TreeItem> {
    flatten(tree, is_expanded, 1, None)
}

fn path_to(nodes: &[TreeNode], is_open_file: &dyn Fn(&str) -> bool) -> Option<Vec<String>> {
    nodes.iter().find_map(|node| {
        if node.kind == TreeKind::File {
            return node
                .path
                .as_deref()
                .is_some_and(is_open_file)
                .then(Vec::new);
        }
        path_to(&node.children, is_open_file).map(|inside| {
            let mut ids = vec![node.id.clone()];
            ids.extend(inside);
            ids
        })
    })
}

pub fn ancestor_ids(tree: &[TreeNode], is_open_file: &dyn Fn(&str) -> bool) -> Vec<String> {
    path_to(tree, is_open_file).unwrap_or_default()
}

pub fn open_file_id(tree: &[TreeNode], is_open_file: &dyn Fn(&str) -> bool) -> Option<String> {
    flatten(tree, &|_| true, 1, None)
        .into_iter()
        .find(|item| {
            item.node.kind == TreeKind::File && item.node.path.as_deref().is_some_and(is_open_file)
        })
        .map(|item| item.node.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::fake_file_system::{create_fake_file_system, file, link};
    use crate::skills::frontmatter_fields::FrontmatterProblemRule;
    use crate::skills::scan_skills::{ScanOptions, scan_skill_roots};

    fn skill_file(path: &str, name: &str) -> SkillFile {
        SkillFile {
            path: path.to_string(),
            name: name.to_string(),
            is_markdown: name.to_lowercase().ends_with(".md"),
        }
    }

    fn skill(folder_name: &str, names: &[&str]) -> Skill {
        Skill {
            real_path: format!("/skills/{folder_name}"),
            folder_name: folder_name.to_string(),
            root: "/skills".to_string(),
            name: folder_name.to_string(),
            description: None,
            problem: None,
            files: names
                .iter()
                .map(|name| skill_file(&format!("/skills/{folder_name}/{name}"), name))
                .collect(),
        }
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

    fn names(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.name.as_str()).collect()
    }

    #[test]
    fn makes_a_node_per_root_with_its_skills_as_children() {
        let tree = build_library_tree(&[root(vec![
            skill("git", &["SKILL.md"]),
            skill("pdf", &["SKILL.md"]),
        ])]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].kind, TreeKind::Root);
        assert_eq!(tree[0].name, "~/.claude/skills");
        assert_eq!(names(&tree[0].children), vec!["git", "pdf"]);
        assert!(
            tree[0]
                .children
                .iter()
                .all(|child| child.kind == TreeKind::Skill)
        );
    }

    #[test]
    fn keeps_the_skill_name_description_and_frontmatter_problem() {
        let problem = FrontmatterProblem {
            rule: FrontmatterProblemRule::FrontmatterNotClosed,
            message: "bad".to_string(),
            line: None,
            column: None,
        };
        let mut git = skill("git", &["SKILL.md"]);
        git.name = "Git helper".to_string();
        git.description = Some("Commits".to_string());
        git.problem = Some(problem.clone());
        let tree = build_library_tree(&[root(vec![git])]);
        assert_eq!(tree[0].children[0].name, "Git helper");
        assert_eq!(tree[0].children[0].description.as_deref(), Some("Commits"));
        assert_eq!(tree[0].children[0].problem, Some(problem));
    }

    #[test]
    fn nests_files_in_folders_by_their_path_inside_the_skill() {
        let tree = build_library_tree(&[root(vec![skill(
            "pdf",
            &[
                "SKILL.md",
                "references/forms.md",
                "references/deep/more.md",
                "run.sh",
            ],
        )])]);
        let pdf = &tree[0].children[0];
        assert_eq!(
            names(&pdf.children),
            vec!["SKILL.md", "references", "run.sh"]
        );
        let references = &pdf.children[1];
        assert_eq!(references.kind, TreeKind::Folder);
        assert_eq!(names(&references.children), vec!["forms.md", "deep"]);
        assert_eq!(names(&references.children[1].children), vec!["more.md"]);
    }

    #[test]
    fn keeps_files_as_leaves_with_their_path_and_whether_they_open() {
        let tree = build_library_tree(&[root(vec![skill("pdf", &["SKILL.md", "run.sh"])])]);
        let skill_md = &tree[0].children[0].children[0];
        let run_sh = &tree[0].children[0].children[1];
        assert_eq!(skill_md.kind, TreeKind::File);
        assert_eq!(skill_md.path.as_deref(), Some("/skills/pdf/SKILL.md"));
        assert_eq!(skill_md.openable, Some(true));
        assert!(skill_md.children.is_empty());
        assert_eq!(run_sh.kind, TreeKind::File);
        assert_eq!(run_sh.openable, Some(false));
    }

    #[test]
    fn folds_evals_into_one_folder_labelled_with_its_eval_count() {
        let tree = build_library_tree(&[root(vec![skill(
            "pdf",
            &[
                "SKILL.md",
                "evals/one/prompt.md",
                "evals/one/graders/a.md",
                "evals/two/prompt.md",
                "evals/two/graders/b.md",
            ],
        )])]);
        let evals = &tree[0].children[0].children[1];
        assert_eq!(evals.kind, TreeKind::Folder);
        assert_eq!(evals.name, "evals");
        assert_eq!(evals.detail.as_deref(), Some("2 Evals"));
        assert_eq!(names(&evals.children), vec!["one", "two"]);
    }

    #[test]
    fn says_1_eval_for_one_and_nothing_for_none() {
        let one = build_library_tree(&[root(vec![skill("a", &["SKILL.md", "evals/x/prompt.md"])])]);
        assert_eq!(
            one[0].children[0].children[1].detail.as_deref(),
            Some("1 Eval")
        );
        let none = build_library_tree(&[root(vec![skill("a", &["SKILL.md", "evals/notes.md"])])]);
        assert_eq!(none[0].children[0].children[1].detail, None);
    }

    #[test]
    fn labels_only_the_evals_at_the_top_of_a_skill() {
        let tree = build_library_tree(&[root(vec![skill(
            "a",
            &["SKILL.md", "refs/evals/x/prompt.md"],
        )])]);
        assert_eq!(tree[0].children[0].children[1].children[0].detail, None);
    }

    fn scanned_tree(
        entries: Vec<(String, crate::skills::fake_file_system::FakeEntry)>,
        roots: &[&str],
    ) -> Vec<TreeNode> {
        let roots: Vec<String> = roots.iter().map(|root| (*root).to_string()).collect();
        let scans = scan_skill_roots(
            &create_fake_file_system(&entries),
            &roots,
            ScanOptions::default(),
        );
        let listing: Vec<LibraryRoot> = scans
            .into_iter()
            .map(|scan| {
                let label = scan.root.clone();
                LibraryRoot::from_scan(scan, label, false)
            })
            .collect();
        build_library_tree(&listing)
    }

    #[test]
    fn shows_a_skill_reached_through_a_link_and_through_its_target_once() {
        let real = "/agents/shared";
        let tree = scanned_tree(
            vec![
                (format!("{real}/SKILL.md"), file("---\nname: shared\n---\n")),
                ("/claude/shared".to_string(), link(real)),
            ],
            &["/claude", "/agents"],
        );
        let skills: Vec<&str> = tree
            .iter()
            .flat_map(|node| node.children.iter().map(|child| child.name.as_str()))
            .collect();
        assert_eq!(skills, vec!["shared"]);
    }

    #[test]
    fn does_not_count_an_eval_whose_prompt_md_is_a_link() {
        let tree = scanned_tree(
            vec![
                ("/s/a/SKILL.md".to_string(), file("---\nname: a\n---\n")),
                ("/s/a/evals/one/prompt.md".to_string(), file("p")),
                (
                    "/s/a/evals/two/prompt.md".to_string(),
                    link("/elsewhere/prompt.md"),
                ),
                ("/elsewhere/prompt.md".to_string(), file("p")),
            ],
            &["/s"],
        );
        let evals = tree[0].children[0]
            .children
            .iter()
            .find(|node| node.name == "evals")
            .expect("evals");
        assert_eq!(evals.detail.as_deref(), Some("1 Eval"));
    }

    #[test]
    fn keeps_unreadable_and_empty_roots_with_the_note_shown_today() {
        let mut gone = root(vec![]);
        gone.readable = false;
        gone.root = "/gone".to_string();
        let mut empty = root(vec![]);
        empty.root = "/empty".to_string();
        let tree = build_library_tree(&[gone, empty]);
        assert_eq!(tree[0].kind, TreeKind::Root);
        assert_eq!(
            tree[0].note.as_deref(),
            Some("Folder not found or not readable")
        );
        assert!(tree[0].children.is_empty());
        assert_eq!(tree[1].note.as_deref(), Some("No skills here"));
        assert!(tree[1].children.is_empty());
    }

    #[test]
    fn carries_removable_and_the_skill_count_on_a_root() {
        let mut counted = root(vec![skill("a", &["SKILL.md"]), skill("b", &["SKILL.md"])]);
        counted.removable = true;
        let tree = build_library_tree(&[counted]);
        assert_eq!(tree[0].removable, Some(true));
        assert_eq!(tree[0].detail.as_deref(), Some("2"));
    }

    #[test]
    fn gives_every_node_a_unique_id() {
        let tree = build_library_tree(&[root(vec![
            skill("a", &["SKILL.md", "x/y.md"]),
            skill("b", &["SKILL.md", "x/y.md"]),
        ])]);
        let ids: Vec<String> = visible_items(&tree, &|_| true)
            .into_iter()
            .map(|item| item.node.id)
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        assert_eq!(ids.len(), 1 + 2 + 2 * 3);
    }

    fn sample_tree() -> Vec<TreeNode> {
        build_library_tree(&[root(vec![
            skill("a", &["SKILL.md", "refs/x.md"]),
            skill("b", &["SKILL.md"]),
        ])])
    }

    #[test]
    fn visible_items_lists_only_nodes_whose_ancestors_are_all_expanded_with_levels() {
        let items = visible_items(&sample_tree(), &|node| node.kind == TreeKind::Root);
        assert_eq!(
            items
                .iter()
                .map(|item| (item.node.name.as_str(), item.level))
                .collect::<Vec<_>>(),
            vec![("~/.claude/skills", 1), ("a", 2), ("b", 2)]
        );
    }

    #[test]
    fn visible_items_opens_a_skill_and_a_folder_inside_it_when_asked() {
        let tree = sample_tree();
        let a_id = tree[0].children[0].id.clone();
        let items = visible_items(&tree, &|node| {
            node.kind == TreeKind::Root || node.id == a_id || node.kind == TreeKind::Folder
        });
        assert_eq!(
            items
                .iter()
                .map(|item| item.node.name.as_str())
                .collect::<Vec<_>>(),
            vec!["~/.claude/skills", "a", "SKILL.md", "refs", "x.md", "b"]
        );
    }

    #[test]
    fn visible_items_records_the_parent_and_whether_a_node_is_open() {
        let tree = sample_tree();
        let items = visible_items(&tree, &|node| node.kind == TreeKind::Root);
        assert_eq!(items[0].parent_id, None);
        assert!(items[0].expandable && items[0].expanded);
        assert_eq!(items[1].parent_id.as_deref(), Some(tree[0].id.as_str()));
        assert!(items[1].expandable && !items[1].expanded);
        assert!(items[2].expandable);
    }

    #[test]
    fn visible_items_does_not_call_a_leaf_or_an_empty_root_expandable() {
        let empty = build_library_tree(&[root(vec![])]);
        let items = visible_items(&empty, &|_| true);
        assert!(!items[0].expandable);
        assert!(!items[0].expanded);
    }

    fn evals_tree() -> Vec<TreeNode> {
        build_library_tree(&[root(vec![
            skill("a", &["SKILL.md", "evals/x/prompt.md"]),
            skill("b", &["SKILL.md"]),
        ])])
    }

    #[test]
    fn ancestor_ids_lists_the_nodes_that_hold_the_open_file_outermost_first() {
        let tree = evals_tree();
        let ids = ancestor_ids(&tree, &|path| path == "/skills/a/evals/x/prompt.md");
        let items = visible_items(&tree, &|node| ids.contains(&node.id));
        assert_eq!(
            items
                .iter()
                .map(|item| item.node.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "~/.claude/skills",
                "a",
                "SKILL.md",
                "evals",
                "x",
                "prompt.md",
                "b"
            ]
        );
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn ancestor_ids_is_empty_when_no_file_matches() {
        assert_eq!(
            ancestor_ids(&evals_tree(), &|_| false),
            Vec::<String>::new()
        );
    }

    #[test]
    fn open_file_id_is_the_id_of_the_file_node_that_is_open_even_in_a_folded_folder() {
        assert_eq!(
            open_file_id(&sample_tree(), &|path| path == "/skills/a/refs/x.md"),
            Some("skill:/skills/a/refs/x.md".to_string())
        );
    }

    #[test]
    fn open_file_id_is_none_when_no_file_is_open() {
        assert_eq!(open_file_id(&sample_tree(), &|_| false), None);
    }
}
