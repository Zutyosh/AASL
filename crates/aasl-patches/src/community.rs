//! Community recipes: session reports as evidence, and the rules that turn
//! them into a status. The same code runs in AASL (to show a recipe's
//! standing) and in the repository's CI (to judge a submission), so a
//! player can predict what CI will say.

use std::collections::BTreeSet;
use std::path::Path;

use aasl_components::ProtonId;
use aasl_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::recipe::{Library, Recipe};

/// The report format this AASL writes and reads.
pub const REPORT_FORMAT: u32 = 1;

/// What the recipe was, exactly: its key ties the report to the recipe
/// text, the Proton and the builder (see `Plan::bundle_key`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeRef {
    pub id: String,
    pub key: String,
    pub proton: String,
}

/// The machine, without anything that names the player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardware {
    pub gpu: String,
    pub gpu_driver: String,
    pub cpu: String,
    pub ram_gb: u32,
    pub distro: String,
    pub kernel: String,
    pub vr_runtime: String,
}

impl Hardware {
    /// What makes two reports independent configurations.
    pub fn config(&self) -> String {
        format!("{} | {} | {}", self.gpu, self.distro, self.vr_runtime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crash {
    /// The game died while playing.
    InGame,
    /// Unity's crash handler fired while shutting down (Somnium does this
    /// on Wine too often to count as instability).
    OnExit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Video {
    /// Players that started opening a stream or file.
    pub opened: u32,
    /// Opens that failed (AVPro errors, yt-dlp extraction errors).
    pub failed: u32,
}

/// Frame pacing from MangoHud's log of the game's own presents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Perf {
    pub frames: u64,
    pub avg_fps: f32,
    /// Average fps over the slowest 1 % of frames.
    pub low1_fps: f32,
    pub p99_frametime_ms: f32,
    /// Frames slower than twice the median.
    pub stutters: u64,
    /// Sustained slowdowns and what the GPU was doing during them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dips: Option<Dips>,
    /// Single frames frozen for at least [HITCH_MS].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hitches: Option<Hitches>,
}

pub const HITCH_MS: f32 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Hitches {
    pub count: u32,
    pub longest_ms: f32,
    /// Set when most hitches come at a fixed interval: a timer, not load.
    pub period_s: Option<f32>,
}

/// Runs of at least [DIP_MIN_FRAMES] frames below 2/3 of the median frame
/// rate, compared with the rest of the session. Pairs are [normal, dip].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dips {
    pub episodes: u32,
    pub seconds: f32,
    pub fps: f32,
    pub gpu_load: Option<[f32; 2]>,
    pub gpu_clock_mhz: Option<[f32; 2]>,
    pub cpu_load: Option<[f32; 2]>,
    pub cause: DipCause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DipCause {
    /// The GPU is saturated: too much to draw (resolution, avatars, world).
    GpuBound,
    /// The GPU clock falls during the dips: power management.
    GpuClock,
    /// The GPU idles during the dips: it waits on the game's main thread,
    /// the VR compositor or loading, not on its own work.
    GpuWaiting,
    Unknown,
}

pub const DIP_MIN_FRAMES: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionReport {
    pub format: u32,
    pub aasl_version: String,
    pub recipe: RecipeRef,
    pub hardware: Hardware,
    pub started_at: u64,
    pub duration_s: u64,
    pub flat: bool,
    pub exit_code: Option<i32>,
    pub crash: Option<Crash>,
    /// Worlds entered (the hub excluded).
    pub worlds: u32,
    /// Multiplayer sessions joined, hub included.
    pub joins: u32,
    pub video: Video,
    /// None when MangoHud was not available.
    pub perf: Option<Perf>,
    /// Managed exceptions the game logged (informational: Somnium logs
    /// many on every platform).
    pub exceptions: u32,
}

impl SessionReport {
    pub fn parse(text: &str) -> Result<Self> {
        let r: Self = serde_json::from_str(text)
            .map_err(|e| Error::Invalid(format!("session report: {e}")))?;
        if r.format != REPORT_FORMAT {
            return Err(Error::Invalid(format!(
                "session report format {} (expected {REPORT_FORMAT})",
                r.format
            )));
        }
        Ok(r)
    }

    /// Long enough to say something about stability.
    pub fn counts(&self) -> bool {
        self.duration_s >= MIN_SESSION_S
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Checks, but no valid report yet.
    Untested,
    /// At least one valid VR session of 5+ minutes.
    Proposed,
    /// Enough independent, stable evidence (see `RULES`).
    Community,
}

/// The thresholds, in one place so the README and CI quote the same ones.
pub const MIN_SESSION_S: u64 = 5 * 60;
pub const MIN_REPORTS: usize = 3;
pub const MIN_CONFIGS: usize = 2;
pub const MIN_HOURS: f32 = 1.0;
pub const MAX_CRASHES_PER_HOUR: f32 = 0.25;
pub const MIN_VIDEO_SUCCESS: f32 = 0.9;
pub const MIN_VIDEO_OPENS: u32 = 5;
pub const RULES: &str = "community: at least 3 sessions of 5+ minutes from at least 2 configurations (GPU, distro, VR runtime), 1+ hour in total, at most 1 in-game crash per 4 hours, and 90 %+ of videos playing once 5+ were opened.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub status: Status,
    pub reports: usize,
    pub configs: usize,
    pub hours: f32,
    pub worlds: u32,
    pub crashes_in_game: u32,
    pub crashes_on_exit: u32,
    pub video_success: Option<f32>,
    /// Mean of low1 / avg across reports with perf: 1.0 is perfectly even.
    pub pacing: Option<f32>,
    pub avg_fps: Option<f32>,
    /// Reports that were set aside, and why; plus what is missing.
    pub problems: Vec<String>,
}

/// Judge `reports` for `recipe`. `key_for` computes the recipe's key on
/// a Proton, so a report made with another version of the recipe (or a
/// forged one) is set aside.
pub fn verify(
    recipe: &Recipe,
    reports: &[SessionReport],
    key_for: impl Fn(&ProtonId) -> Option<String>,
) -> Verdict {
    let mut problems = Vec::new();
    let mut valid = Vec::new();
    for (n, r) in reports.iter().enumerate() {
        let n = n + 1;
        if r.recipe.id != recipe.id {
            problems.push(format!("report {n} is for recipe {}", r.recipe.id));
            continue;
        }
        let Ok(proton) = ProtonId::parse(&r.recipe.proton) else {
            problems.push(format!("report {n}: bad Proton {:?}", r.recipe.proton));
            continue;
        };
        if key_for(&proton).as_deref() != Some(r.recipe.key.as_str()) {
            problems.push(format!(
                "report {n} was made with a different version of the recipe (key {})",
                r.recipe.key
            ));
            continue;
        }
        if r.flat {
            problems.push(format!(
                "report {n} is a desktop-mode session; only VR sessions count"
            ));
            continue;
        }
        valid.push(r);
    }
    let counted: Vec<&&SessionReport> = valid.iter().filter(|r| r.counts()).collect();
    let configs: BTreeSet<String> = counted.iter().map(|r| r.hardware.config()).collect();
    let hours = counted.iter().fold(0.0, |h, r| h + r.duration_s as f32) / 3600.0;
    let crashes_in_game = valid
        .iter()
        .filter(|r| r.crash == Some(Crash::InGame))
        .count() as u32;
    let crashes_on_exit = valid
        .iter()
        .filter(|r| r.crash == Some(Crash::OnExit))
        .count() as u32;
    let opened: u32 = valid.iter().map(|r| r.video.opened).sum();
    let failed: u32 = valid
        .iter()
        .map(|r| r.video.failed.min(r.video.opened))
        .sum();
    let video_success = (opened > 0).then(|| (opened - failed) as f32 / opened as f32);
    let perf: Vec<Perf> = valid
        .iter()
        .filter_map(|r| r.perf)
        .filter(|p| p.avg_fps > 0.0)
        .collect();
    let mean = |f: &dyn Fn(&Perf) -> f32| {
        (!perf.is_empty()).then(|| perf.iter().map(f).sum::<f32>() / perf.len() as f32)
    };
    let pacing = mean(&|p| p.low1_fps / p.avg_fps);
    let avg_fps = mean(&|p| p.avg_fps);

    let mut missing = Vec::new();
    if counted.len() < MIN_REPORTS {
        missing.push(format!(
            "{}/{MIN_REPORTS} sessions of 5+ minutes",
            counted.len()
        ));
    }
    if configs.len() < MIN_CONFIGS {
        missing.push(format!("{}/{MIN_CONFIGS} configurations", configs.len()));
    }
    if hours < MIN_HOURS {
        missing.push(format!("{hours:.1}/{MIN_HOURS:.0} h played"));
    }
    if hours > 0.0 && crashes_in_game as f32 / hours.max(MIN_HOURS) > MAX_CRASHES_PER_HOUR {
        missing.push(format!(
            "{crashes_in_game} in-game crash(es) in {hours:.1} h"
        ));
    }
    if opened >= MIN_VIDEO_OPENS && video_success.is_some_and(|s| s < MIN_VIDEO_SUCCESS) {
        missing.push(format!(
            "videos played {:.0} % of the time",
            video_success.unwrap_or(0.0) * 100.0
        ));
    }
    if counted.len() < valid.len() {
        problems.push(format!(
            "{} session(s) under 5 minutes count for crashes and videos only",
            valid.len() - counted.len()
        ));
    }
    let status = match (counted.is_empty(), missing.is_empty()) {
        (true, _) => Status::Untested,
        (false, true) => Status::Community,
        (false, false) => Status::Proposed,
    };
    if status != Status::Community && !counted.is_empty() {
        problems.push(format!("not yet community: {}", missing.join(", ")));
    }
    Verdict {
        status,
        reports: counted.len(),
        configs: configs.len(),
        hours,
        worlds: valid.iter().map(|r| r.worlds).sum(),
        crashes_in_game,
        crashes_on_exit,
        video_success,
        pacing,
        avg_fps,
        problems,
    }
}

/// What Somnium's Player.log says about a session.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LogProbe {
    pub worlds: u32,
    pub joins: u32,
    pub video: Video,
    pub crash: Option<Crash>,
    pub exceptions: u32,
    /// Unity's "Renderer:" line, e.g. "AMD Radeon RX 7900 XT (RADV NAVI31)".
    pub gpu: Option<String>,
    pub vr: bool,
}

/// Read a Player.log line by line (they reach millions of lines).
pub fn scan_player_log(r: impl std::io::BufRead) -> LogProbe {
    let mut p = LogProbe::default();
    let mut shutting_down = false;
    for line in r.lines().map_while(std::result::Result::ok) {
        let l = line.trim_start();
        if l.contains("Transition scene unloaded: IN_WORLD") {
            p.worlds += 1;
        } else if l.contains("We have joined the game session") {
            p.joins += 1;
        } else if l.starts_with("[AVProVideo] Opening") {
            p.video.opened += 1;
        } else if l.starts_with("[AVProVideo] Error")
            || l.starts_with("[VideoExtractor] Failed to extract")
        {
            p.video.failed += 1;
        } else if l.starts_with("EXCEPTION |") {
            p.exceptions += 1;
        } else if l.starts_with("Input System module state changed to: Shutdown") {
            shutting_down = true;
        } else if l.starts_with("Crash!!!") {
            p.crash = Some(if shutting_down {
                Crash::OnExit
            } else {
                Crash::InGame
            });
        } else if let Some(g) = l.strip_prefix("Renderer:") {
            p.gpu.get_or_insert_with(|| g.trim().to_string());
        } else if l.starts_with("[XR]") && l.contains("Available Layers") {
            p.vr = true;
        }
    }
    p
}

/// Frame pacing from a MangoHud CSV log (`MANGOHUD_CONFIG=autostart_log=…`).
pub fn parse_mangohud(text: &str) -> Option<Perf> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .by_ref()
        .find(|l| l.starts_with("fps,"))?
        .split(',')
        .map(str::trim)
        .collect();
    let col = |name: &str| header.iter().position(|c| *c == name);
    let ft_col = col("frametime")?;
    let extra = [col("gpu_load"), col("gpu_core_clock"), col("cpu_load")];
    let rows: Vec<(f32, [f32; 3])> = lines
        .filter_map(|l| {
            let v: Vec<&str> = l.split(',').collect();
            let num = |c: Option<usize>| {
                c.and_then(|c| v.get(c)?.trim().parse::<f32>().ok())
                    .filter(|x| x.is_finite())
                    .unwrap_or(0.0)
            };
            let ft = v.get(ft_col)?.trim().parse::<f32>().ok()?;
            (ft > 0.0 && ft.is_finite()).then(|| (ft, extra.map(num)))
        })
        .collect();
    if rows.len() < 100 {
        return None;
    }
    let mut ft: Vec<f32> = rows.iter().map(|r| r.0).collect();
    let frames = ft.len() as u64;
    let total: f32 = ft.iter().sum();
    ft.sort_by(|a, b| a.total_cmp(b));
    let median = ft[ft.len() / 2];
    let worst = &ft[ft.len() - (ft.len() / 100).max(1)..];
    let worst_avg = worst.iter().sum::<f32>() / worst.len() as f32;
    Some(Perf {
        frames,
        avg_fps: 1000.0 * frames as f32 / total,
        low1_fps: 1000.0 / worst_avg,
        p99_frametime_ms: ft[(ft.len() * 99 / 100).min(ft.len() - 1)],
        stutters: ft.iter().filter(|f| **f > 2.0 * median).count() as u64,
        dips: dips(&rows, median),
        hitches: hitches(&rows),
    })
}

fn hitches(rows: &[(f32, [f32; 3])]) -> Option<Hitches> {
    let mut at = Vec::new();
    let mut clock = 0.0f32;
    let mut longest = 0.0f32;
    for r in rows {
        clock += r.0;
        if r.0 >= HITCH_MS {
            at.push(clock / 1000.0);
            longest = longest.max(r.0);
        }
    }
    if at.is_empty() {
        return None;
    }
    // A timer shows as hitches followed by another exactly one period
    // later, whatever loading hitches fall in between.
    let follows = |p: f32| -> Vec<f32> {
        at.iter()
            .filter_map(|a| at.iter().map(|b| b - a).find(|d| (d - p).abs() < 0.03 * p))
            .collect()
    };
    let period_s = at
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g >= 2.0)
        .map(|g| (g, follows(g)))
        .max_by_key(|(_, m)| m.len())
        .filter(|(_, m)| m.len() >= 4 && m.len() * 5 >= at.len())
        .map(|(_, m)| m.iter().sum::<f32>() / m.len() as f32);
    Some(Hitches {
        count: at.len() as u32,
        longest_ms: longest,
        period_s,
    })
}

