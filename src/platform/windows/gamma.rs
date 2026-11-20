//! Brillo y temperatura en Windows mediante la rampa gamma de GDI.
//!
//! Cada pantalla tiene una tabla de 3 × 256 valores de 16 bits que la tarjeta
//! gráfica aplica a cada canal. Al abrir se guarda la tabla original (la
//! calibración del monitor) y los ajustes se calculan encima de ella.

use super::wide;
use crate::color;
use crate::platform::{Backend, DisplayInfo};
use std::collections::HashMap;
use windows::core::{s, w, PCWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{BOOL, ERROR_SUCCESS, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW,
    HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp};

const MONITORINFOF_PRIMARY: u32 = 1;

/// Variante de SetDeviceGammaRamp que exporta mscms.dll. No está documentada,
/// pero acepta rampas que la función pública rechaza (brillo muy bajo o colores
/// muy cálidos). Solo se usa si la pública falla y no se pidió «Ajuste estándar».
type InternalSetRamp = unsafe extern "system" fn(HDC, *const u16, u32) -> BOOL;

struct Screen {
    info: DisplayInfo,
    /// "\\.\DISPLAY1", terminado en 0.
    device: Vec<u16>,
    hdc: Option<HDC>,
    baseline: [u16; 768],
}

impl Screen {
    fn dc(&mut self) -> Option<HDC> {
        if self.hdc.is_none() {
            let hdc = unsafe { CreateDCW(w!("DISPLAY"), PCWSTR(self.device.as_ptr()), PCWSTR::null(), None) };
            if !hdc.is_invalid() {
                self.hdc = Some(hdc);
            }
        }
        self.hdc
    }

    fn release(&mut self) {
        if let Some(hdc) = self.hdc.take() {
            unsafe {
                let _ = DeleteDC(hdc);
            }
        }
    }
}

pub struct GdiGamma {
    screens: Vec<Screen>,
    /// Rampas originales por id. Se conservan aunque la pantalla se reconecte,
    /// para no tomar como «original» una rampa que ya tiene el filtro puesto.
    baselines: HashMap<String, [u16; 768]>,
    internal: Option<InternalSetRamp>,
}

impl GdiGamma {
    pub fn new() -> Self {
        let internal = unsafe {
            LoadLibraryW(w!("mscms.dll"))
                .ok()
                .and_then(|lib| GetProcAddress(lib, s!("InternalSetDeviceGammaRamp")))
                .map(|f| std::mem::transmute::<_, InternalSetRamp>(f))
        };
        Self { screens: Vec::new(), baselines: HashMap::new(), internal }
    }

    fn screen(&mut self, id: &str) -> Option<&mut Screen> {
        self.screens.iter_mut().find(|s| s.info.id == id)
    }

    fn write(&mut self, id: &str, ramp: &[u16; 768], standard: bool) -> bool {
        let internal = self.internal;
        let Some(screen) = self.screen(id) else { return false };
        let Some(hdc) = screen.dc() else { return false };
        unsafe {
            if SetDeviceGammaRamp(hdc, ramp.as_ptr() as *const _).as_bool() {
                return true;
            }
            if !standard {
                if let Some(f) = internal {
                    return f(hdc, ramp.as_ptr(), 0).as_bool();
                }
            }
        }
        false
    }
}

impl Drop for GdiGamma {
    fn drop(&mut self) {
        for s in &mut self.screens {
            s.release();
        }
    }
}

/// Una rampa leída al abrir puede traer el filtro de una sesión anterior que
/// terminó mal. Si el blanco de algún canal está muy por debajo del máximo,
/// o la curva no sube, se usa la rampa lineal.
fn sane_baseline(r: &[u16; 768]) -> bool {
    (0..3).all(|ch| {
        let c = &r[ch * 256..ch * 256 + 256];
        c[255] >= 0xF000 && c.windows(2).all(|p| p[1] >= p[0])
    })
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// "\\.\DISPLAY1" → nombre comercial del monitor ("AOC AG276QZD2"), si Windows lo sabe.
fn friendly_names() -> HashMap<String, String> {
    let mut out = HashMap::new();
    unsafe {
        let (mut np, mut nm) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != ERROR_SUCCESS {
            return out;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None)
            != ERROR_SUCCESS
        {
            return out;
        }
        paths.truncate(np as usize);
        for p in &paths {
            let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            src.header.adapterId = p.sourceInfo.adapterId;
            src.header.id = p.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                continue;
            }
            let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            tgt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            tgt.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            tgt.header.adapterId = p.targetInfo.adapterId;
            tgt.header.id = p.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut tgt.header) != 0 {
                continue;
            }
            let name = from_wide(&tgt.monitorFriendlyDeviceName);
            if !name.trim().is_empty() {
                out.insert(from_wide(&src.viewGdiDeviceName), name);
            }
        }
    }
    out
}

