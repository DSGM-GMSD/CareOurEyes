//! Enfoque (franja de lectura, velo del fondo) y Prisma: une la configuración y
//! las páginas con los efectos de Windows (`platform::windows::fx`).

use super::{keys, ui, with, wire_translations};
use crate::i18n;
use crate::ui::{Focus, HotkeyRow, PrismaBar, Theme, T};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// Colores que se pueden elegir, como COLORREF (0x00BBGGRR). Mismo orden que
/// `Focus.swatches` en focus.slint.
const SWATCHES: [i32; 6] = [0x000000, 0x3B291E, 0x5F3A1E, 0x5E1F3B, 0x0A2C4A, 0x2E3D0F];

thread_local! {
    static FX: RefCell<Option<Timer>> = const { RefCell::new(None) };
    static BAR: RefCell<Option<PrismaBar>> = const { RefCell::new(None) };
    /// Cuándo se encendió el velo (para la animación de entrada).
    static VEIL_SINCE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Ventana bajo el ratón y desde cuándo (para la espera de la barra).
    static HOVER: Cell<(isize, Option<Instant>)> = const { Cell::new((0, None)) };
    /// Ventana a la que pertenece la barra visible (0 = oculta) y cuándo salió el ratón.
    static ANCHOR: Cell<(isize, Option<Instant>)> = const { Cell::new((0, None)) };
}

fn swatch_index(c: i32) -> i32 {
    SWATCHES.iter().position(|s| *s == c).unwrap_or(0) as i32
}

// ------------------------------------------------------------------ arranque

pub(super) fn init(app: &crate::ui::AppWindow) -> Result<(), slint::PlatformError> {
    let bar = PrismaBar::new()?;
    wire_translations(bar.global::<T>());
    bar.on_dark(|| apply_from_bar(false));
    bar.on_gray(|| apply_from_bar(true));
    bar.on_clear(|| {
        prisma_clear();
        hide_bar();
        push();
    });
    BAR.with(|b| *b.borrow_mut() = Some(bar));

    let f = app.global::<Focus>();
    f.on_set_bool(|name, on| {
        with(|s| {
            let (fc, pc) = (&mut s.cfg.focus, &mut s.cfg.prisma);
            match name.as_str() {
                "bandEnabled" => fc.band_enabled = on,
                "blurEnabled" => fc.blur_enabled = on,
                "includeTaskbar" => fc.include_taskbar = on,
                "currentMonitorOnly" => fc.current_monitor_only = on,
                "blurAnimate" => fc.blur_animate = on,
                "prismaEnabled" => pc.enabled = on,
                "showToolbar" => pc.show_toolbar = on,
                _ => {}
            }
            s.dirty = true;
        });
        if name == "blurEnabled" && on {
            VEIL_SINCE.with(|v| v.set(Some(Instant::now())));
        }
        if name == "prismaEnabled" && !on {
            prisma_clear();
        }
        if (name == "showToolbar" || name == "prismaEnabled") && !on {
            hide_bar();
        }
        sync();
        push();
    });
    f.on_set_int(|name, v| {
        with(|s| {
            let (fc, pc) = (&mut s.cfg.focus, &mut s.cfg.prisma);
            let color = SWATCHES.get(v.max(0) as usize).copied().unwrap_or(0);
            match name.as_str() {
                "bandHeight" => fc.band_height = v,
                "bandAlpha" => fc.band_alpha = v,
                "bandColor" => fc.band_color = color,
                "blurAlpha" => fc.blur_alpha = v,
                "blurColor" => fc.blur_color = color,
                "blurDurationMs" => fc.blur_duration_ms = v,
                "prismaColor" => pc.color = color,
                "place" => pc.place = v,
                "offset" => pc.offset = v,
                "delay" => pc.delay = v,
                _ => {}
            }
            s.dirty = true;
        });
        refresh_bar_color();
        sync();
        push();
    });
    f.on_clear_prisma(|| {
        prisma_clear();
        push();
    });
    refresh_bar_color();
    sync();
    Ok(())
}