fn dips(rows: &[(f32, [f32; 3])], median: f32) -> Option<Dips> {
    let slow: Vec<bool> = rows.iter().map(|r| r.0 > 1.5 * median).collect();
    let mut in_dip = vec![false; rows.len()];
    let mut episodes = 0;
    let mut i = 0;
    while i < rows.len() {
        let start = i;
        while i < rows.len() && slow[i] {
            i += 1;
        }
        if i - start >= DIP_MIN_FRAMES {
            episodes += 1;
            in_dip[start..i].iter_mut().for_each(|d| *d = true);
        }
        i = i.max(start + 1);
    }
    if episodes == 0 {
        return None;
    }
    let pair = |k: usize| {
        let avg = |dip: bool| {
            let v: Vec<f32> = rows
                .iter()
                .zip(&in_dip)
                .filter(|(_, d)| **d == dip)
                .map(|(r, _)| r.1[k])
                .collect();
            v.iter().sum::<f32>() / v.len().max(1) as f32
        };
        let p = [avg(false), avg(true)];
        (p[0] > 0.0 || p[1] > 0.0).then_some(p)
    };
    let seconds_ms: f32 = rows
        .iter()
        .zip(&in_dip)
        .filter(|(_, d)| **d)
        .map(|(r, _)| r.0)
        .sum();
    let count = in_dip.iter().filter(|d| **d).count() as f32;
    let (gpu_load, gpu_clock_mhz, cpu_load) = (pair(0), pair(1), pair(2));
    let cause = match (gpu_load, gpu_clock_mhz) {
        // A clock that falls while the GPU idles is a consequence, not a cause.
        (Some([_, l]), Some([n, d])) if l >= 70.0 && d < 0.8 * n => DipCause::GpuClock,
        (None, Some([n, d])) if d < 0.8 * n => DipCause::GpuClock,
        (Some([_, d]), _) if d >= 85.0 => DipCause::GpuBound,
        (Some([n, d]), _) if d < 70.0 || d + 15.0 < n => DipCause::GpuWaiting,
        _ => DipCause::Unknown,
    };
    Some(Dips {
        episodes,
        seconds: seconds_ms / 1000.0,
        fps: 1000.0 * count / seconds_ms,
        gpu_load,
        gpu_clock_mhz,
        cpu_load,
        cause,
    })
}

