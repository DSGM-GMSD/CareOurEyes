//! Conexión X11 compartida: inactividad, ventana activa, atajos globales y
//! avisos de cambio de pantallas.

use super::emit;
use crate::hotkeys::{ALT, CTRL, SHIFT, WIN};
use crate::platform::{Foreground, TrayEvent};
use std::sync::{Arc, Mutex, OnceLock};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, NotifyMask};
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, GrabMode, ModMask, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X {
    pub conn: RustConnection,
    pub root: Window,
    active_window: Atom,
    wm_state: Atom,
    fullscreen: Atom,
    wm_pid: Atom,
    wm_name: Atom,
    utf8: Atom,
    /// (id del atajo, keycode, máscara X) registrados ahora.
    grabs: Mutex<Vec<(i32, u8, u16)>>,
}

static XCONN: OnceLock<Option<Arc<X>>> = OnceLock::new();

fn atom(conn: &RustConnection, name: &str) -> Atom {
    conn.intern_atom(false, name.as_bytes())
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.atom)
        .unwrap_or(0)
}

/// La conexión al servidor X, si hay uno (también XWayland).
pub fn x() -> Option<Arc<X>> {
    XCONN
        .get_or_init(|| {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = conn.setup().roots.get(screen)?.root;
            let x = Arc::new(X {
                active_window: atom(&conn, "_NET_ACTIVE_WINDOW"),
                wm_state: atom(&conn, "_NET_WM_STATE"),
                fullscreen: atom(&conn, "_NET_WM_STATE_FULLSCREEN"),
                wm_pid: atom(&conn, "_NET_WM_PID"),
                wm_name: atom(&conn, "_NET_WM_NAME"),
                utf8: atom(&conn, "UTF8_STRING"),
                grabs: Mutex::new(Vec::new()),
                conn,
                root,
            });
            // Avisos de pantallas conectadas o cambiadas.
            if let Ok(c) = x.conn.randr_select_input(root, NotifyMask::SCREEN_CHANGE | NotifyMask::OUTPUT_CHANGE) {
                let _ = c.check();
            }
            let reader = x.clone();
            std::thread::Builder::new().name("x11-events".into()).spawn(move || events(reader)).ok()?;
            Some(x)
        })
        .clone()
}

/// ¿La sesión es Wayland sin X? (entonces no hay gamma ni atajos globales).
pub fn wayland_only() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none()
}

fn events(x: Arc<X>) {
    let ignored = u16::from(ModMask::LOCK | ModMask::M2);
    loop {
        let Ok(ev) = x.conn.wait_for_event() else { return };
        match ev {
            Event::KeyPress(k) => {
                let state = u16::from(k.state) & !ignored & 0xFF;
                let id = x.grabs.lock().ok().and_then(|g| {
                    g.iter().find(|(_, code, mask)| *code == k.detail && *mask == state).map(|(id, _, _)| *id)
                });
                if let Some(id) = id {
                    emit(TrayEvent::Hotkey(id));
                }
            }
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => emit(TrayEvent::DisplaysChanged),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ inactividad

pub fn idle_ms() -> i64 {
    let Some(x) = x() else { return 0 };
    x.conn
        .screensaver_query_info(x.root)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.ms_since_user_input as i64)
        .unwrap_or(0)
}

// ------------------------------------------------------------------ ventana activa

fn prop32(x: &X, w: Window, prop: Atom, kind: impl Into<Atom>) -> Vec<u32> {
    x.conn
        .get_property(false, w, prop, kind, 0, 64)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}

fn prop_text(x: &X, w: Window, prop: impl Into<Atom>, kind: impl Into<Atom>) -> Vec<u8> {
    x.conn
        .get_property(false, w, prop, kind, 0, 1024)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.value)
        .unwrap_or_default()
}

fn active(x: &X) -> Option<Window> {
    prop32(x, x.root, x.active_window, AtomEnum::WINDOW).first().copied().filter(|&w| w != 0)
}

fn is_fullscreen(x: &X, w: Window) -> bool {
    prop32(x, w, x.wm_state, AtomEnum::ATOM).contains(&x.fullscreen)
}

pub fn foreground_fullscreen() -> bool {
    let Some(x) = x() else { return false };
    active(&x).is_some_and(|w| is_fullscreen(&x, w))
}

pub fn foreground_info() -> Option<Foreground> {
    let x = x()?;
    let w = active(&x)?;
    let pid = prop32(&x, w, x.wm_pid, AtomEnum::CARDINAL).first().copied().unwrap_or(0);
    let process = if pid != 0 {
        std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .or_else(|| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string()))
            .unwrap_or_default()
    } else {
        String::new()
    };
    // WM_CLASS = "instancia\0Clase\0": se usa la clase.
    let class_raw = prop_text(&x, w, AtomEnum::WM_CLASS, AtomEnum::STRING);
    let class = class_raw
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .last()
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .unwrap_or_default();
    let mut title = String::from_utf8_lossy(&prop_text(&x, w, x.wm_name, x.utf8)).into_owned();
    if title.is_empty() {
        title = String::from_utf8_lossy(&prop_text(&x, w, AtomEnum::WM_NAME, AtomEnum::STRING)).into_owned();
    }
    Some(Foreground {
        process,
        class,
        title,
        own: pid == std::process::id(),
        fullscreen: is_fullscreen(&x, w),
    })
}

