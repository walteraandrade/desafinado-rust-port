use std::{
    fs,
    path::{Path, PathBuf},
};

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn git_dir_of(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = read_text(&dot_git)?;
    let target = pointer.strip_prefix("gitdir: ")?;
    Some(root.join(target))
}

fn find_git_dir(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(git_dir_of)
}

fn packed_ref(packed: &str, name: &str) -> Option<String> {
    packed.lines().find_map(|line| {
        let mut parts = line.split(' ');
        let hash = parts.next()?;
        (parts.next()? == name).then(|| hash.to_string())
    })
}

fn short_commit(hash: Option<String>) -> String {
    let hash = hash.unwrap_or_default();
    let valid = (7..=40).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        hash[..7].to_lowercase()
    } else {
        "unknown".to_string()
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let commit = find_git_dir(&manifest).and_then(|git_dir| {
        let head_path = git_dir.join("HEAD");
        println!("cargo:rerun-if-changed={}", head_path.display());
        let head = read_text(&head_path)?;
        let Some(name) = head.strip_prefix("ref: ") else {
            return Some(head);
        };
        let refs_dir = read_text(&git_dir.join("commondir"))
            .map_or_else(|| git_dir.clone(), |common| git_dir.join(common));
        let ref_path = refs_dir.join(name);
        println!("cargo:rerun-if-changed={}", ref_path.display());
        println!(
            "cargo:rerun-if-changed={}",
            refs_dir.join("packed-refs").display()
        );
        read_text(&ref_path).or_else(|| {
            read_text(&refs_dir.join("packed-refs")).and_then(|packed| packed_ref(&packed, name))
        })
    });
    let commit = std::env::var("DESAFINADO_COMMIT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map_or_else(
            || short_commit(commit),
            |value| short_commit(Some(value.trim().to_string())),
        );
    println!("cargo:rerun-if-env-changed=DESAFINADO_COMMIT");
    println!("cargo:rustc-env=DESAFINADO_COMMIT={commit}");
}
