//! In-memory log ring buffer plus the on-disk launch.log.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use aasl_core::{Event, Events, LogSource};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub at: u64,
    pub source: LogSource,
    pub line: String,
}

/// Keeps the last `cap` lines of everything on the event bus so the GUI and
/// agents can read recent output without touching files.
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<(u64, VecDeque<LogLine>)>>,
    cap: usize,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new((0, VecDeque::with_capacity(cap.min(1024))))),
            cap,
        }
    }

    pub fn push(&self, source: LogSource, line: String) {
        let mut g = self.inner.lock().unwrap();
        g.0 += 1;
        let seq = g.0;
        if g.1.len() == self.cap {
            g.1.pop_front();
        }
        g.1.push_back(LogLine {
            seq,
            at: aasl_install::state::now(),
            source,
            line,
        });
    }

    /// Lines after `since` (exclusive), at most `limit`, newest last.
    pub fn tail(&self, since: Option<u64>, limit: usize) -> Vec<LogLine> {
        let g = self.inner.lock().unwrap();
        let v: Vec<LogLine> =
            g.1.iter()
                .filter(|l| since.is_none_or(|s| l.seq > s))
                .cloned()
                .collect();
        let skip = v.len().saturating_sub(limit);
        v.into_iter().skip(skip).collect()
    }

    /// Feed the buffer, and optionally a file, from the event bus.
    pub fn attach(&self, events: &Events, file: Option<&Path>) -> tokio::task::JoinHandle<()> {
        let mut rx = events.subscribe();
        let buf = self.clone();
        let file = file.map(Path::to_path_buf);
        tokio::spawn(async move {
            let mut out = match &file {
                Some(p) => {
                    if let Some(d) = p.parent() {
                        let _ = tokio::fs::create_dir_all(d).await;
                    }
                    tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(p)
                        .await
                        .ok()
                }
                None => None,
            };
            loop {
                let ev = match rx.recv().await {
                    Ok(ev) => ev,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        buf.push(LogSource::Aasl, format!("[{n} log lines dropped]"));
                        continue;
                    }
                    Err(_) => break,
                };
                let (source, line) = match ev {
                    Event::Log { source, line } => (source, line),
                    other => (LogSource::Aasl, describe(&other)),
                };
                if let Some(f) = out.as_mut() {
                    let tag = match source {
                        LogSource::Aasl => "aasl",
                        LogSource::Stdout => "out",
                        LogSource::Stderr => "err",
                    };
                    let _ = f.write_all(format!("[{tag}] {line}\n").as_bytes()).await;
                }
                buf.push(source, line);
            }
        })
    }
}

fn describe(e: &Event) -> String {
    match e {
        Event::OperationStarted { operation } => format!("{operation}: started"),
        Event::OperationFinished {
            operation,
            ok,
            message,
        } => {
            format!(
                "{operation}: {}{}",
                if *ok { "done" } else { "failed" },
                message
                    .as_ref()
                    .map(|m| format!(" ({m})"))
                    .unwrap_or_default()
            )
        }
        Event::StepStarted { step } => format!("step {step}"),
        Event::StepFinished { step, skipped } => format!(
            "step {step} {}",
            if *skipped { "already done" } else { "done" }
        ),
        Event::Progress { task, done, total } => match total {
            Some(t) if *t > 0 => format!("{task}: {}%", done * 100 / t),
            _ => format!("{task}: {done} bytes"),
        },
        Event::ProcessStarted { pid, command } => format!("started pid {pid}: {command}"),
        Event::ProcessExited { pid, code } => format!("pid {pid} exited with {code:?}"),
        Event::Notice { severity, message } => format!("{severity:?}: {message}"),
        Event::Log { line, .. } => line.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_and_tail() {
        let b = LogBuffer::new(3);
        for i in 0..5 {
            b.push(LogSource::Stdout, i.to_string());
        }
        let all = b.tail(None, 10);
        assert_eq!(
            all.iter().map(|l| l.line.as_str()).collect::<Vec<_>>(),
            ["2", "3", "4"]
        );
        assert_eq!(b.tail(Some(4), 10).len(), 1);
        assert_eq!(b.tail(None, 1)[0].line, "4");
    }

    #[tokio::test]
    async fn attach_writes_file() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("l/launch.log");
        let ev = aasl_core::event_bus();
        let b = LogBuffer::new(10);
        b.attach(&ev, Some(&f));
        tokio::task::yield_now().await;
        ev.send(Event::Log {
            source: LogSource::Stderr,
            line: "x".into(),
        })
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(b.tail(None, 10)[0].line, "x");
        assert!(std::fs::read_to_string(&f).unwrap().contains("[err] x"));
    }
}
