//! Efectos sobre el escritorio de Windows:
//!
//! - **Franja de lectura**: dos velos (arriba y abajo) que dejan libre una franja
//!   horizontal alrededor del ratón.
//! - **Velo del fondo**: un velo por pantalla con un hueco donde está la ventana activa.
//! - **Prisma**: una ventana del sistema de lupa (Magnification API) puesta encima de
//!   otra ventana, que la muestra invertida (oscuro) o en grises.
//!
//! Todos los velos dejan pasar los clics (WS_EX_TRANSPARENT) y no roban el foco.

use super::wide;
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, EnumDisplayMonitors, FillRect, GetMonitorInfoW,
    InvalidateRect, MonitorFromPoint, MonitorFromWindow, SetWindowRgn, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, RGN_DIFF,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Magnification::{
    MagInitialize, MagSetColorEffect, MagSetWindowSource, MagUninitialize, MAGCOLOREFFECT, WC_MAGNIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetAncestor, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetWindow, GetWindowLongPtrW, IsIconic, IsWindow, IsWindowVisible, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WindowFromPoint, GA_ROOT, GWLP_USERDATA, GW_HWNDPREV,
    HMENU, HWND_TOP, HWND_TOPMOST, LWA_ALPHA, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE, WM_ERASEBKGND,
    WNDCLASSW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP, WS_VISIBLE,
};

fn h(x: isize) -> HWND {
    HWND(x as *mut core::ffi::c_void)
}

// ------------------------------------------------------------ ventanas velo

/// Las ventanas velo se pintan con el color guardado en GWLP_USERDATA.
unsafe extern "system" fn veil_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_ERASEBKGND {
        let color = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32;
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let brush = CreateSolidBrush(COLORREF(color));
        FillRect(HDC(wp.0 as *mut core::ffi::c_void), &r, brush);
        let _ = DeleteObject(brush);
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn register_classes() {
    thread_local!(static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let veil = WNDCLASSW {
            lpfnWndProc: Some(veil_proc),
            hInstance: module.into(),
            lpszClassName: w!("CareOurEyesVeil"),
            ..Default::default()
        };
        RegisterClassW(&veil);
        let host = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: module.into(),
            lpszClassName: w!("CareOurEyesPrismaHost"),
            ..Default::default()
        };
        RegisterClassW(&host);
    }
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Un velo: ventana emergente, siempre encima, transparente a los clics.
fn new_veil() -> Option<isize> {
    register_classes();
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let title = wide("CareOurEyes");
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            w!("CareOurEyesVeil"),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            HWND::default(),
            HMENU::default(),
            module,
            None,
        )
        .ok()?;
        Some(hwnd.0 as isize)
    }
}

/// Coloca un velo con su color, opacidad (0..255) y, si hace falta, un hueco
/// (en coordenadas de pantalla).
fn place_veil(veil: isize, r: RECT, color: u32, alpha: u8, hole: Option<RECT>) {
    unsafe {
        let hwnd = h(veil);
        let old = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, color as isize);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        let (w, ht) = (r.right - r.left, r.bottom - r.top);
        // La región se recalcula siempre: con hueco o entera.
        let rgn = CreateRectRgn(0, 0, w, ht);
        if let Some(hole) = hole {
            let cut = CreateRectRgn(hole.left - r.left, hole.top - r.top, hole.right - r.left, hole.bottom - r.top);
            CombineRgn(rgn, rgn, cut, RGN_DIFF);
            let _ = DeleteObject(cut);
        }
        // SetWindowRgn se queda con la región: no se borra aquí.
        SetWindowRgn(hwnd, rgn, TRUE);
        // Sin cambiar el orden: el velo nació «siempre encima» y así no tapa los
        // avisos y barras de CareOurEyes que aparezcan después.
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, r.left, r.top, w, ht, SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOZORDER);
        if old != color {
            let _ = InvalidateRect(hwnd, None, TRUE);
        }
    }
}

fn hide(x: isize) {
    unsafe {
        let _ = ShowWindow(h(x), SW_HIDE);
    }
}

fn monitor_of(hmon: HMONITOR, work: bool) -> Option<RECT> {
    unsafe {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(hmon, &mut mi).as_bool().then_some(if work { mi.rcWork } else { mi.rcMonitor })
    }
}

unsafe extern "system" fn collect_monitors(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<isize>);
    list.push(hmon.0 as isize);
    TRUE
}

fn all_monitors() -> Vec<HMONITOR> {
    let mut list: Vec<isize> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(collect_monitors), LPARAM(&mut list as *mut _ as isize));
    }
    list.into_iter().map(|m| HMONITOR(m as *mut core::ffi::c_void)).collect()
}

