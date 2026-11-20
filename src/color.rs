//! Temperatura de color → multiplicadores RGB, y construcción de la rampa gamma.
//!
//! La conversión Kelvin → RGB usa el ajuste de curva público de Tanner Helland
//! ("How to Convert Temperature (K) to RGB", 2012:
//! https://tannerhelland.com/2012/09/18/convert-temperature-rgb-algorithm-code.html).
//! El resultado se normaliza para que 6500 K dé exactamente (1, 1, 1), es decir,
//! la pantalla sin cambios.

pub const NEUTRAL_K: i32 = 6500;

/// Rango del control de temperatura (normal y ampliado).
pub fn kelvin_range(wide: bool) -> (i32, i32) {
    if wide { (1000, 10000) } else { (1500, 6500) }
}

/// Rango del control de brillo (normal y ampliado). Un mínimo evita dejar la pantalla negra.
pub fn brightness_range(wide: bool) -> (f64, f64) {
    if wide { (5.0, 100.0) } else { (20.0, 100.0) }
}

/// Color aproximado de un cuerpo negro a `kelvin`, en 0..255 por canal.
pub fn kelvin_to_rgb(kelvin: i32) -> [f64; 3] {
    let t = kelvin.clamp(1000, 40000) as f64 / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698727446 * (t - 60.0).powf(-0.1332047592) };
    let g = if t <= 66.0 {
        99.4708025861 * t.ln() - 161.1195681661
    } else {
        288.1221695283 * (t - 60.0).powf(-0.0755148492)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.5177312231 * (t - 10.0).ln() - 305.0447927307
    };
    [r.clamp(0.0, 255.0), g.clamp(0.0, 255.0), b.clamp(0.0, 255.0)]
}

/// Multiplicadores por canal relativos a 6500 K (el canal más alto vale 1).
pub fn multipliers(kelvin: i32) -> [f64; 3] {
    let t = kelvin_to_rgb(kelvin);
    let n = kelvin_to_rgb(NEUTRAL_K);
    let m = [t[0] / n[0], t[1] / n[1], t[2] / n[2]];
    let peak = m.iter().cloned().fold(1.0_f64, f64::max);
    [m[0] / peak, m[1] / peak, m[2] / peak]
}

/// Rampa lineal de 16 bits: índice × 257 (0x0000, 0x0101 … 0xFFFF).
pub fn identity_ramp() -> [u16; 768] {
    let mut r = [0u16; 768];
    for ch in 0..3 {
        for i in 0..256 {
            r[ch * 256 + i] = (i as u16) * 257;
        }
    }
    r
}

/// Rampa con brillo (0..100) y temperatura aplicados *encima* de la calibración
/// del monitor (`base`), para no destruir su perfil ICC.
pub fn build_ramp(brightness: f64, kelvin: i32, base: &[u16; 768]) -> [u16; 768] {
    let m = multipliers(kelvin);
    let scale = (brightness / 100.0).clamp(0.0, 1.0);
    let mut out = [0u16; 768];
    for ch in 0..3 {
        let f = m[ch] * scale;
        for i in 0..256 {
            let v = base[ch * 256 + i] as f64 * f;
            out[ch * 256 + i] = v.round().clamp(0.0, 65535.0) as u16;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_identity() {
        let m = multipliers(NEUTRAL_K);
        for c in m {
            assert!((c - 1.0).abs() < 1e-9);
        }
        assert_eq!(build_ramp(100.0, NEUTRAL_K, &identity_ramp()), identity_ramp());
    }

    #[test]
    fn warm_cuts_blue_more_than_red() {
        let m = multipliers(3000);
        assert!(m[2] < m[1] && m[1] < m[0]);
    }
}
