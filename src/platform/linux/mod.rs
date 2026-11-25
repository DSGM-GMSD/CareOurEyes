//! Linux (X11): gamma por RandR, bandeja StatusNotifierItem, atajos globales,
//! inactividad y ventana activa. En Wayland puro no hay gamma (se usa el simulador
//! con el motor marcado como no disponible).

pub mod desk;
pub mod gamma;
pub mod shell;
pub mod tray;
pub mod x;

use crate::platform::TrayEvent;
use std::sync::OnceLock;

static HANDLER: OnceLock<fn(TrayEvent)> = OnceLock::new();

/// Quién recibe los avisos (bandeja, atajos, otra copia, pantallas).
pub fn set_handler(h: fn(TrayEvent)) {
    let _ = HANDLER.set(h);
}

/// Entrega un aviso al hilo de la interfaz (se puede llamar desde cualquier hilo).
pub fn emit(ev: TrayEvent) {
    if let Some(&h) = HANDLER.get() {
        let _ = slint::invoke_from_event_loop(move || h(ev));
    }
}
