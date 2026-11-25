//! Brillo y temperatura en X11 con la rampa gamma de cada CRTC (RandR).
//!
//! Igual que en Windows: se guarda la rampa original al abrir y los ajustes se
//! calculan encima. Aquí cada pantalla puede tener un tamaño de rampa distinto.

use super::x::{x, X};
use crate::color;
use crate::platform::{Backend, DisplayInfo};
use std::collections::HashMap;
use std::sync::Arc;
use x11rb::protocol::randr::{Connection as OutputState, ConnectionExt as _, Crtc};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

type Ramp = [Vec<u16>; 3];

struct Screen {
    info: DisplayInfo,
    crtc: Crtc,
    baseline: Ramp,
}

pub struct RandrGamma {
    x: Option<Arc<X>>,
    screens: Vec<Screen>,
    baselines: HashMap<String, Ramp>,
}

impl RandrGamma {
    pub fn new() -> Self {
        Self { x: x(), screens: Vec::new(), baselines: HashMap::new() }
    }

    fn write(&self, id: &str, ramp: &Ramp) -> bool {
        let (Some(x), Some(s)) = (self.x.as_ref(), self.screens.iter().find(|s| s.info.id == id)) else {
            return false;
        };
        let ok = x
            .conn
            .randr_set_crtc_gamma(s.crtc, &ramp[0], &ramp[1], &ramp[2])
            .ok()
            .is_some_and(|c| c.check().is_ok());
        let _ = x11rb::connection::Connection::flush(&x.conn);
        ok
    }
}

fn identity(size: usize) -> Ramp {
    let ch: Vec<u16> = (0..size)
        .map(|i| if size > 1 { (i as u32 * 65535 / (size as u32 - 1)) as u16 } else { 65535 })
        .collect();
    [ch.clone(), ch.clone(), ch]
}

/// Igual que en Windows: una rampa que ya trae un filtro de antes se descarta.
fn sane(r: &Ramp) -> bool {
    r.iter().all(|c| c.last().is_some_and(|&v| v >= 0xF000) && c.windows(2).all(|p| p[1] >= p[0]))
}

/// Nombre comercial y número de serie del monitor desde su EDID.
fn edid_info(edid: &[u8]) -> (Option<String>, Option<String>) {
    if edid.len() < 128 || edid[0..8] != [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0] {
        return (None, None);
    }
    let mut name = None;
    for d in 0..4 {
        let off = 54 + d * 18;
        let b = &edid[off..off + 18];
        if b[0] == 0 && b[1] == 0 && b[3] == 0xFC {
            let text: String = b[5..18].iter().take_while(|&&c| c != 0x0A).map(|&c| c as char).collect();
            let text = text.trim().to_string();
            if !text.is_empty() {
                name = Some(text);
            }
        }
    }
    // Fabricante (3 letras) + producto + serie: estable aunque cambie el puerto.
    let m = u16::from_be_bytes([edid[8], edid[9]]);
    let letter = |v: u16| (b'A' + ((v & 0x1F) as u8).saturating_sub(1)) as char;
    let maker: String = [letter(m >> 10), letter(m >> 5), letter(m)].iter().collect();
    let product = u16::from_le_bytes([edid[10], edid[11]]);
    let serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);
    (name, Some(format!("{maker}{product:04X}-{serial:08X}")))
}

