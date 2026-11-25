//! Atajos de teclado: leer y escribir "modificadores|tecla", nombrarlos para la
//! interfaz y convertir lo que pulsa la persona (texto de Slint) en una tecla.
//!
//! Los números son los de Windows, para que el formato guardado sea estable:
//! modificadores Alt 1, Ctrl 2, Mayús 4, Win 8; tecla = código virtual (VK).

pub const ALT: u32 = 1;
pub const CTRL: u32 = 2;
pub const SHIFT: u32 = 4;
pub const WIN: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    pub mods: u32,
    pub vk: u32,
}

/// "6|38" → Ctrl+Mayús+↑. `None` si no hay atajo.
pub fn parse(s: &str) -> Option<Combo> {
    let (m, k) = s.split_once('|')?;
    let combo = Combo { mods: m.trim().parse().ok()?, vk: k.trim().parse().ok()? };
    (combo.vk != 0).then_some(combo)
}

pub fn format(c: Option<Combo>) -> String {
    match c {
        Some(c) => format!("{}|{}", c.mods, c.vk),
        None => "0|0".into(),
    }
}

fn key_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap_or('?').to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        0x20 => "Space".into(),
        0x21 => "PgUp".into(),
        0x22 => "PgDn".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x2D => "Ins".into(),
        0x2E => "Del".into(),
        0x6A => "Num *".into(),
        0x6B => "Num +".into(),
        0x6D => "Num -".into(),
        0x6F => "Num /".into(),
        0xBB => "+".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        _ => format!("#{vk}"),
    }
}

fn mod_names(mods: u32) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if mods & CTRL != 0 {
        parts.push("Ctrl".into());
    }
    if mods & ALT != 0 {
        parts.push(if cfg!(target_os = "macos") { "Option" } else { "Alt" }.into());
    }
    if mods & SHIFT != 0 {
        parts.push("Shift".into());
    }
    if mods & WIN != 0 {
        parts.push(if cfg!(target_os = "macos") { "Cmd" } else { "Win" }.into());
    }
    parts
}

/// Nombre para mostrar, p. ej. "Ctrl+Alt+↑".
pub fn label(c: Combo) -> String {
    let mut parts = mod_names(c.mods);
    parts.push(key_name(c.vk));
    parts.join("+")
}

/// Lo que se ve mientras se graba: "Ctrl+Alt+…" con solo modificadores,
/// o "… + K" si falta un modificador.
pub fn preview(mods: u32, vk: Option<u32>) -> String {
    let parts = mod_names(mods);
    match vk {
        None if parts.is_empty() => String::new(),
        None => format!("{}+…", parts.join("+")),
        Some(vk) => {
            let need = if cfg!(target_os = "macos") { "Ctrl/Option/Cmd" } else { "Ctrl/Alt/Win" };
            format!("{need}+{}", label(Combo { mods, vk }))
        }
    }
}

/// Tecla virtual a partir del texto que da Slint al pulsarla.
/// Las teclas especiales de Slint son caracteres del área privada de Unicode.
pub fn vk_from_text(text: &str) -> Option<u32> {
    use slint::platform::Key;
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let special = |k: Key| char::from(k) == ch;
    let table: [(Key, u32); 12] = [
        (Key::UpArrow, 0x26),
        (Key::DownArrow, 0x28),
        (Key::LeftArrow, 0x25),
        (Key::RightArrow, 0x27),
        (Key::PageUp, 0x21),
        (Key::PageDown, 0x22),
        (Key::Home, 0x24),
        (Key::End, 0x23),
        (Key::Insert, 0x2D),
        (Key::Delete, 0x2E),
        (Key::Space, 0x20),
        (Key::Tab, 0x09),
    ];
    for (k, vk) in table {
        if special(k) {
            return (vk != 0x09).then_some(vk);
        }
    }
    let fkeys = [
        Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11,
        Key::F12,
    ];
    if let Some(i) = fkeys.iter().position(|k| special(*k)) {
        return Some(0x70 + i as u32);
    }
    let up = ch.to_ascii_uppercase();
    if up.is_ascii_uppercase() || up.is_ascii_digit() {
        return Some(up as u32);
    }
    match ch {
        '+' => Some(0xBB),
        ',' => Some(0xBC),
        '-' => Some(0xBD),
        '.' => Some(0xBE),
        _ => None,
    }
}

/// Lo que pulsó la persona → atajo válido, o `None` si hay que seguir esperando
/// (solo un modificador, o una tecla sin Ctrl/Alt/Win que chocaría al escribir).
#[cfg_attr(not(test), allow(dead_code))]
pub fn from_press(text: &str, ctrl: bool, alt: bool, shift: bool, win: bool) -> Option<Combo> {
    let vk = vk_from_text(text)?;
    let mods = (if ctrl { CTRL } else { 0 }) | (if alt { ALT } else { 0 }) | (if shift { SHIFT } else { 0 }) | (if win { WIN } else { 0 });
    from_vk(mods, vk)
}

/// Tecla real (código virtual) + modificadores → atajo válido.
pub fn from_vk(mods: u32, vk: u32) -> Option<Combo> {
    let fkey = (0x70..=0x87).contains(&vk);
    // Una letra sola o con Mayús impediría escribir: hace falta Ctrl, Alt o Win.
    if mods & (CTRL | ALT | WIN) == 0 && !fkey {
        return None;
    }
    Some(Combo { mods, vk })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_labels() {
        let c = parse("6|38").unwrap();
        assert_eq!(c, Combo { mods: CTRL | SHIFT, vk: 0x26 });
        assert_eq!(label(c), "Ctrl+Shift+↑");
        assert_eq!(format(Some(c)), "6|38");
        assert_eq!(parse("0|0"), None);
        assert_eq!(parse("basura"), None);
    }

    #[test]
    fn presses() {
        assert_eq!(from_press("b", true, true, false, false), Some(Combo { mods: CTRL | ALT, vk: 'B' as u32 }));
        // Sin Ctrl/Alt/Win no vale (salvo teclas F).
        assert_eq!(from_press("b", false, false, true, false), None);
        let f5: String = char::from(slint::platform::Key::F5).into();
        assert_eq!(from_press(&f5, false, false, false, false), Some(Combo { mods: 0, vk: 0x74 }));
        let up: String = char::from(slint::platform::Key::UpArrow).into();
        assert_eq!(from_press(&up, true, false, false, false).map(label), Some("Ctrl+↑".into()));
        // Solo un modificador: sigue esperando.
        let shift: String = char::from(slint::platform::Key::Shift).into();
        assert_eq!(from_press(&shift, false, false, true, false), None);
        // Ctrl o Alt solos (Slint los manda como 0x11 y 0x12): sigue esperando.
        assert_eq!(from_press("\u{11}", true, false, false, false), None);
        assert_eq!(from_press("\u{12}", false, true, false, false), None);
        // En macOS, Alt se llama Option.
        let alt = if cfg!(target_os = "macos") { "Option" } else { "Alt" };
        assert_eq!(from_vk(CTRL | ALT | SHIFT, 'U' as u32).map(label), Some(format!("Ctrl+{alt}+Shift+U")));
        assert_eq!(from_vk(0, 'K' as u32), None);
        assert_eq!(preview(CTRL | ALT, None), format!("Ctrl+{alt}+…"));
        assert_eq!(preview(0, None), "");
    }
}
