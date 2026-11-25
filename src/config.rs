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

/// Recordatorio de pausas para la vista.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RestConfig {
    pub enabled: bool,
    /// Minutos de trabajo entre pausas.
    pub work_minutes: i32,
    pub short_break_minutes: i32,
    /// Archivos antiguos: si es verdadero, `short_break_minutes` está en segundos.
    pub short_break_seconds: bool,
    pub long_break_minutes: i32,
    /// La pausa larga sustituye a la corta cada N sesiones de trabajo.
    pub long_break_every: i32,
    /// Con esto apagado todas las pausas son cortas.
    pub pomodoro: bool,
    /// Recordatorio 20-20-20: cada N minutos, 20 s mirando a lo lejos.
    pub blink_enabled: bool,
    pub blink_interval_minutes: i32,
    /// Aviso antes de la pausa (0 = sin aviso).
    pub pre_break_seconds: i32,
    pub allow_delay: bool,
    pub delay_times_max: i32,
    pub delay_minutes: i32,
    /// "light" (aviso), "fullscreen" (se puede saltar) o "strict" (no se salta).
    pub break_style: String,
    /// "top" o "bottom": dónde sale el aviso.
    pub prompt_place: String,
    /// Detener el temporizador si no se usa el equipo; si la ausencia dura más
    /// que una pausa corta, al volver cuenta como descanso.
    pub pause_when_idle: bool,
    pub pause_idle_amount: i32,
    /// "minutes" o "seconds".
    pub pause_idle_unit: String,
    /// Esperar a que termine la pantalla completa (juegos, películas, presentaciones).
    pub do_not_disturb_fullscreen: bool,
    pub alert_sound: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            work_minutes: 45,
            short_break_minutes: 5,
            short_break_seconds: false,
            long_break_minutes: 15,
            long_break_every: 4,
            pomodoro: false,
            blink_enabled: false,
            blink_interval_minutes: 20,
            pre_break_seconds: 30,
            allow_delay: true,
            delay_times_max: 3,
            delay_minutes: 5,
            break_style: "light".into(),
            prompt_place: "top".into(),
            pause_when_idle: true,
            pause_idle_amount: 5,
            pause_idle_unit: "minutes".into(),
            do_not_disturb_fullscreen: true,
            alert_sound: false,
            extra: Map::new(),
        }
    }
}

impl RestConfig {
    pub fn short_break_ms(&self) -> i64 {
        let n = self.short_break_minutes.max(1) as i64;
        if self.short_break_seconds { n * 1000 } else { n * 60_000 }
    }

    pub fn idle_ms(&self) -> i64 {
        let n = self.pause_idle_amount.max(1) as i64;
        if self.pause_idle_unit == "seconds" { n * 1000 } else { n * 60_000 }
    }
}

/// Atajos de teclado globales. Cada uno se guarda como "modificadores|tecla"
/// (los valores de Windows: Alt 1, Ctrl 2, Mayús 4, Win 8; tecla = código virtual).
/// "0|0" = sin atajo.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeyConfig {
    /// Cuánto cambia el brillo (%) y la temperatura (K) con cada pulsación.
    pub brightness_step: i32,
    pub kelvin_step: i32,
    pub inc_brightness: String,
    pub dec_brightness: String,
    pub inc_kelvin: String,
    pub dec_kelvin: String,
    pub toggle_temperature: String,
    pub toggle_pause: String,
    pub read: String,
    pub edit: String,
    pub game: String,
    pub movie: String,
    pub health: String,
    pub office: String,
    pub custom: String,
    pub toggle_band: String,
    pub toggle_veil: String,
    pub prisma_dark: String,
    pub prisma_gray: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        let none = || "0|0".to_string();
        Self {
            brightness_step: 5,
            kelvin_step: 200,
            inc_brightness: none(),
            dec_brightness: none(),
            inc_kelvin: none(),
            dec_kelvin: none(),
            toggle_temperature: none(),
            toggle_pause: none(),
            read: none(),
            edit: none(),
            game: none(),
            movie: none(),
            health: none(),
            office: none(),
            custom: none(),
            toggle_band: none(),
            toggle_veil: none(),
            prisma_dark: none(),
            prisma_gray: none(),
            extra: Map::new(),
        }
    }
}

impl HotkeyConfig {
    /// Nombres (los del JSON) en el orden en que se muestran.
    pub const NAMES: [&'static str; 17] = [
        "incBrightness",
        "decBrightness",
        "incKelvin",
        "decKelvin",
        "toggleTemperature",
        "togglePause",
        "read",
        "edit",
        "game",
        "movie",
        "health",
        "office",
        "custom",
        "toggleBand",
        "toggleVeil",
        "prismaDark",
        "prismaGray",
    ];

    pub fn get(&self, name: &str) -> &str {
        match name {
            "incBrightness" => &self.inc_brightness,
            "decBrightness" => &self.dec_brightness,
            "incKelvin" => &self.inc_kelvin,
            "decKelvin" => &self.dec_kelvin,
            "toggleTemperature" => &self.toggle_temperature,
            "togglePause" => &self.toggle_pause,
            "read" => &self.read,
            "edit" => &self.edit,
            "game" => &self.game,
            "movie" => &self.movie,
            "health" => &self.health,
            "office" => &self.office,
            "custom" => &self.custom,
            "toggleBand" => &self.toggle_band,
            "toggleVeil" => &self.toggle_veil,
            "prismaDark" => &self.prisma_dark,
            "prismaGray" => &self.prisma_gray,
            _ => "0|0",
        }
    }

