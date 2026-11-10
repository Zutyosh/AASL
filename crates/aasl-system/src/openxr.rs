use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Host, find_in_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    SteamVr,
    WiVRn,
    Monado,
    Envision,
    Other,
}

impl std::fmt::Display for RuntimeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SteamVr => "SteamVR",
            Self::WiVRn => "WiVRn",
            Self::Monado => "Monado",
            Self::Envision => "Envision",
            Self::Other => "other",
        })
    }
}

/// Where the runtime's files live, which decides what has to be shared into
/// the pressure-vessel container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLocation {
    Home,
    FlatpakSystem,
    Opt,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenXrRuntime {
    pub kind: RuntimeKind,
    pub name: Option<String>,
    pub manifest: PathBuf,
    pub library: PathBuf,
    pub location: RuntimeLocation,
}

impl OpenXrRuntime {
    /// Directory that must be shared into the pressure-vessel container,
    /// following the same rule as Somnilux: nothing for home and system
    /// paths, the app directory for Flatpak, the library's directory
    /// otherwise.
    pub fn share_dir(&self, home: &Path) -> Option<PathBuf> {
        share_root_for(&self.library, home)
    }
}

pub fn share_root_for(lib: &Path, home: &Path) -> Option<PathBuf> {
    if !home.as_os_str().is_empty() && lib.starts_with(home) {
        return None;
    }
    const SYSTEM: &[&str] = &[
        "/usr", "/etc", "/bin", "/sbin", "/lib", "/lib64", "/run", "/proc", "/sys", "/dev", "/tmp",
        "/var/tmp", "/var/run",
    ];
    if SYSTEM.iter().any(|s| lib.starts_with(s)) {
        return None;
    }
    let comps: Vec<_> = lib.components().collect();
    for i in 1..comps.len().saturating_sub(1) {
        if comps[i].as_os_str() == "app" && comps[i - 1].as_os_str() == "flatpak" {
            return Some(comps[..=i + 1].iter().collect());
        }
    }
    lib.parent().map(Path::to_path_buf)
}

fn classify(text: &str) -> RuntimeKind {
    let t = text.to_lowercase();
    if t.contains("steamvr") || t.contains("vrclient") {
        RuntimeKind::SteamVr
    } else if t.contains("wivrn") {
        RuntimeKind::WiVRn
    } else if t.contains("envision") {
        RuntimeKind::Envision
    } else if t.contains("monado") {
        RuntimeKind::Monado
    } else {
        RuntimeKind::Other
    }
}

fn location(lib: &Path, home: &Path) -> RuntimeLocation {
    if lib.starts_with("/var/lib/flatpak") {
        RuntimeLocation::FlatpakSystem
    } else if lib.starts_with("/opt") {
        RuntimeLocation::Opt
    } else if !home.as_os_str().is_empty() && lib.starts_with(home) {
        RuntimeLocation::Home
    } else {
        RuntimeLocation::System
    }
}

/// Candidate manifest paths, in the order the OpenXR loader uses.
fn manifest_candidates(host: &dyn Host) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = host.env("XR_RUNTIME_JSON").filter(|p| !p.is_empty()) {
        out.push(PathBuf::from(p));
    }
    let config_home = host
        .env("XDG_CONFIG_HOME")
        .filter(|p| p.starts_with('/'))
        .map(PathBuf::from)
        .unwrap_or_else(|| host.home().join(".config"));
    out.push(config_home.join("openxr/1/active_runtime.json"));
    let dirs = host
        .env("XDG_CONFIG_DIRS")
        .unwrap_or_else(|| "/etc/xdg".into());
    out.extend(
        dirs.split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("openxr/1/active_runtime.json")),
    );
    out.push("/etc/openxr/1/active_runtime.json".into());
    out
}

/// The runtime the OpenXR loader would pick right now. None means no
/// manifest at all, which for WiVRn means no headset is connected yet.
pub fn active_runtime(host: &dyn Host) -> Option<OpenXrRuntime> {
    manifest_candidates(host)
        .into_iter()
        .find_map(|m| parse_manifest(host, &m))
}

