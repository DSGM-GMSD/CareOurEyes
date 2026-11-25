//! Una sola copia, autoarranque, tema del sistema y sonido.

use super::emit;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// `false` si ya hay otra copia (a la que se le pide mostrarse).
pub fn single_instance() -> bool {
    crate::platform::unix_instance::single_instance(emit)
}

/// Quita el socket al salir.
pub fn release_instance() {
    crate::platform::unix_instance::release();
}

fn autostart_file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
    });
    base.join("autostart").join("careoureyes.desktop")
}

/// El ejecutable a lanzar (el AppImage si se corre desde uno).
fn exe() -> String {
    std::env::var("APPIMAGE")
        .ok()
        .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "careoureyes".into())
}

pub fn set_autostart(enabled: bool) -> bool {
    let file = autostart_file();
    if !enabled {
        let _ = std::fs::remove_file(&file);
        return true;
    }
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=CareOurEyes\nComment=Brillo, temperatura y descansos\n\
         Exec=\"{}\" --autostart\nIcon=careoureyes\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe().replace('"', "\\\"")
    );
    std::fs::write(&file, text).is_ok()
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    let out = Command::new("gsettings").args(["get", schema, key]).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_lowercase())
}

/// Modo oscuro del escritorio: preferencia de color (GNOME 42+) o tema GTK con «dark».
pub fn system_prefers_dark() -> bool {
    if let Some(v) = gsettings("org.gnome.desktop.interface", "color-scheme") {
        if v.contains("dark") {
            return true;
        }
    }
    ["org.cinnamon.desktop.interface", "org.gnome.desktop.interface", "org.mate.interface"]
        .iter()
        .filter_map(|s| gsettings(s, "gtk-theme"))
        .next()
        .map(|t| t.contains("dark"))
        .unwrap_or(false)
}

/// Sonido de aviso: el del tema de sonidos si hay reproductor; si no, nada.
pub fn chime() {
    std::thread::spawn(|| {
        let quiet = |c: &mut Command| c.stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        if quiet(Command::new("canberra-gtk-play").args(["-i", "complete"])) {
            return;
        }
        for f in ["/usr/share/sounds/freedesktop/stereo/complete.oga", "/usr/share/sounds/freedesktop/stereo/bell.oga"] {
            if std::path::Path::new(f).exists() && quiet(Command::new("paplay").arg(f)) {
                return;
            }
        }
    });
}

pub fn open(target: &str) {
    let _ = Command::new("xdg-open").arg(target).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
