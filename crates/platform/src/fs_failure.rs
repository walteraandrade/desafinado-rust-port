use std::io::{Error, ErrorKind};

pub const FILE_NOT_FOUND_REASON: &str = "File not found";

const FALLBACK_REASON: &str = "Could not reach the file";

pub fn describe_failure(error: &Error) -> &'static str {
    match error.kind() {
        ErrorKind::NotFound => FILE_NOT_FOUND_REASON,
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::ReadOnlyFilesystem => "Read-only file system",
        ErrorKind::StorageFull => "No space left on the device",
        ErrorKind::IsADirectory | ErrorKind::NotADirectory => "Not a file",
        _ => match error.raw_os_error() {
            Some(libc::EPERM | libc::EACCES) => "Permission denied",
            Some(libc::EROFS) => "Read-only file system",
            Some(libc::ENOSPC) => "No space left on the device",
            Some(libc::EISDIR | libc::ENOTDIR) => "Not a file",
            _ => FALLBACK_REASON,
        },
    }
}

pub fn is_not_found(error: &Error) -> bool {
    error.kind() == ErrorKind::NotFound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_common_causes() {
        assert_eq!(
            describe_failure(&Error::from(ErrorKind::NotFound)),
            "File not found"
        );
        assert_eq!(
            describe_failure(&Error::from_raw_os_error(libc::EACCES)),
            "Permission denied"
        );
        assert_eq!(
            describe_failure(&Error::from_raw_os_error(libc::EROFS)),
            "Read-only file system"
        );
        assert_eq!(
            describe_failure(&Error::from_raw_os_error(libc::ENOSPC)),
            "No space left on the device"
        );
        assert_eq!(
            describe_failure(&Error::from_raw_os_error(libc::EISDIR)),
            "Not a file"
        );
        assert_eq!(
            describe_failure(&Error::from_raw_os_error(libc::ENOTDIR)),
            "Not a file"
        );
    }

    #[test]
    fn does_not_leak_paths_or_messages_for_the_rest() {
        assert_eq!(
            describe_failure(&Error::other("secret /home/me/path")),
            "Could not reach the file"
        );
    }
}