impl Backend for RandrGamma {
    fn name(&self) -> String {
        if self.x.is_some() {
            "X11 (RandR gamma)".into()
        } else {
            "Wayland (sin gamma)".into()
        }
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        self.screens.clear();
        let Some(x) = self.x.clone() else { return Vec::new() };
        let Some(res) = x.conn.randr_get_screen_resources_current(x.root).ok().and_then(|c| c.reply().ok()) else {
            return Vec::new();
        };
        let primary = x.conn.randr_get_output_primary(x.root).ok().and_then(|c| c.reply().ok()).map(|r| r.output);
        let edid_atom = x
            .conn
            .intern_atom(true, b"EDID")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
            .unwrap_or(0);
        let mut found: Vec<Screen> = Vec::new();
        for &output in &res.outputs {
            let Some(oi) = x.conn.randr_get_output_info(output, res.config_timestamp).ok().and_then(|c| c.reply().ok())
            else {
                continue;
            };
            if oi.connection != OutputState::CONNECTED || oi.crtc == 0 {
                continue;
            }
            // Dos salidas en espejo comparten CRTC: basta con una.
            if found.iter().any(|s| s.crtc == oi.crtc) {
                continue;
            }
            let Some(ci) = x.conn.randr_get_crtc_info(oi.crtc, res.config_timestamp).ok().and_then(|c| c.reply().ok())
            else {
                continue;
            };
            let port = String::from_utf8_lossy(&oi.name).into_owned();
            let edid = if edid_atom != 0 {
                x.conn
                    .randr_get_output_property(output, edid_atom, AtomEnum::ANY, 0, 256, false, false)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| r.data)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let (model, stable) = edid_info(&edid);
            let id = format!("X11\\{}", stable.unwrap_or_else(|| port.clone()));
            let mut screen = Screen {
                info: DisplayInfo {
                    id: id.clone(),
                    name: model.unwrap_or(port),
                    width: ci.width as i32,
                    height: ci.height as i32,
                    primary: Some(output) == primary,
                    gamma: false,
                },
                crtc: oi.crtc,
                baseline: identity(256),
            };
            let size = x
                .conn
                .randr_get_crtc_gamma_size(oi.crtc)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.size as usize)
                .unwrap_or(0);
            if size > 1 {
                screen.info.gamma = true;
                screen.baseline = identity(size);
                if let Some(base) = self.baselines.get(&id).filter(|b| b[0].len() == size) {
                    screen.baseline = base.clone();
                } else {
                    if let Some(g) = x.conn.randr_get_crtc_gamma(oi.crtc).ok().and_then(|c| c.reply().ok()) {
                        let r: Ramp = [g.red, g.green, g.blue];
                        if r[0].len() == size && sane(&r) {
                            screen.baseline = r;
                        }
                    }
                    self.baselines.insert(id, screen.baseline.clone());
                }
            }
            found.push(screen);
        }
        // Sin salida principal marcada, la primera hace de principal.
        if !found.iter().any(|s| s.info.primary) {
            if let Some(f) = found.first_mut() {
                f.info.primary = true;
            }
        }
        found.sort_by_key(|s| !s.info.primary);
        self.screens = found;
        self.screens.iter().map(|s| s.info.clone()).collect()
    }

    fn apply(&mut self, id: &str, brightness: f64, kelvin: f64, _standard: bool) -> bool {
        let Some(base) = self.screens.iter().find(|s| s.info.id == id).map(|s| s.baseline.clone()) else {
            return false;
        };
        let m = color::multipliers(kelvin.round() as i32);
        let scale = (brightness / 100.0).clamp(0.0, 1.0);
        let ramp: Ramp = std::array::from_fn(|ch| {
            base[ch].iter().map(|&v| (v as f64 * m[ch] * scale).round().clamp(0.0, 65535.0) as u16).collect()
        });
        self.write(id, &ramp)
    }

    fn restore(&mut self, id: &str) {
        if let Some(base) = self.screens.iter().find(|s| s.info.id == id).map(|s| s.baseline.clone()) {
            self.write(id, &base);
        }
    }

    fn restore_all(&mut self) {
        let ids: Vec<String> = self.screens.iter().map(|s| s.info.id.clone()).collect();
        for id in ids {
            self.restore(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edid_parsing() {
        let mut e = vec![0u8; 128];
        e[0..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        // "DEL" = 00100 00101 01100
        let m: u16 = (4 << 10) | (5 << 5) | 12;
        e[8..10].copy_from_slice(&m.to_be_bytes());
        e[10..12].copy_from_slice(&0xA0B1u16.to_le_bytes());
        e[12..16].copy_from_slice(&7u32.to_le_bytes());
        let d = 54 + 18;
        e[d + 3] = 0xFC;
        e[d + 5..d + 5 + 11].copy_from_slice(b"DELL P2419H");
        e[d + 16] = 0x0A;
        let (name, id) = edid_info(&e);
        assert_eq!(name.as_deref(), Some("DELL P2419H"));
        assert_eq!(id.as_deref(), Some("DELA0B1-00000007"));
        assert_eq!(identity(1024)[0][1023], 65535);
    }
}