/// Rectángulo visible de una ventana (sin la sombra invisible de Windows 10/11).
fn frame_of(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut r = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32).is_ok() {
            return Some(r);
        }
        let mut r = RECT::default();
        windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut r).ok().map(|_| r)
    }
}

// ------------------------------------------------------------ franja y velo

#[derive(Default)]
struct Veils {
    band: [Option<isize>; 2],
    veils: Vec<isize>,
}

thread_local! {
    static VEILS: RefCell<Veils> = RefCell::new(Veils::default());
}

pub struct Band {
    /// Alto en píxeles físicos.
    pub height: i32,
    pub alpha: u8,
    pub color: u32,
}

/// Franja de lectura: libre alrededor del ratón, velada arriba y abajo.
pub fn band(spec: Option<Band>) {
    VEILS.with(|v| {
        let mut v = v.borrow_mut();
        let Some(spec) = spec else {
            for x in v.band.iter().flatten() {
                hide(*x);
            }
            return;
        };
        for slot in v.band.iter_mut() {
            if slot.is_none() {
                *slot = new_veil();
            }
        }
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let Some(m) = monitor_of(unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST) }, false) else { return };
        let half = spec.height.max(20) / 2;
        let top = RECT { left: m.left, top: m.top, right: m.right, bottom: (p.y - half).max(m.top) };
        let bottom = RECT { left: m.left, top: (p.y + half).min(m.bottom), right: m.right, bottom: m.bottom };
        for (slot, r) in v.band.iter().zip([top, bottom]) {
            if let Some(x) = slot {
                if r.bottom - r.top > 0 {
                    place_veil(*x, r, spec.color, spec.alpha, None);
                } else {
                    hide(*x);
                }
            }
        }
    });
}

pub struct Veil {
    pub alpha: u8,
    pub color: u32,
    pub taskbar: bool,
    pub current_monitor_only: bool,
}

/// Velo del fondo: cubre las pantallas menos la ventana activa.
pub fn veil(spec: Option<Veil>) {
    VEILS.with(|v| {
        let mut v = v.borrow_mut();
        let Some(spec) = spec else {
            for x in &v.veils {
                hide(*x);
            }
            return;
        };
        let fg = unsafe { GetForegroundWindow() };
        let hole = if fg.is_invalid() { None } else { frame_of(fg) };
        let monitors: Vec<HMONITOR> = if spec.current_monitor_only {
            vec![unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) }]
        } else {
            all_monitors()
        };
        while v.veils.len() < monitors.len() {
            match new_veil() {
                Some(x) => v.veils.push(x),
                None => return,
            }
        }
        for (i, x) in v.veils.iter().enumerate() {
            match monitors.get(i).and_then(|m| monitor_of(*m, !spec.taskbar)) {
                Some(r) => place_veil(*x, r, spec.color, spec.alpha, hole),
                None => hide(*x),
            }
        }
    });
}

pub fn destroy_veils() {
    VEILS.with(|v| {
        let mut v = v.borrow_mut();
        let mut all: Vec<isize> = v.band.iter_mut().filter_map(|s| s.take()).collect();
        all.extend(v.veils.drain(..));
        for x in all {
            unsafe {
                let _ = DestroyWindow(h(x));
            }
        }
    });
}

// ------------------------------------------------------------ Prisma

/// Matrices de color 5×5 (filas = canal de entrada R, G, B, A, constante).
/// «Oscuro» invierte la luminosidad manteniendo más o menos el tono.
const DARK: [f32; 25] = [
    0.333, -0.667, -0.667, 0.0, 0.0, //
    -0.667, 0.333, -0.667, 0.0, 0.0, //
    -0.667, -0.667, 0.333, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0, //
    1.0, 1.0, 1.0, 0.0, 1.0,
];
/// «Gris»: luminancia (pesos de Rec. 601).
const GRAY: [f32; 25] = [
    0.299, 0.299, 0.299, 0.0, 0.0, //
    0.587, 0.587, 0.587, 0.0, 0.0, //
    0.114, 0.114, 0.114, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 0.0, 1.0,
];

#[derive(Default)]
struct Prisma {
    ready: bool,
    host: isize,
    lens: isize,
    target: isize,
    gray: bool,
}

thread_local! {
    static PRISMA: RefCell<Prisma> = RefCell::new(Prisma::default());
}

