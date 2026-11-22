use desafinado_core::log_err::LogErr;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use rustix::fs::{FlockOperation, flock};

use serde_json::{Value, json};

pub const LOCK_NAME: &str = "instance.lock";
pub const SOCKET_NAME: &str = "instance.sock";
const CONNECT_PATIENCE: Duration = Duration::from_secs(3);
const CONNECT_PAUSE: Duration = Duration::from_millis(50);
const MAX_MESSAGE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchMessage {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
}

pub fn encode_message(message: &LaunchMessage) -> Vec<u8> {
    json!({ "argv": message.argv, "cwd": message.cwd.to_string_lossy() })
        .to_string()
        .into_bytes()
}

pub fn parse_message(bytes: &[u8]) -> Option<LaunchMessage> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let argv = value
        .get("argv")?
        .as_array()?
        .iter()
        .map(|arg| arg.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    let cwd = PathBuf::from(value.get("cwd")?.as_str()?);
    cwd.is_absolute().then_some(LaunchMessage { argv, cwd })
}

pub struct PrimaryInstance {
    _lock: File,
    socket: PathBuf,
}

impl Drop for PrimaryInstance {
    fn drop(&mut self) {
        std::fs::remove_file(&self.socket).log_warn();
    }
}

pub enum Claim {
    Primary(PrimaryInstance),
    HandedOver,
}

fn try_lock(file: &File) -> bool {
    flock(file, FlockOperation::NonBlockingLockExclusive).is_ok()
}

fn hand_over(socket: &Path, message: &LaunchMessage) -> io::Result<()> {
    let deadline = Instant::now() + CONNECT_PATIENCE;
    loop {
        match UnixStream::connect(socket) {
            Ok(mut stream) => {
                stream.write_all(&encode_message(message))?;
                return stream.shutdown(std::net::Shutdown::Write);
            }
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => thread::sleep(CONNECT_PAUSE),
        }
    }
}

fn serve(listener: UnixListener, deliver: impl Fn(LaunchMessage) + Send + 'static) {
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .log_warn();
            let mut bytes = Vec::new();
            if stream
                .take(MAX_MESSAGE_BYTES)
                .read_to_end(&mut bytes)
                .is_ok()
                && let Some(message) = parse_message(&bytes)
            {
                deliver(message);
            }
        }
    });
}

pub fn claim(
    dir: &Path,
    message: &LaunchMessage,
    deliver: impl Fn(LaunchMessage) + Send + 'static,
) -> io::Result<Claim> {
    std::fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_NAME))?;
    let socket = dir.join(SOCKET_NAME);
    if !try_lock(&lock) {
        hand_over(&socket, message)?;
        return Ok(Claim::HandedOver);
    }
    if let Err(error) = std::fs::remove_file(&socket)
        && error.kind() != io::ErrorKind::NotFound
    {
        log::warn!("Could not remove the old instance socket: {error}");
    }
    let listener = UnixListener::bind(&socket)?;
    serve(listener, deliver);
    Ok(Claim::Primary(PrimaryInstance {
        _lock: lock,
        socket,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn message(args: &[&str]) -> LaunchMessage {
        LaunchMessage {
            argv: args.iter().map(std::string::ToString::to_string).collect(),
            cwd: PathBuf::from("/work"),
        }
    }

    #[test]
    fn a_message_survives_the_round_trip() {
        let sent = message(&["desafinado", "notes/a b.md", "--flag"]);
        assert_eq!(parse_message(&encode_message(&sent)), Some(sent));
    }

    #[test]
    fn refuses_a_malformed_message() {
        assert_eq!(parse_message(b"not json"), None);
        assert_eq!(parse_message(br#"{"argv":["a"]}"#), None);
        assert_eq!(parse_message(br#"{"argv":[1],"cwd":"/w"}"#), None);
        assert_eq!(parse_message(br#"{"argv":["a"],"cwd":"relative"}"#), None);
    }

    #[test]
    fn the_second_claim_hands_its_message_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let (sender, receiver) = mpsc::channel();
        let first = claim(dir.path(), &message(&["app"]), move |got| {
            sender.send(got).log_warn();
        })
        .unwrap();
        assert!(matches!(first, Claim::Primary(_)));
        let sent = message(&["app", "b.md"]);
        let second = claim(dir.path(), &sent, |_| {}).unwrap();
        assert!(matches!(second, Claim::HandedOver));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), sent);
        drop(first);
        let again = claim(dir.path(), &message(&["app"]), |_| {}).unwrap();
        assert!(matches!(again, Claim::Primary(_)));
    }
}
