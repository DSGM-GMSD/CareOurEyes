//! Textos de la interfaz en 10 idiomas.
//!
//! Los archivos están en `i18n/` (UTF-8, formato `clave = valor`): `messages.properties`
//! es el español (idioma base) y cada `messages_<código>.properties` una traducción.
//! Son los mismos archivos que usa la versión Kotlin. Una clave que falta en una
//! traducción usa el español.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Código guardado en config.json → nombre en su propio idioma.
pub const LANGUAGES: [(&str, &str); 11] = [
    ("system", "Automático / System"),
    ("es", "Español"),
    ("en", "English"),
    ("zh_CN", "简体中文"),
    ("zh_TW", "繁體中文"),
    ("de", "Deutsch"),
    ("nl", "Nederlands"),
    ("it", "Italiano"),
    ("tr", "Türkçe"),
    ("fr", "Français"),
    ("ja", "日本語"),
];

fn source(code: &str) -> &'static str {
    match code {
        "en" => include_str!("../i18n/messages_en.properties"),
        "zh_CN" => include_str!("../i18n/messages_zh_CN.properties"),
        "zh_TW" => include_str!("../i18n/messages_zh_TW.properties"),
        "de" => include_str!("../i18n/messages_de.properties"),
        "nl" => include_str!("../i18n/messages_nl.properties"),
        "it" => include_str!("../i18n/messages_it.properties"),
        "tr" => include_str!("../i18n/messages_tr.properties"),
        "fr" => include_str!("../i18n/messages_fr.properties"),
        "ja" => include_str!("../i18n/messages_ja.properties"),
        _ => include_str!("../i18n/messages.properties"),
    }
}

struct Strings {
    code: String,
    base: HashMap<String, String>,
    lang: HashMap<String, String>,
}

static STRINGS: OnceLock<Strings> = OnceLock::new();

fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some((k, v)) = line.split_once(" = ") else { continue };
        let mut out = String::with_capacity(v.len());
        let mut chars = v.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o), // "\ " (espacio inicial), "\\", etc.
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        map.insert(k.trim().to_string(), out);
    }
    map
}

/// Convierte el ajuste guardado ("system", un código o un nombre antiguo) en un código.
pub fn resolve(setting: &str) -> String {
    let known: Vec<&str> = LANGUAGES.iter().map(|l| l.0).filter(|c| *c != "system").collect();
    let s = setting.trim();
    if known.contains(&s) {
        return s.to_string();
    }
    let legacy = match s.to_lowercase().as_str() {
        "español" => Some("es"),
        "english" => Some("en"),
        "deutsch" => Some("de"),
        "nederlands" => Some("nl"),
        "italiano" => Some("it"),
        "türkçe" => Some("tr"),
        "français" => Some("fr"),
        "简体中文" => Some("zh_CN"),
        "繁體中文" => Some("zh_TW"),
        "日本語" => Some("ja"),
        _ => None,
    };
    if let Some(c) = legacy {
        return c.to_string();
    }
    // "system" o desconocido: idioma del sistema; si no está traducido, inglés.
    let sys = sys_locale::get_locale().unwrap_or_default().replace('_', "-");
    let lower = sys.to_lowercase();
    if lower.starts_with("zh") {
        let traditional = lower.contains("hant") || lower.ends_with("-tw") || lower.ends_with("-hk") || lower.ends_with("-mo");
        return if traditional { "zh_TW" } else { "zh_CN" }.to_string();
    }
    let lang = lower.split('-').next().unwrap_or("");
    if known.contains(&lang) { lang.to_string() } else { "en".to_string() }
}

/// Elige el idioma una vez, al iniciar.
pub fn init(setting: &str) {
    let code = resolve(setting);
    let _ = STRINGS.set(Strings {
        base: parse(source("es")),
        lang: parse(source(&code)),
        code,
    });
}

pub fn current() -> &'static str {
    STRINGS.get().map(|s| s.code.as_str()).unwrap_or("es")
}

/// Texto de `key`; `{0}`, `{1}`… se reemplazan por `args`.
/// Fuera de Windows, algunos textos tienen otra versión (sin «Windows», sin «.exe»…).
fn platform_key(key: &str) -> &str {
    if cfg!(windows) {
        return key;
    }
    match key {
        "arrancar_con_windows" => "rs_arrancar_con_sistema",
        "rs_atajos_texto" if cfg!(target_os = "macos") => "rs_atajos_texto_mac",
        "rs_atajos_texto" => "rs_atajos_texto_x11",
        "rs_regla_nueva_texto" => "rs_regla_nueva_texto_linux",
        other => other,
    }
}

pub fn tr_args(key: &str, args: &[&str]) -> String {
    let key = platform_key(key);
    let mut text = STRINGS
        .get()
        .and_then(|s| s.lang.get(key).or_else(|| s.base.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_string());
    for (i, a) in args.iter().enumerate() {
        text = text.replace(&format!("{{{i}}}"), a);
    }
    text
}

pub fn tr(key: &str) -> String {
    tr_args(key, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key() {
        let base = parse(source("es"));
        for (code, _) in LANGUAGES.iter().skip(2) {
            let lang = parse(source(code));
            for k in base.keys() {
                assert!(lang.contains_key(k), "{code} no tiene {k}");
            }
        }
    }

    #[test]
    fn leading_space_is_kept() {
        let m = parse("a = \\  x\nb = c\\\\d");
        assert_eq!(m["a"], "  x");
        assert_eq!(m["b"], "c\\d");
    }
}
