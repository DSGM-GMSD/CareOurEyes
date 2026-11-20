//! Todo lo que depende del sistema operativo.
//!
//! `Backend` escribe brillo y color en las pantallas. `Shell` agrupa lo demás
//! (bandeja, autoarranque, abrir enlaces…). Por ahora solo hay versión para
//! Windows; en otros sistemas se usa un simulador para poder probar la interfaz.

#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub mod dummy;

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
    #[cfg(not(windows))]
    {
        Box::new(dummy::Simulated::default())
    }
}

/// Abre un enlace en el navegador.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    windows::shell::open(url);
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
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
    #[cfg(not(windows))]
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
    #[cfg(not(windows))]
    {
        true
    }
}
