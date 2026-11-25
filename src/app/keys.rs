//! Atajos de teclado globales y reglas por programa.

use super::{apply, night_now, push as push_all, ui, with, GRID};
use crate::config::{AppRule, HotkeyConfig, Mode};
use crate::hotkeys;
use crate::i18n;
use crate::platform;
use crate::ui::{App, HotkeyRow, Keys};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::time::Duration;

thread_local! {
    static WATCH: RefCell<Option<Timer>> = const { RefCell::new(None) };
    /// Atajos que Windows no dejó registrar (otra app ya los usa).
    static FAILED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const KINDS: [&str; 3] = ["process", "class", "title"];

fn action_title(name: &str) -> String {
    let key = match name {
        "incBrightness" => "subir_brillo",
        "decBrightness" => "bajar_brillo",
        "incKelvin" => "subir_temperatura",
        "decKelvin" => "bajar_temperatura",
        "toggleTemperature" => "activar_o_quitar_temperatura",
        "togglePause" => "rs_atajo_pausar",
        "toggleBand" => "atajo_de_la_franja",
        "toggleVeil" => "atajo_del_velo",
        "prismaDark" => "atajo_de_oscuro",
        "prismaGray" => "atajo_de_gris",
        other => Mode::from_id(other).label_key(),
    };
    i18n::tr(key)
}

// ------------------------------------------------------------------ arranque

pub(super) fn init(app: &crate::ui::AppWindow) {
    let k = app.global::<Keys>();
    k.on_set_step(|name, v| {
        with(|s| {
            match name.as_str() {
                "brightnessStep" => s.cfg.hotkeys.brightness_step = v,
                "kelvinStep" => s.cfg.hotkeys.kelvin_step = v,
                _ => {}
            }
            s.dirty = true;
        });
        push();
    });
    // Mientras se graba un atajo, los registrados no deben dispararse.
    k.on_start_capture(|_| {
        platform::register_hotkeys(&[]);
        set_preview(String::new());
    });
    k.on_end_capture(|| {
        set_preview(String::new());
        register_all();
        push_all();
    });
    k.on_capture(|name, text, ctrl, alt, shift, win| {
        let flags = (if ctrl { hotkeys::CTRL } else { 0 })
            | (if alt { hotkeys::ALT } else { 0 })
            | (if shift { hotkeys::SHIFT } else { 0 })
            | (if win { hotkeys::WIN } else { 0 });
        // En Windows se lee la tecla real; el texto de Slint queda de reserva.
        let mods = current_mods(flags);
        let vk = platform::pressed_key().or_else(|| hotkeys::vk_from_text(&text));
        super::debug_log(&format!("atajo: {name} {:?} mods={mods} vk={vk:?}", text.as_str()));
        let Some(vk) = vk else {
            set_preview(hotkeys::preview(mods, None));
            return false;
        };
        match hotkeys::from_vk(mods, vk) {
            Some(combo) => {
                save_combo(&name, combo);
                true
            }
            None => {
                // Una letra sola: falta Ctrl, Alt o Win.
                set_preview(hotkeys::preview(mods, Some(vk)));
                false
            }
        }
    });
    k.on_capture_release(|ctrl, alt, shift, win| {
        let flags = (if ctrl { hotkeys::CTRL } else { 0 })
            | (if alt { hotkeys::ALT } else { 0 })
            | (if shift { hotkeys::SHIFT } else { 0 })
            | (if win { hotkeys::WIN } else { 0 });
        set_preview(hotkeys::preview(current_mods(flags), None));
    });
    k.on_clear(|name| {
        with(|s| {
            s.cfg.hotkeys.set(&name, "0|0".into());
            s.dirty = true;
        });
        register_all();
        push_all();
    });

    k.on_set_rules_bool(|name, on| {
        with(|s| {
            match name.as_str() {
                "enabled" => s.cfg.rules.enabled = on,
                "pauseOnFullscreen" => s.cfg.rules.pause_on_fullscreen = on,
                _ => {}
            }
            s.dirty = true;
        });
        check_rules();
        push();
    });
    k.on_add_rule(|| {
        let Some(app) = ui() else { return };
        let k = app.global::<Keys>();
        let query = k.get_new_query().trim().to_string();
        if query.is_empty() {
            return;
        }
        let rule = AppRule {
            query,
            kind: KINDS.get(k.get_new_kind() as usize).unwrap_or(&"process").to_string(),
            condition: if k.get_new_condition() == 1 { "fullscreen" } else { "always" }.into(),
            mode: GRID.get(k.get_new_mode() as usize).copied().unwrap_or(Mode::Pause).id().into(),
        };
        with(|s| {
            s.cfg.rules.items.push(rule);
            s.dirty = true;
        });
        k.set_new_query(SharedString::new());
        check_rules();
        push();
    });
    k.on_remove_rule(|i| {
        with(|s| {
            if (i as usize) < s.cfg.rules.items.len() {
                s.cfg.rules.items.remove(i as usize);
                s.dirty = true;
            }
        });
        check_rules();
        push();
    });

    let names: Vec<SharedString> = GRID.iter().map(|m| SharedString::from(i18n::tr(m.label_key()))).collect();
    k.set_mode_names(ModelRc::new(VecModel::from(names)));

    register_all();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), check_rules);
    WATCH.with(|w| *w.borrow_mut() = Some(timer));
}

