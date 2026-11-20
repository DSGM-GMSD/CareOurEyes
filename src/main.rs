//! CareOurEyes — brillo y temperatura de color por software, con perfiles para
//! cuidar la vista. Versión en Rust + Slint.
//!
//! Copyright (C) 2026 DSGM-GMSD. Licencia GNU GPL v3 (ver LICENSE).

#![cfg_attr(windows, windows_subsystem = "windows")]
// Fuera de Windows solo hay simulador: parte del código no se usa.
#![cfg_attr(not(windows), allow(dead_code))]

mod app;
mod color;
mod config;
mod i18n;
mod net;
mod platform;
mod schedule;

pub mod ui {
    slint::include_modules!();
}

/// Guarda los fallos en `error.log`, junto a config.json. Nada sale del equipo.
fn install_panic_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = config::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let line = format!(
            "[{}] v{} {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            env!("CARGO_PKG_VERSION"),
            info
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("error.log")) {
            let _ = f.write_all(line.as_bytes());
        }
        default(info);
    }));
}

fn main() {
    install_panic_log();

    #[cfg(windows)]
    if !platform::windows::shell::single_instance() {
        return; // La copia que ya corre se mostrará sola.
    }

    let autostart = std::env::args().any(|a| a == "--autostart");
    if let Err(e) = app::run(autostart) {
        panic!("No se pudo abrir la interfaz: {e}");
    }
}