pub(super) fn apply_theme(dark: bool, accent: i32) {
    BAR.with(|b| {
        if let Some(w) = b.borrow().as_ref() {
            w.global::<Theme>().set_dark(dark);
            w.global::<Theme>().set_accent(accent);
        }
    });
}

/// Quitar todo de la pantalla (al salir).
pub(super) fn stop() {
    FX.with(|t| {
        if let Some(t) = t.borrow().as_ref() {
            t.stop();
        }
    });
    hide_bar();
    #[cfg(windows)]
    {
        use crate::platform::windows::fx;
        fx::band(None);
        fx::veil(None);
        fx::prisma_clear();
        fx::destroy_veils();
        fx::prisma_shutdown();
    }
}

pub(super) fn shutdown() {
    FX.with(|t| t.borrow_mut().take());
    BAR.with(|b| b.borrow_mut().take());
}

// ------------------------------------------------------------------ bucle

/// ¿Hace falta el temporizador de efectos? (no se gasta CPU si todo está apagado)
fn needed() -> bool {
    let (band, veil, prisma) =
        with(|s| (s.cfg.focus.band_enabled, s.cfg.focus.blur_enabled, s.cfg.prisma.enabled));
    band || veil || prisma || prisma_target().0 != 0
}

fn sync() {
    let on = needed();
    FX.with(|t| {
        let mut t = t.borrow_mut();
        if on {
            let timer = t.get_or_insert_with(Timer::default);
            if !timer.running() {
                timer.start(TimerMode::Repeated, Duration::from_millis(33), tick);
            }
        } else if let Some(timer) = t.as_ref() {
            timer.stop();
        }
    });
    if !on {
        tick();
    }
}

fn tick() {
    let (fc, pc) = with(|s| (s.cfg.focus.clone(), s.cfg.prisma.clone()));
    #[cfg(windows)]
    {
        use crate::platform::windows::fx;
        let scale = ui().map(|a| a.window().scale_factor()).unwrap_or(1.0);
        let alpha = |pct: i32| (pct.clamp(0, 100) as f32 * 2.55) as u8;
        fx::band(fc.band_enabled.then(|| fx::Band {
            height: (fc.band_height.clamp(40, 800) as f32 * scale) as i32,
            alpha: alpha(fc.band_alpha),
            color: fc.band_color as u32,
        }));
        let fade = if fc.blur_animate {
            let since = VEIL_SINCE.with(|v| v.get()).map(|t| t.elapsed().as_millis() as f32).unwrap_or(f32::MAX);
            (since / fc.blur_duration_ms.clamp(50, 2000) as f32).min(1.0)
        } else {
            1.0
        };
        fx::veil(fc.blur_enabled.then(|| fx::Veil {
            alpha: (alpha(fc.blur_alpha) as f32 * fade) as u8,
            color: fc.blur_color as u32,
            taskbar: fc.include_taskbar,
            current_monitor_only: fc.current_monitor_only,
        }));
        fx::prisma_follow();
        if pc.enabled && pc.show_toolbar {
            toolbar(&pc, scale);
        }
    }
    #[cfg(not(windows))]
    let _ = (fc, pc);
}

// ------------------------------------------------------------------ Prisma

fn prisma_target() -> (isize, bool) {
    #[cfg(windows)]
    {
        crate::platform::windows::fx::prisma_target()
    }
    #[cfg(not(windows))]
    {
        (0, false)
    }
}

fn prisma_clear() {
    #[cfg(windows)]
    crate::platform::windows::fx::prisma_clear();
    sync();
}

fn prisma_toggle(target: isize, gray: bool) {
    #[cfg(windows)]
    {
        // Usar Prisma lo enciende, como en la versión 1.
        with(|s| {
            if !s.cfg.prisma.enabled {
                s.cfg.prisma.enabled = true;
                s.dirty = true;
            }
        });
        crate::platform::windows::fx::prisma_toggle(target, gray);
    }
    #[cfg(not(windows))]
    let _ = (target, gray);
    sync();
    push();
}

