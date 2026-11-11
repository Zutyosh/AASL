//! Read-only probes of the host: distribution, tools, GPU, OpenXR runtime.
//!
//! Every probe goes through the [Host] trait so tests can run against
//! fixtures instead of the real machine.

mod distro;
mod host;
mod openxr;
pub mod procs;
mod tools;
mod vrperf;

pub use distro::{Distro, PackageManager};
pub use host::{FakeHost, Host, RealHost};
pub use openxr::{
    OpenVrLayer, OpenVrRuntime, OpenXrRuntime, RuntimeKind, RuntimeLocation, VrStatus,
    active_runtime, installed_runtimes, openvr_runtime, share_root_for, vr_status,
};
pub use tools::{GpuInfo, PythonVersion, find_in_path, gpus, python_version, vulkan_icds};
pub use vrperf::{Compositor, PowerProfile, VrTuning, power_profile, vr_tuning};

use serde::Serialize;

/// Everything AASL knows about the machine, in one serialisable snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct SystemReport {
    pub distro: Option<Distro>,
    pub in_flatpak: bool,
    pub python: Option<PythonVersion>,
    pub missing_tools: Vec<String>,
    pub gpus: Vec<GpuInfo>,
    pub vulkan_icds: Vec<String>,
    pub active_runtime: Option<OpenXrRuntime>,
    pub installed_runtimes: Vec<RuntimeKind>,
    pub vr_tuning: VrTuning,
}

/// Tools the install and launch path calls on the host.
pub const REQUIRED_TOOLS: &[&str] = &["python3"];
/// Tools that improve the experience but are not required.
pub const OPTIONAL_TOOLS: &[&str] = &["magick", "convert", "vulkaninfo"];

pub fn probe(host: &dyn Host) -> SystemReport {
    let gpus = gpus(host);
    SystemReport {
        vr_tuning: vr_tuning(host, &gpus),
        distro: Distro::detect(host),
        in_flatpak: host.exists("/.flatpak-info".as_ref()),
        python: python_version(host),
        missing_tools: REQUIRED_TOOLS
            .iter()
            .filter(|t| find_in_path(host, t).is_none())
            .map(|t| t.to_string())
            .collect(),
        gpus,
        vulkan_icds: vulkan_icds(host),
        active_runtime: active_runtime(host),
        installed_runtimes: installed_runtimes(host),
    }
}
