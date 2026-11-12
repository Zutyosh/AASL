use std::process::ExitCode;

use serde::Serialize;
use unilux_core::Severity;
use unilux_diag::Finding;
use unilux_service::Status;

pub const SCHEMA: u32 = 1;

pub struct Out {
    json: bool,
}

pub fn envelope_ok<T: Serialize>(data: &T) -> serde_json::Value {
    serde_json::json!({ "schema": SCHEMA, "ok": true, "data": data })
}

pub fn envelope_err(e: &unilux_core::Error) -> serde_json::Value {
    let kind = match e {
        unilux_core::Error::Io { .. } => "io",
        unilux_core::Error::Parse { .. } => "parse",
        unilux_core::Error::Network(_) => "network",
        unilux_core::Error::Checksum { .. } => "checksum",
        unilux_core::Error::Invalid(_) => "invalid",
        unilux_core::Error::Busy(_) => "busy",
        unilux_core::Error::Cancelled => "cancelled",
    };
    serde_json::json!({ "schema": SCHEMA, "ok": false, "error": { "kind": kind, "message": e.to_string() } })
}

impl Out {
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    pub fn ok<T: Serialize>(&self, data: &T, text: impl FnOnce(&T) -> String) -> ExitCode {
        if self.json {
            println!("{}", envelope_ok(data));
        } else {
            println!("{}", text(data));
        }
        ExitCode::SUCCESS
    }

    pub fn error(&self, e: &unilux_core::Error) -> ExitCode {
        if self.json {
            println!("{}", envelope_err(e));
        } else {
            eprintln!("error: {e}");
        }
        ExitCode::FAILURE
    }

    pub fn result<T: Serialize>(
        &self,
        r: unilux_core::Result<T>,
        text: impl FnOnce(&T) -> String,
    ) -> ExitCode {
        match r {
            Ok(v) => self.ok(&v, text),
            Err(e) => self.error(&e),
        }
    }
}

pub fn status(s: &Status) -> String {
    let mut out = vec![
        format!("Unilux {}", s.unilux_version),
        format!("Installed: {}", if s.installed { "yes" } else { "no" }),
        format!("Prefix: {}", s.prefix.display()),
        format!("Recipe: {}", s.recipe),
        format!("Proton: {}", s.proton.as_deref().unwrap_or("none")),
        format!(
            "Patches: {}",
            match &s.patches {
                Some(p) if p.bundle.is_some() => format!(
                    "{}{}",
                    p.bundle.clone().unwrap_or_default(),
                    if p.intact { "" } else { " (files changed)" }
                ),
                _ => "none".into(),
            }
        ),
        format!(
            "VR: {}",
            if s.vr.ready {
                "ready"
            } else {
                "not ready, see unilux vr"
            }
        ),
        format!(
            "OpenXR runtime: {}",
            s.runtime
                .as_ref()
                .map(|r| r.name.clone().unwrap_or_else(|| r.kind.to_string()))
                .unwrap_or_else(|| "none".into())
        ),
    ];
    if let Some(pid) = s.session_pid {
        out.push(format!("Running: pid {pid}"));
    }
    out.join("\n")
}

pub fn vr(v: &unilux_system::VrStatus) -> String {
    let mut out = vec![match &v.active {
        Some(r) => format!(
            "OpenXR runtime: {} ({})",
            r.name.clone().unwrap_or_else(|| r.kind.to_string()),
            r.manifest.display()
        ),
        None => "OpenXR runtime: none active".into(),
    }];
    if !v.installed.is_empty() {
        out.push(format!(
            "Installed: {}",
            v.installed
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(o) = &v.openvr {
        out.push(format!(
            "OpenVR layer: {:?} ({})",
            o.layer,
            o.path.display()
        ));
    }
    out.push(if v.ready {
        "Ready.".into()
    } else {
        "Not ready:".into()
    });
    out.extend(v.blockers.iter().map(|b| format!("  - {b}")));
    out.extend(v.notes.iter().map(|n| format!("  note: {n}")));
    out.join("\n")
}

pub fn findings(v: &[Finding]) -> String {
    if v.is_empty() {
        return "No problems found.".into();
    }
    v.iter()
        .map(|f| {
            let tag = match f.severity {
                Severity::Error => "error",
                Severity::Warn => "warn ",
                Severity::Info => "info ",
            };
            let mut s = format!("[{tag}] {}", f.title);
            for e in &f.evidence {
                s.push_str(&format!("\n        {e}"));
            }
            if let Some(a) = &f.advice {
                s.push_str(&format!("\n        -> {a}"));
            }
            if let Some(fix) = f.fix {
                let id = serde_json::to_value(fix)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                s.push_str(&format!("\n        fix: unilux fix {id}"));
            }
            s
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_are_stable() {
        assert_eq!(
            envelope_ok(&42).to_string(),
            r#"{"data":42,"ok":true,"schema":1}"#
        );
        let e = envelope_err(&unilux_core::Error::Busy("install".into()));
        assert_eq!(e["error"]["kind"], "busy");
        assert_eq!(e["ok"], false);
    }
}