    pub fn set(&mut self, name: &str, value: String) {
        let slot = match name {
            "incBrightness" => &mut self.inc_brightness,
            "decBrightness" => &mut self.dec_brightness,
            "incKelvin" => &mut self.inc_kelvin,
            "decKelvin" => &mut self.dec_kelvin,
            "toggleTemperature" => &mut self.toggle_temperature,
            "togglePause" => &mut self.toggle_pause,
            "read" => &mut self.read,
            "edit" => &mut self.edit,
            "game" => &mut self.game,
            "movie" => &mut self.movie,
            "health" => &mut self.health,
            "office" => &mut self.office,
            "custom" => &mut self.custom,
            "toggleBand" => &mut self.toggle_band,
            "toggleVeil" => &mut self.toggle_veil,
            "prismaDark" => &mut self.prisma_dark,
            "prismaGray" => &mut self.prisma_gray,
            _ => return,
        };
        *slot = value;
    }
}

/// Una regla: si la ventana activa coincide, se usa otro perfil mientras siga activa.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppRule {
    /// Nombre del programa (p. ej. "vlc.exe"), clase de ventana o parte del título.
    pub query: String,
    /// "process", "class" o "title".
    pub kind: String,
    /// "always" o "fullscreen" (solo cuando ocupa toda la pantalla).
    pub condition: String,
    /// Perfil que se aplica (id de `Mode`).
    pub mode: String,
}

impl Default for AppRule {
    fn default() -> Self {
        Self { query: String::new(), kind: "process".into(), condition: "always".into(), mode: Mode::Pause.id().into() }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RulesConfig {
    pub enabled: bool,
    /// Quitar el filtro mientras un programa ocupa toda la pantalla.
    pub pause_on_fullscreen: bool,
    pub items: Vec<AppRule>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Enfoque: franja de lectura que sigue al ratón y velo sobre el resto de ventanas.
/// (Los nombres «blur*» vienen de la versión 1; es un velo de color, no un desenfoque.)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FocusConfig {
    pub band_enabled: bool,
    /// Alto de la franja libre, en píxeles lógicos.
    pub band_height: i32,
    /// Opacidad (0..100) de lo que queda arriba y abajo.
    pub band_alpha: i32,
    /// Color como COLORREF de Windows (0x00BBGGRR); 0 = negro.
    pub band_color: i32,
    pub blur_enabled: bool,
    pub blur_alpha: i32,
    pub blur_color: i32,
    pub blur_animate: bool,
    pub blur_duration_ms: i32,
    pub include_taskbar: bool,
    pub current_monitor_only: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FocusConfig {
    fn default() -> Self {
        Self {
            band_enabled: false,
            band_height: 140,
            band_alpha: 55,
            band_color: 0,
            blur_enabled: false,
            blur_alpha: 60,
            blur_color: 0,
            blur_animate: true,
            blur_duration_ms: 250,
            include_taskbar: false,
            current_monitor_only: true,
            extra: Map::new(),
        }
    }
}

/// Prisma: oscurecer (invertir) o pasar a grises una ventana concreta.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PrismaConfig {
    pub enabled: bool,
    /// Barra con los botones sobre el título de la ventana bajo el ratón.
    pub show_toolbar: bool,
    /// Color de la barra (COLORREF).
    pub color: i32,
    /// 1 izquierda, 2 derecha, 3 centro.
    pub place: i32,
    /// Separación desde el borde (0..100 → 0..200 px).
    pub offset: i32,
    /// Espera antes de mostrar la barra (0..100 → 0..1,5 s).
    pub delay: i32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for PrismaConfig {
    fn default() -> Self {
        Self { enabled: false, show_toolbar: true, color: 0x003B291E, place: 3, offset: 30, delay: 30, extra: Map::new() }
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
    /// «Todas las pantallas» pasó a ser lo normal; esto marca que ya se aplicó una vez.
    pub rs_sync_default: bool,
    pub reset_on_exit: bool,
    pub language: String,
    pub theme_mode: String,
    pub accent_theme: String,
    pub check_updates: bool,
    pub standard_gamma: bool,
    pub wider_kelvin: bool,
    pub wider_brightness: bool,
    pub schedule: ScheduleConfig,
    pub rest: RestConfig,
    pub hotkeys: HotkeyConfig,
    pub rules: RulesConfig,
    pub focus: FocusConfig,
    pub prisma: PrismaConfig,
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
            sync_monitors: true,
            rs_sync_default: false,
            reset_on_exit: true,
            language: "system".into(),
            theme_mode: "system".into(),
            accent_theme: "cyan".into(),
            check_updates: false,
            standard_gamma: false,
            wider_kelvin: false,
            wider_brightness: false,
            schedule: ScheduleConfig::default(),
            rest: RestConfig::default(),
            hotkeys: HotkeyConfig::default(),
            rules: RulesConfig::default(),
            focus: FocusConfig::default(),
            prisma: PrismaConfig::default(),
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
        assert!(cfg.rest.enabled);
        assert_eq!(cfg.rest.work_minutes, 25);
        assert_eq!(cfg.monitors["A"].mode(), Mode::Office);
        let back = serde_json::to_string(&cfg).unwrap();
        assert!(back.contains("\"workMinutes\":25"));
        assert!(back.contains("\"future\":1"));
    }
}