// ------------------------------------------------------------------ atajos

/// Código virtual de Windows → keysym de X11.
fn keysym(vk: u32) -> Option<u32> {
    Some(match vk {
        0x41..=0x5A => vk + 0x20, // a..z
        0x30..=0x39 => vk,
        0x70..=0x87 => 0xFFBE + (vk - 0x70), // F1..F24
        0x60..=0x69 => 0xFFB0 + (vk - 0x60), // teclado numérico
        0x25 => 0xFF51,
        0x26 => 0xFF52,
        0x27 => 0xFF53,
        0x28 => 0xFF54,
        0x21 => 0xFF55,
        0x22 => 0xFF56,
        0x23 => 0xFF57,
        0x24 => 0xFF50,
        0x2D => 0xFF63,
        0x2E => 0xFFFF,
        0x20 => 0x20,
        0x09 => 0xFF09,
        0x6A => 0xFFAA,
        0x6B => 0xFFAB,
        0x6D => 0xFFAD,
        0x6F => 0xFFAF,
        0xBB => 0x2B,
        0xBC => 0x2C,
        0xBD => 0x2D,
        0xBE => 0x2E,
        _ => return None,
    })
}

fn keycode(x: &X, sym: u32) -> Option<u8> {
    let setup = x.conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = x.conn.get_keyboard_mapping(min, max - min + 1).ok()?.reply().ok()?;
    let per = map.keysyms_per_keycode.max(1) as usize;
    map.keysyms.chunks(per).position(|syms| syms.contains(&sym)).map(|i| min + i as u8)
}

fn x_mask(mods: u32) -> u16 {
    let mut m = 0u16;
    if mods & SHIFT != 0 {
        m |= u16::from(ModMask::SHIFT);
    }
    if mods & CTRL != 0 {
        m |= u16::from(ModMask::CONTROL);
    }
    if mods & ALT != 0 {
        m |= u16::from(ModMask::M1);
    }
    if mods & WIN != 0 {
        m |= u16::from(ModMask::M4);
    }
    m
}

/// Registra los atajos (número, modificadores, tecla virtual). Devuelve los que fallaron.
pub fn register_hotkeys(list: &[(i32, u32, u32)]) -> Vec<i32> {
    let Some(x) = x() else { return list.iter().map(|h| h.0).collect() };
    let Ok(mut grabs) = x.grabs.lock() else { return Vec::new() };
    let extras = [0u16, u16::from(ModMask::LOCK), u16::from(ModMask::M2), u16::from(ModMask::LOCK | ModMask::M2)];
    for (_, code, mask) in grabs.drain(..) {
        for e in extras {
            if let Ok(c) = x.conn.ungrab_key(code, x.root, ModMask::from(mask | e)) {
                let _ = c.check();
            }
        }
    }
    let mut failed = Vec::new();
    for &(id, mods, vk) in list {
        let Some(code) = keysym(vk).and_then(|s| keycode(&x, s)) else {
            failed.push(id);
            continue;
        };
        let mask = x_mask(mods);
        // También con Bloq Mayús y Bloq Num puestos.
        let ok = extras.iter().all(|&e| {
            x.conn
                .grab_key(true, x.root, ModMask::from(mask | e), code, GrabMode::ASYNC, GrabMode::ASYNC)
                .ok()
                .is_some_and(|c| c.check().is_ok())
        });
        if ok {
            grabs.push((id, code, mask));
        } else {
            for e in extras {
                if let Ok(c) = x.conn.ungrab_key(code, x.root, ModMask::from(mask | e)) {
                    let _ = c.check();
                }
            }
            failed.push(id);
        }
    }
    failed
}
