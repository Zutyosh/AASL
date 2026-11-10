//! `aasl`: the command-line face of AASL. With `--json` every command
//! prints one envelope `{"schema":1,"ok":...,"data"|"error":...}` on stdout
//! so scripts and agents can parse it.

mod commands;
mod output;

use commands::{PatchCmd, ProtonCmd, parse_id};

use std::path::PathBuf;
use std::process::ExitCode;

use aasl_core::{Event, LogSource};
use aasl_diag::FixId;
use aasl_service::{InstallOptions, LaunchOpts, Service};
use clap::{Parser, Subcommand};

use output::Out;

#[derive(Parser)]
#[command(
    name = "aasl",
    version,
    about = "Install, launch and troubleshoot Somnium Space on Linux"
)]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show what is installed and what is running.
    Status,
    /// Probe the system: distribution, Python, GPU, OpenXR.
    System,
    /// Install everything, then run Somnium's own installer.
    Install {
        /// Somnium Space's Windows installer (.exe) from somniumspace.com.
        #[arg(long)]
        installer: Option<PathBuf>,
        /// Use a local patch bundle instead of the catalog's.
        #[arg(long)]
        bundle: Option<PathBuf>,
        /// Proton build to use (provider:tag). Default: the catalog's pick.
        #[arg(long)]
        proton: Option<String>,
    },
    /// Re-apply umu-run, Proton and the patch bundle. Keeps the prefix.
    Repair,
    /// Remove AASL's components and the menu entry.
    Uninstall {
        /// Also delete the prefix: the Somnium install and its settings.
        #[arg(long)]
        remove_prefix: bool,
    },
    /// Launch Somnium Space.
    Launch {
        /// Start even without an OpenXR runtime (desktop mode).
        #[arg(long)]
        flat: bool,
        /// Return once started instead of waiting for the game to exit.
        #[arg(long)]
        detach: bool,
    },
    /// Stop Somnium Space and every process in its prefix.
    Stop,
    /// Find problems and say how to fix them.
    Doctor {
        /// Skip the network check.
        #[arg(long)]
        offline: bool,
    },
    /// Apply a fix suggested by `doctor`.
    Fix {
        /// repair, stop_session, clear_openxr_override, clear_downloads
        id: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the last lines of the launch log.
    Logs {
        #[arg(short = 'n', long, default_value_t = 200)]
        lines: usize,
    },
    /// Print an anonymised JSON report to attach to a bug report.
    Report,
    /// Show whether the VR stack is ready, and what is missing.
    Vr,
    /// Manage Proton builds (GE-Proton, Proton-CachyOS).
    Proton {
        #[command(subcommand)]
        cmd: ProtonCmd,
    },
    /// Somnium's Wine patches: status, sources, building bundles.
    Patch {
        #[command(subcommand)]
        cmd: PatchCmd,
    },
    /// Serve the MCP protocol on stdio for coding agents. Attaches to a
    /// running AASL window when there is one.
    Mcp {
        /// Never attach to a running AASL; serve a fresh instance.
        #[arg(long)]
        standalone: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run(cli))
}