fn ensure_lens(p: &mut Prisma) -> bool {
    if p.ready {
        return true;
    }
    register_classes();
    unsafe {
        if !MagInitialize().as_bool() {
            return false;
        }
        let Ok(module) = GetModuleHandleW(None) else { return false };
        let title = wide("CareOurEyes Prisma");
        let Ok(host) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("CareOurEyesPrismaHost"),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            HWND::default(),
            HMENU::default(),
            module,
            None,
        ) else {
            return false;
        };
        let _ = SetLayeredWindowAttributes(host, COLORREF(0), 255, LWA_ALPHA);
        let Ok(lens) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WC_MAGNIFIER,
            w!("lens"),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            1,
            1,
            host,
            HMENU::default(),
            module,
            None,
        ) else {
            let _ = DestroyWindow(host);
            return false;
        };
        p.host = host.0 as isize;
        p.lens = lens.0 as isize;
        p.ready = true;
    }
    true
}

fn set_effect(p: &Prisma) {
    let mut effect = MAGCOLOREFFECT { transform: if p.gray { GRAY } else { DARK } };
    unsafe {
        let _ = MagSetColorEffect(h(p.lens), &mut effect);
    }
}

/// Ventana principal (raíz) bajo el ratón y su marco, si es de otro programa.
pub fn window_under_cursor() -> Option<(isize, RECT)> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let root = GetAncestor(WindowFromPoint(pt), GA_ROOT);
        if root.is_invalid() || !acceptable(root) {
            return None;
        }
        frame_of(root).map(|r| (root.0 as isize, r))
    }
}

pub fn cursor() -> (i32, i32) {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    (pt.x, pt.y)
}

pub fn foreground_root() -> isize {
    unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT).0 as isize }
}

/// Ventanas a las que se puede aplicar: visibles, de otro programa, no el escritorio.
fn acceptable(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let mut pid = 0u32;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class).max(0) as usize;
        let name = String::from_utf16_lossy(&class[..n]);
        !matches!(name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
    }
}

/// Aplica Prisma a una ventana. Elegir lo mismo otra vez lo quita.
/// Devuelve `true` si quedó aplicado.
pub fn prisma_toggle(target: isize, gray: bool) -> bool {
    if target == 0 || !acceptable(h(target)) {
        return false;
    }
    PRISMA.with(|p| {
        let mut p = p.borrow_mut();
        if p.target == target && p.gray == gray {
            clear(&mut p);
            return false;
        }
        if !ensure_lens(&mut p) {
            return false;
        }
        p.target = target;
        p.gray = gray;
        set_effect(&p);
        true
    })
}

fn clear(p: &mut Prisma) {
    p.target = 0;
    if p.ready {
        hide(p.host);
    }
}

pub fn prisma_clear() {
    PRISMA.with(|p| clear(&mut p.borrow_mut()));
}

/// Ventana con Prisma, o 0.
pub fn prisma_target() -> (isize, bool) {
    PRISMA.with(|p| {
        let p = p.borrow();
        (p.target, p.gray)
    })
}

/// Sigue a la ventana: posición, tamaño, orden y contenido. Llamar a menudo.
pub fn prisma_follow() {
    PRISMA.with(|p| {
        let mut p = p.borrow_mut();
        if p.target == 0 || !p.ready {
            return;
        }
        let target = h(p.target);
        unsafe {
            if !IsWindow(target).as_bool() {
                clear(&mut p);
                return;
            }
            if !IsWindowVisible(target).as_bool() || IsIconic(target).as_bool() {
                hide(p.host);
                return;
            }
            let Some(r) = frame_of(target) else { return };
            let (w, ht) = (r.right - r.left, r.bottom - r.top);
            // Justo encima de la ventana: lo que la tape la sigue tapando.
            // Si la lente ya está justo encima, no se toca el orden: mandarla arriba del
            // todo la pondría sobre otras ventanas (p. ej. esta app) y en el siguiente
            // paso volvería a bajar, y así sin parar (parpadeo).
            let above = GetWindow(target, GW_HWNDPREV).unwrap_or_default();
            let (after, keep) = if above.0 as isize == p.host {
                (HWND::default(), SWP_NOZORDER)
            } else if above.is_invalid() {
                (HWND_TOP, SET_WINDOW_POS_FLAGS(0))
            } else {
                (above, SET_WINDOW_POS_FLAGS(0))
            };
            let _ = SetWindowPos(h(p.host), after, r.left, r.top, w, ht, SWP_NOACTIVATE | SWP_SHOWWINDOW | keep);
            let _ = SetWindowPos(h(p.lens), HWND::default(), 0, 0, w, ht, SWP_NOACTIVATE);
            let _ = MagSetWindowSource(h(p.lens), r);
            let _ = InvalidateRect(h(p.lens), None, TRUE);
        }
    });
}

pub fn prisma_shutdown() {
    PRISMA.with(|p| {
        let mut p = p.borrow_mut();
        if p.ready {
            unsafe {
                let _ = DestroyWindow(h(p.host));
                let _ = MagUninitialize();
            }
        }
        *p = Prisma::default();
    });
}
