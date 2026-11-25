//! Enfoque en X11: franja de lectura y velo del fondo.
//!
//! Son ventanas sin gestor (override-redirect) semitransparentes (visual ARGB de
//! 32 bits, necesita un compositor, como el de Cinnamon). La extensión Shape
//! les quita la entrada (los clics pasan) y les recorta huecos: la ventana
//! activa y las de la propia app (avisos de pausa), que así nunca se oscurecen.

use super::x::x;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, MapState,
    Rectangle, StackMode, VisualClass, Window, WindowClass,
};

/// (x, y, ancho, alto) en píxeles de pantalla.
type Rect = (i32, i32, i32, i32);

pub struct Band {
    /// Alto en píxeles físicos.
    pub height: i32,
    pub alpha: u8,
    /// Color como en Windows (0x00BBGGRR).
    pub color: u32,
}

pub struct Veil {
    pub alpha: u8,
    pub color: u32,
    pub taskbar: bool,
    pub current_monitor_only: bool,
}

#[derive(Clone, PartialEq)]
struct Shown {
    rect: Rect,
    pixel: u32,
    holes: Vec<Rect>,
}

struct Pane {
    win: Window,
    shown: Option<Shown>,
}

#[derive(Default)]
struct Fx {
    /// Visual ARGB y su mapa de colores.
    argb: Option<(u32, u32)>,
    band: Vec<Pane>,
    veils: Vec<Pane>,
    /// Ventanas visibles de la app, revisadas cada medio segundo.
    own: Vec<Rect>,
    own_at: Option<Instant>,
    /// Última revisión del orden de las ventanas.
    stacked: Option<Instant>,
    /// La ventana principal: se vela como cualquier otra (como en Windows).
    main: Window,
}

thread_local! {
    static FX: RefCell<Fx> = RefCell::new(Fx::default());
}

fn atom(name: &str) -> u32 {
    let Some(x) = x() else { return 0 };
    x.conn.intern_atom(false, name.as_bytes()).ok().and_then(|c| c.reply().ok()).map(|r| r.atom).unwrap_or(0)
}

fn screen_index() -> usize {
    let Some(x) = x() else { return 0 };
    x.conn.setup().roots.iter().position(|s| s.root == x.root).unwrap_or(0)
}

fn argb_visual() -> Option<u32> {
    let x = x()?;
    let screen = x.conn.setup().roots.get(screen_index())?;
    screen
        .allowed_depths
        .iter()
        .filter(|d| d.depth == 32)
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.class == VisualClass::TRUE_COLOR)
        .map(|v| v.visual_id)
}

/// ¿Se puede usar Enfoque? Hace falta X11, un compositor y un visual ARGB.
pub fn available() -> bool {
    let Some(x) = x() else { return false };
    let cm = atom(&format!("_NET_WM_CM_S{}", screen_index()));
    let composited = cm != 0
        && x.conn.get_selection_owner(cm).ok().and_then(|c| c.reply().ok()).is_some_and(|r| r.owner != 0);
    composited && argb_visual().is_some()
}

fn new_pane(fx: &mut Fx) -> Option<Pane> {
    let x = x()?;
    if fx.argb.is_none() {
        let visual = argb_visual()?;
        let cmap = x.conn.generate_id().ok()?;
        x.conn.create_colormap(ColormapAlloc::NONE, cmap, x.root, visual).ok()?.check().ok()?;
        fx.argb = Some((visual, cmap));
    }
    let (visual, cmap) = fx.argb?;
    let win = x.conn.generate_id().ok()?;
    let aux = CreateWindowAux::new().background_pixel(0).border_pixel(0).override_redirect(1).colormap(cmap);
    x.conn.create_window(32, win, x.root, 0, 0, 1, 1, 0, WindowClass::INPUT_OUTPUT, visual, &aux).ok()?.check().ok()?;
    let _ = x.conn.change_property8(
        x11rb::protocol::xproto::PropMode::REPLACE,
        win,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"CareOurEyes focus",
    );
    // Sin entrada: los clics y el ratón llegan a lo que hay debajo.
    let _ = x.conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &[]);
    Some(Pane { win, shown: None })
}