/// Modificadores pulsados: los del sistema si los da; si no, los de Slint.
/// En macOS, Slint llama «control» a Cmd y «meta» a Ctrl: se intercambian.
fn current_mods(flags: u32) -> u32 {
    match platform::held_mods() {
        Some(m) if m != 0 || flags == 0 => m,
        _ if cfg!(target_os = "macos") => {
            let swap = |from: u32, to: u32| if flags & from != 0 { to } else { 0 };
            (flags & (hotkeys::ALT | hotkeys::SHIFT))
                | swap(hotkeys::CTRL, hotkeys::WIN)
                | swap(hotkeys::WIN, hotkeys::CTRL)
        }
        _ => flags,
    }
}

fn set_preview(text: String) {
    if let Some(app) = ui() {
        app.global::<Keys>().set_capture_preview(text.into());
    }
}

fn save_combo(name: &str, combo: hotkeys::Combo) {
    with(|s| {
        // Una combinación solo puede tener una acción: se quita de la anterior.
        let value = hotkeys::format(Some(combo));
        for other in HotkeyConfig::NAMES {
            if s.cfg.hotkeys.get(other) == value {
                s.cfg.hotkeys.set(other, "0|0".into());
            }
        }
        s.cfg.hotkeys.set(name, value);
        s.dirty = true;
    });
}

pub(super) fn shutdown() {
    WATCH.with(|w| w.borrow_mut().take());
    platform::register_hotkeys(&[]);
}

fn register_all() {
    let list: Vec<(i32, u32, u32, &str)> = with(|s| {
        HotkeyConfig::NAMES
            .iter()
            .enumerate()
            // Los de Enfoque y Prisma solo donde existen.
            .filter(|(_, n)| platform::HAS_FX || !matches!(**n, "toggleBand" | "toggleVeil" | "prismaDark" | "prismaGray"))
            .filter_map(|(i, n)| hotkeys::parse(s.cfg.hotkeys.get(n)).map(|c| (i as i32 + 1, c.mods, c.vk, *n)))
            .collect()
    });
    let plain: Vec<(i32, u32, u32)> = list.iter().map(|x| (x.0, x.1, x.2)).collect();
    let failed = platform::register_hotkeys(&plain);
    FAILED.with(|f| {
        *f.borrow_mut() = list.iter().filter(|x| failed.contains(&x.0)).map(|x| x.3.to_string()).collect();
    });
}

// ------------------------------------------------------------------ acciones

