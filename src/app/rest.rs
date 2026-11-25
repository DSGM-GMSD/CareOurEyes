//! Pausas para la vista: une el temporizador (`crate::breaks`) con la página
//! Pausa, el aviso pequeño y la pausa a pantalla completa.

use super::{debug_log, ui, with, wire_translations};
use crate::breaks::{clock, Breaks, Context, Event, Paused, Phase, BLINK_MS};
use crate::config::RestConfig;
use crate::i18n;
use crate::platform;
use crate::ui::{BreakOverlay, BreakToast, Rest, Theme, T};
use slint::{ComponentHandle, Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// Qué muestra el aviso pequeño.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Toast {
    Hidden,
    Warn,
    Break,
    Blink,
}

thread_local! {
    static TICK: RefCell<Option<Timer>> = const { RefCell::new(None) };
    static LAST: Cell<Option<Instant>> = const { Cell::new(None) };
    static TOAST: RefCell<Option<BreakToast>> = const { RefCell::new(None) };
    static OVERLAY: RefCell<Option<BreakOverlay>> = const { RefCell::new(None) };
    static SHOWN: Cell<Toast> = const { Cell::new(Toast::Hidden) };
    static OVERLAY_ON: Cell<bool> = const { Cell::new(false) };
    static TIP: Cell<usize> = const { Cell::new(0) };
}

const TIPS: [&str; 4] = ["rs_tip_1", "rs_tip_2", "rs_tip_3", "rs_tip_4"];
const STYLES: [&str; 3] = ["light", "fullscreen", "strict"];

fn rest_cfg() -> RestConfig {
    with(|s| s.cfg.rest.clone())
}

fn style_index(c: &RestConfig) -> i32 {
    STYLES.iter().position(|x| *x == c.break_style).unwrap_or(0) as i32
}

// ------------------------------------------------------------------ arranque

