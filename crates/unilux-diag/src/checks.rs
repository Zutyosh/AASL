//! Every check is a pure function of a [DiagContext] snapshot, so each one
//! has fixture tests and the same result in the GUI, the CLI and MCP.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use unilux_components::ProtonId;
use unilux_components::sources::LAUNCHER_REL_PATH;
use unilux_core::{Config, Paths, Severity};
use unilux_install::InstallState;
use unilux_patches::community::{DipCause, Perf};
use unilux_patches::{PatchStatus, TargetStatus};
use unilux_system::procs::ProcInfo;
use unilux_system::{SystemReport, VrStatus};

/// Fixes Unilux can apply itself. Anything not listed here is advice only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixId {
    /// Re-run the repair pipeline (prefix and Somnium untouched).
    Repair,
    /// Stop every process in the prefix.
    StopSession,
    /// Remove `openxr_env` from the config, back to automatic detection.
    ClearOpenxrOverride,
    /// Delete partial downloads.
    ClearDownloads,
}

impl FixId {
    pub const ALL: &[FixId] = &[
        Self::Repair,
        Self::StopSession,
        Self::ClearOpenxrOverride,
        Self::ClearDownloads,
    ];

    pub fn describe(self) -> &'static str {
        match self {
            Self::Repair => {
                "Re-download and re-apply umu-run, Proton and the patched libraries. Your prefix and Somnium settings stay as they are."
            }
            Self::StopSession => {
                "Stop Somnium Space and every Wine process in its prefix. Unsaved in-game state is lost."
            }
            Self::ClearOpenxrOverride => {
                "Remove the manual OpenXR settings and go back to automatic detection."
            }
            Self::ClearDownloads => "Delete cached and partial downloads.",
        }
    }

    /// Whether applying it can lose user state.
    pub fn destructive(self) -> bool {
        matches!(self, Self::StopSession)
    }

    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.into())).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub check: &'static str,
    pub severity: Severity,
    pub title: String,
    /// The facts the conclusion rests on: paths, versions, log lines.
    pub evidence: Vec<String>,
    /// What to do, in one or two sentences.
    pub advice: Option<String>,
    pub fix: Option<FixId>,
}

impl Finding {
    fn new(check: &'static str, severity: Severity, title: impl Into<String>) -> Self {
        Self {
            check,
            severity,
            title: title.into(),
            evidence: Vec::new(),
            advice: None,
            fix: None,
        }
    }
    fn evidence(mut self, e: impl Into<String>) -> Self {
        self.evidence.push(e.into());
        self
    }
    fn advice(mut self, a: impl Into<String>) -> Self {
        self.advice = Some(a.into());
        self
    }
    fn fix(mut self, f: FixId) -> Self {
        self.fix = Some(f);
        self
    }
}

/// Everything the checks look at, gathered once.
pub struct DiagContext {
    pub paths: Paths,
    pub config: Config,
    pub state: InstallState,
    pub system: SystemReport,
    pub prefix: PathBuf,
    pub proton_dir: Option<PathBuf>,
    pub proton: Option<ProtonId>,
    /// Installed patch bundle and whether its files are untouched.
    pub patches: Option<PatchStatus>,
    /// Best catalog bundle for this Proton; None when there is none.
    pub patch_match: Option<TargetStatus>,
    pub vr: Option<VrStatus>,
    pub processes: Vec<ProcInfo>,
    /// None when the network was not tested.
    pub github_reachable: Option<bool>,
    /// Tail of launch.log.
    pub launch_log: Vec<String>,
    /// Frame pacing of the last recorded session.
    pub last_perf: Option<Perf>,
    pub exists: Box<dyn Fn(&std::path::Path) -> bool + Send + Sync>,
}

impl DiagContext {
    fn exists(&self, p: &std::path::Path) -> bool {
        (self.exists)(p)
    }
}

pub struct Check {
    pub id: &'static str,
    pub run: fn(&DiagContext) -> Vec<Finding>,
}

