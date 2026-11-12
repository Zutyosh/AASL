//! Running external programs with their output streamed onto the event bus.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::{Error, Event, Events, LogSource, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.insert(k.into(), v.into());
        self
    }
    /// Shell-like rendering for logs. Not meant to be executed.
    pub fn display(&self) -> String {
        let env = self.env.iter().map(|(k, v)| format!("{k}={v:?}"));
        let args = self.args.iter().map(|a| format!("{a:?}"));
        env.chain(std::iter::once(self.program.display().to_string()))
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Runs a command to completion. Swappable so tests never start Wine.
pub trait Exec: Send + Sync {
    fn run<'a>(
        &'a self,
        spec: &'a CommandSpec,
        events: &'a Events,
    ) -> BoxFuture<'a, Result<Option<i32>>>;
}

/// The real implementation, on tokio::process.
pub struct TokioExec;

impl Exec for TokioExec {
    fn run<'a>(
        &'a self,
        spec: &'a CommandSpec,
        events: &'a Events,
    ) -> BoxFuture<'a, Result<Option<i32>>> {
        Box::pin(async move {
            let mut child = spawn(spec, events)?;
            let pid = child.id().unwrap_or_default();
            let status = child
                .wait()
                .await
                .map_err(|e| Error::io("waiting for process", e))?;
            let _ = events.send(Event::ProcessExited {
                pid,
                code: status.code(),
            });
            Ok(status.code())
        })
    }
}

/// Spawn with stdout and stderr forwarded line by line to the event bus.
pub fn spawn(spec: &CommandSpec, events: &Events) -> Result<tokio::process::Child> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::io(format!("starting {}", spec.program.display()), e))?;
    let pid = child.id().unwrap_or_default();
    let _ = events.send(Event::ProcessStarted {
        pid,
        command: spec.display(),
    });
    if let Some(out) = child.stdout.take() {
        forward(out, LogSource::Stdout, events.clone());
    }
    if let Some(err) = child.stderr.take() {
        forward(err, LogSource::Stderr, events.clone());
    }
    Ok(child)
}

/// Spawn a long-lived program detached from the caller: its own process
/// group, output written to `log` instead of pipes. Closing the launcher
/// (GUI, CLI with --detach, an agent's MCP process) then cannot take the
/// game down with a broken pipe or a group signal. The file is tailed onto
/// the event bus while the process lives.
pub fn spawn_logged(
    spec: &CommandSpec,
    log: &Path,
    events: &Events,
) -> Result<tokio::process::Child> {
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)
            .map_err(|e| Error::io(format!("creating {}", d.display()), e))?;
    }
    let out = std::fs::File::create(log)
        .map_err(|e| Error::io(format!("creating {}", log.display()), e))?;
    let err = out
        .try_clone()
        .map_err(|e| Error::io("duplicating the log handle", e))?;
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .kill_on_drop(false);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    let child = cmd
        .spawn()
        .map_err(|e| Error::io(format!("starting {}", spec.program.display()), e))?;
    let pid = child.id().unwrap_or_default();
    let _ = events.send(Event::ProcessStarted {
        pid,
        command: spec.display(),
    });
    tail(log.to_path_buf(), pid, events.clone());
    Ok(child)
}

/// Follow `path` onto the bus until process `pid` is gone and the file is drained.
fn tail(path: PathBuf, pid: u32, events: Events) {
    tokio::spawn(async move {
        let Ok(f) = tokio::fs::File::open(&path).await else {
            return;
        };
        let mut lines = BufReader::new(f).lines();
        let mut gone = false;
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let _ = events.send(Event::Log {
                        source: LogSource::Stdout,
                        line,
                    });
                }
                Ok(None) => {
                    // One more pass after the process is gone, for its last writes.
                    if gone {
                        break;
                    }
                    gone = !Path::new(&format!("/proc/{pid}")).exists();
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                Err(_) => break,
            }
        }
    });
}

fn forward(
    r: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    source: LogSource,
    events: Events,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = events.send(Event::Log { source, line });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn logged_spawn_writes_file_and_tails_it() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("l/game.log");
        let events = crate::event_bus();
        let mut rx = events.subscribe();
        let spec = CommandSpec::new("sh")
            .arg("-c")
            .arg("echo one; sleep 0.3; echo two >&2");
        let mut c = spawn_logged(&spec, &log, &events).unwrap();
        assert_eq!(c.wait().await.unwrap().code(), Some(0));
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let mut lines = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let Event::Log { line, .. } = e {
                lines.push(line);
            }
        }
        assert_eq!(lines, ["one", "two"]);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "one\ntwo\n");
    }

    #[tokio::test]
    async fn streams_output_and_exit_code() {
        let events = crate::event_bus();
        let mut rx = events.subscribe();
        let spec = CommandSpec::new("sh")
            .arg("-c")
            .arg("echo out; echo err >&2; exit 3");
        let code = TokioExec.run(&spec, &events).await.unwrap();
        assert_eq!(code, Some(3));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut lines = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let Event::Log { line, .. } = e {
                lines.push(line);
            }
        }
        lines.sort();
        assert_eq!(lines, vec!["err", "out"]);
    }

    #[test]
    fn display_quotes_args() {
        let s = CommandSpec::new("/bin/umu-run").arg("a b").env("K", "v");
        assert_eq!(s.display(), "K=\"v\" /bin/umu-run \"a b\"");
    }
}
