//! Icono de la bandeja y avisos del sistema (reanudar, cambio de pantallas…).
//!
//! Una ventana oculta recibe los clics del icono y los mensajes de energía.
//! Su procedimiento corre dentro del bucle de eventos de Slint; para no
//! reentrar en él, cada aviso se entrega con un temporizador de 0 ms.

use super::shell;
use super::wide;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use windows::core::{w, GUID};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::RegisterPowerSettingNotification;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetSystemMetrics, RegisterClassW, RegisterWindowMessageW, HMENU,
    REGISTER_NOTIFICATION_FLAGS, SM_CXSMICON, WINDOW_EX_STYLE, WM_APP, WM_DISPLAYCHANGE, WM_LBUTTONUP,
    WM_POWERBROADCAST, WM_RBUTTONUP, WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPED,
};

const WM_TRAY: u32 = WM_APP + 1;
const PBT_APMRESUMESUSPEND: usize = 0x7;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
const PBT_POWERSETTINGCHANGE: usize = 0x8013;
const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);

#[derive(Clone, Copy, Debug)]
pub enum TrayEvent {
    /// Clic izquierdo en el icono.
    Open,
    /// Clic derecho, con la posición del cursor en píxeles de pantalla.
    Menu(i32, i32),
    /// El equipo volvió de suspensión o se encendió la pantalla.
    Resumed,
    /// Se conectó, quitó o cambió de resolución una pantalla.
    DisplaysChanged,
    /// Otra copia de la app pidió mostrar la ventana.
    ShowRequested,
    /// Cambió el tema claro/oscuro de Windows.
    ThemeChanged,
}

struct TrayState {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    taskbar_created: u32,
    show_msg: u32,
    handler: Rc<dyn Fn(TrayEvent)>,
}

thread_local! {
    static TRAY: RefCell<Option<TrayState>> = const { RefCell::new(None) };
}

fn emit(ev: TrayEvent) {
    let handler = TRAY.with(|t| t.borrow().as_ref().map(|s| s.handler.clone()));
    if let Some(h) = handler {
        slint::Timer::single_shot(Duration::ZERO, move || h(ev));
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let (taskbar_created, show_msg) =
        TRAY.with(|t| t.borrow().as_ref().map(|s| (s.taskbar_created, s.show_msg)).unwrap_or((0, 0)));
    match msg {
        WM_TRAY => match (lparam.0 as u32) & 0xFFFF {
            WM_LBUTTONUP => emit(TrayEvent::Open),
            WM_RBUTTONUP => {
                let (x, y) = shell::cursor();
                emit(TrayEvent::Menu(x, y));
            }
            _ => {}
        },
        WM_POWERBROADCAST => {
            match wparam.0 {
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => emit(TrayEvent::Resumed),
                PBT_POWERSETTINGCHANGE if lparam.0 != 0 => {
                    // POWERBROADCAST_SETTING: GUID (16 bytes), longitud (u32), datos.
                    let p = lparam.0 as *const u8;
                    let guid = std::ptr::read_unaligned(p as *const GUID);
                    let state = std::ptr::read_unaligned(p.add(20) as *const u32);
                    if guid == GUID_CONSOLE_DISPLAY_STATE && state == 1 {
                        emit(TrayEvent::Resumed);
                    }
                }
                _ => {}
            }
            return LRESULT(1);
        }
        WM_DISPLAYCHANGE => emit(TrayEvent::DisplaysChanged),
        WM_SETTINGCHANGE => {
            if lparam.0 != 0 {
                let text = windows::core::PCWSTR(lparam.0 as *const u16).to_string().unwrap_or_default();
                if text == "ImmersiveColorSet" {
                    emit(TrayEvent::ThemeChanged);
                }
            }
        }
        m if m != 0 && m == taskbar_created => {
            // El Explorador se reinició: hay que volver a poner el icono.
            TRAY.with(|t| {
                if let Some(s) = t.borrow().as_ref() {
                    let _ = Shell_NotifyIconW(NIM_ADD, &s.nid);
                }
            });
        }
        m if m != 0 && m == show_msg => emit(TrayEvent::ShowRequested),
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Crea el icono de la bandeja. `false` si no se pudo.
pub fn create(icon_png: &[u8], tooltip: &str, handler: impl Fn(TrayEvent) + 'static) -> bool {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return false };
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: module.into(),
            lpszClassName: w!("CareOurEyesTray"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("CareOurEyesTray"),
            w!("CareOurEyes"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            module,
            None,
        ) else {
            return false;
        };
        let _ = RegisterPowerSettingNotification(hwnd, &GUID_CONSOLE_DISPLAY_STATE, REGISTER_NOTIFICATION_FLAGS(0));

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            ..Default::default()
        };
        if let Some(icon) = shell::icon_from_png(icon_png, GetSystemMetrics(SM_CXSMICON)) {
            nid.hIcon = icon;
        }
        let tip = wide(tooltip);
        let n = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..n].copy_from_slice(&tip[..n]);

        let added = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
        TRAY.with(|t| {
            *t.borrow_mut() = Some(TrayState {
                hwnd,
                nid,
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                show_msg: shell::show_message(),
                handler: Rc::new(handler),
            })
        });
        added
    }
}

pub fn remove() {
    TRAY.with(|t| {
        if let Some(s) = t.borrow_mut().take() {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &s.nid);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(s.hwnd);
            }
        }
    });
}