pub const CHECKS: &[Check] = &[
    Check {
        id: "python",
        run: python,
    },
    Check {
        id: "gpu",
        run: gpu,
    },
    Check {
        id: "network",
        run: network,
    },
    Check {
        id: "install",
        run: install,
    },
    Check {
        id: "patches",
        run: patches,
    },
    Check {
        id: "proton",
        run: proton,
    },
    Check {
        id: "openxr",
        run: openxr,
    },
    Check {
        id: "session",
        run: session,
    },
    Check {
        id: "desktop_entry",
        run: desktop_entry,
    },
    Check {
        id: "sandbox",
        run: sandbox,
    },
    Check {
        id: "launch_log",
        run: launch_log,
    },
    Check {
        id: "vr_perf",
        run: vr_perf,
    },
];

/// Run every check; the most severe findings first.
pub fn run_all(ctx: &DiagContext) -> Vec<Finding> {
    let mut v: Vec<Finding> = CHECKS.iter().flat_map(|c| (c.run)(ctx)).collect();
    v.sort_by_key(|f| std::cmp::Reverse(f.severity));
    v
}

fn python(ctx: &DiagContext) -> Vec<Finding> {
    let hint = ctx
        .system
        .distro
        .as_ref()
        .and_then(|d| d.install_hint("python3"));
    match ctx.system.python {
        None => {
            let mut f = Finding::new("python", Severity::Error, "python3 is not installed").evidence("python3 --version did not run");
            f = f.advice(match hint {
                Some(h) => format!("umu-run needs Python 3.10 or newer. Install it: {h}"),
                None => "umu-run needs Python 3.10 or newer. Install it with your package manager.".into(),
            });
            vec![f]
        }
        Some(v) if !v.supported() => vec![
            Finding::new("python", Severity::Error, format!("python3 is {v}, older than 3.10"))
                .evidence(format!("python3 --version reports {v}"))
                .advice("umu-run needs 3.10 or newer as python3 on your PATH. On older LTS releases this usually means upgrading the distribution."),
        ],
        Some(_) => vec![],
    }
}

fn gpu(ctx: &DiagContext) -> Vec<Finding> {
    let mut out = Vec::new();
    if ctx.system.vulkan_icds.is_empty() {
        out.push(
            Finding::new("gpu", Severity::Error, "No Vulkan driver found")
                .evidence("no *.json in /usr/share/vulkan/icd.d or /etc/vulkan/icd.d")
                .advice("Proton renders through Vulkan. Install your GPU's Vulkan driver (mesa-vulkan-drivers, vulkan-radeon, nvidia-utils, ...)."),
        );
    }
    if ctx.system.gpus.is_empty() {
        out.push(Finding::new(
            "gpu",
            Severity::Warn,
            "No GPU found under /sys/class/drm",
        ));
    }
    out
}

fn network(ctx: &DiagContext) -> Vec<Finding> {
    match ctx.github_reachable {
        Some(false) => vec![
            Finding::new("network", Severity::Error, "GitHub is not reachable")
                .evidence("request to raw.githubusercontent.com failed")
                .advice("Installs and repairs download from GitHub. Check your connection, proxy or firewall."),
        ],
        _ => vec![],
    }
}

fn install(ctx: &DiagContext) -> Vec<Finding> {
    let mut out = Vec::new();
    let launcher = ctx.prefix.join(LAUNCHER_REL_PATH);
    if !ctx.exists(&launcher) {
        out.push(
            Finding::new(
                "install",
                Severity::Error,
                "Somnium Space is not installed in the prefix",
            )
            .evidence(format!("missing {}", launcher.display()))
            .advice("Run the installer with Somnium's Windows installer from somniumspace.com."),
        );
        return out;
    }
    let umu = unilux_install::umu_run(&ctx.paths);
    if !ctx.exists(&umu) {
        out.push(
            Finding::new("install", Severity::Error, "umu-run is missing")
                .evidence(format!("missing {}", umu.display()))
                .fix(FixId::Repair),
        );
    }
    match &ctx.proton_dir {
        Some(p) if ctx.exists(&p.join("proton")) => {}
        Some(p) => out.push(
            Finding::new("install", Severity::Error, "Proton is missing")
                .evidence(format!("missing {}", p.display()))
                .fix(FixId::Repair),
        ),
        None => out.push(
            Finding::new("install", Severity::Error, "No Proton version recorded")
                .fix(FixId::Repair),
        ),
    }
    out
}

