//! Host settings that decide whether VR frames arrive on time: the
//! compositor's scheduling priority, the GPU's power profile and other
//! OpenXR clients competing for the compositor.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{GpuInfo, Host, find_in_path};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VrTuning {
    pub compositor: Option<Compositor>,
    pub amd_power: Vec<PowerProfile>,
    /// XR overlays running next to the game.
    pub overlays: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Compositor {
    pub name: String,
    pub path: PathBuf,
    /// Whether it holds CAP_SYS_NICE to run its threads at realtime
    /// priority; None when getcap is unavailable.
    pub realtime: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerProfile {
    pub card: String,
    pub active: String,
    /// Index of the VR profile in pp_power_profile_mode, when the driver has one.
    pub vr_index: Option<u32>,
}

pub const COMPOSITORS: &[&str] = &["wivrn-server", "monado-service"];
pub const OVERLAYS: &[&str] = &["wayvr", "wlx-overlay-s", "xrdesktop", "stardust-xr-ser"];

pub fn vr_tuning(host: &dyn Host, gpus: &[GpuInfo]) -> VrTuning {
    VrTuning {
        compositor: compositor(host),
        amd_power: gpus
            .iter()
            .filter(|g| g.vendor == "AMD")
            .filter_map(|g| {
                let p = Path::new("/sys/class/drm")
                    .join(&g.card)
                    .join("device/pp_power_profile_mode");
                power_profile(&g.card, &host.read(&p)?)
            })
            .collect(),
        overlays: overlays(host),
    }
}

fn compositor(host: &dyn Host) -> Option<Compositor> {
    COMPOSITORS.iter().find_map(|name| {
        let path = host.canonicalize(&find_in_path(host, name)?);
        let caps = host.run("getcap", &[path.to_str()?]);
        Some(Compositor {
            name: name.to_string(),
            realtime: caps.map(|c| c.to_ascii_lowercase().contains("cap_sys_nice")),
            path,
        })
    })
}

/// Parse amdgpu's table, where the active row carries a star:
/// ` 0 BOOTUP_DEFAULT*:` / ` 4             VR :`.
pub fn power_profile(card: &str, table: &str) -> Option<PowerProfile> {
    let mut active = None;
    let mut vr_index = None;
    for l in table.lines() {
        let mut it = l.split_whitespace();
        let (Some(idx), Some(name)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(idx) = idx.parse::<u32>() else {
            continue;
        };
        let star = name.contains('*') || it.next().is_some_and(|t| t.starts_with('*'));
        let name = name.trim_end_matches([':', '*']).to_string();
        if name == "VR" {
            vr_index = Some(idx);
        }
        if star {
            active = Some(name);
        }
    }
    Some(PowerProfile {
        card: card.into(),
        active: active?,
        vr_index,
    })
}

fn overlays(host: &dyn Host) -> Vec<String> {
    let mut out: Vec<String> = host
        .list_dir("/proc".as_ref())
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|p| host.read(&p.join("comm")))
        .map(|c| c.trim().to_string())
        .filter(|c| OVERLAYS.contains(&c.as_str()))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeHost;

    #[test]
    fn amd_profile_table() {
        let t = "PROFILE_INDEX(NAME) CLOCK_TYPE(NAME) FPS MinActive\n 0 BOOTUP_DEFAULT*:\n                    0(       GFXCLK)       0\n 1 3D_FULL_SCREEN :\n 4             VR :\n 6         CUSTOM :\n";
        let p = power_profile("card1", t).unwrap();
        assert_eq!((p.active.as_str(), p.vr_index), ("BOOTUP_DEFAULT", Some(4)));
        let p = power_profile(
            "card1",
            &t.replace("BOOTUP_DEFAULT*", "BOOTUP_DEFAULT ")
                .replace("VR :", "VR*:"),
        )
        .unwrap();
        assert_eq!(p.active, "VR");
    }

    #[test]
    fn compositor_caps_and_overlays() {
        let h = FakeHost::new()
            .with_file("/usr/bin/wivrn-server", "")
            .with_command("getcap /usr/bin/wivrn-server", "")
            .with_file("/proc/42/comm", "wayvr\n")
            .with_file("/proc/7/comm", "bash\n");
        let t = vr_tuning(&h, &[]);
        let c = t.compositor.unwrap();
        assert_eq!((c.name.as_str(), c.realtime), ("wivrn-server", Some(false)));
        assert_eq!(t.overlays, vec!["wayvr".to_string()]);
        let h = FakeHost::new()
            .with_file("/usr/bin/wivrn-server", "")
            .with_command(
                "getcap /usr/bin/wivrn-server",
                "/usr/bin/wivrn-server cap_sys_nice=ep\n",
            );
        assert_eq!(vr_tuning(&h, &[]).compositor.unwrap().realtime, Some(true));
    }
}