pub(super) fn on_hotkey(id: i32) {
    let Some(name) = HotkeyConfig::NAMES.get((id - 1).max(0) as usize).copied() else { return };
    match name {
        "incBrightness" => nudge(1.0, 0),
        "decBrightness" => nudge(-1.0, 0),
        "incKelvin" => nudge(0.0, 1),
        "decKelvin" => nudge(0.0, -1),
        "toggleTemperature" => {
            with(|s| {
                s.cfg.temperature_disabled = !s.cfg.temperature_disabled;
                s.dirty = true;
            });
            apply(true);
        }
        "togglePause" => {
            with(|s| s.toggle_pause());
            apply(true);
        }
        "toggleBand" | "toggleVeil" | "prismaDark" | "prismaGray" => super::focus::hotkey(name),
        mode => {
            let mode = Mode::from_id(mode);
            with(|s| {
                for d in s.displays.clone() {
                    if let Some(mc) = s.cfg.monitors.get_mut(&d.id) {
                        mc.mode = mode.id().into();
                        if mode != Mode::Pause {
                            mc.resume_mode = Some(mode.id().into());
                        }
                    }
                }
                s.dirty = true;
            });
            apply(true);
        }
    }
    push_all();
}

/// Sube o baja brillo/temperatura en todas las pantallas, en el periodo (día o
/// noche) que se ve ahora. En Pausa no hay valores que mover.
fn nudge(brightness: f64, kelvin: i32) {
    with(|s| {
        let night = night_now(s);
        let db = brightness * s.cfg.hotkeys.brightness_step.clamp(1, 20) as f64;
        let dk = kelvin * s.cfg.hotkeys.kelvin_step.clamp(50, 1000);
        let (kmin, kmax) = s.kelvin_bounds();
        let bmin = s.min_brightness();
        for d in s.displays.clone() {
            let Some(mc) = s.cfg.monitors.get_mut(&d.id) else { continue };
            let mode = mc.mode();
            let Some(v) = mc.settings_mut(mode) else { continue };
            if night {
                v.night_brightness = (v.night_brightness + db).clamp(bmin, 100.0);
                v.night_kelvin = (v.night_kelvin + dk).clamp(kmin, kmax);
            } else {
                v.brightness = (v.brightness + db).clamp(bmin, 100.0);
                v.kelvin = (v.kelvin + dk).clamp(kmin, kmax);
            }
        }
        s.dirty = true;
    });
    apply(false);
}

// ------------------------------------------------------------------ reglas

fn matches(rule: &AppRule, fg: &platform::Foreground) -> bool {
    let q = rule.query.trim();
    if q.is_empty() || (rule.condition == "fullscreen" && !fg.fullscreen) {
        return false;
    }
    match rule.kind.as_str() {
        "class" => fg.class.eq_ignore_ascii_case(q),
        "title" => fg.title.to_lowercase().contains(&q.to_lowercase()),
        _ => {
            let p = fg.process.to_lowercase();
            let q = q.to_lowercase();
            p == q || p == format!("{q}.exe")
        }
    }
}

/// Cada segundo: mira la ventana activa y aplica la regla que corresponda.
fn check_rules() {
    let (enabled, pause_fs, items) = with(|s| (s.cfg.rules.enabled, s.cfg.rules.pause_on_fullscreen, s.cfg.rules.items.clone()));
    // También se mira con la pestaña Reglas abierta, para ofrecer «Usar …».
    let rules_tab = ui().is_some_and(|a| {
        let g = a.global::<App>();
        a.window().is_visible() && g.get_page() == 4 && g.get_option_tab() == 4
    });
    let mut found: Option<Mode> = None;
    if enabled || pause_fs || rules_tab {
        let Some(fg) = platform::foreground_info() else { return };
        if fg.own {
            // Abrir CareOurEyes no cambia la regla que ya estaba.
            return;
        }
        if !fg.process.is_empty() {
            let name = fg.process.clone();
            let changed = with(|s| std::mem::replace(&mut s.last_window, name.clone()) != name);
            if changed {
                if let Some(app) = ui() {
                    app.global::<Keys>().set_last_window(name.into());
                }
            }
        }
        if enabled {
            found = items.iter().find(|r| matches(r, &fg)).map(|r| Mode::from_id(&r.mode));
        }
        if found.is_none() && pause_fs && fg.fullscreen {
            found = Some(Mode::Pause);
        }
    }
    let changed = with(|s| std::mem::replace(&mut s.rule_override, found) != found);
    if changed {
        apply(true);
        push_all();
    }
}