/// One community recipe as the index lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub proton: Option<String>,
    pub ingredients: Vec<String>,
    pub custom_code: bool,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub format: u32,
    pub rules: String,
    pub recipes: Vec<Entry>,
}

/// Judge one community recipe directory: `recipe.toml` and
/// `reports/*.json`.
pub fn judge_dir(lib: &Library, dir: &Path) -> Result<(Recipe, Entry)> {
    let text = std::fs::read_to_string(dir.join("recipe.toml"))
        .map_err(|e| Error::io(format!("reading {}/recipe.toml", dir.display()), e))?;
    let recipe = lib.import(&text)?;
    let plan = lib.plan(&recipe)?;
    let mut reports = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir.join("reports"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut bad = Vec::new();
    for f in files {
        match std::fs::read_to_string(&f)
            .map_err(|e| Error::io("reading a report", e))
            .and_then(|t| SessionReport::parse(&t))
        {
            Ok(r) => reports.push(r),
            Err(e) => bad.push(format!(
                "{}: {e}",
                f.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    let mut verdict = verify(&recipe, &reports, |p| Some(plan.bundle_key(p)));
    verdict.problems.extend(bad);
    let entry = Entry {
        id: recipe.id.clone(),
        title: recipe.title.clone(),
        description: recipe.description.clone(),
        proton: recipe.proton.clone(),
        ingredients: recipe.ingredients.clone(),
        custom_code: plan.has_custom_code(),
        verdict,
    };
    Ok((recipe, entry))
}

/// Build the index of `root/recipes/*`. Directories that do not check are
/// reported in the error list instead of failing the whole index.
pub fn index(lib: &Library, root: &Path) -> (Index, Vec<String>) {
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("recipes"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut out = Index {
        format: 1,
        rules: RULES.into(),
        recipes: Vec::new(),
    };
    let mut errors = Vec::new();
    for d in dirs {
        match judge_dir(lib, &d) {
            Ok((_, e)) => out.recipes.push(e),
            Err(e) => errors.push(format!("{}: {e}", d.display())),
        }
    }
    out.recipes.sort_by(|a, b| {
        b.verdict
            .status
            .cmp(&a.verdict.status)
            .then(a.id.cmp(&b.id))
    });
    (out, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> Recipe {
        Recipe::parse(
            "format = 1\nid = \"r\"\ntitle = \"R\"\ningredients = [\"ytdlp-android-client\"]\n",
        )
        .unwrap()
    }

    fn report(gpu: &str, minutes: u64) -> SessionReport {
        SessionReport {
            format: REPORT_FORMAT,
            aasl_version: "0".into(),
            recipe: RecipeRef {
                id: "r".into(),
                key: "k".into(),
                proton: "ge:GE-Proton11-12".into(),
            },
            hardware: Hardware {
                gpu: gpu.into(),
                distro: "Arch".into(),
                vr_runtime: "WiVRn".into(),
                ..Default::default()
            },
            started_at: 0,
            duration_s: minutes * 60,
            flat: false,
            exit_code: Some(0),
            crash: None,
            worlds: 3,
            joins: 4,
            video: Video {
                opened: 4,
                failed: 0,
            },
            perf: Some(Perf {
                frames: 1000,
                avg_fps: 90.0,
                low1_fps: 72.0,
                p99_frametime_ms: 14.0,
                stutters: 3,
                dips: None,
                hitches: None,
            }),
            exceptions: 0,
        }
    }

    fn key(_: &ProtonId) -> Option<String> {
        Some("k".into())
    }

    #[test]
    fn statuses() {
        let r = recipe();
        assert_eq!(verify(&r, &[], key).status, Status::Untested);
        let one = verify(&r, &[report("A", 30)], key);
        assert_eq!(one.status, Status::Proposed);
        assert!(one.problems[0].contains("1/3 sessions"));
        let three = [report("A", 30), report("B", 20), report("A", 15)];
        let v = verify(&r, &three, key);
        assert_eq!(v.status, Status::Community, "{:?}", v.problems);
        assert_eq!(v.configs, 2);
        assert!((v.pacing.unwrap() - 0.8).abs() < 1e-3);
    }

    #[test]
    fn evidence_is_checked() {
        let r = recipe();
        let mut other = report("A", 30);
        other.recipe.key = "old".into();
        let mut flat = report("B", 30);
        flat.flat = true;
        let v = verify(&r, &[other, flat], key);
        assert_eq!(v.status, Status::Untested);
        assert!(v.problems.iter().any(|p| p.contains("different version")));
        assert!(v.problems.iter().any(|p| p.contains("desktop-mode")));

        let mut crashy = vec![report("A", 30), report("B", 20), report("A", 15)];
        crashy[0].crash = Some(Crash::InGame);
        assert_eq!(verify(&r, &crashy, key).status, Status::Proposed);
        crashy[0].crash = Some(Crash::OnExit);
        assert_eq!(
            verify(&r, &crashy, key).status,
            Status::Community,
            "exit crashes do not count"
        );
        crashy[1].video = Video {
            opened: 10,
            failed: 5,
        };
        assert_eq!(verify(&r, &crashy, key).status, Status::Proposed);
    }

    #[test]
    fn player_log_probe() {
        let log = "Renderer:  AMD Radeon RX 7900 XT (RADV NAVI31)\n[XR] [760] Available Layers: (0)\n\tWe have joined the game session. Spawning\n\t[World Transition]: Transition scene unloaded: IN_WORLD, False\n[AVProVideo] Opening https://x (offset 0)\n[AVProVideo] Opening rtsp://y\n\t[VideoExtractor] Failed to extract: \nEXCEPTION | 10/11\nInput System module state changed to: Shutdown.\nCrash!!!\n";
        let p = scan_player_log(log.as_bytes());
        assert_eq!((p.worlds, p.joins, p.exceptions), (1, 1, 1));
        assert_eq!(
            p.video,
            Video {
                opened: 2,
                failed: 1
            }
        );
        assert_eq!(p.crash, Some(Crash::OnExit));
        assert_eq!(
            p.gpu.as_deref(),
            Some("AMD Radeon RX 7900 XT (RADV NAVI31)")
        );
        assert!(p.vr);
        assert_eq!(
            scan_player_log("Crash!!!\n".as_bytes()).crash,
            Some(Crash::InGame)
        );
    }

    #[test]
    fn mangohud_pacing() {
        let mut csv = String::from("os,cpu,gpu\nArch,x,y\nfps,frametime,cpu_load,gpu_load\n");
        for i in 0..1000 {
            let ft = if i % 100 == 0 { 40.0 } else { 10.0 };
            csv.push_str(&format!("100,{ft},10,50\n"));
        }
        let p = parse_mangohud(&csv).unwrap();
        assert_eq!(p.frames, 1000);
        assert!((p.avg_fps - 1000.0 / 10.3).abs() < 0.1);
        assert!((p.low1_fps - 25.0).abs() < 0.01);
        assert_eq!(p.stutters, 10);
        assert!(parse_mangohud("fps,frametime\n1,2\n").is_none());
    }

    #[test]
    fn dips_name_their_cause() {
        let csv = |dip: &str| {
            let mut s = String::from("fps,frametime,cpu_load,gpu_load,gpu_core_clock\n");
            for i in 0..2000 {
                if (500..560).contains(&i) || (1500..1530).contains(&i) {
                    s.push_str(dip);
                } else {
                    s.push_str("90,11.1,30,60,2400\n");
                }
            }
            s
        };
        let d = parse_mangohud(&csv("30,33.3,30,25,2400\n"))
            .unwrap()
            .dips
            .unwrap();
        assert_eq!((d.episodes, d.cause), (2, DipCause::GpuWaiting));
        assert!(
            (d.fps - 30.0).abs() < 0.1 && (d.seconds - 3.0).abs() < 0.01,
            "{d:?}"
        );
        assert_eq!(d.gpu_load, Some([60.0, 25.0]));
        let d = parse_mangohud(&csv("30,33.3,30,99,2400\n"))
            .unwrap()
            .dips
            .unwrap();
        assert_eq!(d.cause, DipCause::GpuBound);
        let d = parse_mangohud(&csv("30,33.3,30,99,900\n"))
            .unwrap()
            .dips
            .unwrap();
        assert_eq!(d.cause, DipCause::GpuClock);
        let d = parse_mangohud(&csv("30,33.3,30,10,500\n"))
            .unwrap()
            .dips
            .unwrap();
        assert_eq!(d.cause, DipCause::GpuWaiting);
        let mut spikes = String::from("fps,frametime\n");
        for i in 0..1000 {
            spikes.push_str(if i % 50 == 0 { "30,33\n" } else { "90,11\n" });
        }
        assert_eq!(parse_mangohud(&spikes).unwrap().dips, None);
    }

    #[test]
    fn periodic_hitches_have_a_period() {
        let mut s = String::from("fps,frametime\n");
        for i in 0..20000 {
            let ft = if i % 2700 == 0 && i > 0 {
                480.0
            } else if i == 4000 {
                300.0
            } else {
                11.1
            };
            s.push_str(&format!("90,{ft}\n"));
        }
        let h = parse_mangohud(&s).unwrap().hitches.unwrap();
        assert_eq!(h.count, 8);
        assert_eq!(h.longest_ms, 480.0);
        let p = h.period_s.unwrap();
        assert!((p - 30.45).abs() < 0.6, "{h:?}");
        let mut s = String::from("fps,frametime\n");
        for i in 0..20000 {
            let ft = if [900, 1300, 5000, 5200, 12000].contains(&i) {
                300.0
            } else {
                11.1
            };
            s.push_str(&format!("90,{ft}\n"));
        }
        assert_eq!(parse_mangohud(&s).unwrap().hitches.unwrap().period_s, None);
    }

    #[test]
    fn real_mangohud_log() {
        // MangoHud 0.8.4 logging vkcube on an RX 7900 XT at 144 Hz.
        let p = parse_mangohud(include_str!("../testdata/mangohud-vkcube.csv")).unwrap();
        assert_eq!(p.frames, 300);
        assert!((140.0..148.0).contains(&p.avg_fps), "{p:?}");
        assert!(p.low1_fps > 100.0 && p.low1_fps <= p.avg_fps, "{p:?}");
    }

    #[test]
    fn judge_and_index_a_directory() {
        let lib = Library::embedded().unwrap();
        let d = tempfile::tempdir().unwrap();
        let rd = d.path().join("recipes/r");
        std::fs::create_dir_all(rd.join("reports")).unwrap();
        std::fs::write(rd.join("recipe.toml"), recipe().to_toml().unwrap()).unwrap();
        let k = lib
            .plan(&recipe())
            .unwrap()
            .bundle_key(&ProtonId::new("ge", "GE-Proton11-12"));
        let mut r = report("A", 30);
        r.recipe.key = k;
        std::fs::write(
            rd.join("reports/1.json"),
            serde_json::to_string(&r).unwrap(),
        )
        .unwrap();
        std::fs::write(rd.join("reports/2.json"), "{").unwrap();
        let (idx, errors) = index(&lib, d.path());
        assert!(errors.is_empty());
        assert_eq!(idx.recipes[0].verdict.status, Status::Proposed);
        assert!(
            idx.recipes[0]
                .verdict
                .problems
                .iter()
                .any(|p| p.starts_with("2.json"))
        );
    }
}