fn patches(ctx: &DiagContext) -> Vec<Finding> {
    let Some(p) = &ctx.proton_dir else {
        return vec![];
    };
    if !ctx.exists(&p.join("proton")) {
        return vec![];
    }
    let mut out = Vec::new();
    match &ctx.patches {
        None | Some(PatchStatus { bundle: None, .. }) => out.push(
            Finding::new("patches", Severity::Error, "No Somnium patch bundle is installed in this Proton")
                .evidence(format!("no .unilux-patches.json in {}", p.display()))
                .advice("Without it the launcher cannot sign in: stock Wine lacks the TLS client-certificate support Somnium needs.")
                .fix(FixId::Repair),
        ),
        Some(PatchStatus { intact: false, changed, bundle, .. }) => {
            let mut f = Finding::new("patches", Severity::Error, format!("Patched files from {} were replaced", bundle.as_deref().unwrap_or("?")))
                .advice("Something (a Proton update, a manual copy) overwrote them. A repair puts them back.")
                .fix(FixId::Repair);
            for c in changed.iter().take(6) {
                f = f.evidence(format!("changed: {c}"));
            }
            out.push(f);
        }
        Some(PatchStatus { bundle: Some(b), .. }) => out.push(Finding::new("patches", Severity::Info, format!("Patch bundle {b} installed and intact"))),
    }
    let id = ctx
        .proton
        .as_ref()
        .map(|i| i.to_string())
        .unwrap_or_default();
    match ctx.patch_match {
        None if ctx.proton.is_some() => out.push(
            Finding::new("patches", Severity::Warn, format!("The catalog has no patch bundle for {id}"))
                .advice(format!("Switch to a verified Proton, or build a bundle with `unilux patch build --proton {id}` and test it.")),
        ),
        Some(TargetStatus::Untested) => out.push(
            Finding::new("patches", Severity::Warn, format!("The patch bundle for {id} is untested"))
                .advice("It was built for a nearby Proton. If sign-in or video fails, switch to a verified Proton and report the result."),
        ),
        _ => {}
    }
    out
}

fn proton(ctx: &DiagContext) -> Vec<Finding> {
    match ctx.proton.as_ref().and_then(|p| p.major().map(|m| (p, m))) {
        Some((p, m)) if m < 9 => vec![
            Finding::new("proton", Severity::Error, format!("{p} is older than Proton 9"))
                .advice("Proton 8 and older cannot initialise VR. Pick Proton 9 or newer in the Proton settings."),
        ],
        _ => vec![],
    }
}

