mod create_skill;
mod frontmatter;
mod handlers;
mod roots;
mod scan;
mod skill_name;

pub use create_skill::{CreateSkillResult, create_skill, skill_template};
pub use frontmatter::{FrontmatterProblem, FrontmatterReading, find_frontmatter, read_frontmatter};
pub use handlers::{
    AddRootResult, LibraryDeps, LibraryRoot, LibrarySnapshot, RemoveRootResult, add_library_root,
    is_listed_root, list_library, remove_library_root, skill_real_paths,
};
pub use roots::{
    ROOTS_FILE_NAME, RootsLoad, RootsStore, default_roots, display_path, parse_stored_roots,
    serialize_roots, with_root, without_root,
};
pub use scan::{
    DEFAULT_MAX_DEPTH, DirectoryEntry, EntryKind, RealFileSystem, RootScan, SKILL_FILE_NAME, Skill,
    SkillFile, SkillFileSystem, scan_skill_roots,
};
pub use skill_name::{SkillProblem, check_skill_name};
