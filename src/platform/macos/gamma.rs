//! Brillo y temperatura en macOS con la tabla de transferencia de CoreGraphics.
//!
//! Igual que en Windows y Linux: se guarda la tabla original al abrir y los
//! ajustes se calculan encima. Al salir, macOS vuelve a cargar el perfil de
//! ColorSync de cada pantalla.

use super::cg::*;
use crate::color;
use crate::platform::{Backend, DisplayInfo};
use std::collections::HashMap;

type Ramp = [Vec<f32>; 3];

struct Screen {
    info: DisplayInfo,
    display: CGDirectDisplayID,
    baseline: Ramp,
}

pub struct QuartzGamma {
    screens: Vec<Screen>,
    baselines: HashMap<String, Ramp>,
}

impl QuartzGamma {
    pub fn new() -> Self {
        Self { screens: Vec::new(), baselines: HashMap::new() }
    }

    fn write(&self, id: &str, ramp: &Ramp) -> bool {
        let Some(s) = self.screens.iter().find(|s| s.info.id == id) else { return false };
        let n = ramp[0].len() as u32;
        unsafe {
            CGSetDisplayTransferByTable(s.display, n, ramp[0].as_ptr(), ramp[1].as_ptr(), ramp[2].as_ptr()) == 0
        }
    }
}

fn identity(size: usize) -> Ramp {
    let ch: Vec<f32> = (0..size).map(|i| if size > 1 { i as f32 / (size - 1) as f32 } else { 1.0 }).collect();
    [ch.clone(), ch.clone(), ch]
}

/// Una tabla que ya trae un filtro de antes se descarta.
fn sane(r: &Ramp) -> bool {
    r.iter().all(|c| c.last().is_some_and(|&v| v >= 0.94) && c.windows(2).all(|p| p[1] >= p[0]))
}

/// Nombre de cada pantalla según macOS (NSScreen.localizedName).
fn screen_names() -> HashMap<CGDirectDisplayID, String> {
    let mut out = HashMap::new();
    let Some(mtm) = objc2::MainThreadMarker::new() else { return out };
    for screen in objc2_app_kit::NSScreen::screens(mtm).iter() {
        let name = screen.localizedName().to_string();
        let desc = screen.deviceDescription();
        let key = objc2_foundation::NSString::from_str("NSScreenNumber");
        let number = desc
            .objectForKey(&key)
            .and_then(|o| o.downcast::<objc2_foundation::NSNumber>().ok())
            .map(|n| n.unsignedIntValue());
        if let Some(id) = number {
            out.insert(id, name);
        }
    }
    out
}

impl Backend for QuartzGamma {
    fn name(&self) -> String {
        "macOS (CoreGraphics)".into()
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        self.screens.clear();
        let mut ids = [0u32; 16];
        let mut count = 0u32;
        if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
            return Vec::new();
        }
        let main = unsafe { CGMainDisplayID() };
        let names = screen_names();
        for &d in &ids[..count as usize] {
            let (vendor, model, serial) =
                unsafe { (CGDisplayVendorNumber(d), CGDisplayModelNumber(d), CGDisplaySerialNumber(d)) };
            // Fabricante + modelo + serie: estable aunque cambie el puerto.
            let id = format!("MAC\\{vendor:04X}{model:04X}-{serial:08X}");
            let name = names.get(&d).cloned().unwrap_or_else(|| {
                if unsafe { CGDisplayIsBuiltin(d) } != 0 { "Built-in display".into() } else { format!("Display {d}") }
            });
            let mut screen = Screen {
                info: DisplayInfo {
                    id: id.clone(),
                    name,
                    width: unsafe { CGDisplayPixelsWide(d) } as i32,
                    height: unsafe { CGDisplayPixelsHigh(d) } as i32,
                    primary: d == main,
                    gamma: false,
                },
                display: d,
                baseline: identity(256),
            };
            let capacity = unsafe { CGDisplayGammaTableCapacity(d) } as usize;
            if capacity > 1 {
                screen.info.gamma = true;
                if let Some(base) = self.baselines.get(&id) {
                    screen.baseline = base.clone();
                } else {
                    let mut r = vec![0f32; capacity];
                    let mut g = vec![0f32; capacity];
                    let mut b = vec![0f32; capacity];
                    let mut n = 0u32;
                    let ok = unsafe {
                        CGGetDisplayTransferByTable(
                            d,
                            capacity as u32,
                            r.as_mut_ptr(),
                            g.as_mut_ptr(),
                            b.as_mut_ptr(),
                            &mut n,
                        )
                    } == 0;
                    if ok && n > 1 {
                        r.truncate(n as usize);
                        g.truncate(n as usize);
                        b.truncate(n as usize);
                        let ramp: Ramp = [r, g, b];
                        if sane(&ramp) {
                            screen.baseline = ramp;
                        }
                    }
                    self.baselines.insert(id, screen.baseline.clone());
                }
            }
            self.screens.push(screen);
        }
        self.screens.sort_by_key(|s| !s.info.primary);
        self.screens.iter().map(|s| s.info.clone()).collect()
    }

    fn apply(&mut self, id: &str, brightness: f64, kelvin: f64, _standard: bool) -> bool {
        let Some(base) = self.screens.iter().find(|s| s.info.id == id).map(|s| s.baseline.clone()) else {
            return false;
        };
        let m = color::multipliers(kelvin.round() as i32);
        let scale = (brightness / 100.0).clamp(0.0, 1.0);
        let ramp: Ramp = std::array::from_fn(|ch| {
            base[ch].iter().map(|&v| (v as f64 * m[ch] * scale).clamp(0.0, 1.0) as f32).collect()
        });
        self.write(id, &ramp)
    }

    fn restore(&mut self, id: &str) {
        if let Some(base) = self.screens.iter().find(|s| s.info.id == id).map(|s| s.baseline.clone()) {
            self.write(id, &base);
        }
    }

    fn restore_all(&mut self) {
        // Vuelve a cargar los perfiles de ColorSync de todas las pantallas.
        unsafe { CGDisplayRestoreColorSyncSettings() };
    }
}

extern "C" fn reconfigured(_display: CGDirectDisplayID, flags: u32, _user: *mut core::ffi::c_void) {
    // kCGDisplayBeginConfigurationFlag = 1: el aviso de antes del cambio no interesa.
    if flags & 1 == 0 {
        super::emit(crate::platform::TrayEvent::DisplaysChanged);
    }
}

/// Avisa cuando se conecta, quita o cambia una pantalla (también al despertar).
pub fn watch_displays() {
    unsafe {
        CGDisplayRegisterReconfigurationCallback(reconfigured, core::ptr::null_mut());
    }
}