fn openxr(ctx: &DiagContext) -> Vec<Finding> {
    if let Some(over) = &ctx.config.openxr_env {
        let mut f = Finding::new("openxr", Severity::Info, "OpenXR detection is replaced by manual settings")
            .advice("These settings replace detection entirely. If you need them, please report your runtime so detection can handle it.")
            .fix(FixId::ClearOpenxrOverride);
        for (k, v) in over {
            f = f.evidence(format!("{k}={v}"));
        }
        return vec![f];
    }
    let Some(vr) = &ctx.vr else { return vec![] };
    let mut out = Vec::new();
    let name = vr
        .active
        .as_ref()
        .map(|r| r.name.clone().unwrap_or_else(|| r.kind.to_string()));
    if vr.ready {
        let mut f = Finding::new(
            "openxr",
            Severity::Info,
            format!("VR ready: {}", name.clone().unwrap_or_default()),
        );
        if let Some(rt) = &vr.active {
            f = f
                .evidence(format!("manifest {}", rt.manifest.display()))
                .evidence(format!("library {}", rt.library.display()));
        }
        if let Some(o) = &vr.openvr {
            f = f.evidence(format!(
                "OpenVR layer {:?} at {}",
                o.layer,
                o.path.display()
            ));
        }
        out.push(f);
    }
    for b in &vr.blockers {
        let mut f = Finding::new("openxr", Severity::Warn, "VR is not ready").advice(b.clone());
        if let Some(n) = &name {
            f = f.evidence(format!("active runtime: {n}"));
        }
        out.push(f);
    }
    for n in &vr.notes {
        out.push(Finding::new("openxr", Severity::Info, "VR note").advice(n.clone()));
    }
    out
}

fn session(ctx: &DiagContext) -> Vec<Finding> {
    if ctx.processes.is_empty() {
        return vec![];
    }
    let mut f = Finding::new(
        "session",
        Severity::Info,
        format!(
            "Somnium Space is running ({} processes)",
            ctx.processes.len()
        ),
    )
    .fix(FixId::StopSession);
    for p in ctx.processes.iter().take(8) {
        f = f.evidence(format!("{} {}", p.pid, p.cmdline));
    }
    vec![f]
}

fn desktop_entry(ctx: &DiagContext) -> Vec<Finding> {
    if ctx.state.is_installed() && !ctx.exists(&ctx.paths.desktop_entry()) {
        return vec![
            Finding::new(
                "desktop_entry",
                Severity::Warn,
                "The start-menu entry is missing",
            )
            .evidence(format!("missing {}", ctx.paths.desktop_entry().display()))
            .fix(FixId::Repair),
        ];
    }
    vec![]
}

fn sandbox(ctx: &DiagContext) -> Vec<Finding> {
    let mut out = Vec::new();
    if ctx.system.in_flatpak {
        out.push(
            Finding::new("sandbox", Severity::Warn, "Unilux is running inside a Flatpak sandbox")
                .advice("Proton's container cannot start from inside Flatpak without host access. Use the AppImage or the native package."),
        );
    }
    if let Some(d) = &ctx.system.distro
        && d.immutable
    {
        out.push(Finding::new("sandbox", Severity::Info, format!("{} is an immutable distribution", d.name)).advice("Unilux writes only to your home directory, so this should work. Reports are welcome either way."));
    }
    out
}

/// Known failure signatures in Proton and Somnium output.
const LOG_PATTERNS: &[(&str, Severity, &str, &str)] = &[
    (
        "SEC_E_",
        Severity::Error,
        "TLS errors in the launch log",
        "The patched secur32/crypt32 are probably not in use. Run a repair.",
    ),
    (
        "certificate verify failed",
        Severity::Error,
        "Certificate errors in the launch log",
        "The patched libraries are probably not in use. Run a repair.",
    ),
    (
        "err:vulkan",
        Severity::Error,
        "Vulkan errors in the launch log",
        "Check the GPU driver and that Vulkan works (vulkaninfo).",
    ),
    (
        "XR_ERROR_RUNTIME_UNAVAILABLE",
        Severity::Warn,
        "The game could not reach the OpenXR runtime",
        "Connect the headset before launching and check the runtime sees it.",
    ),
    (
        "pressure-vessel-wrap: E:",
        Severity::Error,
        "Proton's container failed to start",
        "Look at the line in the evidence; it often names a missing path or permission.",
    ),
];