async fn run(cli: Cli) -> ExitCode {
    let out = Out::new(cli.json);
    if let Cmd::Mcp { standalone } = cli.cmd {
        return mcp(standalone).await;
    }
    let svc = match Service::from_env() {
        Ok(s) => s,
        Err(e) => return out.error(&e),
    };
    if !cli.json {
        spawn_progress_printer(&svc);
    }
    match cli.cmd {
        Cmd::Status => out.ok(&svc.status(), output::status),
        Cmd::System => out.ok(&svc.system(), |s| {
            serde_json::to_string_pretty(s).unwrap_or_default()
        }),
        Cmd::Install {
            installer,
            bundle,
            proton,
        } => {
            if let Some(p) = proton {
                match parse_id(&p) {
                    Ok(id) => {
                        let mut cfg = svc.config();
                        cfg.proton = Some(id.to_string());
                        if let Err(e) = svc.set_config(cfg) {
                            return out.error(&e);
                        }
                    }
                    Err(e) => return out.error(&e),
                }
            }
            let opts = InstallOptions {
                somnium_installer: installer,
                local_bundle: bundle,
                aasl_exe: std::env::current_exe().ok(),
            };
            out.result(svc.install(opts).await, |_| {
                "Somnium Space is installed. Start it with `aasl launch` or from the menu.".into()
            })
        }
        Cmd::Repair => out.result(svc.repair().await, |_| "Repair finished.".into()),
        Cmd::Uninstall { remove_prefix } => out.result(svc.uninstall(remove_prefix).await, |v| {
            format!("Removed {} items.", v.len())
        }),
        Cmd::Launch { flat, detach } => {
            let r = svc.launch(LaunchOpts { allow_flat: flat }).await;
            if let (Ok(_), false) = (&r, detach) {
                svc.wait_session().await;
            }
            out.result(r, |pid| format!("Somnium Space started (pid {pid})."))
        }
        Cmd::Stop => out.result(svc.stop().await, |n| format!("Stopped {n} processes.")),
        Cmd::Doctor { offline } => {
            let f = svc.doctor(!offline).await;
            let failed = f.iter().any(|f| f.severity == aasl_core::Severity::Error);
            let code = out.ok(&f, |f| output::findings(f));
            if failed { ExitCode::from(2) } else { code }
        }
        Cmd::Fix { id, dry_run } => match FixId::parse(&id) {
            Some(fix) => out.result(svc.apply_fix(fix, dry_run).await, |o| o.message.clone()),
            None => out.error(&aasl_core::Error::Invalid(format!("unknown fix '{id}'"))),
        },
        Cmd::Logs { lines } => {
            let text = std::fs::read_to_string(svc.paths().launch_log()).unwrap_or_default();
            let v: Vec<&str> = text.lines().collect();
            let tail: Vec<&str> = v[v.len().saturating_sub(lines)..].to_vec();
            out.ok(&tail, |t| t.join("\n"))
        }
        Cmd::Mcp { .. } => unreachable!("handled above"),
        Cmd::Vr => {
            let v = svc.vr_status();
            let ready = v.ready;
            let code = out.ok(&v, output::vr);
            if ready { code } else { ExitCode::from(2) }
        }
        Cmd::Proton { cmd } => commands::proton(&out, &svc, cmd).await,
        Cmd::Patch { cmd } => commands::patch(&out, &svc, cmd).await,
        Cmd::Report => {
            let r = svc.report().await;
            println!("{r}");
            ExitCode::SUCCESS
        }
    }
}

async fn mcp(standalone: bool) -> ExitCode {
    let paths = match aasl_core::Paths::from_env() {
        Some(p) => p,
        None => return ExitCode::FAILURE,
    };
    if !standalone {
        match aasl_agent::proxy_stdio_to_socket(&paths.socket()).await {
            Ok(true) => return ExitCode::SUCCESS,
            Ok(false) => {}
            Err(e) => {
                eprintln!("aasl mcp: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let svc = match Service::builder(paths).build() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aasl mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    match aasl_agent::serve_stdio(aasl_agent::Server::new(svc)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aasl mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

/// In text mode, show steps, notices and download progress on stderr.
fn spawn_progress_printer(svc: &Service) {
    let mut rx = svc.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::StepStarted { step } => eprintln!("> {}", step.replace('_', " ")),
                Event::StepFinished { skipped: true, .. } => eprintln!("  already done"),
                Event::Notice { severity, message } => eprintln!("  {severity:?}: {message}"),
                Event::Progress {
                    task,
                    done,
                    total: Some(t),
                } if t > 0 => {
                    eprint!("\r  {task}: {}%   ", done * 100 / t);
                    if done == t {
                        eprintln!();
                    }
                }
                Event::Log {
                    source: LogSource::Stderr,
                    line,
                } if std::env::var_os("AASL_VERBOSE").is_some() => eprintln!("  | {line}"),
                _ => {}
            }
        }
    });
}
