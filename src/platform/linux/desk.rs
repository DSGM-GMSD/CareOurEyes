//! Ventanas en X11: posición en la pantalla principal (sin paneles), avisos que
//! no roban el foco y siempre encima.

use super::x::x;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask};

/// Id X11 de una ventana de Slint (Xlib o XCB).
pub fn xid(window: &slint::Window) -> Option<u32> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => Some(h.window as u32),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

fn atom(name: &str) -> u32 {
    let Some(x) = x() else { return 0 };
    x.conn.intern_atom(false, name.as_bytes()).ok().and_then(|c| c.reply().ok()).map(|r| r.atom).unwrap_or(0)
}

/// Pantalla principal en píxeles: (x, y, ancho, alto).
pub fn primary_rect() -> Option<(i32, i32, i32, i32)> {
    let x = x()?;
    let res = x.conn.randr_get_screen_resources_current(x.root).ok()?.reply().ok()?;
    let primary = x.conn.randr_get_output_primary(x.root).ok()?.reply().ok()?.output;
    let mut crtcs: Vec<u32> = Vec::new();
    if primary != 0 {
        if let Some(oi) = x.conn.randr_get_output_info(primary, res.config_timestamp).ok().and_then(|c| c.reply().ok()) {
            if oi.crtc != 0 {
                crtcs.push(oi.crtc);
            }
        }
    }
    crtcs.extend(res.crtcs.iter().copied());
    crtcs.into_iter().find_map(|c| {
        let ci = x.conn.randr_get_crtc_info(c, res.config_timestamp).ok()?.reply().ok()?;
        (ci.width > 0).then_some((ci.x as i32, ci.y as i32, ci.width as i32, ci.height as i32))
    })
}

/// La pantalla principal sin los paneles del escritorio (_NET_WORKAREA).
pub fn work_rect() -> Option<(i32, i32, i32, i32)> {
    let (px, py, pw, ph) = primary_rect()?;
    let x = x()?;
    let area: Vec<u32> = x
        .conn
        .get_property(false, x.root, atom("_NET_WORKAREA"), AtomEnum::CARDINAL, 0, 4)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default();
    if area.len() < 4 {
        return Some((px, py, pw, ph));
    }
    // El área de trabajo abarca todo el escritorio: se recorta a la principal.
    let (ax, ay, aw, ah) = (area[0] as i32, area[1] as i32, area[2] as i32, area[3] as i32);
    let left = px.max(ax);
    let top = py.max(ay);
    let right = (px + pw).min(ax + aw);
    let bottom = (py + ph).min(ay + ah);
    if right - left < 200 || bottom - top < 200 {
        return Some((px, py, pw, ph));
    }
    Some((left, top, right - left, bottom - top))
}

fn send_wm(window: u32, kind: &str, data: [u32; 5]) {
    let Some(x) = x() else { return };
    let ev = ClientMessageEvent::new(32, window, atom(kind), data);
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    if let Ok(c) = x.conn.send_event(false, x.root, mask, ev) {
        let _ = c.check();
    }
    let _ = x.conn.flush();
}

/// Ventana activa ahora (0 si no se sabe).
pub fn active_window() -> u32 {
    let Some(x) = x() else { return 0 };
    x.conn
        .get_property(false, x.root, atom("_NET_ACTIVE_WINDOW"), AtomEnum::WINDOW, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().and_then(|mut v| v.next()))
        .unwrap_or(0)
}

/// Pide al gestor de ventanas que active otra ventana (devolver el foco).
pub fn activate(window: u32) {
    if window != 0 {
        // 2 = petición de un «pager»: los gestores la respetan siempre.
        send_wm(window, "_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0]);
    }
}

/// Encima de todo y sin botón en el panel ni en el selector de ventanas.
pub fn make_toast(window: u32) {
    const ADD: u32 = 1;
    for (a, b) in [("_NET_WM_STATE_ABOVE", "_NET_WM_STATE_SKIP_TASKBAR"), ("_NET_WM_STATE_SKIP_PAGER", "")] {
        let second = if b.is_empty() { 0 } else { atom(b) };
        send_wm(window, "_NET_WM_STATE", [ADD, atom(a), second, 1, 0]);
    }
}

/// Antes de volver a mostrar una ventana oculta: le dice al gestor de ventanas
/// que la posición la eligió la persona (USPosition en WM_NORMAL_HINTS) y la deja
/// ahí, para que aparezca directamente en su sitio en lugar de centrarse primero.
pub fn pin_position(window: u32, px: i32, py: i32) {
    use x11rb::protocol::xproto::{ConfigureWindowAux, PropMode};
    use x11rb::wrapper::ConnectionExt as _;
    let Some(x) = x() else { return };
    const US_POSITION: u32 = 1;
    const P_POSITION: u32 = 4;
    let mut hints: Vec<u32> = x
        .conn
        .get_property(false, window, AtomEnum::WM_NORMAL_HINTS, AtomEnum::WM_SIZE_HINTS, 0, 18)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default();
    hints.resize(18, 0);
    hints[0] |= US_POSITION | P_POSITION;
    hints[1] = px as u32;
    hints[2] = py as u32;
    if let Ok(c) = x.conn.change_property32(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NORMAL_HINTS,
        AtomEnum::WM_SIZE_HINTS,
        &hints,
    ) {
        let _ = c.check();
    }
    if let Ok(c) = x.conn.configure_window(window, &ConfigureWindowAux::new().x(px).y(py)) {
        let _ = c.check();
    }
    let _ = x.conn.flush();
}
