//! Inactividad, app activa, teclas pulsadas, sonido y área útil de la pantalla.

use super::cg::*;
use crate::hotkeys::{ALT, CTRL, SHIFT, WIN};
use crate::platform::Foreground;

pub fn idle_ms() -> i64 {
    let secs = unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION, ANY_INPUT) };
    (secs * 1000.0).max(0.0) as i64
}

/// App en primer plano. macOS no deja leer el título de sus ventanas sin permiso
/// de accesibilidad: como «título» va el nombre de la app.
pub fn foreground_info() -> Option<Foreground> {
    let ws = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    let name = app.localizedName().map(|s| s.to_string()).unwrap_or_default();
    let bundle = app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default();
    let exe = app
        .executableURL()
        .and_then(|u| u.lastPathComponent())
        .map(|s| s.to_string())
        .unwrap_or_else(|| name.clone());
    let pid = app.processIdentifier();
    Some(Foreground { process: exe, class: bundle, title: name, own: pid as u32 == std::process::id(), fullscreen: false })
}

/// Pantalla principal sin la barra de menús ni el Dock, en puntos y con el
/// origen arriba a la izquierda: (x, y, ancho, alto).
pub fn work_rect() -> Option<(f64, f64, f64, f64)> {
    let mtm = objc2::MainThreadMarker::new()?;
    let screens = objc2_app_kit::NSScreen::screens(mtm);
    // La primera es la principal (la de la barra de menús).
    let main = screens.firstObject()?;
    let full = main.frame();
    let vis = main.visibleFrame();
    let top = full.size.height - (vis.origin.y + vis.size.height);
    Some((vis.origin.x, top, vis.size.width, vis.size.height))
}

/// Pantalla principal completa, en puntos: (x, y, ancho, alto).
pub fn primary_rect() -> (f64, f64, f64, f64) {
    let r = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

pub fn chime() {
    let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").spawn();
}

/// Teclas de macOS (kVK_*) y su código virtual de Windows, que es el que se guarda.
pub const KEYS: &[(u16, u32)] = &[
    (0x00, 0x41), (0x0B, 0x42), (0x08, 0x43), (0x02, 0x44), (0x0E, 0x45), (0x03, 0x46), (0x05, 0x47),
    (0x04, 0x48), (0x22, 0x49), (0x26, 0x4A), (0x28, 0x4B), (0x25, 0x4C), (0x2E, 0x4D), (0x2D, 0x4E),
    (0x1F, 0x4F), (0x23, 0x50), (0x0C, 0x51), (0x0F, 0x52), (0x01, 0x53), (0x11, 0x54), (0x20, 0x55),
    (0x09, 0x56), (0x0D, 0x57), (0x07, 0x58), (0x10, 0x59), (0x06, 0x5A),
    (0x1D, 0x30), (0x12, 0x31), (0x13, 0x32), (0x14, 0x33), (0x15, 0x34), (0x17, 0x35), (0x16, 0x36),
    (0x1A, 0x37), (0x1C, 0x38), (0x19, 0x39),
    (0x7A, 0x70), (0x78, 0x71), (0x63, 0x72), (0x76, 0x73), (0x60, 0x74), (0x61, 0x75), (0x62, 0x76),
    (0x64, 0x77), (0x65, 0x78), (0x6D, 0x79), (0x67, 0x7A), (0x6F, 0x7B), (0x69, 0x7C), (0x6B, 0x7D),
    (0x71, 0x7E), (0x6A, 0x7F), (0x40, 0x80), (0x4F, 0x81), (0x50, 0x82), (0x5A, 0x83),
    (0x52, 0x60), (0x53, 0x61), (0x54, 0x62), (0x55, 0x63), (0x56, 0x64), (0x57, 0x65), (0x58, 0x66),
    (0x59, 0x67), (0x5B, 0x68), (0x5C, 0x69), (0x43, 0x6A), (0x45, 0x6B), (0x4E, 0x6D), (0x4B, 0x6F),
    (0x7B, 0x25), (0x7E, 0x26), (0x7C, 0x27), (0x7D, 0x28), (0x74, 0x21), (0x79, 0x22), (0x77, 0x23),
    (0x73, 0x24), (0x72, 0x2D), (0x75, 0x2E), (0x31, 0x20), (0x30, 0x09),
    (0x18, 0xBB), (0x2B, 0xBC), (0x1B, 0xBD), (0x2F, 0xBE),
];

/// Tecla (no modificador) pulsada ahora, como código virtual de Windows. Con
/// Option, el texto de la tecla es otro símbolo (Option+H = «˙»); esto no.
pub fn pressed_key() -> Option<u32> {
    KEYS.iter().find(|(k, _)| unsafe { CGEventSourceKeyState(COMBINED_SESSION, *k) }).map(|(_, vk)| *vk)
}

/// Ctrl, Option (Alt), Mayús y Cmd (Win) pulsados ahora.
pub fn held_mods() -> u32 {
    let f = unsafe { CGEventSourceFlagsState(COMBINED_SESSION) };
    (if f & 0x40000 != 0 { CTRL } else { 0 })
        | (if f & 0x80000 != 0 { ALT } else { 0 })
        | (if f & 0x20000 != 0 { SHIFT } else { 0 })
        | (if f & 0x100000 != 0 { WIN } else { 0 })
}
