//! Configuración persistida en JSON.
//!
//! Usa el mismo archivo y el mismo formato que la versión Kotlin
//! (`%APPDATA%\CareOurEyes\config.json` en Windows), así que las dos versiones
//! comparten ajustes. Los campos que esta versión todavía no usa (descansos,
//! enfoque, Prisma, atajos, reglas…) se conservan tal cual en `extra`, de modo
//! que guardar desde aquí nunca los borra.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Perfil de color. El identificador es el que se guarda en el JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Pause,
    Health,
    Game,
    Movie,
    Office,
    Edit,
    Read,
    Custom,
}

impl Mode {
    pub const ALL: [Mode; 8] = [
        Mode::Read,
        Mode::Edit,
        Mode::Health,
        Mode::Office,
        Mode::Game,
        Mode::Movie,
        Mode::Custom,
        Mode::Pause,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Mode::Pause => "pause",
            Mode::Health => "health",
            Mode::Game => "game",
            Mode::Movie => "movie",
            Mode::Office => "office",
            Mode::Edit => "edit",
            Mode::Read => "read",
            Mode::Custom => "custom",
        }
    }

    /// Clave de traducción del nombre del perfil.
    pub fn label_key(self) -> &'static str {
        match self {
            Mode::Pause => "pausa",
            Mode::Health => "salud_visual",
            Mode::Game => "juego",
            Mode::Movie => "pelicula",
            Mode::Office => "oficina",
            Mode::Edit => "edicion",
            Mode::Read => "lectura",
            Mode::Custom => "personalizado",
        }
    }

    pub fn from_id(id: &str) -> Mode {
        Mode::ALL.into_iter().find(|m| m.id() == id).unwrap_or(Mode::Custom)
    }
}

/// Valores de un perfil: brillo 0..100 y temperatura en Kelvin, de día y de noche.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeSettings {
    pub brightness: f64,
    pub kelvin: i32,
    pub night_brightness: f64,
    pub night_kelvin: i32,
}

impl ModeSettings {
    const fn new(b: f64, k: i32, nb: f64, nk: i32) -> Self {
        Self { brightness: b, kelvin: k, night_brightness: nb, night_kelvin: nk }
    }
    pub const NEUTRAL: ModeSettings = ModeSettings::new(100.0, 6500, 100.0, 6500);
}

fn d_custom() -> ModeSettings { ModeSettings::new(90.0, 5500, 90.0, 5500) }
fn d_read() -> ModeSettings { ModeSettings::new(85.0, 5500, 85.0, 5500) }
fn d_office() -> ModeSettings { ModeSettings::new(85.0, 5500, 80.0, 5000) }
fn d_game() -> ModeSettings { ModeSettings::new(90.0, 6500, 90.0, 6000) }
fn d_movie() -> ModeSettings { ModeSettings::new(90.0, 6000, 90.0, 5500) }
fn d_health() -> ModeSettings { ModeSettings::new(90.0, 5000, 80.0, 3700) }
fn d_edit() -> ModeSettings { ModeSettings::new(85.0, 6500, 85.0, 6500) }
fn d_mode() -> String { Mode::Health.id().to_string() }

