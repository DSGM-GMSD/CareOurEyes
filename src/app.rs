//! Lógica de la app: guarda el estado, calcula qué aplicar a cada pantalla y
//! mantiene la interfaz al día.
//!
//! Todo corre en el hilo de la interfaz. El estado vive en `STATE` y cada
//! acción sigue el mismo camino: modificar el estado → aplicar a las pantallas
//! → `push()` para refrescar la interfaz.

mod focus;
mod keys;
mod rest;

use crate::color;
use crate::config::{AppConfig, Mode, MonitorConfig};
use crate::i18n;
use crate::net;
use crate::platform::{self, Backend, DisplayInfo};
use crate::schedule;
use crate::ui::{App, AppWindow, MonitorItem, Theme, TrayMenu, T};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

/// Orden de los perfiles en la cuadrícula de la página Pantalla.
const GRID: [Mode; 8] = [Mode::Read, Mode::Edit, Mode::Game, Mode::Movie, Mode::Health, Mode::Office, Mode::Custom, Mode::Pause];
const ACCENTS: [&str; 7] = ["emerald", "cyan", "blue", "indigo", "violet", "amber", "rose"];
const THEMES: [&str; 3] = ["dark", "light", "system"];

const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
const TRAY_PNG: &[u8] = include_bytes!("../assets/tray-icon.png");

