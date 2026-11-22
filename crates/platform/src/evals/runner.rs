use desafinado_core::log_err::LogErr;
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process_group};

const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_KILL_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct RunnerEnv {
    pub home: String,
    pub path: Option<String>,
    pub extra: Vec<(String, String)>,
}

pub fn child_env(env: &RunnerEnv) -> Vec<(String, String)> {
    let mut vars = vec![("HOME".to_owned(), env.home.clone())];
    if let Some(path) = &env.path {
        vars.push(("PATH".to_owned(), path.clone()));
    }
    vars.extend(env.extra.iter().cloned());
    vars
}

#[derive(Debug, Clone)]
pub struct EvalJob {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalExit {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalEvent {
    Started { pid: u32 },
    Output { stream: OutputStream, line: String },
    Exited(EvalExit),
}

#[derive(Default)]
struct State {
    pid: Option<u32>,
    generation: u64,
    killed: bool,
}

pub struct EvalRunner {
    env: RunnerEnv,
    kill_after: Duration,
    state: Arc<Mutex<State>>,
}

fn signal_group(pid: u32, signal: Signal) -> bool {
    i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .is_some_and(|group| kill_process_group(group, signal).is_ok())
}

pub fn parse_version(output: &str) -> Option<String> {
    let trimmed = output.trim();
    let mut end = 0;
    let bytes = trimmed.as_bytes();
    let mut parts = 0;
    let mut index = 0;
    loop {
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            break;
        }
        parts += 1;
        end = index;
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
        } else {
            break;
        }
    }
    (parts >= 2).then(|| trimmed[..end].to_owned())
}

fn pump(
    reader: impl Read + Send + 'static,
    stream: OutputStream,
    sender: std::sync::mpsc::Sender<(OutputStream, String)>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line)
                .trim_end_matches('\r')
                .to_owned();
            if sender.send((stream, text)).is_err() {
                break;
            }
        }
    });
}

impl EvalRunner {
    pub fn new(env: RunnerEnv) -> Self {
        Self::with_kill_after(env, DEFAULT_KILL_AFTER)
    }

    pub fn with_kill_after(env: RunnerEnv, kill_after: Duration) -> Self {
        Self {
            env,
            kill_after,
            state: Arc::default(),
        }
    }

    fn command(&self, executable: &PathBuf, args: &[String]) -> Command {
        let mut command = Command::new(executable);
        command.args(args).env_clear().envs(child_env(&self.env));
        command
    }