fn parse_manifest(host: &dyn Host, manifest: &Path) -> Option<OpenXrRuntime> {
    let text = host.read(manifest)?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let rt = json.get("runtime")?;
    let lib_raw = rt.get("library_path")?.as_str()?;
    let lib = Path::new(lib_raw);
    let library = if lib.is_absolute() {
        lib.to_path_buf()
    } else {
        manifest.parent().unwrap_or(Path::new("/")).join(lib)
    };
    let name = rt.get("name").and_then(|n| n.as_str()).map(str::to_string);
    let kind = classify(&format!(
        "{} {}",
        name.as_deref().unwrap_or(""),
        library.display()
    ));
    Some(OpenXrRuntime {
        kind,
        name,
        location: location(&library, &host.home()),
        manifest: manifest.to_path_buf(),
        library,
    })
}

/// Runtimes that look installed, active or not.
pub fn installed_runtimes(host: &dyn Host) -> Vec<RuntimeKind> {
    let home = host.home();
    let mut out = Vec::new();
    let any = |paths: &[PathBuf]| paths.iter().any(|p| host.exists(p));
    if any(&[
        home.join(".steam/steam/steamapps/common/SteamVR"),
        home.join(".local/share/Steam/steamapps/common/SteamVR"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/SteamVR"),
    ]) {
        out.push(RuntimeKind::SteamVr);
    }
    if find_in_path(host, "wivrn-server").is_some()
        || any(&[
            "/var/lib/flatpak/app/io.github.wivrn.wivrn".into(),
            home.join(".local/share/flatpak/app/io.github.wivrn.wivrn"),
        ])
    {
        out.push(RuntimeKind::WiVRn);
    }
    if find_in_path(host, "monado-service").is_some()
        || any(&["/usr/share/openxr/1/openxr_monado.json".into()])
    {
        out.push(RuntimeKind::Monado);
    }
    if find_in_path(host, "envision").is_some() || any(&[home.join(".local/share/envision")]) {
        out.push(RuntimeKind::Envision);
    }
    out
}

/// OpenVR implementation registered in openvrpaths.vrpath. Proton only
/// exposes OpenXR to a game when an OpenVR runtime is configured, so
/// Monado and WiVRn need one of the translation layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenVrLayer {
    Xrizer,
    OpenComposite,
    VapoR,
    SteamVr,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenVrRuntime {
    pub layer: OpenVrLayer,
    pub path: PathBuf,
    pub exists: bool,
}

pub fn openvr_runtime(host: &dyn Host) -> Option<OpenVrRuntime> {
    let config_home = host
        .env("XDG_CONFIG_HOME")
        .filter(|p| p.starts_with('/'))
        .map(PathBuf::from)
        .unwrap_or_else(|| host.home().join(".config"));
    let text = host.read(&config_home.join("openvr/openvrpaths.vrpath"))?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let first = json.get("runtime")?.as_array()?.first()?.as_str()?;
    let path = PathBuf::from(first);
    let t = first.to_lowercase();
    let layer = if t.contains("xrizer") {
        OpenVrLayer::Xrizer
    } else if t.contains("opencomposite") {
        OpenVrLayer::OpenComposite
    } else if t.contains("vapor") {
        OpenVrLayer::VapoR
    } else if t.contains("steamvr") {
        OpenVrLayer::SteamVr
    } else {
        OpenVrLayer::Other
    };
    Some(OpenVrRuntime {
        layer,
        exists: host.exists(&path),
        path,
    })
}

/// Everything AASL knows about VR readiness right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VrStatus {
    pub active: Option<OpenXrRuntime>,
    pub installed: Vec<RuntimeKind>,
    pub service_running: bool,
    pub openvr: Option<OpenVrRuntime>,
    /// True when launching now should reach the headset.
    pub ready: bool,
    /// Why it is not ready, in the order to fix them.
    pub blockers: Vec<String>,
    /// Advice that does not block the launch.
    pub notes: Vec<String>,
}

fn running(procs: &[String], names: &[&str]) -> bool {
    procs.iter().any(|p| names.iter().any(|n| p == n))
}

