use std::path::Path;

use unilux_core::{Result, write_atomic};

/// Desktop entry that starts the game through `unilux launch`, so every
/// launch gets runtime detection and logging.
pub fn write_entry(path: &Path, unilux_exe: &Path, icon: Option<&Path>) -> Result<()> {
    let icon = icon
        .map(|i| format!("Icon={}\n", i.display()))
        .unwrap_or_default();
    let exe = unilux_exe.display().to_string().replace(' ', "\\s");
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Somnium Space\nComment=Launch Somnium Space through Unilux\nExec={exe} launch\n{icon}Terminal=false\nCategories=Game;\nStartupNotify=true\n"
    );
    write_atomic(path, text.as_bytes())
}
