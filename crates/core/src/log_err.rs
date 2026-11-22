//! Visible handling for failures nobody upstream can act on.
//!
//! `let _ = fallible();` hides a failure for good. These helpers keep going
//! the same way but leave a line in the log, at the caller's location.

use std::fmt::Display;
use std::io;
use std::panic::Location;

pub trait LogErr<T> {
    /// Logs the error at error level and returns the value, if any.
    fn log_err(self) -> Option<T>;
    /// Logs the error at warn level, for best-effort cleanup and for a peer that went away.
    fn log_warn(self) -> Option<T>;
}

impl<T, E: Display> LogErr<T> for Result<T, E> {
    #[track_caller]
    fn log_err(self) -> Option<T> {
        log_at(self, log::Level::Error, Location::caller())
    }

    #[track_caller]
    fn log_warn(self) -> Option<T> {
        log_at(self, log::Level::Warn, Location::caller())
    }
}

fn log_at<T, E: Display>(
    result: Result<T, E>,
    level: log::Level,
    caller: &Location<'_>,
) -> Option<T> {
    result
        .inspect_err(|error| {
            log::log!(level, "{error} ({}:{})", caller.file(), caller.line());
        })
        .ok()
}

/// For removing something that may not be there: a missing file or folder counts as removed.
pub trait MissingIsRemoved {
    fn missing_is_removed(self) -> io::Result<()>;
}

impl MissingIsRemoved for io::Result<()> {
    fn missing_is_removed(self) -> io::Result<()> {
        match self {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_value_of_a_success() {
        let result: Result<u8, String> = Ok(7);
        assert_eq!(result.log_err(), Some(7));
    }

    #[test]
    fn turns_a_failure_into_none() {
        let result: Result<u8, String> = Err("disk full".into());
        assert_eq!(result.log_warn(), None);
    }

    #[test]
    fn counts_a_missing_file_as_removed() {
        let missing = std::fs::remove_file("/nonexistent/desafinado/file");
        assert!(missing.missing_is_removed().is_ok());
    }

    #[test]
    fn keeps_other_removal_failures() {
        let denied: io::Result<()> = Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(
            denied.missing_is_removed().map_err(|error| error.kind()),
            Err(io::ErrorKind::PermissionDenied)
        );
    }
}