struct State {
    cfg: AppConfig,
    backend: Box<dyn Backend>,
    displays: Vec<DisplayInfo>,
    selected: usize,
    /// Brillo y temperatura escritos ahora en cada pantalla.
    live: HashMap<String, (f64, f64)>,
    /// Hacia dónde va cada pantalla (la animación avanza hacia aquí).
    target: HashMap<String, (f64, f64)>,
    dirty: bool,
    update_text: String,
    update_url: Option<String>,
    checking: bool,
    lookup: i32,
    has_tray: bool,
    placed: bool,
    /// Dónde estaba la ventana al ocultarla (Linux: el gestor de ventanas la
    /// centraría al volver a mostrarla; Windows la deja donde estaba).
    last_pos: Option<slint::PhysicalPosition>,
    /// Último periodo visto (día o noche), para cambiar la pestaña Día/Noche sola.
    last_night: Option<bool>,
    /// Vista previa del otro periodo: Some(true) = se muestra la noche aunque sea de día.
    preview: Option<bool>,
    /// Último cambio hecho durante la vista previa (se cierra sola tras un minuto quieta).
    last_touch: std::time::Instant,
    /// Temporizador de pausas.
    breaks: crate::breaks::Breaks,
    /// Perfil que impone una regla por programa mientras su ventana está activa.
    rule_override: Option<Mode>,
    /// Último programa en primer plano (para crear reglas).
    last_window: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static UI: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
    static MENU: RefCell<Option<TrayMenu>> = const { RefCell::new(None) };
    // Los temporizadores se guardan en Option para poder soltarlos antes de que
    // Slint cierre (si no, Windows aborta el proceso al salir).
    static ANIM: RefCell<Option<Timer>> = const { RefCell::new(None) };
    static MENU_WATCH: RefCell<Option<Timer>> = const { RefCell::new(None) };
    static PREVIEW_TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

fn stop_anim() {
    ANIM.with(|a| {
        if let Some(t) = a.borrow().as_ref() {
            t.stop();
        }
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("estado sin iniciar")))
}

fn ui() -> Option<AppWindow> {
    UI.with(|u| u.borrow().as_ref().and_then(|w| w.upgrade()))
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

// ------------------------------------------------------------------ cálculo

impl State {
    fn current_id(&self) -> Option<String> {
        self.displays.get(self.selected).map(|d| d.id.clone())
    }

    /// Pantallas a las que afecta un cambio: la elegida, o todas si están sincronizadas.
    fn affected(&self) -> Vec<String> {
        if self.cfg.sync_monitors {
            self.displays.iter().map(|d| d.id.clone()).collect()
        } else {
            self.current_id().into_iter().collect()
        }
    }

    /// Copia perfil y valores de la pantalla elegida a las demás.
    fn copy_selected_to_all(&mut self) {
        let source = self.current_id().and_then(|id| self.cfg.monitors.get(&id).cloned());
        let Some(src) = source else { return };
        for d in &self.displays {
            if let Some(mc) = self.cfg.monitors.get_mut(&d.id) {
                let (id, name, extra) = (mc.display_id.clone(), mc.display_name.clone(), mc.extra.clone());
                *mc = src.clone();
                mc.display_id = id;
                mc.display_name = name;
                mc.extra = extra;
            }
        }
    }

    fn kelvin_bounds(&self) -> (i32, i32) {
        color::kelvin_range(self.cfg.wider_kelvin)
    }

    fn min_brightness(&self) -> f64 {
        color::brightness_range(self.cfg.wider_brightness).0
    }

    /// Valores que tocan ahora según el perfil y la hora.
    fn wanted(&self, id: &str) -> (f64, f64) {
        let Some(mc) = self.cfg.monitors.get(id) else { return (100.0, 6500.0) };
        // Una regla activa manda sobre el perfil elegido (sin cambiarlo).
        let mode = self.rule_override.unwrap_or_else(|| mc.mode());
        if mode == Mode::Pause {
            return (100.0, color::NEUTRAL_K as f64);
        }
        let s = mc.settings(mode);
        let f = match self.preview {
            Some(true) => 1.0,
            Some(false) => 0.0,
            None => schedule::night_factor(&self.cfg.schedule, schedule::now_minutes()),
        };
        let b = lerp(s.brightness, s.night_brightness, f);
        let k = if self.cfg.temperature_disabled {
            color::NEUTRAL_K as f64
        } else {
            lerp(s.kelvin as f64, s.night_kelvin as f64, f)
        };
        (b.clamp(self.min_brightness(), 100.0), k)
    }

    fn write(&mut self, id: &str, b: f64, k: f64) {
        let standard = self.cfg.standard_gamma;
        self.backend.apply(id, b, k, standard);
        self.live.insert(id.to_string(), (b, k));
    }

    /// Recalcula los objetivos. `smooth` = deslizar (si la opción está activa);
    /// `force` = escribir aunque no haya cambiado (tras reanudar el equipo).
    fn retarget(&mut self, smooth: bool, force: bool) -> bool {
        let ids: Vec<String> = self.displays.iter().filter(|d| d.gamma).map(|d| d.id.clone()).collect();
        let animate = smooth && self.cfg.smooth_changes;
        let mut needs_anim = false;
        for id in ids {
            let t = self.wanted(&id);
            self.target.insert(id.clone(), t);
            let live = self.live.get(&id).copied();
            let same = live.is_some_and(|(b, k)| (b - t.0).abs() < 0.05 && (k - t.1).abs() < 0.5);
            if same && !force {
                continue;
            }
            if animate && live.is_some() {
                needs_anim = true;
            } else {
                self.write(&id, t.0, t.1);
            }
        }
        needs_anim
    }

    /// Un paso de la animación. Devuelve `true` cuando todas llegaron.
    fn step(&mut self) -> bool {
        let mut done = true;
        let pending: Vec<(String, (f64, f64))> = self.target.iter().map(|(k, v)| (k.clone(), *v)).collect();
        for (id, (tb, tk)) in pending {
            let Some((b, k)) = self.live.get(&id).copied() else { continue };
            if (b - tb).abs() < 0.05 && (k - tk).abs() < 0.5 {
                continue;
            }
            let mut nb = b + (tb - b) * 0.22;
            let mut nk = k + (tk - k) * 0.22;
            if (tb - nb).abs() < 0.3 {
                nb = tb;
            }
            if (tk - nk).abs() < 4.0 {
                nk = tk;
            }
            self.write(&id, nb, nk);
            if nb != tb || nk != tk {
                done = false;
            }
        }
        done
    }

    fn refresh_displays(&mut self) {
        let prev = self.current_id();
        self.displays = self.backend.displays();
        for d in &self.displays {
            let mc = self.cfg.monitors.entry(d.id.clone()).or_insert_with(|| MonitorConfig::new(&d.id, &d.name));
            if mc.display_name != d.name {
                mc.display_name = d.name.clone();
                self.dirty = true;
            }
        }
        self.selected = prev
            .and_then(|id| self.displays.iter().position(|d| d.id == id))
            .unwrap_or(0);
        // Lo escrito antes de reconectar ya no vale.
        self.live.retain(|id, _| self.displays.iter().any(|d| &d.id == id));
    }

    fn set_mode(&mut self, mode: Mode) {
        for id in self.affected() {
            if let Some(mc) = self.cfg.monitors.get_mut(&id) {
                mc.mode = mode.id().to_string();
                if mode != Mode::Pause {
                    mc.resume_mode = Some(mode.id().to_string());
                }
            }
        }
        self.dirty = true;
    }

    /// Cambia un valor del perfil activo (o del de noche). En Pausa pasa a Personalizado.
    fn edit_values(&mut self, night: bool, f: impl Fn(&mut crate::config::ModeSettings, bool)) {
        self.last_touch = std::time::Instant::now();
        for id in self.affected() {
            let Some(mc) = self.cfg.monitors.get_mut(&id) else { continue };
            let mut mode = mc.mode();
            if mode == Mode::Pause {
                mode = Mode::Custom;
                mc.mode = mode.id().to_string();
                mc.resume_mode = Some(mode.id().to_string());
            }
            if let Some(s) = mc.settings_mut(mode) {
                f(s, night);
            }
        }
        self.dirty = true;
    }

    fn all_paused(&self) -> bool {
        !self.displays.is_empty()
            && self.displays.iter().all(|d| self.cfg.monitors.get(&d.id).is_some_and(|m| m.mode() == Mode::Pause))
    }

    /// Pausar el filtro en todas las pantallas, o volver al último perfil.
    fn toggle_pause(&mut self) {
        let resume = !self.all_paused();
        for d in &self.displays {
            if let Some(mc) = self.cfg.monitors.get_mut(&d.id) {
                if resume {
                    if mc.mode() != Mode::Pause {
                        mc.resume_mode = Some(mc.mode.clone());
                    }
                    mc.mode = Mode::Pause.id().to_string();
                } else {
                    mc.mode = mc.resume().id().to_string();
                }
            }
        }
        self.dirty = true;
    }

    fn resume_label(&self) -> String {
        let mode = self
            .current_id()
            .and_then(|id| self.cfg.monitors.get(&id))
            .map(|m| m.resume())
            .unwrap_or(Mode::Health);
        i18n::tr(mode.label_key())
    }

    fn schedule_on(&self) -> bool {
        self.cfg.schedule.mode != "off"
    }

    fn schedule_range(&self) -> String {
        let s = &self.cfg.schedule;
        if s.use_location {
            match schedule::sun_times_today(s.latitude, s.longitude) {
                Some((r, t)) => format!("{} — {}", schedule::format_minutes(r), schedule::format_minutes(t)),
                None => String::new(),
            }
        } else {
            format!("{} — {}", schedule::format_minutes(s.day_start), schedule::format_minutes(s.night_start))
        }
    }

    fn dark(&self) -> bool {
        match self.cfg.theme_mode.as_str() {
            "light" => false,
            "system" => platform::system_prefers_dark(),
            _ => true,
        }
    }
}

/// Inicia la animación hacia los objetivos si hace falta.
fn animate(needed: bool) {
    if !needed {
        return;
    }
    ANIM.with(|a| {
        let mut a = a.borrow_mut();
        let t = a.get_or_insert_with(Timer::default);
        if !t.running() {
            t.start(TimerMode::Repeated, Duration::from_millis(16), || {
                if with(|s| s.step()) {
                    stop_anim();
                }
                push_live();
            });
        }
    });
}

fn apply(smooth: bool) {
    let needed = with(|s| s.retarget(smooth, false));
    animate(needed);
}

// ------------------------------------------------------------------ interfaz

fn apply_theme(dark: bool, accent: i32) {
    rest::apply_theme(dark, accent);
    focus::apply_theme(dark, accent);
    if let Some(app) = ui() {
        let t = app.global::<Theme>();
        t.set_dark(dark);
        t.set_accent(accent);
    }
    MENU.with(|m| {
        if let Some(menu) = m.borrow().as_ref() {
            let t = menu.global::<Theme>();
            t.set_dark(dark);
            t.set_accent(accent);
        }
    });
}

fn accent_index(id: &str) -> i32 {
    ACCENTS.iter().position(|a| a.eq_ignore_ascii_case(id)).unwrap_or(1) as i32
}

/// Solo el confort visual (cambia en cada paso de la animación).
fn push_live() {
    let Some(app) = ui() else { return };
    let (b, k) = with(|s| s.current_id().and_then(|id| s.live.get(&id).copied()).unwrap_or((100.0, 6500.0)));
    let g = app.global::<App>();
    g.set_live_brightness(b as f32);
    g.set_live_kelvin(k.round() as i32);
}

fn push_monitors() {
    let Some(app) = ui() else { return };
    let items: Vec<MonitorItem> = with(|s| {
        s.displays
            .iter()
            .map(|d| MonitorItem {
                id: d.id.as_str().into(),
                name: d.name.as_str().into(),
                width: d.width,
                height: d.height,
                primary: d.primary,
            })
            .collect()
    });
    app.global::<App>().set_monitors(ModelRc::new(VecModel::from(items)));
}

/// Vuelve a rellenar la interfaz desde el estado.
fn push() {
    let Some(app) = ui() else { return };
    let g = app.global::<App>();
    let editing_night = g.get_editing_night();
    with(|s| {
        g.set_selected(s.selected as i32);
        g.set_active(s.displays.iter().any(|d| d.gamma));
        g.set_engine(s.backend.name().into());
        let mc = s.current_id().and_then(|id| s.cfg.monitors.get(&id).cloned());
        let display = s.displays.get(s.selected);
        g.set_supports_gamma(display.is_some_and(|d| d.gamma));
        if let Some(mc) = mc {
            let mode = mc.mode();
            let values = mc.settings(mode);
            let night = s.schedule_on() && editing_night && mode != Mode::Pause;
            let (b, k) = if night { (values.night_brightness, values.night_kelvin) } else { (values.brightness, values.kelvin) };
            g.set_mode(GRID.iter().position(|m| *m == mode).unwrap_or(4) as i32);
            g.set_mode_name(i18n::tr(mode.label_key()).into());
            g.set_brightness(b as f32);
            g.set_kelvin(k);
        }
        let (kmin, kmax) = s.kelvin_bounds();
        g.set_kelvin_min(kmin);
        g.set_kelvin_max(kmax);
        g.set_brightness_min(s.min_brightness() as f32);
        g.set_schedule_on(s.schedule_on());
        g.set_previewing(s.preview.is_some());
        if let Some(night) = s.preview {
            let (day, nt) = schedule::day_night_starts(&s.cfg.schedule);
            let at = schedule::format_minutes(if night { nt } else { day });
            g.set_preview_text(i18n::tr_args("rs_vista_previa_texto", &[&at]).into());
        }
        g.set_schedule_range(s.schedule_range().into());

        // Opciones
        let c = &s.cfg;
        g.set_theme_mode(THEMES.iter().position(|t| *t == c.theme_mode).unwrap_or(2) as i32);
        let lang = i18n::LANGUAGES.iter().position(|l| l.0 == c.language).unwrap_or(0);
        g.set_language(lang as i32);
        g.set_language_restart(i18n::resolve(&c.language) != i18n::current());
        g.set_check_updates(c.check_updates);
        g.set_update_text(s.update_text.as_str().into());
        g.set_update_available(s.update_url.is_some());
        g.set_checking(s.checking);
        g.set_autostart(c.start_with_windows);
        g.set_smooth(c.smooth_changes);
        g.set_standard_gamma(c.standard_gamma);
        g.set_wider_kelvin(c.wider_kelvin);
        g.set_wider_brightness(c.wider_brightness);
        // Con una sola pantalla no hay «todas»: se muestra esa.
        g.set_brightness_supported(s.backend.supports_brightness());
        g.set_sync_monitors(c.sync_monitors && s.displays.len() > 1);
        g.set_close_to_tray(c.close_to_tray);
        g.set_start_in_tray(c.start_in_tray);
        g.set_reset_on_exit(c.reset_on_exit);
        g.set_d_lookup(s.lookup);
    });
    push_live();
    rest::push();
    keys::push();
    focus::push();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    update_tray_menu();
}

fn update_tray_menu() {
    let (paused, label) = with(|s| (s.all_paused(), s.resume_label()));
    #[cfg(target_os = "linux")]
    crate::platform::linux::tray::update(tray_labels(paused, &label));
    #[cfg(target_os = "macos")]
    crate::platform::macos::tray::update(tray_labels(paused, &label));
    MENU.with(|m| {
        if let Some(menu) = m.borrow().as_ref() {
            menu.set_paused(paused);
            menu.set_resume_label(label.into());
        }
    });
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn tray_labels(paused: bool, resume: &str) -> platform::TrayLabels {
    platform::TrayLabels {
        show: i18n::tr("mostrar_ventana"),
        pause: if paused { i18n::tr_args("reanudar_filtro", &[resume]) } else { i18n::tr("pausar_filtro") },
        quit: i18n::tr("salir"),
    }
}

fn wire_translations(t: T) {
    t.on_lookup(|k| i18n::tr(&k).into());
    t.on_lookup1(|k, a| i18n::tr_args(&k, &[&a]).into());
    t.on_lookup2(|k, a, b| i18n::tr_args(&k, &[&a, &b]).into());
    t.set_rev(1);
}

fn fmt_coord(v: f64) -> String {
    format!("{v:.4}")
}

fn parse_coord(text: &str, limit: f64) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.abs() <= limit)
}

/// Amanecer y atardecer de muestra en el diálogo, según lo escrito.
fn preview_sun(app: &AppWindow) {
    let g = app.global::<App>();
    let sun = match (parse_coord(&g.get_d_lat(), 90.0), parse_coord(&g.get_d_lon(), 180.0)) {
        (Some(la), Some(lo)) => schedule::sun_times_today(la, lo),
        _ => None,
    };
    let (rise, set) = sun
        .map(|(r, s)| (schedule::format_minutes(r), schedule::format_minutes(s)))
        .unwrap_or(("--:--".into(), "--:--".into()));
    g.set_d_sunrise(rise.into());
    g.set_d_sunset(set.into());
}

// ------------------------------------------------------------------ ventanas

/// Abajo a la derecha de la pantalla principal, sin tapar el panel (Linux).
#[cfg(target_os = "linux")]
fn linux_home_position(size: slint::PhysicalSize, scale: f32) -> Option<slint::PhysicalPosition> {
    let (x, y, width, height) = crate::platform::linux::desk::work_rect()?;
    // Antes de crear la ventana su tamaño aún es 0: se usa el de diseño.
    let (w, h) = if size.width == 0 {
        ((980.0 * scale) as i32, (700.0 * scale) as i32)
    } else {
        (size.width as i32, size.height as i32)
    };
    let margin = (8.0 * scale) as i32;
    Some(slint::PhysicalPosition::new(x + width - w - margin, y + height - h - margin))
}

/// Muestra la ventana principal y la trae al frente.
fn show_window() {
    let Some(app) = ui() else { return };
    // macOS: abajo a la derecha, sin tapar el Dock. Se pide antes de crear la
    // ventana; al ocultarla y mostrarla, macOS conserva su sitio.
    #[cfg(target_os = "macos")]
    if !with(|s| s.placed) {
        if let Some((x, y, w, h)) = crate::platform::macos::desk::work_rect() {
            let pos = slint::LogicalPosition::new((x + w - 980.0 - 8.0) as f32, (y + h - 700.0 - 8.0) as f32);
            app.window().set_position(pos);
        }
    }
    // La primera vez, la posición se pide antes de crear la ventana: el gestor
    // de ventanas la recibe como elegida por el usuario y no la centra antes.
    #[cfg(target_os = "linux")]
    if !with(|s| s.placed) && with(|s| s.last_pos).is_none() {
        if let Some(pos) = linux_home_position(app.window().size(), app.window().scale_factor()) {
            app.window().set_position(pos);
            // Si la ventana nativa ya existe (oculta), se marca la posición como elegida.
            if let Some(id) = crate::platform::linux::desk::xid(app.window()) {
                crate::platform::linux::desk::pin_position(id, pos.x, pos.y);
            }
            debug_log(&format!("primera vez: posición pedida {pos:?}, tamaño {:?}", app.window().size()));
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(pos) = with(|s| s.last_pos) {
        // Se fija la posición antes de mostrarla: así aparece directamente ahí.
        // Si el gestor de ventanas no lo respeta, se queda donde él la ponga
        // (sin moverla después, para que no salte de un sitio a otro).
        app.window().set_position(pos);
        if let Some(id) = crate::platform::linux::desk::xid(app.window()) {
            crate::platform::linux::desk::pin_position(id, pos.x, pos.y);
        }
    }
    let _ = app.show();
    push();
    // La ventana nativa se crea cuando el bucle de eventos ya corre; antes de
    // eso no hay HWND. Por eso los detalles de Windows van un instante después.
    Timer::single_shot(Duration::from_millis(30), finish_show);
}

fn finish_show() {
    let Some(app) = ui() else { return };
    #[cfg(windows)]
    {
        use crate::platform::windows::shell;
        let Some(hwnd) = shell::hwnd(app.window()) else {
            debug_log("show: todavía sin HWND, reintento");
            Timer::single_shot(Duration::from_millis(50), finish_show);
            return;
        };
        let first = with(|s| !std::mem::replace(&mut s.placed, true));
        if first {
            if std::env::var_os("COE_NOSTYLE").is_none() {
                shell::style_window(hwnd, false);
                shell::enable_title_drag(hwnd);
                shell::set_icon(hwnd, ICON_PNG);
            }
            shell::place_bottom_right(hwnd);
        }
        shell::bring_to_front(hwnd);
    }
    #[cfg(target_os = "linux")]
    {
        use crate::platform::linux::desk;
        let first = with(|s| !std::mem::replace(&mut s.placed, true));
        if first {
            // Ya se pidió esta posición antes de mostrarla; solo se corrige si el
            // tamaño real resultó distinto del previsto.
            if let Some(pos) = linux_home_position(app.window().size(), app.window().scale_factor()) {
                if app.window().position() != pos {
                    app.window().set_position(pos);
                }
            }
        }
        if let Some(id) = desk::xid(app.window()) {
            desk::activate(id);
        }
    }
    #[cfg(target_os = "macos")]
    {
        with(|s| s.placed = true);
        // Trae la app al frente aunque estuviera en segundo plano.
        use slint::winit_030::WinitWindowAccessor;
        app.window().with_winit_window(|w| w.focus_window());
    }
    let w = app.window();
    debug_log(&format!(
        "show: escala={} tamaño={:?} posición={:?} visible={}",
        w.scale_factor(),
        w.size(),
        w.position(),
        w.is_visible()
    ));
}

/// Registro de diagnóstico: solo si se define la variable COE_DEBUG
/// (se escribe en careoureyes-debug.log, en la carpeta actual).
pub fn debug_log(line: &str) {
    if std::env::var_os("COE_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("careoureyes-debug.log") {
        let _ = writeln!(f, "[{}] {line}", chrono::Local::now().format("%H:%M:%S%.3f"));
    }
}

fn hide_or_quit() {
    end_preview();
    let keep = with(|s| s.has_tray && s.cfg.close_to_tray);
    if keep {
        if let Some(app) = ui() {
            let pos = app.window().position();
            with(|s| s.last_pos = Some(pos));
            let _ = app.hide();
        }
    } else {
        quit();
    }
}

fn quit() {
    stop_anim();
    rest::stop();
    focus::stop();
    stop_preview_timer();
    MENU_WATCH.with(|w| {
        if let Some(t) = w.borrow().as_ref() {
            t.stop();
        }
    });
    with(|s| {
        if s.cfg.reset_on_exit {
            s.backend.restore_all();
        }
        s.cfg.save();
        s.dirty = false;
    });
    #[cfg(windows)]
    crate::platform::windows::tray::remove();
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::tray::remove();
        crate::platform::linux::shell::release_instance();
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::tray::remove();
        crate::platform::macos::shell::release_instance();
    }
    MENU.with(|m| {
        if let Some(menu) = m.borrow().as_ref() {
            let _ = menu.hide();
        }
    });
    let _ = slint::quit_event_loop();
}

fn open_tray_menu(x: i32, y: i32) {
    update_tray_menu();
    MENU.with(|m| {
        let menu = m.borrow();
        let Some(menu) = menu.as_ref() else { return };
        let _ = menu.show();
        #[cfg(windows)]
        {
            use crate::platform::windows::shell;
            if let Some(hwnd) = shell::hwnd(menu.window()) {
                shell::style_window(hwnd, true);
                shell::place_menu(hwnd, x, y);
                watch_menu(hwnd.0 as isize);
            }
        }
        #[cfg(not(windows))]
        let _ = (x, y);
    });
}

/// Cierra el menú en cuanto otra ventana toma el foco (como un menú normal).
#[cfg(windows)]
fn watch_menu(hwnd: isize) {
    use crate::platform::windows::shell;
    let opened = Instant::now();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
        let h = windows::Win32::Foundation::HWND(hwnd as *mut core::ffi::c_void);
        let visible = MENU.with(|m| m.borrow().as_ref().is_some_and(|w| w.window().is_visible()));
        if !visible || (opened.elapsed() > Duration::from_millis(250) && !shell::is_foreground(h)) {
            MENU.with(|m| {
                if let Some(menu) = m.borrow().as_ref() {
                    let _ = menu.hide();
                }
            });
            MENU_WATCH.with(|w| {
                if let Some(t) = w.borrow().as_ref() {
                    t.stop();
                }
            });
        }
    });
    // Reemplaza (y suelta) el temporizador de la vez anterior.
    MENU_WATCH.with(|w| *w.borrow_mut() = Some(timer));
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn watch_menu(_hwnd: isize) {}

fn close_menu() {
    MENU.with(|m| {
        if let Some(menu) = m.borrow().as_ref() {
            let _ = menu.hide();
        }
    });
}

fn on_tray(ev: platform::TrayEvent) {
    use platform::TrayEvent;
    debug_log(&format!("bandeja: {ev:?}"));
    match ev {
        TrayEvent::TogglePause => {
            with(|s| s.toggle_pause());
            apply(true);
            push();
        }
        TrayEvent::Quit => quit(),
        TrayEvent::Hotkey(id) => keys::on_hotkey(id),
        TrayEvent::Open | TrayEvent::ShowRequested => show_window(),
        TrayEvent::Menu(x, y) => open_tray_menu(x, y),
        TrayEvent::Resumed | TrayEvent::DisplaysChanged => {
            // Tras despertar, el controlador de vídeo tarda un poco en estar listo.
            Timer::single_shot(Duration::from_millis(1500), || {
                with(|s| {
                    s.refresh_displays();
                    s.retarget(false, true);
                });
                push_monitors();
                push();
            });
        }
        TrayEvent::ThemeChanged => {
            let (dark, accent) = with(|s| (s.dark(), accent_index(&s.cfg.accent_theme)));
            apply_theme(dark, accent);
        }
    }
}

// ------------------------------------------------------------------ arranque

/// Crea la ventana, conecta todo y corre el bucle de eventos.
pub fn run(start_hidden: bool) -> Result<(), slint::PlatformError> {
    let cfg = AppConfig::load();
    i18n::init(&cfg.language);

    let breaks = crate::breaks::Breaks::new(&cfg.rest);
    let mut state = State {
        breaks,
        rule_override: None,
        last_window: String::new(),
        cfg,
        backend: platform::backend(),
        displays: Vec::new(),
        selected: 0,
        live: HashMap::new(),
        target: HashMap::new(),
        dirty: false,
        update_text: i18n::tr_args("version_instalada", &[env!("CARGO_PKG_VERSION")]),
        update_url: None,
        checking: false,
        lookup: 0,
        has_tray: false,
        placed: false,
        last_pos: None,
        last_night: None,
        preview: None,
        last_touch: std::time::Instant::now(),
    };
    state.refresh_displays();
    // Una sola vez: «Todas las pantallas» pasa a ser lo normal, con los ajustes de la principal.
    if !state.cfg.rs_sync_default {
        state.cfg.rs_sync_default = true;
        state.cfg.sync_monitors = true;
        if let Some(i) = state.displays.iter().position(|d| d.primary) {
            state.selected = i;
        }
        state.copy_selected_to_all();
        state.dirty = true;
    }
    if state.cfg.start_with_windows {
        // Reescribe la entrada por si el programa cambió de carpeta.
        platform::set_autostart(true);
    }
    let dark = state.dark();
    let accent = accent_index(&state.cfg.accent_theme);
    let check_updates = state.cfg.check_updates;
    // Al iniciar sesión, la bandeja del escritorio puede no estar lista todavía.
    #[cfg(target_os = "linux")]
    let at_login = start_hidden;
    let start_hidden = start_hidden || state.cfg.start_in_tray;
    STATE.with(|s| *s.borrow_mut() = Some(state));

    let app = AppWindow::new()?;
    wire_translations(app.global::<T>());
    UI.with(|u| *u.borrow_mut() = Some(app.as_weak()));

    let menu = TrayMenu::new()?;
    wire_translations(menu.global::<T>());
    menu.on_show_window(|| {
        close_menu();
        show_window();
    });
    menu.on_toggle_pause(|| {
        close_menu();
        with(|s| s.toggle_pause());
        apply(true);
        push();
    });
    menu.on_quit(|| {
        close_menu();
        quit();
    });
    MENU.with(|m| *m.borrow_mut() = Some(menu));
    apply_theme(dark, accent);

    // Aplica los perfiles al abrir (sin animación).
    with(|s| s.retarget(false, true));

    let g = app.global::<App>();
    g.set_version(env!("CARGO_PKG_VERSION").into());
    g.set_rounded_window(!cfg!(windows) && std::env::var_os("COE_SQUARE").is_none());
    let names: Vec<SharedString> = i18n::LANGUAGES.iter().map(|l| SharedString::from(l.1)).collect();
    g.set_language_names(ModelRc::new(VecModel::from(names)));
    push_monitors();

    connect_screen(&app);
    connect_dialog(&app);
    connect_options(&app);
    connect_window(&app);
    rest::init(&app)?;
    keys::init(&app);
    focus::init(&app)?;
    apply_theme(dark, accent);

    // Bandeja (COE_NOTRAY la desactiva, para diagnóstico)
    #[cfg(windows)]
    if std::env::var_os("COE_NOTRAY").is_none() {
        let ok = crate::platform::windows::tray::create(TRAY_PNG, "CareOurEyes", on_tray);
        with(|s| s.has_tray = ok);
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::set_handler(on_tray);
        if std::env::var_os("COE_NOTRAY").is_none() {
            let (paused, label) = with(|s| (s.all_paused(), s.resume_label()));
            let ok = crate::platform::linux::tray::create(TRAY_PNG, tray_labels(paused, &label), at_login);
            with(|s| s.has_tray = ok);
        }
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::set_handler(on_tray);
        if std::env::var_os("COE_NOTRAY").is_none() {
            // La barra de menús existe siempre en macOS; el icono se crea en
            // cuanto arranca el bucle de eventos (antes no hay NSApplication).
            with(|s| s.has_tray = true);
            Timer::single_shot(Duration::ZERO, || {
                let (paused, label) = with(|s| (s.all_paused(), s.resume_label()));
                let ok = crate::platform::macos::tray::create(TRAY_PNG, tray_labels(paused, &label));
                with(|s| s.has_tray = ok);
            });
        }
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = TRAY_PNG;

    // Revisa el horario cada 30 s; guarda los cambios pendientes cada 1,5 s.
    let schedule_timer = Timer::default();
    schedule_timer.start(TimerMode::Repeated, Duration::from_secs(30), || {
        apply(true);
        sync_period();
        push();
    });
    let save_timer = Timer::default();
    save_timer.start(TimerMode::Repeated, Duration::from_millis(1500), || {
        with(|s| {
            if s.dirty {
                s.cfg.save();
                s.dirty = false;
            }
        });
    });
    // Reaplicar cada N segundos (por si otro programa cambia la rampa).
    let reapply_timer = Timer::default();
    let every = with(|s| s.cfg.reapply_interval_seconds);
    if every > 0 {
        reapply_timer.start(TimerMode::Repeated, Duration::from_secs(every.max(5) as u64), || {
            with(|s| s.retarget(false, true));
        });
    }

    if check_updates {
        check_updates_now();
    }

    // Solo en compilaciones de desarrollo: abrir en una página concreta (capturas).
    if cfg!(debug_assertions) {
        let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i32>().ok());
        if let Some(p) = num("COE_PAGE") {
            g.set_page(p);
        }
        if let Some(t) = num("COE_TAB") {
            g.set_option_tab(t);
        }
        if num("COE_DIALOG").is_some() {
            g.invoke_open_dialog();
        }
        if let Ok(what) = std::env::var("COE_BREAK") {
            Timer::single_shot(Duration::from_millis(500), move || rest::debug_show(&what));
        }
        if num("COE_MENU").is_some() {
            Timer::single_shot(Duration::from_millis(300), || open_tray_menu(900, 700));
        }
    }

    if std::env::var_os("COE_DEBUG").is_some() {
        let _ = std::fs::remove_file("careoureyes-debug.log");
        g.set_debug(true);
        g.on_debug_pointer(|kind, x, y| debug_log(&format!("ratón: {kind} x={x:.0} y={y:.0}")));
        debug_log(&format!("inicio v{} bandeja={}", env!("CARGO_PKG_VERSION"), with(|s| s.has_tray)));
    }

    sync_period();
    let hidden = start_hidden && with(|s| s.has_tray);
    if !hidden {
        show_window();
    }
    push();
    slint::run_event_loop_until_quit()?;

    // Soltar todo lo que depende de Slint mientras Slint sigue vivo.
    drop((schedule_timer, save_timer, reapply_timer));
    ANIM.with(|a| a.borrow_mut().take());
    MENU_WATCH.with(|w| w.borrow_mut().take());
    PREVIEW_TIMER.with(|w| w.borrow_mut().take());
    rest::shutdown();
    keys::shutdown();
    focus::shutdown();
    MENU.with(|m| m.borrow_mut().take());
    UI.with(|u| u.borrow_mut().take());
    drop(app);
    STATE.with(|s| s.borrow_mut().take());
    Ok(())
}

fn connect_screen(app: &AppWindow) {
    let g = app.global::<App>();
    g.on_refresh(side_changed);
    g.on_select_monitor(|i| {
        with(|s| s.selected = (i.max(0) as usize).min(s.displays.len().saturating_sub(1)));
        push();
    });
    g.on_refresh_monitors(|| {
        with(|s| {
            s.refresh_displays();
            s.retarget(false, true);
        });
        push_monitors();
        push();
    });
    g.on_set_mode(|i| {
        let mode = GRID.get(i as usize).copied().unwrap_or(Mode::Health);
        with(|s| s.set_mode(mode));
        apply(true);
        push();
    });
    g.on_set_brightness(|v| {
        let night = editing_night();
        with(|s| {
            let v = (v as f64).round().clamp(s.min_brightness(), 100.0);
            s.edit_values(night, |m, n| if n { m.night_brightness = v } else { m.brightness = v });
        });
        apply(false);
        push();
    });
    g.on_step_brightness(|dir| {
        let night = editing_night();
        with(|s| {
            let min = s.min_brightness();
            s.edit_values(night, |m, n| {
                let cur = if n { m.night_brightness } else { m.brightness };
                let v = (cur + 5.0 * dir as f64).clamp(min, 100.0);
                if n { m.night_brightness = v } else { m.brightness = v }
            });
        });
        apply(false);
        push();
    });
    g.on_set_kelvin(|v| {
        let night = editing_night();
        with(|s| {
            // La barra tiene 100 pasos entre el mínimo y el máximo.
            let (min, max) = s.kelvin_bounds();
            let step = ((max - min) / 100).max(1);
            let k = (min + ((v as f64 - min as f64) / step as f64).round() as i32 * step).clamp(min, max);
            s.edit_values(night, |m, n| if n { m.night_kelvin = k } else { m.kelvin = k });
        });
        apply(false);
        push();
    });
    g.on_step_kelvin(|dir| {
        let night = editing_night();
        with(|s| {
            let (min, max) = s.kelvin_bounds();
            s.edit_values(night, |m, n| {
                let cur = if n { m.night_kelvin } else { m.kelvin };
                let k = (cur + 200 * dir).clamp(min, max);
                if n { m.night_kelvin = k } else { m.kelvin = k }
            });
        });
        apply(false);
        push();
    });
    g.on_slider_released(|| with(|s| s.dirty = true));
    g.on_reset_monitor(|| {
        // Devuelve el perfil activo (día y noche) a sus valores de fábrica.
        with(|s| {
            for id in s.affected() {
                let Some(mc) = s.cfg.monitors.get_mut(&id) else { continue };
                let mode = mc.mode();
                let factory = MonitorConfig::new(&id, "").settings(mode);
                if let Some(v) = mc.settings_mut(mode) {
                    *v = factory;
                }
            }
            s.dirty = true;
        });
        apply(true);
        push();
    });
    g.on_set_schedule(|on| {
        with(|s| {
            let sch = &mut s.cfg.schedule;
            sch.mode = if !on { "off" } else if sch.use_location { "sun" } else { "manual" }.into();
            s.dirty = true;
        });
        with(|s| {
            s.last_night = None;
            s.preview = None;
        });
        stop_preview_timer();
        sync_period();
        apply(true);
        push();
    });
}

/// Los botones y barras editan los valores del periodo que se ve en pantalla:
/// al abrir, y cada vez que llega el día o la noche, se elige la pestaña que
/// toca. Si no, de noche se editarían los valores de día y no cambiaría nada.
fn sync_period() {
    let changed = with(|s| {
        let night = s.schedule_on() && schedule::night_factor(&s.cfg.schedule, schedule::now_minutes()) >= 0.5;
        (s.last_night.replace(night) != Some(night)).then_some(night)
    });
    if let (Some(night), Some(app)) = (changed, ui()) {
        app.global::<App>().set_editing_night(night);
        // Si llegó el periodo que se estaba previsualizando, la vista previa sobra.
        if with(|s| s.preview.take().is_some()) {
            stop_preview_timer();
            apply(true);
        }
    }
}

fn night_now(s: &State) -> bool {
    s.schedule_on() && schedule::night_factor(&s.cfg.schedule, schedule::now_minutes()) >= 0.5
}

/// La persona eligió la pestaña Día o Noche. Si no es el periodo actual, la
/// pantalla pasa a mostrar ese periodo (vista previa) hasta pulsar «Listo».
fn side_changed() {
    let Some(app) = ui() else { return };
    let tab = app.global::<App>().get_editing_night();
    let preview = with(|s| {
        let p = (s.schedule_on() && tab != night_now(s)).then_some(tab);
        s.preview = p;
        s.last_touch = std::time::Instant::now();
        p
    });
    if preview.is_some() {
        PREVIEW_TIMER.with(|t| {
            let timer = Timer::default();
            timer.start(TimerMode::Repeated, Duration::from_secs(1), || {
                if with(|s| s.last_touch.elapsed() > Duration::from_secs(60)) {
                    end_preview();
                }
            });
            *t.borrow_mut() = Some(timer);
        });
    } else {
        stop_preview_timer();
    }
    apply(true);
    push();
}

fn stop_preview_timer() {
    PREVIEW_TIMER.with(|t| {
        if let Some(timer) = t.borrow().as_ref() {
            timer.stop();
        }
    });
}

/// Termina la vista previa y vuelve al periodo actual.
fn end_preview() {
    let was = with(|s| s.preview.take().is_some());
    stop_preview_timer();
    if !was {
        return;
    }
    let night = with(|s| night_now(s));
    if let Some(app) = ui() {
        app.global::<App>().set_editing_night(night);
    }
    apply(true);
    push();
}

fn editing_night() -> bool {
    let on = ui().is_some_and(|a| a.global::<App>().get_editing_night());
    on && with(|s| s.schedule_on())
}

fn connect_dialog(app: &AppWindow) {
    let g = app.global::<App>();
    g.on_open_dialog(|| {
        let Some(app) = ui() else { return };
        let g = app.global::<App>();
        let sch = with(|s| s.cfg.schedule.clone());
        g.set_d_use_location(sch.use_location);
        let has = sch.latitude != 0.0 || sch.longitude != 0.0;
        g.set_d_lat(if has { fmt_coord(sch.latitude) } else { String::new() }.into());
        g.set_d_lon(if has { fmt_coord(sch.longitude) } else { String::new() }.into());
        g.set_d_day_h((sch.day_start / 60) % 24);
        g.set_d_day_m(sch.day_start % 60);
        g.set_d_night_h((sch.night_start / 60) % 24);
        g.set_d_night_m(sch.night_start % 60);
        g.set_d_smooth(sch.smooth_transition);
        let m = sch.transition_minutes.clamp(15, 300);
        g.set_d_minutes((m + 7) / 15 * 15);
        with(|s| s.lookup = 0);
        g.set_d_lookup(0);
        preview_sun(&app);
        g.set_dialog_open(true);
    });
    g.on_coords_edited(|| {
        if let Some(app) = ui() {
            preview_sun(&app);
        }
    });
    g.on_detect_location(|| {
        let busy = with(|s| std::mem::replace(&mut s.lookup, 1) == 1);
        if busy {
            return;
        }
        push();
        let weak = UI.with(|u| u.borrow().clone());
        std::thread::spawn(move || {
            let found = net::locate();
            if let Some(weak) = weak {
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let g = app.global::<App>();
                    match found {
                        Some((la, lo)) => {
                            g.set_d_lat(fmt_coord(la).into());
                            g.set_d_lon(fmt_coord(lo).into());
                            with(|s| s.lookup = 0);
                            preview_sun(&app);
                        }
                        None => with(|s| s.lookup = 2),
                    }
                    push();
                });
            }
        });
    });
    g.on_save_dialog(|| {
        let Some(app) = ui() else { return };
        let g = app.global::<App>();
        let use_location = g.get_d_use_location();
        let lat = parse_coord(&g.get_d_lat(), 90.0);
        let lon = parse_coord(&g.get_d_lon(), 180.0);
        let day = g.get_d_day_h().clamp(0, 23) * 60 + g.get_d_day_m().clamp(0, 59);
        let night = g.get_d_night_h().clamp(0, 23) * 60 + g.get_d_night_m().clamp(0, 59);
        let smooth = g.get_d_smooth();
        let minutes = g.get_d_minutes().clamp(15, 300);
        with(|s| {
            let sch = &mut s.cfg.schedule;
            sch.use_location = use_location;
            if sch.mode != "off" {
                sch.mode = if use_location { "sun" } else { "manual" }.into();
            }
            if use_location {
                if let (Some(la), Some(lo)) = (lat, lon) {
                    sch.latitude = la;
                    sch.longitude = lo;
                }
            } else {
                sch.day_start = day;
                sch.night_start = night;
            }
            sch.smooth_transition = smooth;
            sch.transition_minutes = minutes;
            s.dirty = true;
        });
        g.set_dialog_open(false);
        apply(true);
        sync_period();
        push();
    });
}

