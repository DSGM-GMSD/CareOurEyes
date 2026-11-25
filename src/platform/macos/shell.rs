//! Una sola copia, arrancar al iniciar sesión, tema del sistema y abrir enlaces.

use super::emit;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn single_instance() -> bool {
    crate::platform::unix_instance::single_instance(emit)
}

pub fn release_instance() {
    crate::platform::unix_instance::release();
}

const LABEL: &str = "io.github.dsgm-gmsd.careoureyes";

fn agent_file() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Un LaunchAgent del usuario que abre la app (oculta) al iniciar sesión.
pub fn set_autostart(enabled: bool) -> bool {
    let file = agent_file();
    if !enabled {
        let _ = std::fs::remove_file(&file);
        return true;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
  <key>Label</key><string>{LABEL}</string>\n\
  <key>ProgramArguments</key><array><string>{}</string><string>--autostart</string></array>\n\
  <key>RunAtLoad</key><true/>\n\
  <key>ProcessType</key><string>Interactive</string>\n\
</dict>\n</plist>\n",
        xml_escape(&exe.to_string_lossy())
    );
    std::fs::write(&file, text).is_ok()
}

/// Modo oscuro: `defaults read -g AppleInterfaceStyle` devuelve «Dark».
pub fn system_prefers_dark() -> bool {
    Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().eq_ignore_ascii_case("dark"))
        .unwrap_or(false)
}

pub fn open(target: &str) {
    let _ = Command::new("open").arg(target).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
