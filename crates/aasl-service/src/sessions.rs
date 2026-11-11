//! Session reports (what a play session showed about a recipe), sharing a
//! recipe with its reports, and the community index.

use std::path::{Path, PathBuf};

use aasl_components::sources::{COMMUNITY_RAW, REPO};
use aasl_core::{Error, Result, Severity};
use aasl_patches::community::{self, Hardware, REPORT_FORMAT, RecipeRef, SessionReport};
use serde::{Deserialize, Serialize};

use crate::Service;

/// Written at launch so the report can be made later, even when the
/// launcher that started the game is gone (`aasl launch --detach`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Pending {
    pub started_at: u64,
    pub recipe: RecipeRef,
    pub mangohud: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedReport {
    pub id: String,
    pub report: SessionReport,
}

/// A recipe and its reports, ready for the submission form.
#[derive(Debug, Clone, Serialize)]
pub struct Share {
    /// Opens the GitHub form, filled in when everything fits in a URL.
    pub url: String,
    /// False: paste `recipe` and `reports` into the form yourself.
    pub prefilled: bool,
    pub recipe: String,
    pub reports: String,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn mtime(p: &Path) -> Option<u64> {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Percent-encoding for a query value.
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// GitHub refuses longer issue URLs.
const MAX_URL: usize = 7500;

impl Service {
    fn sessions_dir(&self) -> PathBuf {
        self.inner.paths.state_dir.join("sessions")
    }

    fn mangohud_dir(&self) -> PathBuf {
        self.inner.paths.state_dir.join("mangohud")
    }

    fn pending_file(&self) -> PathBuf {
        self.inner.paths.state_dir.join("session-pending.json")
    }

    pub(crate) fn game_data_dir(&self) -> PathBuf {
        self.prefix()
            .join("drive_c/users/steamuser/AppData/LocalLow/Somnium Space Ltd/Somnium Space VR")
    }

    /// Called at launch: what to add to the environment for measuring,
    /// and the pending record for `record_session`.
    pub(crate) fn start_session(&self, env: &mut std::collections::BTreeMap<String, String>) {
        let cfg = self.config();
        if !cfg.session_reports {
            return;
        }
        let Ok(plan) = self.active_plan() else { return };
        let Some(proton) = self.current_proton() else {
            return;
        };
        let user_mangohud = env.keys().any(|k| k.starts_with("MANGOHUD"));
        let mangohud = !user_mangohud
            && aasl_system::find_in_path(self.inner.host.as_ref(), "mangohud").is_some();
        if mangohud {
            let dir = self.mangohud_dir();
            let _ = std::fs::remove_dir_all(&dir);
            if std::fs::create_dir_all(&dir).is_ok() {
                env.insert("MANGOHUD".into(), "1".into());
                env.insert(
                    "MANGOHUD_CONFIG".into(),
                    // MangoHud only logs while its HUD exists (no_display stops
                    // logging too), so draw it fully transparent instead.
                    // It times the desktop mirror's presents, which Unity
                    // paces with the headset frames.
                    format!(
                        "alpha=0,background_alpha=0,hud_compact,autostart_log=1,log_interval=0,output_folder={}",
                        dir.display()
                    ),
                );
            }
        }
        let pending = Pending {
            started_at: now(),
            recipe: RecipeRef {
                id: plan.recipe.id.clone(),
                key: plan.bundle_key(&proton),
                proton: proton.to_string(),
            },
            mangohud,
        };
        let _ = std::fs::create_dir_all(&self.inner.paths.state_dir);
        if let Ok(text) = serde_json::to_string(&pending) {
            let _ = std::fs::write(self.pending_file(), text);
        }
    }

    fn hardware(&self, gpu_from_log: Option<String>) -> Hardware {
        let host = self.inner.host.as_ref();
        let sys = self.system();
        let read = |p: &str| host.read(Path::new(p)).unwrap_or_default();
        let cpu = read("/proc/cpuinfo")
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .and_then(|r| r.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
            })
            .unwrap_or_default();
        let ram_gb = read("/proc/meminfo")
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kb| ((kb as f64) / 1024.0 / 1024.0).round() as u32)
            .unwrap_or_default();
        let gpu = gpu_from_log
            .map(|g| g.split(" (ID=").next().unwrap_or(&g).to_string())
            .or_else(|| sys.gpus.first().map(|g| g.vendor.clone()))
            .unwrap_or_default();
        Hardware {
            gpu,
            gpu_driver: sys
                .gpus
                .first()
                .and_then(|g| g.driver.clone())
                .unwrap_or_default(),
            cpu,
            ram_gb,
            distro: sys.distro.map(|d| d.name).unwrap_or_default(),
            kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
            vr_runtime: sys
                .active_runtime
                .map(|r| r.name.unwrap_or_else(|| r.kind.to_string()))
                .unwrap_or_else(|| "none".into()),
        }
    }

