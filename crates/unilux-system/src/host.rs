use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The slice of the operating system the probes need.
pub trait Host: Send + Sync {
    fn env(&self, key: &str) -> Option<String>;
    fn read(&self, path: &Path) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    fn list_dir(&self, path: &Path) -> Vec<PathBuf>;
    /// Run a command and return its stdout and stderr joined, if it ran.
    fn run(&self, program: &str, args: &[&str]) -> Option<String>;
    /// The path with symlinks resolved, or the path itself.
    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn home(&self) -> PathBuf {
        PathBuf::from(self.env("HOME").unwrap_or_default())
    }
}

pub struct RealHost;

impl Host for RealHost {
    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(path)
            .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        out.sort();
        out
    }
    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(program).args(args).output().ok()?;
        let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&out.stderr));
        Some(s)
    }
    fn canonicalize(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// In-memory host for tests.
#[derive(Default)]
pub struct FakeHost {
    pub env: HashMap<String, String>,
    pub files: HashMap<PathBuf, String>,
    pub commands: HashMap<String, String>,
    pub links: HashMap<PathBuf, PathBuf>,
}

impl FakeHost {
    pub fn new() -> Self {
        let mut h = Self::default();
        h.env.insert("HOME".into(), "/home/u".into());
        h.env.insert("PATH".into(), "/usr/bin".into());
        h
    }
    pub fn with_env(mut self, k: &str, v: &str) -> Self {
        self.env.insert(k.into(), v.into());
        self
    }
    pub fn with_file(mut self, p: &str, content: &str) -> Self {
        self.files.insert(p.into(), content.into());
        self
    }
    pub fn with_link(mut self, link: &str, target: &str) -> Self {
        self.links.insert(link.into(), target.into());
        self
    }
    pub fn with_command(mut self, cmdline: &str, out: &str) -> Self {
        self.commands.insert(cmdline.into(), out.into());
        self
    }
}

impl Host for FakeHost {
    fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }
    fn read(&self, path: &Path) -> Option<String> {
        self.files.get(&self.canonicalize(path)).cloned()
    }
    fn exists(&self, path: &Path) -> bool {
        let path = self.canonicalize(path);
        self.files.contains_key(&path) || self.files.keys().any(|k| k.starts_with(&path))
    }
    fn canonicalize(&self, path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for c in path.components() {
            match c {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                c => {
                    out.push(c);
                    if let Some(t) = self.links.get(&out) {
                        out = t.clone();
                    }
                }
            }
        }
        out
    }
    fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .files
            .keys()
            .filter_map(|k| {
                let rest = k.strip_prefix(path).ok()?;
                let first = rest.components().next()?;
                Some(path.join(first))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        let line = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.commands.get(&line).cloned()
    }
}
