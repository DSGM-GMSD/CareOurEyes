//! macOS: gamma de CoreGraphics, icono en la barra de menús, atajos globales,
//! inactividad y app activa.

pub mod cg;
pub mod desk;
pub mod gamma;
pub mod keys;
pub mod shell;
pub mod tray;

use crate::platform::TrayEvent;
use std::sync::OnceLock;

static HANDLER: OnceLock<fn(TrayEvent)> = OnceLock::new();

/// Quién recibe los avisos (barra de menús, atajos, otra copia, pantallas).
pub fn set_handler(h: fn(TrayEvent)) {
    let _ = HANDLER.set(h);
}

/// Entrega un aviso al hilo de la interfaz (se puede llamar desde cualquier hilo).
pub fn emit(ev: TrayEvent) {
    if let Some(&h) = HANDLER.get() {
        let _ = slint::invoke_from_event_loop(move || h(ev));
    }
}