    /// Turn the last session into a report and save it. Runs by itself
    /// when the game exits under AASL; `aasl session record` does it after
    /// a detached launch.
    pub fn record_session(&self, exit_code: Option<i32>) -> Result<SavedReport> {
        let text = std::fs::read_to_string(self.pending_file()).map_err(|_| {
            Error::Invalid("no session to record: launch Somnium through AASL first".into())
        })?;
        let pending: Pending =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        let data = self.game_data_dir();
        let mut probe = community::LogProbe::default();
        let mut end = pending.started_at;
        let mut found = false;
        // Somnium moves the previous log aside on start: a session that
        // restarted the game spans both files.
        for name in ["Player-prev.log", "Player.log"] {
            let p = data.join(name);
            let Some(t) = mtime(&p).filter(|t| *t >= pending.started_at) else {
                continue;
            };
            end = end.max(t);
            found = true;
            if let Ok(f) = std::fs::File::open(&p) {
                let part = community::scan_player_log(std::io::BufReader::new(f));
                probe.worlds += part.worlds;
                probe.joins += part.joins;
                probe.video.opened += part.video.opened;
                probe.video.failed += part.video.failed;
                probe.exceptions += part.exceptions;
                probe.crash = probe.crash.or(part.crash);
                probe.gpu = probe.gpu.or(part.gpu);
                probe.vr |= part.vr;
            }
        }
        if !found {
            return Err(Error::Invalid(
                "Somnium did not write a log in this session (did the game start?)".into(),
            ));
        }
        // Wine's own crash dialog, outside Unity's handler.
        let wine_log = std::fs::read_to_string(self.inner.paths.logs_dir().join("somnium.log"))
            .unwrap_or_default();
        if probe.crash.is_none() && wine_log.contains("Unhandled exception") {
            probe.crash = Some(community::Crash::InGame);
        }
        let perf = pending
            .mangohud
            .then(|| {
                std::fs::read_dir(self.mangohud_dir())
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("Somnium Space VR"))
                    })
                    .filter_map(|p| std::fs::read_to_string(p).ok())
                    .filter_map(|t| community::parse_mangohud(&t))
                    .max_by_key(|p| p.frames)
            })
            .flatten();
        let report = SessionReport {
            format: REPORT_FORMAT,
            aasl_version: env!("CARGO_PKG_VERSION").into(),
            hardware: self.hardware(probe.gpu.clone()),
            recipe: pending.recipe,
            started_at: pending.started_at,
            duration_s: end.saturating_sub(pending.started_at),
            flat: !probe.vr,
            exit_code,
            crash: probe.crash,
            worlds: probe.worlds,
            joins: probe.joins,
            video: probe.video,
            perf,
            exceptions: probe.exceptions,
        };
        let dir = self.sessions_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::io("creating the sessions directory", e))?;
        let id = format!("{}-{}", report.started_at, report.recipe.id);
        let json =
            serde_json::to_string_pretty(&report).map_err(|e| Error::Invalid(e.to_string()))?;
        std::fs::write(dir.join(format!("{id}.json")), json)
            .map_err(|e| Error::io("saving the report", e))?;
        let _ = std::fs::remove_file(self.pending_file());
        let _ = std::fs::remove_dir_all(self.mangohud_dir());
        self.notice(
            Severity::Info,
            format!(
                "Session saved: {} min, {} world(s), videos {}/{}{}",
                report.duration_s / 60,
                report.worlds,
                report.video.opened - report.video.failed.min(report.video.opened),
                report.video.opened,
                report
                    .perf
                    .map(|p| format!(", {:.0} fps (1 % low {:.0})", p.avg_fps, p.low1_fps))
                    .unwrap_or_default()
            ),
        );
        Ok(SavedReport { id, report })
    }

    /// Saved reports, newest first.
    pub fn sessions(&self) -> Vec<SavedReport> {
        let mut out: Vec<SavedReport> = std::fs::read_dir(self.sessions_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| {
                let report = SessionReport::parse(&std::fs::read_to_string(&p).ok()?).ok()?;
                Some(SavedReport {
                    id: p.file_stem()?.to_string_lossy().into_owned(),
                    report,
                })
            })
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.report.started_at));
        out
    }

    /// How a recipe stands on this machine's own reports, with the rules
    /// CI applies to a submission.
    pub fn recipe_verdict(&self, id: &str) -> Result<community::Verdict> {
        let recipe = self.recipe(id)?;
        let plan = self.library()?.plan(&recipe)?;
        let reports: Vec<SessionReport> = self
            .sessions()
            .into_iter()
            .map(|s| s.report)
            .filter(|r| r.recipe.id == id)
            .collect();
        Ok(community::verify(&recipe, &reports, |p| {
            Some(plan.bundle_key(p))
        }))
    }

    /// The recipe (library ingredients by reference, its own inline) and
    /// its reports, for the GitHub submission form. `reports` empty means
    /// every saved report of this recipe.
    pub fn recipe_share(&self, id: &str, reports: &[String]) -> Result<Share> {
        let lib = self.library()?;
        let recipe = lib.import(&lib.export(&self.recipe(id)?)?)?;
        let chosen: Vec<SessionReport> = self
            .sessions()
            .into_iter()
            .filter(|s| s.report.recipe.id == id && (reports.is_empty() || reports.contains(&s.id)))
            .map(|s| s.report)
            .collect();
        if chosen.is_empty() {
            return Err(Error::Invalid(format!(
                "no session report for {id} yet: play with it through AASL first"
            )));
        }
        let recipe_text = recipe.to_toml()?;
        let reports_text =
            serde_json::to_string(&chosen).map_err(|e| Error::Invalid(e.to_string()))?;
        let base = format!(
            "https://github.com/{REPO}/issues/new?template=recipe.yml&labels=recipe&title={}",
            enc(&format!("Recipe: {}", recipe.id))
        );
        let full = format!(
            "{base}&recipe={}&reports={}",
            enc(&recipe_text),
            enc(&reports_text)
        );
        let prefilled = full.len() <= MAX_URL;
        Ok(Share {
            url: if prefilled { full } else { base },
            prefilled,
            recipe: recipe_text,
            reports: reports_text,
        })
    }

    /// The community index from the repository.
    pub async fn community(&self) -> Result<community::Index> {
        let text = aasl_components::Downloader::new()
            .text(&format!("{COMMUNITY_RAW}/index.json"))
            .await?;
        serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("community index: {e}")))
    }

    /// Fetch a community recipe and save it as one of yours.
    pub async fn community_install(&self, id: &str) -> Result<crate::RecipeInfo> {
        if !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(Error::Invalid(format!("bad recipe id {id:?}")));
        }
        let text = aasl_components::Downloader::new()
            .text(&format!("{COMMUNITY_RAW}/recipes/{id}/recipe.toml"))
            .await?;
        self.recipe_import(&text)
    }
}