fn vr_perf(ctx: &DiagContext) -> Vec<Finding> {
    let mut out = Vec::new();
    let t = &ctx.system.vr_tuning;
    if let Some(c) = &t.compositor
        && c.realtime == Some(false)
    {
        let path = c.path.display();
        out.push(
            Finding::new("vr_perf", Severity::Warn, format!("{} cannot run its compositor at realtime priority", c.name))
                .evidence(format!("getcap {path}: no cap_sys_nice"))
                .advice(format!("When the CPU is busy the compositor misses its deadline and the headset rate halves (90 to 45 or lower) for a while. Grant it once, then restart it: sudo setcap CAP_SYS_NICE=+ep {path}. A package update removes it.")),
        );
    }
    for p in &t.amd_power {
        if let Some(vr) = p.vr_index
            && p.active != "VR"
        {
            let file = format!("/sys/class/drm/{}/device/pp_power_profile_mode", p.card);
            out.push(
                Finding::new("vr_perf", Severity::Warn, "The AMD GPU is not in its VR power profile")
                    .evidence(format!("{file}: {} active", p.active))
                    .advice(format!("VR load comes in short bursts and the default profile lets the clocks fall between them, so some frames miss the headset deadline. SteamVR switches to the VR profile itself, WiVRn and Monado do not. Until reboot: echo {vr} | sudo tee {file}, or set it permanently in LACT or CoreCtrl.")),
            );
        }
    }
    if !t.overlays.is_empty() {
        out.push(
            Finding::new("vr_perf", Severity::Info, format!("XR overlay running: {}", t.overlays.join(", ")))
                .advice("It is a second client of the compositor and takes focus from the game when shown. If slowdowns line up with it, close it to compare."),
        );
    }
    if let Some(h) = ctx.last_perf.and_then(|p| p.hitches) {
        let ev = format!(
            "last session: {} freezes, the longest {:.0} ms",
            h.count, h.longest_ms
        );
        out.push(match h.period_s {
            Some(p) => Finding::new("vr_perf", Severity::Warn, format!("The game freezes every {p:.1} s"))
                .evidence(ev)
                .advice("Freezes on a fixed clock come from a timer inside the game (a periodic cleanup, a poll, a network or voice task), not from the GPU or the compositor. Note what you were doing and compare with voice chat or the microphone off."),
            None => Finding::new("vr_perf", Severity::Info, "Freezes in the last session")
                .evidence(ev)
                .advice("Irregular freezes usually line up with loading: worlds, avatars joining, videos."),
        });
    }
    if let Some(d) = ctx.last_perf.and_then(|p| p.dips) {
        let pair = |v: Option<[f32; 2]>, unit: &str| {
            v.map(|[n, d]| format!("{n:.0}{unit} normally, {d:.0}{unit} during them"))
        };
        let (sev, title, advice) = match d.cause {
            DipCause::GpuWaiting => (
                Severity::Warn,
                "The GPU idles during the slowdowns",
                "Frames are late without the GPU being busy: the game's main thread, the compositor or loading (avatars, worlds, video) holds them back. Check the findings above first; a scene with many avatars on a 6-core CPU is the usual limit.",
            ),
            DipCause::GpuClock => (
                Severity::Warn,
                "The GPU clock drops during the slowdowns",
                "Power management lowers the clock just when frames need it. Use the VR power profile (see above) or a fixed performance level.",
            ),
            DipCause::GpuBound => (
                Severity::Info,
                "The GPU is saturated during the slowdowns",
                "Lower the resolution in WiVRn or the in-game graphics settings, or hide distant avatars.",
            ),
            DipCause::Unknown => (
                Severity::Info,
                "Slowdowns in the last session",
                "MangoHud did not report GPU load, so the cause is unknown.",
            ),
        };
        let mut f = Finding::new("vr_perf", sev, title)
            .evidence(format!(
                "last session: {} slowdowns, {:.0} s in total, {:.0} fps during them",
                d.episodes, d.seconds, d.fps
            ))
            .advice(advice);
        for e in [
            pair(d.gpu_load, " % GPU"),
            pair(d.gpu_clock_mhz, " MHz"),
            pair(d.cpu_load, " % CPU (all cores)"),
        ]
        .into_iter()
        .flatten()
        {
            f = f.evidence(e);
        }
        out.push(f);
    }
    out
}