fn hide(p: &mut Pane) {
    if p.shown.take().is_some() {
        if let Some(x) = x() {
            let _ = x.conn.unmap_window(p.win);
        }
    }
}

/// Coloca y pinta una pieza. Solo habla con el servidor si algo cambió.
fn show(p: &mut Pane, rect: Rect, color: u32, alpha: u8, holes: &[Rect]) {
    let Some(x) = x() else { return };
    let (w, h) = (rect.2.max(1), rect.3.max(1));
    let a = alpha as u32;
    let (r, g, b) = (color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF);
    // ARGB premultiplicado.
    let pixel = (a << 24) | ((r * a / 255) << 16) | ((g * a / 255) << 8) | (b * a / 255);
    // Solo los huecos que tocan esta pieza, en sus coordenadas.
    let local: Vec<Rect> = holes
        .iter()
        .filter(|hl| hl.0 < rect.0 + w && hl.0 + hl.2 > rect.0 && hl.1 < rect.1 + h && hl.1 + hl.3 > rect.1)
        .map(|hl| (hl.0 - rect.0, hl.1 - rect.1, hl.2, hl.3))
        .collect();
    let want = Shown { rect: (rect.0, rect.1, w, h), pixel, holes: local.clone() };
    let was = p.shown.clone();
    if was.as_ref() != Some(&want) {
        let aux = ConfigureWindowAux::new().x(rect.0).y(rect.1).width(w as u32).height(h as u32);
        let _ = x.conn.configure_window(p.win, &aux);
        if was.as_ref().map(|s| s.pixel) != Some(pixel) {
            let _ = x.conn.change_window_attributes(
                p.win,
                &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().background_pixel(pixel),
            );
            let _ = x.conn.clear_area(false, p.win, 0, 0, 0, 0);
        }
        let full = [Rectangle { x: 0, y: 0, width: w as u16, height: h as u16 }];
        let _ = x.conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, p.win, 0, 0, &full);
        if !local.is_empty() {
            let cut: Vec<Rectangle> = local
                .iter()
                .map(|hl| Rectangle {
                    x: hl.0.clamp(-32768, 32767) as i16,
                    y: hl.1.clamp(-32768, 32767) as i16,
                    width: hl.2.clamp(0, 65535) as u16,
                    height: hl.3.clamp(0, 65535) as u16,
                })
                .collect();
            let _ = x.conn.shape_rectangles(SO::SUBTRACT, SK::BOUNDING, ClipOrdering::UNSORTED, p.win, 0, 0, &cut);
        }
        if was.is_none() {
            let _ = x.conn.map_window(p.win);
        }
        p.shown = Some(want);
    }
}

/// Mantiene las piezas encima de todo (lo que debe verse queda en un hueco).
/// Solo las sube si otra ventana visible quedó por encima, y siempre en el mismo
/// orden (velo debajo, franja encima): subirlas sin necesidad hace que el
/// compositor redibuje y se note un parpadeo.
fn keep_on_top(fx: &mut Fx) {
    if fx.stacked.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
        return;
    }
    fx.stacked = Some(Instant::now());
    let Some(x) = x() else { return };
    let ours: Vec<Window> =
        fx.veils.iter().chain(fx.band.iter()).filter(|p| p.shown.is_some()).map(|p| p.win).collect();
    if ours.is_empty() {
        return;
    }
    let Some(tree) = x.conn.query_tree(x.root).ok().and_then(|c| c.reply().ok()) else { return };
    // Hijos de la raíz, de abajo hacia arriba.
    let Some(lowest) = tree.children.iter().position(|w| ours.contains(w)) else { return };
    let covered = tree.children[lowest + 1..].iter().any(|w| {
        !ours.contains(w)
            && x.conn
                .get_window_attributes(*w)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|a| a.map_state == MapState::VIEWABLE)
    });
    if covered {
        for w in ours {
            let _ = x.conn.configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
        }
    }
}