/// Ajustes de un monitor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorConfig {
    pub display_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default = "d_mode")]
    pub mode: String,
    #[serde(default = "d_custom")]
    pub custom: ModeSettings,
    #[serde(default = "d_read")]
    pub read: ModeSettings,
    #[serde(default = "d_office")]
    pub office: ModeSettings,
    #[serde(default = "d_game")]
    pub game: ModeSettings,
    #[serde(default = "d_movie")]
    pub movie: ModeSettings,
    #[serde(default = "d_health")]
    pub health: ModeSettings,
    #[serde(default = "d_edit")]
    pub edit: ModeSettings,
    /// Último perfil antes de una pausa; «Reanudar» vuelve a él.
    #[serde(default)]
    pub resume_mode: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MonitorConfig {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            display_id: id.to_string(),
            display_name: name.to_string(),
            mode: d_mode(),
            custom: d_custom(),
            read: d_read(),
            office: d_office(),
            game: d_game(),
            movie: d_movie(),
            health: d_health(),
            edit: d_edit(),
            resume_mode: None,
            extra: Map::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        Mode::from_id(&self.mode)
    }

    pub fn settings(&self, mode: Mode) -> ModeSettings {
        match mode {
            Mode::Pause => ModeSettings::NEUTRAL,
            Mode::Health => self.health,
            Mode::Game => self.game,
            Mode::Movie => self.movie,
            Mode::Office => self.office,
            Mode::Edit => self.edit,
            Mode::Read => self.read,
            Mode::Custom => self.custom,
        }
    }

    pub fn settings_mut(&mut self, mode: Mode) -> Option<&mut ModeSettings> {
        match mode {
            Mode::Pause => None,
            Mode::Health => Some(&mut self.health),
            Mode::Game => Some(&mut self.game),
            Mode::Movie => Some(&mut self.movie),
            Mode::Office => Some(&mut self.office),
            Mode::Edit => Some(&mut self.edit),
            Mode::Read => Some(&mut self.read),
            Mode::Custom => Some(&mut self.custom),
        }
    }

    /// Perfil al que se vuelve después de una pausa.
    pub fn resume(&self) -> Mode {
        self.resume_mode
            .as_deref()
            .map(Mode::from_id)
            .filter(|m| *m != Mode::Pause)
            .unwrap_or(Mode::Health)
    }
}

/// Horario día/noche.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleConfig {
    /// "off", "manual" o "sun".
    pub mode: String,
    /// Minutos desde medianoche.
    pub day_start: i32,
    pub night_start: i32,
    pub latitude: f64,
    pub longitude: f64,
    pub use_location: bool,
    pub smooth_transition: bool,
    pub transition_minutes: i32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            mode: "sun".into(),
            day_start: 7 * 60,
            night_start: 19 * 60,
            latitude: 0.0,
            longitude: 0.0,
            use_location: true,
            smooth_transition: true,
            transition_minutes: 45,
            extra: Map::new(),
        }
    }
}

/// Configuración completa.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub version: i32,
    pub start_in_tray: bool,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub smooth_changes: bool,
    pub temperature_disabled: bool,
    pub reapply_interval_seconds: i32,
    pub sync_monitors: bool,
    pub reset_on_exit: bool,
    pub language: String,
    pub theme_mode: String,
    pub accent_theme: String,
    pub check_updates: bool,
    pub standard_gamma: bool,
    pub wider_kelvin: bool,
    pub wider_brightness: bool,
    pub schedule: ScheduleConfig,
    pub monitors: BTreeMap<String, MonitorConfig>,
    /// Secciones que esta versión aún no usa; se guardan sin tocarlas.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            start_in_tray: false,
            close_to_tray: true,
            start_with_windows: false,
            smooth_changes: true,
            temperature_disabled: false,
            reapply_interval_seconds: 0,
            sync_monitors: false,
            reset_on_exit: true,
            language: "system".into(),
            theme_mode: "dark".into(),
            accent_theme: "emerald".into(),
            check_updates: false,
            standard_gamma: false,
            wider_kelvin: false,
            wider_brightness: false,
            schedule: ScheduleConfig::default(),
            monitors: BTreeMap::new(),
            extra: Map::new(),
        }
    }
}

/// Carpeta de configuración del usuario.
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("CareOurEyes");
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        return home.join("Library/Application Support/CareOurEyes");
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("CareOurEyes")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

impl AppConfig {
    pub fn load() -> AppConfig {
        std::fs::read_to_string(config_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Escritura atómica: archivo temporal y luego renombrar.
    pub fn save(&self) {
        let path = config_file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_sections_survive_a_round_trip() {
        let json = r#"{"language":"es","rest":{"enabled":true,"workMinutes":25},
            "monitors":{"A":{"displayId":"A","mode":"office","future":1}}}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.language, "es");
        assert!(cfg.extra.contains_key("rest"));
        assert_eq!(cfg.monitors["A"].mode(), Mode::Office);
        let back = serde_json::to_string(&cfg).unwrap();
        assert!(back.contains("\"workMinutes\":25"));
        assert!(back.contains("\"future\":1"));
    }
}
