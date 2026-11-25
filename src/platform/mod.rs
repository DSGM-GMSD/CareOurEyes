//! Todo lo que depende del sistema operativo.
//!
//! `Backend` escribe brillo y color en las pantallas; el resto de funciones
//! cubren bandeja, autoarranque, atajos, etc. Hay versión para Windows y para
//! Linux (X11). En otros sistemas se usa un simulador para probar la interfaz.

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod unix_instance;

#[cfg(not(windows))]
pub mod dummy;

/// Textos (ya traducidos) del menú nativo de la bandeja (Linux y macOS).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrayLabels {
    pub show: String,
    pub pause: String,
    pub quit: String,
}

/// Avisos del sistema para la app: icono de bandeja, atajos, otra copia…
#[derive(Clone, Copy, Debug)]
pub enum TrayEvent {
    /// Clic izquierdo en el icono (o «Mostrar» en el menú nativo).
    Open,
    /// Clic derecho, con la posición del cursor (Windows: menú propio).
    Menu(i32, i32),
    /// El equipo volvió de suspensión o se encendió la pantalla.
    Resumed,
    /// Se conectó, quitó o cambió de resolución una pantalla.
    DisplaysChanged,
    /// Otra copia de la app pidió mostrar la ventana.
    ShowRequested,
    /// Cambió el tema claro/oscuro del sistema.
    ThemeChanged,
    /// Se pulsó un atajo registrado (su número).
    Hotkey(i32),
    /// «Pausar/Reanudar» del menú nativo (Linux).
    TogglePause,
    /// «Salir» del menú nativo (Linux).
    Quit,
}

/// Una pantalla conectada.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    /// Identificador estable (el mismo que guarda la versión Kotlin).
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
    /// Se pudo leer su rampa gamma.
    pub gamma: bool,
}

pub trait Backend {
    /// Nombre corto del motor, para la interfaz.
    fn name(&self) -> String;
    /// Vuelve a buscar las pantallas (la principal primero).
    fn displays(&mut self) -> Vec<DisplayInfo>;
    /// Aplica brillo (0..100) y temperatura (K). `standard` = solo la API documentada.
    fn apply(&mut self, id: &str, brightness: f64, kelvin: f64, standard: bool) -> bool;
    /// Deja la pantalla como estaba al abrir la app.
    fn restore(&mut self, id: &str);
    fn restore_all(&mut self);
}

pub fn backend() -> Box<dyn Backend> {
    #[cfg(windows)]
    {
        Box::new(windows::gamma::GdiGamma::new())
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("COE_SIM").is_none() && linux::x::x().is_some() {
            return Box::new(linux::gamma::RandrGamma::new());
        }
        Box::new(dummy::Simulated::default())
    }
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("COE_SIM").is_some() {
            return Box::new(dummy::Simulated::default());
        }
        macos::gamma::watch_displays();
        Box::new(macos::gamma::QuartzGamma::new())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Box::new(dummy::Simulated::default())
    }
}

/// Abre un enlace en el navegador.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    windows::shell::open(url);
    #[cfg(target_os = "linux")]
    linux::shell::open(url);
    #[cfg(target_os = "macos")]
    macos::shell::open(url);
}

/// Abre una carpeta en el explorador de archivos.
pub fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    open_url(&path.to_string_lossy());
}

/// Arrancar al iniciar sesión.
pub fn set_autostart(enabled: bool) -> bool {
    #[cfg(windows)]
    {
        windows::shell::set_autostart(enabled)
    }
    #[cfg(target_os = "linux")]
    {
        linux::shell::set_autostart(enabled)
    }
    #[cfg(target_os = "macos")]
    {
        macos::shell::set_autostart(enabled)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = enabled;
        false
    }
}

/// ¿El sistema está en modo oscuro? (para el tema «Automático»).
pub fn system_prefers_dark() -> bool {
    #[cfg(windows)]
    {
        windows::shell::system_prefers_dark()
    }
    #[cfg(target_os = "linux")]
    {
        linux::shell::system_prefers_dark()
    }
    #[cfg(target_os = "macos")]
    {
        macos::shell::system_prefers_dark()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        true
    }
}

/// Milisegundos desde la última tecla o movimiento del ratón.
pub fn idle_ms() -> i64 {
    #[cfg(windows)]
    {
        windows::desk::idle_ms()
    }
    #[cfg(target_os = "linux")]
    {
        linux::x::idle_ms()
    }
    #[cfg(target_os = "macos")]
    {
        macos::desk::idle_ms()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        0
    }
}

/// Tecla física (no modificador) pulsada ahora, como código virtual de Windows.
/// Con Ctrl o AltGr el texto de la tecla no siempre es la letra; esto sí.
pub fn pressed_key() -> Option<u32> {
    #[cfg(windows)]
    {
        windows::desk::pressed_key()
    }
    #[cfg(target_os = "macos")]
    {
        macos::desk::pressed_key()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// Modificadores pulsados ahora (Alt 1, Ctrl 2, Mayús 4, Win 8), si se puede saber.
pub fn held_mods() -> Option<u32> {
    #[cfg(windows)]
    {
        Some(windows::desk::held_mods())
    }
    #[cfg(target_os = "macos")]
    {
        Some(macos::desk::held_mods())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

/// ¿Hay un programa a pantalla completa en primer plano?
pub fn foreground_fullscreen() -> bool {
    #[cfg(windows)]
    {
        windows::desk::foreground_fullscreen()
    }
    #[cfg(target_os = "linux")]
    {
        linux::x::foreground_fullscreen()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        false
    }
}

/// Sonido de aviso del sistema.
pub fn chime() {
    #[cfg(windows)]
    windows::desk::chime();
    #[cfg(target_os = "linux")]
    linux::shell::chime();
    #[cfg(target_os = "macos")]
    macos::desk::chime();
}

/// La ventana en primer plano, para las reglas por programa.
#[derive(Clone, Debug, Default)]
pub struct Foreground {
    /// Nombre del ejecutable, p. ej. "vlc.exe".
    pub process: String,
    pub class: String,
    pub title: String,
    /// Es una ventana de CareOurEyes.
    pub own: bool,
    pub fullscreen: bool,
}

pub fn foreground_info() -> Option<Foreground> {
    #[cfg(windows)]
    {
        windows::desk::foreground_info()
    }
    #[cfg(target_os = "linux")]
    {
        linux::x::foreground_info()
    }
    #[cfg(target_os = "macos")]
    {
        macos::desk::foreground_info()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Registra los atajos globales (número, modificadores, tecla). Devuelve los que fallaron.
pub fn register_hotkeys(list: &[(i32, u32, u32)]) -> Vec<i32> {
    #[cfg(windows)]
    {
        windows::tray::register_hotkeys(list)
    }
    #[cfg(target_os = "linux")]
    {
        linux::x::register_hotkeys(list)
    }
    #[cfg(target_os = "macos")]
    {
        macos::keys::register_hotkeys(list)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = list;
        Vec::new()
    }
}

/// `false` si ya corre otra copia (a la que se le pide mostrarse).
pub fn single_instance() -> bool {
    #[cfg(windows)]
    {
        windows::shell::single_instance()
    }
    #[cfg(target_os = "linux")]
    {
        linux::shell::single_instance()
    }
    #[cfg(target_os = "macos")]
    {
        macos::shell::single_instance()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        true
    }
}

/// ¿Se pueden usar Enfoque y Prisma? (por ahora solo en Windows).
pub const HAS_FX: bool = cfg!(windows);
