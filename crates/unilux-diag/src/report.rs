//! Bug-report bundle: one JSON document with paths anonymised.

use serde::Serialize;

use crate::checks::{DiagContext, Finding};

#[derive(Debug, Serialize)]
pub struct Report<'a> {
    pub unilux_version: &'static str,
    pub system: &'a unilux_system::SystemReport,
    pub findings: &'a [Finding],
    pub config: &'a unilux_core::Config,
    pub state: &'a unilux_install::InstallState,
    pub launch_log_tail: &'a [String],
}

/// Serialise and replace the home directory and user name, so a report
/// can be pasted into a public issue.
pub fn render(ctx: &DiagContext, findings: &[Finding]) -> String {
    let r = Report {
        unilux_version: env!("CARGO_PKG_VERSION"),
        system: &ctx.system,
        findings,
        config: &ctx.config,
        state: &ctx.state,
        launch_log_tail: &ctx.launch_log,
    };
    let text = serde_json::to_string_pretty(&r).unwrap_or_default();
    anonymise(&text, &ctx.paths.home.display().to_string())
}

pub fn anonymise(text: &str, home: &str) -> String {
    let mut out = if home.len() > 1 {
        text.replace(home, "~")
    } else {
        text.to_string()
    };
    if let Some(user) = std::path::Path::new(home)
        .file_name()
        .and_then(|n| n.to_str())
        && user.len() > 2
    {
        out = out.replace(user, "<user>");
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn anonymise_home_and_user() {
        let t = super::anonymise("/home/alice/x and alice@host", "/home/alice");
        assert_eq!(t, "~/x and <user>@host");
    }
}