fn flush() {
    if let Some(x) = x() {
        let _ = x.conn.flush();
    }
}

fn prop32(win: Window, name: &str, kind: impl Into<u32>) -> Vec<u32> {
    let Some(x) = x() else { return Vec::new() };
    x.conn
        .get_property(false, win, atom(name), kind.into(), 0, 1024)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}

/// Rectángulo visible de una ventana: con el marco del gestor y sin la sombra de GTK.
fn frame_of(win: Window) -> Option<Rect> {
    let x = x()?;
    let attrs = x.conn.get_window_attributes(win).ok()?.reply().ok()?;
    if attrs.map_state != MapState::VIEWABLE {
        return None;
    }
    let geo = x.conn.get_geometry(win).ok()?.reply().ok()?;
    let pos = x.conn.translate_coordinates(win, x.root, 0, 0).ok()?.reply().ok()?;
    let (mut rx, mut ry, mut rw, mut rh) = (pos.dst_x as i32, pos.dst_y as i32, geo.width as i32, geo.height as i32);
    let f = prop32(win, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL);
    if f.len() == 4 {
        rx -= f[0] as i32;
        ry -= f[2] as i32;
        rw += (f[0] + f[1]) as i32;
        rh += (f[2] + f[3]) as i32;
    }
    let g = prop32(win, "_GTK_FRAME_EXTENTS", AtomEnum::CARDINAL);
    if g.len() == 4 {
        rx += g[0] as i32;
        ry += g[2] as i32;
        rw -= (g[0] + g[1]) as i32;
        rh -= (g[2] + g[3]) as i32;
    }
    (rw > 0 && rh > 0).then_some((rx, ry, rw, rh))
}

/// La ventana activa, salvo el escritorio.
fn active_frame() -> Option<Rect> {
    let x = x()?;
    let win = prop32(x.root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW).first().copied().filter(|&w| w != 0)?;
    let desktop = atom("_NET_WM_WINDOW_TYPE_DESKTOP");
    if prop32(win, "_NET_WM_WINDOW_TYPE", AtomEnum::ATOM).contains(&desktop) {
        return None;
    }
    frame_of(win)
}

/// Ventanas visibles de esta app (avisos de pausa, la principal…).
fn own_windows(fx: &mut Fx) -> Vec<Rect> {
    if fx.own_at.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
        return fx.own.clone();
    }
    let me = std::process::id();
    let mut out = Vec::new();
    if let Some(x) = x() {
        for w in prop32(x.root, "_NET_CLIENT_LIST", AtomEnum::WINDOW) {
            if w != fx.main && prop32(w, "_NET_WM_PID", AtomEnum::CARDINAL).first() == Some(&me) {
                if let Some(r) = frame_of(w) {
                    out.push(r);
                }
            }
        }
    }
    fx.own = out.clone();
    fx.own_at = Some(Instant::now());
    out
}

fn monitors() -> Vec<Rect> {
    let Some(x) = x() else { return Vec::new() };
    let Some(res) = x.conn.randr_get_screen_resources_current(x.root).ok().and_then(|c| c.reply().ok()) else {
        return Vec::new();
    };
    res.crtcs
        .iter()
        .filter_map(|&c| x.conn.randr_get_crtc_info(c, res.config_timestamp).ok()?.reply().ok())
        .filter(|ci| ci.width > 0 && ci.height > 0)
        .map(|ci| (ci.x as i32, ci.y as i32, ci.width as i32, ci.height as i32))
        .collect()
}

fn contains(r: &Rect, px: i32, py: i32) -> bool {
    px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3
}

