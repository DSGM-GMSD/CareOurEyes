//! GNOME y KDE Plasma en Wayland: no dejan cambiar la gamma desde otra app, así
//! que se usa su propia luz nocturna. Solo cambia la temperatura; el brillo no.
//!
//! Los cambios se aplican en un hilo aparte, como mucho unas pocas veces por
//! segundo (cada cambio lanza un comando del escritorio), y al salir se deja
//! la configuración de luz nocturna como estaba.

use crate::platform::{Backend, DisplayInfo};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Desktop {
    Gnome,
    Kde,
}

/// Lo que se pidió por última vez: temperatura, o `None` = dejar como estaba.
type Wanted = Arc<(Mutex<Option<Option<i32>>>, Condvar)>;

pub struct NightLight {
    desktop: Desktop,
    wanted: Wanted,
    original: Arc<Vec<(String, Option<String>)>>,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

const GNOME_SCHEMA: &str = "org.gnome.settings-daemon.plugins.color";
const GNOME_KEYS: [&str; 5] = [
    "night-light-enabled",
    "night-light-temperature",
    "night-light-schedule-automatic",
    "night-light-schedule-from",
    "night-light-schedule-to",
];
const KDE_KEYS: [&str; 3] = ["Active", "Mode", "NightTemperature"];

fn kde_tool(base: &str) -> &'static str {
    let six = if base == "read" { "kreadconfig6" } else { "kwriteconfig6" };
    let five = if base == "read" { "kreadconfig5" } else { "kwriteconfig5" };
    if run("which", &[six]).is_some() {
        six
    } else {
        five
    }
}

fn kde_reload() {
    let _ = run(
        "dbus-send",
        &["--session", "--type=method_call", "--dest=org.kde.KWin", "/KWin", "org.kde.KWin.reconfigure"],
    );
}

/// Configuración original (para dejarla igual al salir).
fn snapshot(desktop: Desktop) -> Vec<(String, Option<String>)> {
    match desktop {
        Desktop::Gnome => GNOME_KEYS.iter().map(|k| (k.to_string(), run("gsettings", &["get", GNOME_SCHEMA, k]))).collect(),
        Desktop::Kde => {
            let tool = kde_tool("read");
            KDE_KEYS
                .iter()
                .map(|k| {
                    let v = run(tool, &["--file", "kwinrc", "--group", "NightColor", "--key", k]).filter(|v| !v.is_empty());
                    (k.to_string(), v)
                })
                .collect()
        }
    }
}

fn set(desktop: Desktop, kelvin: Option<i32>, original: &[(String, Option<String>)]) {
    match (desktop, kelvin) {
        (Desktop::Gnome, Some(k)) => {
            let k = k.clamp(1700, 6500).to_string();
            for (key, value) in [
                ("night-light-schedule-automatic", "false"),
                ("night-light-schedule-from", "0.0"),
                ("night-light-schedule-to", "23.99"),
                ("night-light-temperature", k.as_str()),
                ("night-light-enabled", "true"),
            ] {
                let _ = run("gsettings", &["set", GNOME_SCHEMA, key, value]);
            }
        }
        (Desktop::Kde, Some(k)) => {
            let tool = kde_tool("write");
            let k = k.clamp(1000, 6500).to_string();
            for (key, value) in [("Active", "true"), ("Mode", "Constant"), ("NightTemperature", k.as_str())] {
                let _ = run(tool, &["--file", "kwinrc", "--group", "NightColor", "--key", key, value]);
            }
            kde_reload();
        }
        (Desktop::Gnome, None) => {
            for (key, value) in original {
                match value {
                    Some(v) => run("gsettings", &["set", GNOME_SCHEMA, key, v]),
                    None => run("gsettings", &["reset", GNOME_SCHEMA, key]),
                };
            }
        }
        (Desktop::Kde, None) => {
            let tool = kde_tool("write");
            for (key, value) in original {
                match value {
                    Some(v) => run(tool, &["--file", "kwinrc", "--group", "NightColor", "--key", key, v]),
                    None => run(tool, &["--file", "kwinrc", "--group", "NightColor", "--key", key, "--delete"]),
                };
            }
            kde_reload();
        }
    }
}

impl NightLight {
    /// Si el escritorio es GNOME o KDE Plasma (y tiene sus herramientas).
    pub fn detect() -> Option<Self> {
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
        let desktop = if current.contains("GNOME") && run("gsettings", &["get", GNOME_SCHEMA, "night-light-enabled"]).is_some() {
            Desktop::Gnome
        } else if current.contains("KDE") && run("which", &[kde_tool("write")]).is_some() {
            Desktop::Kde
        } else {
            return None;
        };
        let original = Arc::new(snapshot(desktop));
        let wanted: Wanted = Arc::new((Mutex::new(None), Condvar::new()));
        let shared = wanted.clone();
        let keep = original.clone();
        std::thread::Builder::new()
            .name("night-light".into())
            .spawn(move || {
                let mut applied: Option<Option<i32>> = None;
                loop {
                    let next = {
                        let (lock, cv) = &*shared;
                        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                        while g.is_none() || *g == applied {
                            g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                        }
                        *g
                    };
                    if let Some(k) = next {
                        set(desktop, k, &keep);
                        applied = Some(k);
                    }
                    // Unas pocas veces por segundo basta (el escritorio ya suaviza).
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
            .ok()?;
        Some(Self { desktop, wanted, original })
    }

    fn want(&self, kelvin: Option<i32>) {
        let (lock, cv) = &*self.wanted;
        if let Ok(mut g) = lock.lock() {
            *g = Some(kelvin);
            cv.notify_one();
        }
    }
}

impl Backend for NightLight {
    fn name(&self) -> String {
        match self.desktop {
            Desktop::Gnome => "GNOME (luz nocturna)".into(),
            Desktop::Kde => "KDE Plasma (luz nocturna)".into(),
        }
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        // La luz nocturna es una sola para todas las pantallas.
        vec![DisplayInfo {
            id: "WL\\night-light".into(),
            name: match self.desktop {
                Desktop::Gnome => "GNOME".into(),
                Desktop::Kde => "KDE Plasma".into(),
            },
            width: 0,
            height: 0,
            primary: true,
            gamma: true,
        }]
    }

    fn apply(&mut self, _id: &str, _brightness: f64, kelvin: f64, _standard: bool) -> bool {
        // Cerca de neutro, la luz nocturna se apaga en lugar de ponerla a 6500 K.
        let k = (kelvin / 100.0).round() as i32 * 100;
        self.want(if k >= 6400 { None } else { Some(k) });
        true
    }

    fn restore(&mut self, _id: &str) {
        self.want(None);
    }

    fn restore_all(&mut self) {
        // Al salir se deja como estaba enseguida, sin esperar al hilo.
        let (lock, _) = &*self.wanted;
        if let Ok(mut g) = lock.lock() {
            *g = None;
        }
        set(self.desktop, None, &self.original);
    }

    fn supports_brightness(&self) -> bool {
        false
    }
}