// ------------------------------------------------------------------ interfaz

pub(super) fn hotkey_row(name: &str, hk: &HotkeyConfig, failed: &[String]) -> HotkeyRow {
    HotkeyRow {
        name: name.into(),
        title: action_title(name).into(),
        keys: hotkeys::parse(hk.get(name)).map(hotkeys::label).unwrap_or_default().into(),
        failed: failed.iter().any(|f| f == name),
    }
}

pub(super) fn failed_hotkeys() -> Vec<String> {
    FAILED.with(|f| f.borrow().clone())
}

pub(super) fn push() {
    let Some(app) = ui() else { return };
    let k = app.global::<Keys>();
    let failed = FAILED.with(|f| f.borrow().clone());
    let (hk, rules, notice) = with(|s| (s.cfg.hotkeys.clone(), s.cfg.rules.clone(), s.rule_override));
    k.set_brightness_step(hk.brightness_step);
    k.set_kelvin_step(hk.kelvin_step);
    // Sin Enfoque ni Prisma en este sistema, sus atajos no se muestran.
    let fx = ["toggleBand", "toggleVeil", "prismaDark", "prismaGray"];
    let rows: Vec<HotkeyRow> = HotkeyConfig::NAMES
        .iter()
        .filter(|n| platform::HAS_FX || !fx.contains(n))
        .map(|n| hotkey_row(n, &hk, &failed))
        .collect();
    // Reemplazar la lista mientras se graba un atajo le quitaría el foco: solo si cambió.
    let current = k.get_rows();
    let same = current.row_count() == rows.len() && rows.iter().enumerate().all(|(i, r)| current.row_data(i).as_ref() == Some(r));
    if !same {
        k.set_rows(ModelRc::new(VecModel::from(rows)));
    }
    k.set_rules_enabled(rules.enabled);
    k.set_pause_fullscreen(rules.pause_on_fullscreen);
    let summaries: Vec<SharedString> = rules
        .items
        .iter()
        .map(|r| {
            let kind = match r.kind.as_str() {
                "class" => "clase_lc",
                "title" => "titulo_2",
                _ => "proceso_lc",
            };
            let cond = if r.condition == "fullscreen" { "solo_completa_lc" } else { "siempre_lc" };
            i18n::tr_args(
                "regla_resumen",
                &[&r.query, &i18n::tr(kind), &i18n::tr(cond), &i18n::tr(Mode::from_id(&r.mode).label_key())],
            )
            .into()
        })
        .collect();
    k.set_rules(ModelRc::new(VecModel::from(summaries)));
    let text = notice
        .map(|m| i18n::tr_args("una_regla_esta_aplicando_sobre_la_ventana_ac", &[&i18n::tr(m.label_key())]))
        .unwrap_or_default();
    app.global::<App>().set_rule_notice(text.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fg(process: &str, title: &str, fullscreen: bool) -> platform::Foreground {
        platform::Foreground { process: process.into(), class: "MediaPlayerClassicW".into(), title: title.into(), own: false, fullscreen }
    }

    fn rule(query: &str, kind: &str, condition: &str) -> AppRule {
        AppRule { query: query.into(), kind: kind.into(), condition: condition.into(), mode: "movie".into() }
    }

    #[test]
    fn rules_match() {
        assert!(matches(&rule("vlc", "process", "always"), &fg("VLC.exe", "", false)));
        assert!(matches(&rule("vlc.exe", "process", "always"), &fg("vlc.exe", "", false)));
        assert!(!matches(&rule("vlc", "process", "fullscreen"), &fg("vlc.exe", "", false)));
        assert!(matches(&rule("youtube", "title", "always"), &fg("chrome.exe", "Video - YouTube", false)));
        assert!(matches(&rule("mediaplayerclassicw", "class", "always"), &fg("mpc.exe", "", false)));
        assert!(!matches(&rule("  ", "process", "always"), &fg("", "", false)));
    }
}