fn launch_log(ctx: &DiagContext) -> Vec<Finding> {
    let mut out = Vec::new();
    for (needle, sev, title, advice) in LOG_PATTERNS {
        let hits: Vec<&String> = ctx
            .launch_log
            .iter()
            .filter(|l| l.contains(needle))
            .collect();
        if let Some(first) = hits.first() {
            let mut f = Finding::new("launch_log", *sev, *title)
                .advice(*advice)
                .evidence(format!(
                    "{} matching lines, first: {}",
                    hits.len(),
                    first.trim()
                ));
            if needle.starts_with("SEC_E") || needle.starts_with("certificate") {
                f = f.fix(FixId::Repair);
            }
            out.push(f);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;
    use unilux_system::{FakeHost, PythonVersion};

    fn ctx(files: &[&str], f: impl FnOnce(&mut DiagContext)) -> DiagContext {
        let paths = Paths::resolve(|k| (k == "HOME").then(|| "/h".to_string())).unwrap();
        let mut system = unilux_system::probe(&FakeHost::new());
        system.python = Some(PythonVersion {
            major: 3,
            minor: 12,
        });
        system.vulkan_icds = vec!["radeon.json".into()];
        system.gpus = vec![unilux_system::GpuInfo {
            card: "card0".into(),
            vendor: "AMD".into(),
            driver: None,
        }];
        let set: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
        let mut c = DiagContext {
            prefix: paths.default_prefix(),
            proton_dir: Some(paths.proton_root().join("ge/GE-Proton11-5")),
            proton: Some(ProtonId::new("ge", "GE-Proton11-5")),
            patches: Some(PatchStatus {
                bundle: Some("b1".into()),
                intact: true,
                changed: vec![],
            }),
            patch_match: Some(TargetStatus::Verified),
            vr: None,
            paths,
            config: Config::default(),
            state: InstallState::default(),
            system,
            processes: vec![],
            github_reachable: Some(true),
            launch_log: vec![],
            last_perf: None,
            exists: Box::new(move |p: &Path| set.contains(p)),
        };
        f(&mut c);
        c
    }

    const LAUNCHER: &str = "/h/.local/share/unilux/prefix/drive_c/Program Files/Somnium Space/Somnium Space Launcher.exe";
    const UMU: &str = "/h/.local/share/unilux/components/umu/umu-run";
    const PROTON: &str = "/h/.local/share/unilux/components/proton/ge/GE-Proton11-5/proton";

    fn vr(ready: bool, blockers: &[&str]) -> VrStatus {
        VrStatus {
            active: None,
            installed: vec![],
            service_running: ready,
            openvr: None,
            ready,
            blockers: blockers.iter().map(|b| b.to_string()).collect(),
            notes: vec![],
        }
    }

    fn ids(v: &[Finding]) -> Vec<(&str, Severity)> {
        v.iter().map(|f| (f.check, f.severity)).collect()
    }

    #[test]
    fn healthy_install_has_only_info() {
        let c = ctx(&[LAUNCHER, UMU, PROTON], |c| c.vr = Some(vr(true, &[])));
        let v = run_all(&c);
        assert!(v.iter().all(|f| f.severity == Severity::Info), "{v:#?}");
    }

    #[test]
    fn not_installed() {
        let v = install(&ctx(&[], |_| {}));
        assert_eq!(ids(&v), [("install", Severity::Error)]);
        assert!(v[0].fix.is_none());
    }

    #[test]
    fn missing_components_offer_repair() {
        let v = install(&ctx(&[LAUNCHER], |_| {}));
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|f| f.fix == Some(FixId::Repair)));
    }

    #[test]
    fn patch_states() {
        let none = patches(&ctx(&[PROTON], |c| c.patches = None));
        assert_eq!(none[0].fix, Some(FixId::Repair));
        let changed = patches(&ctx(&[PROTON], |c| {
            c.patches = Some(PatchStatus {
                bundle: Some("b1".into()),
                intact: false,
                changed: vec!["files/x.dll".into()],
            })
        }));
        assert!(changed[0].title.contains("replaced"));
        let ok = patches(&ctx(&[PROTON], |_| {}));
        assert_eq!(ids(&ok), [("patches", Severity::Info)]);
        let nomatch = patches(&ctx(&[PROTON], |c| c.patch_match = None));
        assert!(nomatch.iter().any(|f| {
            f.advice
                .as_ref()
                .is_some_and(|a| a.contains("unilux patch build"))
        }));
        assert!(
            patches(&ctx(&[], |_| {})).is_empty(),
            "no Proton, nothing to say"
        );
    }

    #[test]
    fn old_proton() {
        assert_eq!(
            proton(&ctx(&[], |c| c.proton =
                Some(ProtonId::new("ge", "GE-Proton8-32"))))[0]
                .severity,
            Severity::Error
        );
        assert!(proton(&ctx(&[], |_| {})).is_empty());
    }

    #[test]
    fn python_cases() {
        assert!(python(&ctx(&[], |_| {})).is_empty());
        assert_eq!(python(&ctx(&[], |c| c.system.python = None)).len(), 1);
        let old = python(&ctx(&[], |c| {
            c.system.python = Some(PythonVersion { major: 3, minor: 9 })
        }));
        assert!(old[0].title.contains("3.9"));
    }

    #[test]
    fn vr_blockers_become_findings() {
        let v = openxr(&ctx(&[], |c| {
            c.vr = Some(vr(false, &["Start WiVRn", "Install xrizer"]))
        }));
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|f| f.severity == Severity::Warn));
        assert_eq!(
            openxr(&ctx(&[], |c| c.vr = Some(vr(true, &[]))))[0].severity,
            Severity::Info
        );
    }

    #[test]
    fn override_and_network_and_session() {
        let v = openxr(&ctx(&[], |c| {
            c.config.openxr_env = Some([("A".into(), "1".into())].into())
        }));
        assert_eq!(v[0].fix, Some(FixId::ClearOpenxrOverride));
        assert_eq!(
            network(&ctx(&[], |c| c.github_reachable = Some(false))).len(),
            1
        );
        assert!(network(&ctx(&[], |c| c.github_reachable = None)).is_empty());
        let v = session(&ctx(&[], |c| {
            c.processes = vec![ProcInfo {
                pid: 7,
                ppid: None,
                name: "wine".into(),
                cmdline: "x".into(),
                wineprefix: None,
            }]
        }));
        assert_eq!(v[0].fix, Some(FixId::StopSession));
    }

    #[test]
    fn gpu_and_desktop_and_sandbox() {
        assert_eq!(gpu(&ctx(&[], |c| c.system.vulkan_icds.clear())).len(), 1);
        assert_eq!(
            desktop_entry(&ctx(&[], |c| c.state.installed_at = Some(1))).len(),
            1
        );
        assert!(desktop_entry(&ctx(&[], |_| {})).is_empty());
        assert_eq!(sandbox(&ctx(&[], |c| c.system.in_flatpak = true)).len(), 1);
    }

    #[test]
    fn log_signatures() {
        let v = launch_log(&ctx(&[], |c| {
            c.launch_log = vec!["ok".into(), "0024:err:secur32: SEC_E_INTERNAL_ERROR".into()]
        }));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].fix, Some(FixId::Repair));
        assert!(launch_log(&ctx(&[], |c| c.launch_log = vec!["fine".into()])).is_empty());
    }

    #[test]
    fn fix_ids_round_trip() {
        for f in FixId::ALL {
            let s = serde_json::to_value(f).unwrap();
            assert_eq!(FixId::parse(s.as_str().unwrap()), Some(*f));
        }
        assert_eq!(FixId::parse("rm_rf"), None);
    }
}
