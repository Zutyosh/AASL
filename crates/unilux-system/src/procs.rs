//! Process inspection through /proc (Linux only, current user's processes).

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub cmdline: String,
    pub wineprefix: Option<PathBuf>,
}

fn read_proc(proc_root: &Path, pid: u32) -> Option<ProcInfo> {
    let dir = proc_root.join(pid.to_string());
    let cmdline = std::fs::read(dir.join("cmdline")).ok()?;
    let cmdline = cmdline
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
    let field = |k: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(k).map(|v| v.trim().to_string()))
    };
    let wineprefix = std::fs::read(dir.join("environ")).ok().and_then(|env| {
        env.split(|b| *b == 0).find_map(|kv| {
            kv.strip_prefix(b"WINEPREFIX=")
                .map(|v| PathBuf::from(String::from_utf8_lossy(v).into_owned()))
        })
    });
    Some(ProcInfo {
        pid,
        ppid: field("PPid:").and_then(|v| v.parse().ok()),
        name: field("Name:").unwrap_or_default(),
        cmdline,
        wineprefix,
    })
}

pub fn list_processes_in(proc_root: &Path) -> Vec<ProcInfo> {
    let Ok(rd) = std::fs::read_dir(proc_root) else {
        return Vec::new();
    };
    let mut out: Vec<ProcInfo> = rd
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| read_proc(proc_root, pid))
        .collect();
    out.sort_by_key(|p| p.pid);
    out
}

pub fn list_processes() -> Vec<ProcInfo> {
    list_processes_in(Path::new("/proc"))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    a == b || canon(a) == canon(b)
}

/// Processes whose WINEPREFIX is `prefix`: Somnium, its launcher, wineserver.
pub fn prefix_processes(prefix: &Path) -> Vec<ProcInfo> {
    list_processes()
        .into_iter()
        .filter(|p| {
            p.wineprefix
                .as_deref()
                .is_some_and(|w| same_path(w, prefix))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fake_proc() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("42");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("cmdline"), b"wine\0Somnium Space Launcher.exe\0").unwrap();
        std::fs::write(p.join("status"), "Name:\twine\nPPid:\t1\n").unwrap();
        std::fs::write(p.join("environ"), b"HOME=/h\0WINEPREFIX=/pfx\0").unwrap();
        std::fs::create_dir_all(d.path().join("self")).unwrap();
        let v = list_processes_in(d.path());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].cmdline, "wine Somnium Space Launcher.exe");
        assert_eq!(v[0].ppid, Some(1));
        assert_eq!(v[0].wineprefix.as_deref(), Some(Path::new("/pfx")));
    }

    #[test]
    fn real_proc_lists_self() {
        let me = std::process::id();
        assert!(list_processes().iter().any(|p| p.pid == me));
    }
}