fn apply_from_bar(gray: bool) {
    let anchor = ANCHOR.with(|a| a.get().0);
    if anchor != 0 {
        prisma_toggle(anchor, gray);
        update_bar_state(anchor);
    }
}

/// Atajos de Enfoque y Prisma.
pub(super) fn hotkey(name: &str) {
    match name {
        "toggleBand" | "toggleVeil" => {
            let on = with(|s| {
                let f = &mut s.cfg.focus;
                s.dirty = true;
                if name == "toggleBand" {
                    f.band_enabled = !f.band_enabled;
                    false
                } else {
                    f.blur_enabled = !f.blur_enabled;
                    f.blur_enabled
                }
            });
            if on {
                VEIL_SINCE.with(|v| v.set(Some(Instant::now())));
            }
            sync();
        }
        "prismaDark" | "prismaGray" => {
            #[cfg(windows)]
            prisma_toggle(crate::platform::windows::fx::foreground_root(), name == "prismaGray");
        }
        _ => {}
    }
    push();
}

fn refresh_bar_color() {
    let c = with(|s| s.cfg.prisma.color) as u32;
    let color = slint::Color::from_rgb_u8((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8);
    BAR.with(|b| {
        if let Some(w) = b.borrow().as_ref() {
            w.set_bar(color);
        }
    });
}

fn update_bar_state(anchor: isize) {
    let (target, gray) = prisma_target();
    BAR.with(|b| {
        if let Some(w) = b.borrow().as_ref() {
            let on = target != 0 && target == anchor;
            w.set_applied(on);
            w.set_current(if !on { 0 } else if gray { 2 } else { 1 });
        }
    });
}

fn hide_bar() {
    ANCHOR.with(|a| a.set((0, None)));
    BAR.with(|b| {
        if let Some(w) = b.borrow().as_ref() {
            let _ = w.hide();
        }
    });
}

/// La barra aparece al dejar el ratón sobre la franja del título de una ventana.
#[cfg(windows)]
fn toolbar(pc: &crate::config::PrismaConfig, scale: f32) {
    use crate::platform::windows::fx;
    let (cx, cy) = fx::cursor();
    let title_band = (40.0 * scale) as i32;
    let over_bar = BAR.with(|b| {
        b.borrow().as_ref().is_some_and(|w| {
            let p = w.window().position();
            let s = w.window().size();
            w.window().is_visible() && cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32
        })
    });
    let under = fx::window_under_cursor().filter(|(_, r)| cy >= r.top && cy < r.top + title_band);
    let (anchor, left_at) = ANCHOR.with(|a| a.get());

    match under {
        Some((hwnd, r)) => {
            let (h, since) = HOVER.with(|v| v.get());
            let since = if h == hwnd { since.unwrap_or_else(Instant::now) } else { Instant::now() };
            HOVER.with(|v| v.set((hwnd, Some(since))));
            let wait = Duration::from_millis(pc.delay.clamp(0, 100) as u64 * 15);
            if anchor != hwnd && since.elapsed() >= wait {
                show_bar(hwnd, r, pc, scale);
            } else if anchor == hwnd {
                ANCHOR.with(|a| a.set((hwnd, None)));
            }
        }
        None if over_bar => ANCHOR.with(|a| a.set((anchor, None))),
        None => {
            HOVER.with(|v| v.set((0, None)));
            if anchor != 0 {
                // Un margen para poder llevar el ratón de la ventana a la barra.
                let left = left_at.unwrap_or_else(Instant::now);
                ANCHOR.with(|a| a.set((anchor, Some(left))));
                if left.elapsed() > Duration::from_millis(400) {
                    hide_bar();
                }
            }
        }
    }
}

#[cfg(windows)]
fn show_bar(anchor: isize, frame: windows::Win32::Foundation::RECT, pc: &crate::config::PrismaConfig, scale: f32) {
    use crate::platform::windows::{desk, shell};
    ANCHOR.with(|a| a.set((anchor, None)));
    update_bar_state(anchor);
    BAR.with(|b| {
        let b = b.borrow();
        let Some(w) = b.as_ref() else { return };
        let previous = desk::foreground();
        let first = !w.window().is_visible();
        if first {
            let _ = w.show();
        }
        let size = w.window().size();
        let (bw, bh) = (size.width as i32, size.height as i32);
        let off = (pc.offset.clamp(0, 100) as f32 * 2.0 * scale) as i32 + (8.0 * scale) as i32;
        let x = match pc.place {
            1 => frame.left + off,
            2 => frame.right - bw - off,
            _ => frame.left + (frame.right - frame.left - bw) / 2,
        };
        let y = frame.top + ((36.0 * scale) as i32 - bh).max(0) / 2;
        w.window().set_position(slint::PhysicalPosition::new(x, y.max(frame.top)));
        if first {
            let weak = w.as_weak();
            Timer::single_shot(Duration::from_millis(30), move || {
                let Some(w) = weak.upgrade() else { return };
                if let Some(hwnd) = shell::hwnd(w.window()) {
                    shell::style_window(hwnd, true);
                    desk::make_toast(hwnd);
                    desk::set_foreground(previous);
                }
            });
        }
    });
}

// ------------------------------------------------------------------ interfaz

fn rows_changed(current: ModelRc<HotkeyRow>, rows: &[HotkeyRow]) -> bool {
    !(current.row_count() == rows.len() && rows.iter().enumerate().all(|(i, r)| current.row_data(i).as_ref() == Some(r)))
}

pub(super) fn push() {
    let Some(app) = ui() else { return };
    let f = app.global::<Focus>();
    f.set_available(crate::platform::HAS_FX);
    let (fc, pc, hk) = with(|s| (s.cfg.focus.clone(), s.cfg.prisma.clone(), s.cfg.hotkeys.clone()));
    f.set_band(fc.band_enabled);
    f.set_band_height(fc.band_height);
    f.set_band_alpha(fc.band_alpha);
    f.set_band_color(swatch_index(fc.band_color));
    f.set_veil(fc.blur_enabled);
    f.set_veil_alpha(fc.blur_alpha);
    f.set_veil_color(swatch_index(fc.blur_color));
    f.set_taskbar(fc.include_taskbar);
    f.set_animate(fc.blur_animate);
    f.set_duration(fc.blur_duration_ms);
    f.set_current_monitor(fc.current_monitor_only);
    f.set_prisma(pc.enabled);
    f.set_toolbar(pc.show_toolbar);
    f.set_bar_color(swatch_index(pc.color));
    f.set_place(pc.place);
    f.set_offset(pc.offset);
    f.set_delay(pc.delay);
    let (target, gray) = prisma_target();
    f.set_prisma_status(
        if target == 0 {
            String::new()
        } else {
            i18n::tr_args("rs_prisma_activo", &[&i18n::tr(if gray { "gris" } else { "oscuro" })])
        }
        .into(),
    );

    let failed = keys::failed_hotkeys();
    let set = |names: &[&str], current: ModelRc<HotkeyRow>, apply: &dyn Fn(ModelRc<HotkeyRow>)| {
        let rows: Vec<HotkeyRow> = names.iter().map(|n| keys::hotkey_row(n, &hk, &failed)).collect();
        if rows_changed(current, &rows) {
            apply(ModelRc::new(VecModel::from(rows)));
        }
    };
    set(&["toggleBand"], f.get_band_keys(), &|m| f.set_band_keys(m));
    set(&["toggleVeil"], f.get_veil_keys(), &|m| f.set_veil_keys(m));
    set(&["prismaDark", "prismaGray"], f.get_prisma_keys(), &|m| f.set_prisma_keys(m));
}