fn check_updates_now() {
    let busy = with(|s| std::mem::replace(&mut s.checking, true));
    if busy {
        return;
    }
    push();
    let weak = UI.with(|u| u.borrow().clone());
    std::thread::spawn(move || {
        let result = net::check_latest(env!("CARGO_PKG_VERSION"));
        if let Some(weak) = weak {
            let _ = weak.upgrade_in_event_loop(move |_| {
                with(|s| {
                    s.checking = false;
                    match result {
                        Ok(net::Update::UpToDate) => {
                            s.update_url = None;
                            s.update_text = i18n::tr_args("tienes_la_ultima_version", &[env!("CARGO_PKG_VERSION")]);
                        }
                        Ok(net::Update::New { version, url }) => {
                            s.update_text = i18n::tr_args("hay_una_version_nueva", &[&version]);
                            s.update_url = Some(url);
                        }
                        Err(reason) => {
                            s.update_url = None;
                            s.update_text = i18n::tr_args("no_se_pudo_comprobar", &[&reason]);
                        }
                    }
                });
                push();
            });
        }
    });
}

fn connect_options(app: &AppWindow) {
    let g = app.global::<App>();
    g.on_set_theme_mode(|i| {
        let (dark, accent) = with(|s| {
            s.cfg.theme_mode = THEMES.get(i as usize).unwrap_or(&"system").to_string();
            s.dirty = true;
            (s.dark(), accent_index(&s.cfg.accent_theme))
        });
        apply_theme(dark, accent);
        push();
    });
    g.on_set_accent(|i| {
        let dark = with(|s| {
            s.cfg.accent_theme = ACCENTS.get(i as usize).unwrap_or(&"cyan").to_string();
            s.dirty = true;
            s.dark()
        });
        apply_theme(dark, i);
        push();
    });
    g.on_set_language(|i| {
        with(|s| {
            s.cfg.language = i18n::LANGUAGES.get(i as usize).map(|l| l.0).unwrap_or("system").to_string();
            s.dirty = true;
        });
        push();
    });
    g.on_set_option(|name, on| {
        let reapply = with(|s| {
            let c = &mut s.cfg;
            s.dirty = true;
            match name.as_str() {
                "checkUpdates" => c.check_updates = on,
                "startWithWindows" => {
                    c.start_with_windows = on;
                    platform::set_autostart(on);
                }
                "smoothChanges" => c.smooth_changes = on,
                "standardGamma" => {
                    c.standard_gamma = on;
                    return true;
                }
                "widerKelvin" | "widerBrightness" => {
                    if name.as_str() == "widerKelvin" {
                        c.wider_kelvin = on;
                    } else {
                        c.wider_brightness = on;
                    }
                    // Lleva los perfiles al nuevo rango.
                    let (kmin, kmax) = color::kelvin_range(c.wider_kelvin);
                    let bmin = color::brightness_range(c.wider_brightness).0;
                    for mc in c.monitors.values_mut() {
                        for mode in Mode::ALL {
                            if let Some(v) = mc.settings_mut(mode) {
                                v.kelvin = v.kelvin.clamp(kmin, kmax);
                                v.night_kelvin = v.night_kelvin.clamp(kmin, kmax);
                                v.brightness = v.brightness.clamp(bmin, 100.0);
                                v.night_brightness = v.night_brightness.clamp(bmin, 100.0);
                            }
                        }
                    }
                    return true;
                }
                "syncMonitors" => {
                    c.sync_monitors = on;
                    // Al juntar las pantallas, todas toman los ajustes de la elegida.
                    if on {
                        s.copy_selected_to_all();
                    }
                    return true;
                }
                "closeToTray" => c.close_to_tray = on,
                "startInTray" => c.start_in_tray = on,
                "resetOnExit" => c.reset_on_exit = on,
                _ => {}
            }
            false
        });
        if reapply {
            with(|s| s.retarget(false, true));
        }
        push();
    });
    g.on_check_now(check_updates_now);
    g.on_download_update(|| {
        if let Some(url) = with(|s| s.update_url.clone()) {
            platform::open_url(&url);
        }
    });
    g.on_open_issues(|| platform::open_url(net::ISSUES_URL));
    g.on_open_repo(|| platform::open_url(net::REPO_URL));
    g.on_open_config_folder(|| platform::open_folder(&crate::config::config_dir()));
}

fn connect_window(app: &AppWindow) {
    let g = app.global::<App>();
    g.on_end_preview(end_preview);
    g.on_minimize(|| {
        end_preview();
        if let Some(app) = ui() {
            app.window().set_minimized(true);
        }
    });
    g.on_close(hide_or_quit);
    g.on_start_drag(|| {
        #[cfg(not(windows))]
        if let Some(app) = ui() {
            use slint::winit_030::WinitWindowAccessor;
            app.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
            // El gestor de ventanas se queda con el ratón durante el arrastre y la
            // app nunca recibe el «soltar». Sin esto, Slint cree que el botón sigue
            // pulsado sobre la barra de título y no atiende nada más.
            use slint::platform::{PointerEventButton, WindowEvent};
            let position = slint::LogicalPosition::new(0.0, 0.0);
            app.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
            app.window().dispatch_event(WindowEvent::PointerExited);
        }
    });
    // Alt+F4 o cerrar desde la barra de tareas.
    app.window().on_close_requested(|| {
        if !with(|s| s.has_tray && s.cfg.close_to_tray) {
            quit();
        }
        slint::CloseRequestResponse::HideWindow
    });
}
