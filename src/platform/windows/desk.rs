//! Lo que el temporizador de pausas necesita saber del escritorio y las
//! ventanas que muestra encima de todo (aviso y pausa a pantalla completa).

use super::wide;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM, BOOL, TRUE};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HBRUSH, HDC, HMONITOR,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClassNameW, GetDesktopWindow, GetForegroundWindow,
    GetShellWindow, GetWindowLongPtrW, GetWindowRect, RegisterClassW, SetForegroundWindow,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HMENU,
    HWND_TOPMOST, LWA_ALPHA, MB_ICONASTERISK, SWP_NOACTIVATE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WNDCLASSW,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use std::cell::RefCell;

/// Milisegundos desde la última tecla o movimiento del ratón.
pub fn idle_ms() -> i64 {
    unsafe {
        let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        // Los dos son contadores de 32 bits que dan la vuelta cada ~49 días.
        GetTickCount().wrapping_sub(info.dwTime) as i64
    }
}

fn monitor_rect(hmon: HMONITOR, work: bool) -> Option<RECT> {
    unsafe {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(hmon, &mut mi).as_bool().then_some(if work { mi.rcWork } else { mi.rcMonitor })
    }
}

/// ¿La ventana activa ocupa toda su pantalla? (juegos, vídeo, presentaciones)
pub fn foreground_fullscreen() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || fg == GetDesktopWindow() || fg == GetShellWindow() {
            return false;
        }
        // El escritorio (iconos) también «ocupa» la pantalla, pero no cuenta.
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class).max(0) as usize;
        let name = String::from_utf16_lossy(&class[..n]);
        if name == "Progman" || name == "WorkerW" {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let Some(m) = monitor_rect(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), false) else { return false };
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// Sonido de aviso del sistema.
pub fn chime() {
    unsafe {
        let _ = MessageBeep(MB_ICONASTERISK);
    }
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn set_foreground(h: isize) {
    if h != 0 {
        unsafe {
            let _ = SetForegroundWindow(HWND(h as *mut core::ffi::c_void));
        }
    }
}

/// Ventana de aviso: siempre encima, sin botón en la barra de tareas y sin
/// robar el foco al pulsarla.
pub fn make_toast(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0) as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
    }
}

/// Coloca el aviso centrado arriba (o abajo) del área de trabajo principal.
pub fn place_toast(hwnd: HWND, top: bool, scale: f32) {
    unsafe {
        let Some(work) = monitor_rect(MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY), true) else { return };
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let margin = (16.0 * scale) as i32;
        let x = work.left + (work.right - work.left - w) / 2;
        let y = if top { work.top + margin } else { work.bottom - h - margin };
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Rectángulo (en píxeles) de la pantalla principal.
pub fn primary_rect() -> (i32, i32, i32, i32) {
    unsafe {
        monitor_rect(MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY), false)
            .map(|r| (r.left, r.top, r.right - r.left, r.bottom - r.top))
            .unwrap_or((0, 0, 1920, 1080))
    }
}

use crate::platform::Foreground;

pub fn foreground_info() -> Option<Foreground> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId};
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        let mut process = String::new();
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                process = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
            }
            let _ = CloseHandle(h);
        }
        let mut class = [0u16; 256];
        let n = GetClassNameW(fg, &mut class).max(0) as usize;
        let mut title = [0u16; 512];
        let t = GetWindowTextW(fg, &mut title).max(0) as usize;
        Some(Foreground {
            process,
            class: String::from_utf16_lossy(&class[..n]),
            title: String::from_utf16_lossy(&title[..t]),
            own: pid == std::process::id(),
            fullscreen: foreground_fullscreen(),
        })
    }
}

// ------------------------------------------------------------ otras pantallas

thread_local! {
    static COVERS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "system" fn cover_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn collect(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<RECT>);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    // MONITORINFOF_PRIMARY = 1: la principal ya tiene la ventana de la pausa.
    if GetMonitorInfoW(hmon, &mut mi).as_bool() && mi.dwFlags & 1 == 0 {
        list.push(mi.rcMonitor);
    }
    TRUE
}

/// Oscurece las pantallas que no son la principal durante una pausa.
pub fn cover_other_monitors() {
    uncover_other_monitors();
    unsafe {
        let mut rects: Vec<RECT> = Vec::new();
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(collect), LPARAM(&mut rects as *mut _ as isize));
        if rects.is_empty() {
            return;
        }
        let Ok(module) = GetModuleHandleW(None) else { return };
        let class = WNDCLASSW {
            lpfnWndProc: Some(cover_proc),
            hInstance: module.into(),
            lpszClassName: w!("CareOurEyesCover"),
            hbrBackground: HBRUSH(CreateSolidBrush(COLORREF(0x000D0A07)).0),
            ..Default::default()
        };
        RegisterClassW(&class);
        let title = wide("CareOurEyes");
        for r in rects {
            if let Ok(h) = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
                w!("CareOurEyesCover"),
                windows::core::PCWSTR(title.as_ptr()),
                WS_POPUP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                HWND::default(),
                HMENU::default(),
                module,
                None,
            ) {
                let _ = SetLayeredWindowAttributes(h, COLORREF(0), 235, LWA_ALPHA);
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                COVERS.with(|c| c.borrow_mut().push(h.0 as isize));
            }
        }
    }
}

pub fn uncover_other_monitors() {
    COVERS.with(|c| {
        for h in c.borrow_mut().drain(..) {
            unsafe {
                let _ = DestroyWindow(HWND(h as *mut core::ffi::c_void));
            }
        }
    });
}

fn down(vk: u32) -> bool {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(vk as i32) < 0 }
}

/// Tecla (no modificador) pulsada según el mensaje que se está atendiendo.
pub fn pressed_key() -> Option<u32> {
    let skip = |vk: u32| {
        matches!(vk, 0x01..=0x07 | 0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5 | 0xE5)
    };
    // Primero las teclas con nombre (letras, números, F, flechas…); luego el resto.
    (0x41..=0x5Au32)
        .chain(0x30..=0x39)
        .chain(0x70..=0x87)
        .chain(0x20..=0x2F)
        .chain(0x60..=0x6F)
        .chain(0x08..=0xFE)
        .find(|&vk| !skip(vk) && down(vk))
}

/// Ctrl, Alt, Mayús y Win pulsados según el mensaje que se está atendiendo.
pub fn held_mods() -> u32 {
    use crate::hotkeys::{ALT, CTRL, SHIFT, WIN};
    (if down(0x11) { CTRL } else { 0 })
        | (if down(0x12) { ALT } else { 0 })
        | (if down(0x10) { SHIFT } else { 0 })
        | (if down(0x5B) || down(0x5C) { WIN } else { 0 })
}