pub(super) fn init(app: &crate::ui::AppWindow) -> Result<(), slint::PlatformError> {
    let toast = BreakToast::new()?;
    wire_translations(toast.global::<T>());
    toast.on_take_break(take_break);
    toast.on_delay(postpone);
    toast.on_skip(skip);
    toast.on_dismiss(|| {
        with(|s| s.breaks.blink_showing_ms = 0);
        hide_toast();
    });
    TOAST.with(|t| *t.borrow_mut() = Some(toast));

    let overlay = BreakOverlay::new()?;
    wire_translations(overlay.global::<T>());
    overlay.on_delay(postpone);
    overlay.on_skip(skip);
    // Alt+F4 en la pausa: si es estricta no se cierra; si no, cuenta como saltarla.
    overlay.window().on_close_requested(|| {
        if rest_cfg().break_style != "strict" {
            skip();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
    OVERLAY.with(|o| *o.borrow_mut() = Some(overlay));

    let r = app.global::<Rest>();
    r.on_set_enabled(|on| {
        with(|s| {
            s.cfg.rest.enabled = on;
            s.dirty = true;
            s.breaks = Breaks::new(&s.cfg.rest);
        });
        sync_timer();
        if !on {
            hide_all();
        }
        push();
    });
    r.on_set_int(|name, v| {
        let restart = with(|s| {
            let c = &mut s.cfg.rest;
            s.dirty = true;
            match name.as_str() {
                "workMinutes" => {
                    c.work_minutes = v;
                    return true;
                }
                "shortBreakMinutes" => {
                    c.short_break_minutes = v;
                    c.short_break_seconds = false;
                }
                "longBreakMinutes" => c.long_break_minutes = v,
                "longBreakEvery" => c.long_break_every = v,
                "breakStyle" => c.break_style = STYLES.get(v as usize).unwrap_or(&"light").to_string(),
                "blinkIntervalMinutes" => {
                    c.blink_interval_minutes = v;
                    let c = c.clone();
                    s.breaks.reset_blink(&c);
                }
                "preBreakSeconds" => c.pre_break_seconds = v,
                "delayTimesMax" => c.delay_times_max = v,
                "delayMinutes" => c.delay_minutes = v,
                "pauseIdleMinutes" => {
                    c.pause_idle_amount = v;
                    c.pause_idle_unit = "minutes".into();
                }
                _ => {}
            }
            false
        });
        if restart {
            // Cambió la duración del trabajo: se empieza a contar de nuevo.
            with(|s| {
                if s.breaks.phase == Phase::Work {
                    s.breaks.restart_work(&s.cfg.rest);
                }
            });
        }
        push();
    });
    r.on_set_bool(|name, on| {
        with(|s| {
            let c = &mut s.cfg.rest;
            s.dirty = true;
            match name.as_str() {
                "pomodoro" => c.pomodoro = on,
                "blinkEnabled" => {
                    c.blink_enabled = on;
                    let c = c.clone();
                    s.breaks.reset_blink(&c);
                }
                "promptTop" => c.prompt_place = if on { "top" } else { "bottom" }.into(),
                "alertSound" => c.alert_sound = on,
                "allowDelay" => c.allow_delay = on,
                "pauseWhenIdle" => c.pause_when_idle = on,
                "doNotDisturbFullscreen" => c.do_not_disturb_fullscreen = on,
                _ => {}
            }
        });
        sync_timer();
        push();
    });
    r.on_take_break(take_break);
    r.on_pause(|minutes| {
        with(|s| s.breaks.pause((minutes > 0).then_some(minutes as i64)));
        hide_all();
        push();
    });
    r.on_resume(|| {
        with(|s| s.breaks.resume());
        push();
    });
    r.on_restart(|| {
        with(|s| {
            s.breaks.resume();
            s.breaks.restart_work(&s.cfg.rest);
        });
        hide_all();
        push();
    });

    sync_timer();
    Ok(())
}

/// El temporizador solo corre con las pausas activadas.
fn sync_timer() {
    let on = with(|s| s.cfg.rest.enabled);
    TICK.with(|t| {
        let mut t = t.borrow_mut();
        if on {
            let timer = t.get_or_insert_with(Timer::default);
            if !timer.running() {
                LAST.with(|l| l.set(Some(Instant::now())));
                timer.start(TimerMode::Repeated, Duration::from_secs(1), tick);
            }
        } else if let Some(timer) = t.as_ref() {
            timer.stop();
        }
    });
}

pub(super) fn apply_theme(dark: bool, accent: i32) {
    TOAST.with(|t| {
        if let Some(w) = t.borrow().as_ref() {
            w.global::<Theme>().set_dark(dark);
            w.global::<Theme>().set_accent(accent);
        }
    });
    OVERLAY.with(|o| {
        if let Some(w) = o.borrow().as_ref() {
            w.global::<Theme>().set_dark(dark);
            w.global::<Theme>().set_accent(accent);
        }
    });
}

/// Antes de salir: quitar todo lo que esté en pantalla.
pub(super) fn stop() {
    TICK.with(|t| {
        if let Some(timer) = t.borrow().as_ref() {
            timer.stop();
        }
    });
    hide_all();
}

/// Soltar las ventanas y el temporizador mientras Slint sigue vivo.
pub(super) fn shutdown() {
    TICK.with(|t| t.borrow_mut().take());
    TOAST.with(|t| t.borrow_mut().take());
    OVERLAY.with(|o| o.borrow_mut().take());
}

// ------------------------------------------------------------------ cada segundo

fn tick() {
    let now = Instant::now();
    let dt = LAST.with(|l| l.replace(Some(now))).map(|t| now.duration_since(t).as_millis() as i64).unwrap_or(1000);
    // Un salto grande (equipo suspendido) cuenta como ausencia.
    let idle = platform::idle_ms().max(if dt > 60_000 { dt } else { 0 });
    let fullscreen = !OVERLAY_ON.with(|o| o.get()) && platform::foreground_fullscreen();
    let events = with(|s| {
        let c = s.cfg.rest.clone();
        s.breaks.tick(dt, &c, Context { idle_ms: idle, fullscreen })
    });
    for ev in events {
        debug_log(&format!("pausas: {ev:?}"));
        handle(ev);
    }
    refresh_windows();
    push();
}

fn handle(ev: Event) {
    let c = rest_cfg();
    match ev {
        Event::Warn => show_toast(Toast::Warn),
        Event::BreakStarted { .. } => {
            if c.alert_sound {
                platform::chime();
            }
            TIP.with(|t| t.set((t.get() + 1) % TIPS.len()));
            if c.break_style == "light" {
                show_toast(Toast::Break);
            } else {
                hide_toast();
                show_overlay();
            }
        }
        Event::BreakEnded => {
            if c.alert_sound {
                platform::chime();
            }
            hide_all();
        }
        Event::BlinkStarted => {
            if SHOWN.with(|s| s.get()) == Toast::Hidden {
                show_toast(Toast::Blink);
            }
        }
        Event::BlinkEnded => {
            if SHOWN.with(|s| s.get()) == Toast::Blink {
                hide_toast();
            }
        }
        Event::Rested => hide_all(),
    }
}

fn take_break() {
    let ev = with(|s| {
        let c = s.cfg.rest.clone();
        s.breaks.take_break_now(&c)
    });
    // Si el temporizador estaba apagado (vista previa), que corra.
    LAST.with(|l| l.set(Some(Instant::now())));
    sync_timer();
    handle(ev);
    refresh_windows();
    push();
}

fn postpone() {
    let ok = with(|s| {
        let c = s.cfg.rest.clone();
        s.breaks.postpone(&c)
    });
    if ok {
        hide_all();
    }
    push();
}

fn skip() {
    with(|s| {
        let c = s.cfg.rest.clone();
        s.breaks.skip(&c);
    });
    hide_all();
    push();
}

// ------------------------------------------------------------------ ventanas

fn delay_text(c: &RestConfig) -> String {
    i18n::tr_args("rs_posponer_n", &[&c.delay_minutes.clamp(1, 60).to_string()])
}

/// Actualiza los textos y cuentas atrás de lo que esté visible.
fn refresh_windows() {
    let (b, c) = with(|s| (s.breaks.clone(), s.cfg.rest.clone()));
    let kind = SHOWN.with(|s| s.get());
    if kind != Toast::Hidden {
        TOAST.with(|t| {
            let t = t.borrow();
            let Some(w) = t.as_ref() else { return };
            w.set_can_delay(b.can_delay(&c));
            w.set_delay_text(delay_text(&c).into());
            match kind {
                Toast::Warn => {
                    w.set_kind(0);
                    w.set_heading(i18n::tr(if b.next_is_long(&c) { "proxima_pausa_larga_en" } else { "proxima_pausa_corta_en" }).into());
                    w.set_detail(i18n::tr("rs_pausa_en_texto").into());
                    w.set_countdown(clock(b.left_ms).into());
                }
                Toast::Break => {
                    let long = matches!(b.phase, Phase::Break { long: true });
                    w.set_kind(1);
                    w.set_heading(i18n::tr(if long { "descanso_largo" } else { "toca_descansar_la_vista" }).into());
                    w.set_detail(i18n::tr(TIPS[TIP.with(|t| t.get())]).into());
                    w.set_countdown(clock(b.left_ms).into());
                }
                Toast::Blink => {
                    w.set_kind(2);
                    w.set_heading(i18n::tr("rs_parpadeo_titulo").into());
                    w.set_detail(i18n::tr("rs_parpadeo_detalle").into());
                    w.set_countdown(clock(b.blink_showing_ms).into());
                }
                Toast::Hidden => {}
            }
        });
    }
    if OVERLAY_ON.with(|o| o.get()) {
        OVERLAY.with(|o| {
            let o = o.borrow();
            let Some(w) = o.as_ref() else { return };
            let long = matches!(b.phase, Phase::Break { long: true });
            let total = if long { c.long_break_minutes.max(1) as i64 * 60_000 } else { c.short_break_ms() };
            w.set_heading(i18n::tr(if long { "descanso_largo" } else { "toca_descansar_la_vista" }).into());
            w.set_countdown(clock(b.left_ms).into());
            w.set_tip(i18n::tr(TIPS[TIP.with(|t| t.get())]).into());
            w.set_progress(1.0 - (b.left_ms as f32 / total as f32));
            w.set_strict(c.break_style == "strict");
            w.set_can_delay(b.can_delay(&c));
            w.set_delay_text(delay_text(&c).into());
        });
    }
    let _ = BLINK_MS;
}

fn show_toast(kind: Toast) {
    SHOWN.with(|s| s.set(kind));
    refresh_windows();
    let top = rest_cfg().prompt_place != "bottom";
    TOAST.with(|t| {
        let t = t.borrow();
        let Some(w) = t.as_ref() else { return };
        #[cfg(windows)]
        let previous = platform::windows::desk::foreground();
        #[cfg(target_os = "linux")]
        let previous = platform::linux::desk::active_window();
        // macOS: se coloca antes de mostrarlo (al mostrarlo no le quita el foco a
        // la app activa, porque la app no se activa).
        #[cfg(target_os = "macos")]
        place_toast_mac(w.window(), top);
        let _ = w.show();
        // Ya existe la ventana nativa un instante después: estilo, posición y
        // devolver el foco a lo que la persona estaba usando.
        let weak = w.as_weak();
        Timer::single_shot(Duration::from_millis(30), move || {
            let Some(w) = weak.upgrade() else { return };
            #[cfg(windows)]
            {
                use crate::platform::windows::{desk, shell};
                if let Some(hwnd) = shell::hwnd(w.window()) {
                    shell::style_window(hwnd, true);
                    desk::make_toast(hwnd);
                    desk::place_toast(hwnd, top, w.window().scale_factor());
                    desk::set_foreground(previous);
                }
            }
            #[cfg(target_os = "linux")]
            {
                use crate::platform::linux::desk;
                if let Some(id) = desk::xid(w.window()) {
                    desk::make_toast(id);
                }
                // Centrado arriba o abajo de la pantalla principal, como en Windows.
                if let Some((x, y, width, height)) = desk::work_rect() {
                    let size = w.window().size();
                    let margin = (16.0 * w.window().scale_factor()) as i32;
                    let left = x + (width - size.width as i32) / 2;
                    let tp = if top { y + margin } else { y + height - size.height as i32 - margin };
                    w.window().set_position(slint::PhysicalPosition::new(left, tp));
                }
                if previous != 0 && desk::xid(w.window()) != Some(previous) {
                    desk::activate(previous);
                }
            }
            #[cfg(target_os = "macos")]
            place_toast_mac(w.window(), top);
            #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
            {
                let _ = (w, top);
            }
        });
    });
}

/// Centrado arriba o abajo del área útil de la pantalla principal (macOS).
#[cfg(target_os = "macos")]
fn place_toast_mac(window: &slint::Window, top: bool) {
    let Some((x, y, width, height)) = platform::macos::desk::work_rect() else { return };
    let scale = window.scale_factor().max(1.0) as f64;
    let size = window.size();
    // La primera vez aún no tiene tamaño: se usa el de diseño.
    let (w, h) = if size.width == 0 { (400.0, 120.0) } else { (size.width as f64 / scale, size.height as f64 / scale) };
    let left = x + (width - w) / 2.0;
    let tp = if top { y + 16.0 } else { y + height - h - 16.0 };
    window.set_position(slint::LogicalPosition::new(left as f32, tp as f32));
}

fn hide_toast() {
    SHOWN.with(|s| s.set(Toast::Hidden));
    TOAST.with(|t| {
        if let Some(w) = t.borrow().as_ref() {
            let _ = w.hide();
        }
    });
}

fn show_overlay() {
    OVERLAY_ON.with(|o| o.set(true));
    refresh_windows();
    OVERLAY.with(|o| {
        let o = o.borrow();
        let Some(w) = o.as_ref() else { return };
        #[cfg(windows)]
        {
            let (x, y, width, height) = platform::windows::desk::primary_rect();
            w.window().set_position(slint::PhysicalPosition::new(x, y));
            w.window().set_size(slint::PhysicalSize::new(width as u32, height as u32));
        }
        #[cfg(target_os = "linux")]
        if let Some((x, y, width, height)) = platform::linux::desk::primary_rect() {
            w.window().set_position(slint::PhysicalPosition::new(x, y));
            w.window().set_size(slint::PhysicalSize::new(width as u32, height as u32));
        }
        #[cfg(target_os = "macos")]
        {
            // Toda la pantalla principal, encima de lo demás (sin abrir otro
            // escritorio como haría la pantalla completa de macOS).
            let (x, y, width, height) = platform::macos::desk::primary_rect();
            w.window().set_position(slint::LogicalPosition::new(x as f32, y as f32));
            w.window().set_size(slint::LogicalSize::new(width as f32, height as f32));
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        w.window().set_size(slint::LogicalSize::new(980.0, 700.0));
        let _ = w.show();
        let weak = w.as_weak();
        Timer::single_shot(Duration::from_millis(30), move || {
            let Some(w) = weak.upgrade() else { return };
            #[cfg(windows)]
            {
                use crate::platform::windows::{desk, shell};
                if let Some(hwnd) = shell::hwnd(w.window()) {
                    shell::style_window(hwnd, true);
                    let (x, y, width, height) = desk::primary_rect();
                    w.window().set_position(slint::PhysicalPosition::new(x, y));
                    w.window().set_size(slint::PhysicalSize::new(width as u32, height as u32));
                    shell::bring_to_front(hwnd);
                    desk::cover_other_monitors();
                }
            }
            #[cfg(target_os = "linux")]
            {
                use crate::platform::linux::desk;
                if let Some(id) = desk::xid(w.window()) {
                    desk::make_toast(id);
                    desk::activate(id);
                }
                // Pantalla completa sobre la principal (tapa también los paneles).
                w.window().set_fullscreen(true);
            }
            #[cfg(target_os = "macos")]
            {
                use slint::winit_030::WinitWindowAccessor;
                w.window().with_winit_window(|w| w.focus_window());
            }
            #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
            let _ = w;
        });
    });
}

fn hide_overlay() {
    if !OVERLAY_ON.with(|o| o.replace(false)) {
        return;
    }
    #[cfg(windows)]
    platform::windows::desk::uncover_other_monitors();
    OVERLAY.with(|o| {
        if let Some(w) = o.borrow().as_ref() {
            w.window().set_fullscreen(false);
            let _ = w.hide();
        }
    });
}

fn hide_all() {
    hide_toast();
    hide_overlay();
}

// ------------------------------------------------------------------ página

/// Rellena la página Pausa, la pestaña Temporizador y el contador de la barra lateral.
pub(super) fn push() {
    let Some(app) = ui() else { return };
    let r = app.global::<Rest>();
    let (b, c) = with(|s| (s.breaks.clone(), s.cfg.rest.clone()));
    r.set_enabled(c.enabled);
    r.set_paused(b.paused.is_some());
    r.set_held(b.held);
    r.set_next_long(b.next_is_long(&c));
    let (phase, total) = match b.phase {
        Phase::Work => (0, c.work_minutes.clamp(1, 240) as i64 * 60_000),
        Phase::Break { long: false } => (1, c.short_break_ms()),
        Phase::Break { long: true } => (2, c.long_break_minutes.clamp(1, 120) as i64 * 60_000),
    };
    r.set_phase(phase);
    r.set_countdown(clock(b.left_ms).into());
    r.set_progress((1.0 - b.left_ms as f32 / total.max(1) as f32).clamp(0.0, 1.0));
    let status = match (b.paused, b.phase) {
        (Some(Paused::Until(ms)), _) => i18n::tr_args("rs_estado_pausa_hasta", &[&clock(ms)]),
        (Some(Paused::Indefinitely), _) => i18n::tr("rs_estado_pausa"),
        (None, Phase::Work) if b.held => i18n::tr("la_pausa_espera_a_que_salgas_de_la_ventana_c"),
        (None, Phase::Work) => {
            i18n::tr(if b.next_is_long(&c) { "proxima_pausa_larga_en" } else { "proxima_pausa_corta_en" }).to_uppercase()
        }
        (None, Phase::Break { long }) => i18n::tr(if long { "descanso_largo" } else { "descanso_corto" }).to_uppercase(),
    };
    r.set_status(status.into());

    r.set_work(c.work_minutes);
    r.set_short(if c.short_break_seconds { (c.short_break_minutes / 60).max(1) } else { c.short_break_minutes });
    r.set_long(c.long_break_minutes);
    r.set_every(c.long_break_every);
    r.set_pomodoro(c.pomodoro);
    r.set_style(style_index(&c));
    r.set_blink(c.blink_enabled);
    r.set_blink_every(c.blink_interval_minutes);
    r.set_warn_seconds(c.pre_break_seconds);
    r.set_place_top(c.prompt_place != "bottom");
    r.set_sound(c.alert_sound);
    r.set_allow_delay(c.allow_delay);
    r.set_delay_times(c.delay_times_max);
    r.set_delay_minutes(c.delay_minutes);
    r.set_idle(c.pause_when_idle);
    r.set_idle_minutes(if c.pause_idle_unit == "seconds" { (c.pause_idle_amount / 60).max(1) } else { c.pause_idle_amount });
    r.set_dnd(c.do_not_disturb_fullscreen);
}

/// Solo para capturas en compilaciones de desarrollo.
pub(super) fn debug_show(what: &str) {
    match what {
        "toast" => {
            with(|s| {
                s.breaks.left_ms = 25_000;
            });
            show_toast(Toast::Warn)
        }
        _ => take_break(),
    }
}
