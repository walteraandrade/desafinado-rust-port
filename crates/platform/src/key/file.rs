use desafinado_core::log_err::{LogErr, MissingIsRemoved};
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use super::store::KeyFile;

const OWNER_ONLY: u32 = 0o600;

#[derive(Debug, Clone)]
pub struct FsKeyFile {
    path: PathBuf,
}

impl FsKeyFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn temporary(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(format!(".{}.tmp", std::process::id()));
        PathBuf::from(name)
    }
}

impl KeyFile for FsKeyFile {
    fn read(&self) -> io::Result<Option<String>> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, content: &str) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.temporary();
        fs::remove_file(&temporary).missing_is_removed().log_warn();
        let written = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(OWNER_ONLY)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if written.is_err() {
            fs::remove_file(&temporary).missing_is_removed().log_warn();
        }
        written
    }

    fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reads_none_when_the_file_does_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            FsKeyFile::new(dir.path().join("k.json")).read().unwrap(),
            None
        );
    }

    #[test]
    fn writes_and_reads_back_creating_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = FsKeyFile::new(dir.path().join("a/b/k.json"));
        file.write("hello").unwrap();
        assert_eq!(file.read().unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn keeps_the_file_readable_by_its_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.json");
        let file = FsKeyFile::new(&path);
        file.write("x").unwrap();
        file.write("y").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn replaces_the_content_and_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let file = FsKeyFile::new(dir.path().join("k.json"));
        file.write("one").unwrap();
        file.write("two").unwrap();
        assert_eq!(file.read().unwrap().as_deref(), Some("two"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn removes_the_file_and_removing_twice_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let file = FsKeyFile::new(dir.path().join("k.json"));
        file.write("x").unwrap();
        file.remove().unwrap();
        file.remove().unwrap();
        assert_eq!(file.read().unwrap(), None);
    }
}