    pub fn version(&self, executable: &PathBuf) -> Option<String> {
        let mut child = self
            .command(executable, &["--version".to_owned()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            stdout.read_to_string(&mut text).log_warn();
            sender.send(text).log_warn();
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < VERSION_TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    child.kill().log_warn();
                    child.wait().log_warn();
                    return None;
                }
            }
        };
        let output = receiver.recv_timeout(Duration::from_secs(2)).ok()?;
        if status.success() {
            parse_version(&output)
        } else {
            None
        }
    }

    pub fn run(&self, job: &EvalJob, on_event: &mut dyn FnMut(EvalEvent)) -> io::Result<EvalExit> {
        let mut child: Child = self
            .command(&job.executable, &job.args)
            .current_dir(&job.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let pid = child.id();
        let generation = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.generation += 1;
            state.pid = Some(pid);
            state.killed = false;
            state.generation
        };
        on_event(EvalEvent::Started { pid });
        let (sender, receiver) = channel();
        if let Some(stdout) = child.stdout.take() {
            pump(stdout, OutputStream::Stdout, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            pump(stderr, OutputStream::Stderr, sender.clone());
        }
        drop(sender);
        for (stream, line) in receiver {
            on_event(EvalEvent::Output { stream, line });
        }
        let status = child.wait()?;
        let killed = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.generation == generation {
                state.pid = None;
            }
            state.killed
        };
        if killed {
            signal_group(pid, Signal::KILL);
        }
        let exit = EvalExit {
            exit_code: status.code(),
            signal: status.signal(),
        };
        on_event(EvalEvent::Exited(exit));
        Ok(exit)
    }

    pub fn kill(&self) -> bool {
        let (pid, generation) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(pid) = state.pid else {
                return false;
            };
            state.killed = true;
            (pid, state.generation)
        };
        let sent = signal_group(pid, Signal::TERM);
        let state = Arc::clone(&self.state);
        let wait = self.kill_after;
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            let current = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if current.generation == generation && current.pid == Some(pid) {
                signal_group(pid, Signal::KILL);
            }
        });
        sent
    }

    pub fn is_running(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pid
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::process::test_kill_process;
    use std::sync::Arc;

    fn sh(script: &str, cwd: &std::path::Path) -> EvalJob {
        EvalJob {
            executable: PathBuf::from("/bin/sh"),
            args: vec!["-c".into(), script.into()],
            cwd: cwd.to_path_buf(),
        }
    }

    fn env_for(dir: &std::path::Path) -> RunnerEnv {
        RunnerEnv {
            home: dir.to_string_lossy().into_owned(),
            path: std::env::var("PATH").ok(),
            extra: Vec::new(),
        }
    }

    fn run_collect(runner: &EvalRunner, job: &EvalJob) -> (io::Result<EvalExit>, Vec<EvalEvent>) {
        let mut events = Vec::new();
        let result = runner.run(job, &mut |e| events.push(e));
        (result, events)
    }

    #[test]
    fn the_child_env_carries_only_home_and_path_plus_the_extras() {
        let env = RunnerEnv {
            home: "/home/u".into(),
            path: Some("/bin".into()),
            extra: vec![("A".into(), "1".into())],
        };
        assert_eq!(
            child_env(&env),
            [
                ("HOME".to_owned(), "/home/u".to_owned()),
                ("PATH".into(), "/bin".into()),
                ("A".into(), "1".into())
            ]
        );
        let bare = RunnerEnv {
            home: "/h".into(),
            ..Default::default()
        };
        assert_eq!(child_env(&bare), [("HOME".to_owned(), "/h".to_owned())]);
    }

    #[test]
    fn runs_in_the_given_folder_with_only_home_and_path() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = std::fs::canonicalize(dir.path()).unwrap();
        assert!(
            std::env::var_os("CARGO_PKG_NAME").is_some(),
            "cargo sets CARGO_PKG_NAME for tests; the child must not inherit it"
        );
        let runner = EvalRunner::new(env_for(&cwd));
        let (result, events) = run_collect(&runner, &sh("pwd; env | sort | cut -d= -f1", &cwd));
        assert_eq!(
            result.unwrap(),
            EvalExit {
                exit_code: Some(0),
                signal: None
            }
        );
        let lines: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                EvalEvent::Output {
                    stream: OutputStream::Stdout,
                    line,
                } => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0], cwd.to_string_lossy());
        let names: Vec<&str> = lines[1..]
            .iter()
            .copied()
            .filter(|n| !n.starts_with('_') && *n != "PWD" && *n != "SHLVL" && *n != "OLDPWD")
            .collect();
        assert_eq!(names, ["HOME", "PATH"]);
        assert!(matches!(events.first(), Some(EvalEvent::Started { .. })));
        assert!(matches!(events.last(), Some(EvalEvent::Exited(_))));
    }

    #[test]
    fn streams_stdout_and_stderr_lines_as_they_come() {
        let dir = tempfile::tempdir().unwrap();
        let runner = EvalRunner::new(env_for(dir.path()));
        let (result, events) = run_collect(
            &runner,
            &sh("echo one; echo two >&2; printf 'no newline'", dir.path()),
        );
        assert!(result.is_ok());
        let out: Vec<(OutputStream, &str)> = events
            .iter()
            .filter_map(|e| match e {
                EvalEvent::Output { stream, line } => Some((*stream, line.as_str())),
                _ => None,
            })
            .collect();
        assert!(out.contains(&(OutputStream::Stdout, "one")));
        assert!(out.contains(&(OutputStream::Stderr, "two")));
        assert!(out.contains(&(OutputStream::Stdout, "no newline")));
    }

    #[test]
    fn reports_a_failing_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let runner = EvalRunner::new(env_for(dir.path()));
        let (result, _) = run_collect(&runner, &sh("exit 3", dir.path()));
        assert_eq!(
            result.unwrap(),
            EvalExit {
                exit_code: Some(3),
                signal: None
            }
        );
    }

    #[test]
    fn fails_when_the_program_cannot_start() {
        let dir = tempfile::tempdir().unwrap();
        let runner = EvalRunner::new(env_for(dir.path()));
        let job = EvalJob {
            executable: PathBuf::from("/definitely/not/here"),
            args: vec![],
            cwd: dir.path().to_path_buf(),
        };
        assert!(runner.run(&job, &mut |_| {}).is_err());
        assert!(!runner.is_running());
    }

    #[test]
    fn kills_the_running_child_with_sigterm() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(EvalRunner::new(env_for(dir.path())));
        let background = Arc::clone(&runner);
        let cwd = dir.path().to_path_buf();
        let handle = std::thread::spawn(move || background.run(&sh("sleep 30", &cwd), &mut |_| {}));
        while !runner.is_running() {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(runner.kill());
        let exit = handle.join().unwrap().unwrap();
        assert_eq!(exit.signal, Some(libc::SIGTERM));
        assert!(!runner.is_running());
    }

    #[test]
    fn kills_the_whole_process_tree_even_when_sigterm_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Arc::new(EvalRunner::with_kill_after(
            env_for(dir.path()),
            Duration::from_millis(150),
        ));
        let background = Arc::clone(&runner);
        let cwd = dir.path().to_path_buf();
        let marker = dir.path().join("pid");
        let script = format!(
            "trap '' TERM; sleep 60 & echo $! > {}; wait",
            marker.display()
        );
        let handle = std::thread::spawn(move || background.run(&sh(&script, &cwd), &mut |_| {}));
        while !marker.exists()
            || std::fs::read_to_string(&marker)
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        runner.kill();
        let exit = handle.join().unwrap().unwrap();
        assert_eq!(exit.signal, Some(libc::SIGKILL));
        let grandchild: i32 = std::fs::read_to_string(&marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let alive = Pid::from_raw(grandchild).is_some_and(|pid| test_kill_process(pid).is_ok())
            && std::fs::read_to_string(format!("/proc/{grandchild}/stat"))
                .is_ok_and(|stat| !stat.contains(") Z"));
        assert!(!alive);
    }

    #[test]
    fn does_not_kill_anything_when_nothing_runs() {
        let runner = EvalRunner::new(RunnerEnv::default());
        assert!(!runner.kill());
    }

    #[test]
    fn reads_the_version_from_the_version_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(&script, "#!/bin/sh\necho '2.1.5 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let runner = EvalRunner::new(env_for(dir.path()));
        assert_eq!(runner.version(&script).as_deref(), Some("2.1.5"));
    }

    #[test]
    fn returns_none_when_there_is_no_version() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(&script, "#!/bin/sh\necho 'unknown'\n").unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let runner = EvalRunner::new(env_for(dir.path()));
        assert_eq!(runner.version(&script), None);
        assert_eq!(runner.version(&PathBuf::from("/definitely/not/here")), None);
    }

    #[test]
    fn parses_dotted_numbers_only() {
        assert_eq!(
            parse_version("2.1.5 (Claude Code)").as_deref(),
            Some("2.1.5")
        );
        assert_eq!(parse_version("  10.20\n").as_deref(), Some("10.20"));
        assert_eq!(parse_version("7").as_deref(), None);
        assert_eq!(parse_version("v2.1"), None);
    }
}