/// Combine manifest, processes and the OpenVR registration into a
/// verdict. `process_names` are the names of the user's processes.
pub fn vr_status(host: &dyn Host, process_names: &[String]) -> VrStatus {
    let active = active_runtime(host);
    let installed = installed_runtimes(host);
    let openvr = openvr_runtime(host);
    let mut blockers = Vec::new();
    let mut notes = Vec::new();
    let runtime_dir = host
        .env("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_default();
    let service_running = match active.as_ref().map(|r| r.kind) {
        None => {
            if installed.contains(&RuntimeKind::WiVRn) {
                blockers.push("WiVRn has no runtime file yet: it writes it only once a headset connects. Start WiVRn and connect the headset.".into());
            } else if installed.is_empty() {
                blockers.push("No VR runtime is installed. WiVRn suits standalone headsets (Quest, Pico); Monado through Envision suits wired PC headsets.".into());
            } else {
                blockers.push(format!(
                    "No OpenXR runtime is active. Start {} and connect the headset.",
                    installed
                        .iter()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                        .join(" or ")
                ));
            }
            false
        }
        Some(RuntimeKind::SteamVr) => {
            let up = running(process_names, &["vrserver", "vrcompositor"]);
            if !up {
                blockers.push(
                    "SteamVR is the active runtime but is not running. Start SteamVR.".into(),
                );
            }
            notes.push("The Linux VR community recommends WiVRn or Monado over SteamVR, which Valve ships as a development release on Linux.".into());
            up
        }
        Some(RuntimeKind::WiVRn) => {
            let up = running(process_names, &["wivrn-server"])
                || host.exists(&runtime_dir.join("wivrn/comp_ipc"));
            if !up {
                blockers.push(
                    "WiVRn's runtime file exists but the server is not running. Start WiVRn."
                        .into(),
                );
            }
            up
        }
        Some(RuntimeKind::Monado | RuntimeKind::Envision) => {
            let up = running(process_names, &["monado-service"])
                || host.exists(&runtime_dir.join("monado_comp_ipc"));
            if !up {
                blockers.push("Monado is the active runtime but monado-service is not running. Start it (or press Start in Envision).".into());
            }
            up
        }
        Some(RuntimeKind::Other) => {
            notes.push(
                "Unrecognised OpenXR runtime; AASL cannot tell whether it is running.".into(),
            );
            true
        }
    };
    if let Some(rt) = &active
        && !host.exists(&rt.library)
    {
        blockers.push(format!("The active runtime points at {}, which does not exist. Restart or reinstall the runtime.", rt.library.display()));
    }
    let needs_layer = matches!(
        active.as_ref().map(|r| r.kind),
        Some(RuntimeKind::WiVRn | RuntimeKind::Monado | RuntimeKind::Envision)
    );
    if needs_layer {
        match &openvr {
            None => blockers.push("No OpenVR runtime is registered. Proton only hands OpenXR to a game when one is: install xrizer (or OpenComposite) for your runtime.".into()),
            Some(o) if !o.exists => blockers.push(format!("openvrpaths.vrpath points at {}, which does not exist. Reinstall xrizer.", o.path.display())),
            Some(o) if o.layer == OpenVrLayer::SteamVr => blockers.push("OpenVR still points at SteamVR while the OpenXR runtime is Monado-based. Point it at xrizer (WiVRn's and Envision's settings can do this).".into()),
            Some(_) => {}
        }
    }
    VrStatus {
        ready: blockers.is_empty() && active.is_some(),
        active,
        installed,
        service_running,
        openvr,
        blockers,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeHost;

    const STEAMVR: &str = r#"{"file_format_version":"1.0.0","runtime":{"name":"SteamVR","library_path":"/home/u/.steam/steam/steamapps/common/SteamVR/bin/linux64/vrclient.so"}}"#;
    const WIVRN_FLATPAK: &str = r#"{"runtime":{"name":"WiVRn","library_path":"/var/lib/flatpak/app/io.github.wivrn.wivrn/current/active/files/lib/wivrn/libopenxr_wivrn.so"}}"#;
    const MONADO_REL: &str = r#"{"runtime":{"library_path":"../../../lib/libopenxr_monado.so"}}"#;

    #[test]
    fn steamvr_in_home() {
        let h = FakeHost::new().with_file("/home/u/.config/openxr/1/active_runtime.json", STEAMVR);
        let rt = active_runtime(&h).unwrap();
        assert_eq!(rt.kind, RuntimeKind::SteamVr);
        assert_eq!(rt.location, RuntimeLocation::Home);
        assert_eq!(rt.share_dir(Path::new("/home/u")), None);
    }

    #[test]
    fn wivrn_flatpak_shares_app_dir() {
        let h = FakeHost::new().with_file(
            "/home/u/.config/openxr/1/active_runtime.json",
            WIVRN_FLATPAK,
        );
        let rt = active_runtime(&h).unwrap();
        assert_eq!(rt.kind, RuntimeKind::WiVRn);
        assert_eq!(rt.location, RuntimeLocation::FlatpakSystem);
        assert_eq!(
            rt.share_dir(Path::new("/home/u")),
            Some("/var/lib/flatpak/app/io.github.wivrn.wivrn".into())
        );
    }

    #[test]
    fn relative_library_and_system_dir() {
        let h = FakeHost::new().with_file("/etc/xdg/openxr/1/active_runtime.json", MONADO_REL);
        let rt = active_runtime(&h).unwrap();
        assert_eq!(rt.kind, RuntimeKind::Monado);
        assert_eq!(
            rt.library,
            PathBuf::from("/etc/xdg/openxr/1/../../../lib/libopenxr_monado.so")
        );
    }

    #[test]
    fn env_override_wins() {
        let h = FakeHost::new()
            .with_env("XR_RUNTIME_JSON", "/opt/envision/rt.json")
            .with_file("/opt/envision/rt.json", r#"{"runtime":{"name":"Envision Monado","library_path":"/opt/envision/lib/libopenxr_monado.so"}}"#)
            .with_file("/home/u/.config/openxr/1/active_runtime.json", STEAMVR);
        let rt = active_runtime(&h).unwrap();
        assert_eq!(rt.kind, RuntimeKind::Envision);
        assert_eq!(rt.location, RuntimeLocation::Opt);
        assert_eq!(
            rt.share_dir(Path::new("/home/u")),
            Some("/opt/envision/lib".into())
        );
    }

    #[test]
    fn no_manifest_and_garbage() {
        assert!(active_runtime(&FakeHost::new()).is_none());
        let h =
            FakeHost::new().with_file("/home/u/.config/openxr/1/active_runtime.json", "not json");
        assert!(active_runtime(&h).is_none());
    }

    const XRIZER: &str = r#"{"runtime":["/home/u/.local/share/envision/xrizer"],"version":1}"#;
    const MONADO: &str =
        r#"{"runtime":{"name":"Monado","library_path":"/usr/lib/libopenxr_monado.so"}}"#;

    fn procs(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn monado_needs_service_and_openvr_layer() {
        let base = || {
            FakeHost::new()
                .with_env("XDG_RUNTIME_DIR", "/run/user/1000")
                .with_file("/home/u/.config/openxr/1/active_runtime.json", MONADO)
                .with_file("/usr/lib/libopenxr_monado.so", "")
        };
        let s = vr_status(&base(), &[]);
        assert!(!s.ready);
        assert_eq!(s.blockers.len(), 2, "{:?}", s.blockers);
        let h = base()
            .with_file("/home/u/.config/openvr/openvrpaths.vrpath", XRIZER)
            .with_file("/home/u/.local/share/envision/xrizer/bin/x", "");
        let s = vr_status(&h, &procs(&["monado-service"]));
        assert!(s.ready, "{:?}", s.blockers);
        assert_eq!(s.openvr.unwrap().layer, OpenVrLayer::Xrizer);
    }

    #[test]
    fn steamvr_and_wivrn_and_nothing() {
        let h = FakeHost::new()
            .with_file("/home/u/.config/openxr/1/active_runtime.json", STEAMVR)
            .with_file(
                "/home/u/.steam/steam/steamapps/common/SteamVR/bin/linux64/vrclient.so",
                "",
            );
        assert!(!vr_status(&h, &[]).ready);
        let s = vr_status(&h, &procs(&["vrserver"]));
        assert!(s.ready);
        assert!(!s.notes.is_empty());
        let w = FakeHost::new().with_file("/usr/bin/wivrn-server", "");
        let s = vr_status(&w, &[]);
        assert!(s.blockers[0].contains("headset connects"));
        assert!(vr_status(&FakeHost::new(), &[]).blockers[0].contains("No VR runtime"));
    }

    #[test]
    fn installed_detection() {
        let h = FakeHost::new()
            .with_file("/home/u/.steam/steam/steamapps/common/SteamVR/bin/x", "")
            .with_file("/usr/bin/wivrn-server", "");
        assert_eq!(
            installed_runtimes(&h),
            vec![RuntimeKind::SteamVr, RuntimeKind::WiVRn]
        );
    }
}