fn cursor() -> Option<(i32, i32)> {
    let x = x()?;
    let p = x.conn.query_pointer(x.root).ok()?.reply().ok()?;
    Some((p.root_x as i32, p.root_y as i32))
}

/// Sin los paneles del escritorio (_NET_WORKAREA recortado a la pantalla).
fn without_panels(m: Rect) -> Rect {
    let Some(x) = x() else { return m };
    let a = prop32(x.root, "_NET_WORKAREA", AtomEnum::CARDINAL);
    if a.len() < 4 {
        return m;
    }
    let (ax, ay, aw, ah) = (a[0] as i32, a[1] as i32, a[2] as i32, a[3] as i32);
    let l = m.0.max(ax);
    let t = m.1.max(ay);
    let r = (m.0 + m.2).min(ax + aw);
    let b = (m.1 + m.3).min(ay + ah);
    if r - l < 100 || b - t < 100 {
        m
    } else {
        (l, t, r - l, b - t)
    }
}

/// La ventana principal de la app no lleva hueco: solo los avisos de pausa.
pub fn set_main_window(win: u32) {
    FX.with(|fx| fx.borrow_mut().main = win);
}

/// Franja de lectura: libre alrededor del ratón, velada arriba y abajo.
pub fn band(spec: Option<Band>) {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let Some(spec) = spec else {
            fx.band.iter_mut().for_each(hide);
            flush();
            return;
        };
        while fx.band.len() < 2 {
            let Some(p) = new_pane(&mut fx) else { return };
            fx.band.push(p);
        }
        let Some((cx, cy)) = cursor() else { return };
        let mons = monitors();
        let Some(m) = mons.iter().find(|m| contains(m, cx, cy)).or(mons.first()).copied() else { return };
        let half = spec.height.max(20) / 2;
        let top_h = (cy - half - m.1).max(0);
        let bottom_y = (cy + half).min(m.1 + m.3);
        let parts = [(m.0, m.1, m.2, top_h), (m.0, bottom_y, m.2, m.1 + m.3 - bottom_y)];
        let holes = own_windows(&mut fx);
        for (i, r) in parts.into_iter().enumerate() {
            let p = &mut fx.band[i];
            if r.3 > 0 {
                show(p, r, spec.color, spec.alpha, &holes);
            } else {
                hide(p);
            }
        }
        keep_on_top(&mut fx);
        flush();
    });
}

/// Velo del fondo: cubre las pantallas menos la ventana activa.
pub fn veil(spec: Option<Veil>) {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let Some(spec) = spec else {
            fx.veils.iter_mut().for_each(hide);
            flush();
            return;
        };
        let active = active_frame();
        let mut mons = monitors();
        if spec.current_monitor_only {
            // La pantalla de la ventana activa (o la del ratón).
            let (px, py) = active
                .map(|a| (a.0 + a.2 / 2, a.1 + a.3 / 2))
                .or_else(cursor)
                .unwrap_or((0, 0));
            mons.retain(|m| contains(m, px, py));
        }
        let areas: Vec<Rect> = mons.into_iter().map(|m| if spec.taskbar { m } else { without_panels(m) }).collect();
        while fx.veils.len() < areas.len() {
            let Some(p) = new_pane(&mut fx) else { return };
            fx.veils.push(p);
        }
        let mut holes = own_windows(&mut fx);
        holes.extend(active);
        for i in 0..fx.veils.len() {
            let area = areas.get(i).copied();
            let p = &mut fx.veils[i];
            match area {
                Some(r) => show(p, r, spec.color, spec.alpha, &holes),
                None => hide(p),
            }
        }
        keep_on_top(&mut fx);
        flush();
    });
}

pub fn destroy_veils() {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let fx = &mut *fx;
        if let Some(x) = x() {
            for p in fx.band.drain(..).chain(fx.veils.drain(..)) {
                let _ = x.conn.destroy_window(p.win);
            }
            let _ = x.conn.flush();
        }
    });
}
