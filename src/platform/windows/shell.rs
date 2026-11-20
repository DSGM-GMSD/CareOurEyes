//! Integración con el escritorio de Windows: enlaces, autoarranque, tema,
//! instancia única y detalles de las ventanas sin marco.

use super::wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, GetClientRect, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, PostMessageW,
    RegisterWindowMessageW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, GWL_EXSTYLE, HICON, HTCAPTION, HTCLIENT, HWND_BROADCAST, HWND_TOPMOST, ICON_BIG, ICON_SMALL,
    LR_DEFAULTCOLOR, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_RESTORE, SW_SHOWNORMAL,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_NCHITTEST, WM_SETICON, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
/// Mismo nombre y argumento que la versión Kotlin.
const RUN_NAME: PCWSTR = w!("CareOurEyes");
pub const AUTOSTART_ARG: &str = "--autostart";

pub fn open(target: &str) {
    let t = wide(target);
    unsafe {
        ShellExecuteW(HWND::default(), w!("open"), PCWSTR(t.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

pub fn set_autostart(enabled: bool) -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None)
            != ERROR_SUCCESS
        {
            return false;
        }
        let ok = if enabled {
            let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            let value = wide(&format!("\"{exe}\" {AUTOSTART_ARG}"));
            let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
            RegSetValueExW(key, RUN_NAME, 0, REG_SZ, Some(bytes)) == ERROR_SUCCESS
        } else {
            let r = RegDeleteValueW(key, RUN_NAME);
            r == ERROR_SUCCESS || r.0 == 2 // ERROR_FILE_NOT_FOUND: ya no estaba
        };
        let _ = RegCloseKey(key);
        ok
    }
}

pub fn system_prefers_dark() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            0,
            KEY_READ,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return true;
        }
        let mut value: u32 = 1;
        let mut size = 4u32;
        let r = RegQueryValueExW(key, w!("AppsUseLightTheme"), None, None, Some(&mut value as *mut u32 as *mut u8), Some(&mut size));
        let _ = RegCloseKey(key);
        r != ERROR_SUCCESS || value == 0
    }
}

/// Mensaje con el que una segunda copia pide a la primera que se muestre.
pub fn show_message() -> u32 {
    unsafe { RegisterWindowMessageW(w!("CareOurEyes.Show")) }
}

/// `true` si es la primera copia en ejecución. Si no, avisa a la que ya corre.
pub fn single_instance() -> bool {
    unsafe {
        // El mutex se queda abierto mientras viva el proceso.
        let _mutex = CreateMutexW(None, true, w!("Local\\CareOurEyes.Rust"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = PostMessageW(HWND_BROADCAST, show_message(), WPARAM(0), LPARAM(0));
            return false;
        }
    }
    true
}

/// HWND de una ventana de Slint (solo existe después de `show()`).
pub fn hwnd(window: &slint::Window) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle();
    let raw = handle.window_handle().ok()?.as_raw();
    match raw {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

/// Esquinas redondeadas de Windows 11 (y su sombra) en una ventana sin marco.
/// `tool` = sin botón en la barra de tareas (el menú de la bandeja).
pub fn style_window(hwnd: HWND, tool: bool) {
    unsafe {
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&pref) as u32,
        );
        if tool {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        }
    }
}

/// Icono de la ventana (barra de tareas y Alt+Tab) a partir de un PNG.
pub fn set_icon(hwnd: HWND, png: &[u8]) {
    if let Some(icon) = icon_from_png(png, 0) {
        unsafe {
            SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_BIG as usize), LPARAM(icon.0 as isize));
            SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(icon.0 as isize));
        }
    }
}

pub fn icon_from_png(png: &[u8], size: i32) -> Option<HICON> {
    unsafe { CreateIconFromResourceEx(png, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR).ok() }
}

/// Hace que la franja de la barra de título (sin los botones) mueva la ventana,
/// como la barra de título normal de Windows. Lo resuelve Windows con
/// WM_NCHITTEST, así la interfaz nunca se queda «agarrada» tras arrastrar.
pub fn enable_title_drag(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(title_hit_test), 1, 0);
    }
}

unsafe extern "system" fn title_hit_test(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let result = DefSubclassProc(hwnd, msg, wparam, lparam);
    if msg == WM_NCHITTEST && result.0 == HTCLIENT as isize {
        let mut p = POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 };
        let _ = ScreenToClient(hwnd, &mut p);
        let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        // Barra de 40 px; a la derecha, 88 px para minimizar y cerrar.
        let title = (40.0 * scale) as i32;
        let buttons = (88.0 * scale) as i32;
        if p.y >= 0 && p.y < title && p.x < rc.right - buttons {
            return LRESULT(HTCAPTION as isize);
        }
    }
    result
}

fn work_area_at(p: POINT) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            return mi.rcWork;
        }
        let mut r = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        r
    }
}

fn window_size(hwnd: HWND) -> (i32, i32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    (r.right - r.left, r.bottom - r.top)
}

/// Pone la ventana en la esquina inferior derecha del área de trabajo principal.
pub fn place_bottom_right(hwnd: HWND) {
    let work = work_area_at(POINT { x: 0, y: 0 });
    let (w, h) = window_size(hwnd);
    unsafe {
        let _ = SetWindowPos(hwnd, HWND::default(), work.right - w, work.bottom - h, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// Coloca el menú de la bandeja junto al cursor, dentro de la pantalla, y le da el foco.
pub fn place_menu(hwnd: HWND, x: i32, y: i32) {
    let work = work_area_at(POINT { x, y });
    let (w, h) = window_size(hwnd);
    let left = (x - w).clamp(work.left, (work.right - w).max(work.left));
    let top = if y - h >= work.top { y - h } else { y.min(work.bottom - h) };
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, left, top, 0, 0, SWP_NOSIZE);
        let _ = SetForegroundWindow(hwnd);
    }
}

pub fn is_foreground(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}

pub fn bring_to_front(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}
