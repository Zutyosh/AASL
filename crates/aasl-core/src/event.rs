use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    Aasl,
    Stdout,
    Stderr,
}

/// Everything observable that happens inside AASL. The GUI, the CLI and the
/// agent server all consume the same stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    OperationStarted {
        operation: String,
    },
    OperationFinished {
        operation: String,
        ok: bool,
        message: Option<String>,
    },
    StepStarted {
        step: String,
    },
    StepFinished {
        step: String,
        skipped: bool,
    },
    Progress {
        task: String,
        done: u64,
        total: Option<u64>,
    },
    Log {
        source: LogSource,
        line: String,
    },
    ProcessStarted {
        pid: u32,
        command: String,
    },
    ProcessExited {
        pid: u32,
        code: Option<i32>,
    },
    Notice {
        severity: Severity,
        message: String,
    },
}