struct Found {
    device: String,
    width: i32,
    height: i32,
    primary: bool,
}

unsafe extern "system" fn on_monitor(hmon: HMONITOR, _hdc: HDC, _rc: *mut RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<Found>);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(hmon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
        let r = mi.monitorInfo.rcMonitor;
        list.push(Found {
            device: from_wide(&mi.szDevice),
            width: r.right - r.left,
            height: r.bottom - r.top,
            primary: mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    TRUE
}

impl Backend for GdiGamma {
    fn name(&self) -> String {
        "Windows (GDI gamma)".into()
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        for s in &mut self.screens {
            s.release();
        }
        self.screens.clear();

        let mut found: Vec<Found> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(HDC::default(), None, Some(on_monitor), LPARAM(&mut found as *mut _ as isize));
        }
        found.sort_by_key(|f| !f.primary);
        let names = friendly_names();

        for f in found {
            let device = wide(&f.device);
            // El DeviceID del monitor es estable aunque cambie el puerto.
            let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            let (id, generic) = unsafe {
                if EnumDisplayDevicesW(PCWSTR(device.as_ptr()), 0, &mut dd, 0).as_bool() {
                    let id = from_wide(&dd.DeviceID);
                    let desc = from_wide(&dd.DeviceString);
                    (if id.is_empty() { f.device.clone() } else { id }, desc)
                } else {
                    (f.device.clone(), String::new())
                }
            };
            let name = names
                .get(&f.device)
                .cloned()
                .or_else(|| (!generic.is_empty()).then_some(generic))
                .unwrap_or_else(|| f.device.clone());

            let mut screen = Screen {
                info: DisplayInfo { id: id.clone(), name, width: f.width, height: f.height, primary: f.primary, gamma: false },
                device,
                hdc: None,
                baseline: color::identity_ramp(),
            };
            if let Some(base) = self.baselines.get(&id) {
                screen.baseline = *base;
                screen.info.gamma = true;
            } else if let Some(hdc) = screen.dc() {
                let mut ramp = [0u16; 768];
                if unsafe { GetDeviceGammaRamp(hdc, ramp.as_mut_ptr() as *mut _) }.as_bool() {
                    screen.info.gamma = true;
                    if sane_baseline(&ramp) {
                        screen.baseline = ramp;
                    }
                    self.baselines.insert(id.clone(), screen.baseline);
                }
            }
            self.screens.push(screen);
        }
        self.screens.iter().map(|s| s.info.clone()).collect()
    }

    fn apply(&mut self, id: &str, brightness: f64, kelvin: f64, standard: bool) -> bool {
        let Some(base) = self.screen(id).map(|s| s.baseline) else { return false };
        let ramp = color::build_ramp(brightness, kelvin.round() as i32, &base);
        self.write(id, &ramp, standard)
    }

    fn restore(&mut self, id: &str) {
        if let Some(base) = self.screen(id).map(|s| s.baseline) {
            self.write(id, &base, false);
        }
    }

    fn restore_all(&mut self) {
        let ids: Vec<String> = self.screens.iter().map(|s| s.info.id.clone()).collect();
        for id in ids {
            self.restore(&id);
        }
    }
}
