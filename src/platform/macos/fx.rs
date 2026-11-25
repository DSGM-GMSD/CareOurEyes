//! Enfoque en macOS: franja de lectura y velo del fondo.
//!
//! Cada zona velada es una ventana sin borde, semitransparente, que deja pasar
//! el ratón. Los huecos (la ventana activa y los avisos de la app) se dejan
//! partiendo el área en rectángulos que los rodean.

use super::cg::{self, CGRect};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::RefCell;

/// (x, y, ancho, alto) en puntos, origen arriba a la izquierda de la principal.
type Rect = (f64, f64, f64, f64);

pub struct Band {
    /// Alto en puntos.
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

struct Pane {
    win: Retained<NSWindow>,
    shown: Option<(Rect, u32, u8)>,
}

#[derive(Default)]
struct Fx {
    band: Vec<Pane>,
    veils: Vec<Pane>,
}

thread_local! {
    static FX: RefCell<Fx> = RefCell::new(Fx::default());
}

fn new_pane(mtm: MainThreadMarker) -> Pane {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
    let win = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { win.setReleasedWhenClosed(false) };
    win.setOpaque(false);
    win.setHasShadow(false);
    win.setIgnoresMouseEvents(true);
    win.setLevel(NSFloatingWindowLevel);
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    Pane { win, shown: None }
}

fn primary_height() -> f64 {
    unsafe { cg::CGDisplayBounds(cg::CGMainDisplayID()).size.height }
}

fn show(p: &mut Pane, r: Rect, color: u32, alpha: u8) {
    let want = (r, color, alpha);
    if p.shown == Some(want) {
        return;
    }
    if p.shown.map(|s| (s.1, s.2)) != Some((color, alpha)) {
        let (red, green, blue) = (color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF);
        let c = NSColor::colorWithSRGBRed_green_blue_alpha(
            red as f64 / 255.0,
            green as f64 / 255.0,
            blue as f64 / 255.0,
            alpha as f64 / 255.0,
        );
        p.win.setBackgroundColor(Some(&c));
    }
    // De arriba-izquierda a las coordenadas de Cocoa (abajo-izquierda).
    let y = primary_height() - r.1 - r.3;
    p.win.setFrame_display(NSRect::new(NSPoint::new(r.0, y), NSSize::new(r.2, r.3)), true);
    if p.shown.is_none() {
        p.win.orderFrontRegardless();
    }
    p.shown = Some(want);
}

fn hide(p: &mut Pane) {
    if p.shown.take().is_some() {
        p.win.orderOut(None);
    }
}

/// Muestra las zonas `rects` usando (y creando si hace falta) ventanas de `pool`.
fn fill(pool: &mut Vec<Pane>, rects: &[Rect], color: u32, alpha: u8) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    while pool.len() < rects.len() {
        pool.push(new_pane(mtm));
    }
    for (i, p) in pool.iter_mut().enumerate() {
        match rects.get(i) {
            Some(r) => show(p, *r, color, alpha),
            None => hide(p),
        }
    }
}

/// `area` menos `hole`: hasta cuatro rectángulos alrededor.
fn subtract(area: Rect, hole: Rect) -> Vec<Rect> {
    let (ax2, ay2) = (area.0 + area.2, area.1 + area.3);
    let (hx1, hy1) = (hole.0.max(area.0), hole.1.max(area.1));
    let (hx2, hy2) = ((hole.0 + hole.2).min(ax2), (hole.1 + hole.3).min(ay2));
    if hx1 >= hx2 || hy1 >= hy2 {
        return vec![area];
    }
    let mut out = Vec::new();
    if hy1 > area.1 {
        out.push((area.0, area.1, area.2, hy1 - area.1));
    }
    if ay2 > hy2 {
        out.push((area.0, hy2, area.2, ay2 - hy2));
    }
    if hx1 > area.0 {
        out.push((area.0, hy1, hx1 - area.0, hy2 - hy1));
    }
    if ax2 > hx2 {
        out.push((hx2, hy1, ax2 - hx2, hy2 - hy1));
    }
    out
}

fn cut(areas: Vec<Rect>, holes: &[Rect]) -> Vec<Rect> {
    let mut parts = areas;
    for h in holes {
        parts = parts.into_iter().flat_map(|a| subtract(a, *h)).collect();
    }
    parts.retain(|r| r.2 >= 1.0 && r.3 >= 1.0);
    parts
}

fn as_rect(r: CGRect) -> Rect {
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

/// Pantallas: completas o sin la barra de menús y el Dock.
fn screens(visible: bool) -> Vec<Rect> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let h = primary_height();
    objc2_app_kit::NSScreen::screens(mtm)
        .iter()
        .map(|s| {
            let f = if visible { s.visibleFrame() } else { s.frame() };
            (f.origin.x, h - f.origin.y - f.size.height, f.size.width, f.size.height)
        })
        .collect()
}

fn contains(r: &Rect, x: f64, y: f64) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

/// Huecos: los avisos de la app (ventanas flotantes propias que no son velos).
fn own_holes(fx: &Fx, list: &[cg::WindowInfo]) -> Vec<Rect> {
    let me = std::process::id() as i64;
    let ours: Vec<i64> = fx.band.iter().chain(fx.veils.iter()).map(|p| p.win.windowNumber() as i64).collect();
    list.iter()
        .filter(|w| w.pid == me && w.layer != 0 && !ours.contains(&w.number))
        .map(|w| as_rect(w.bounds))
        .collect()
}

/// Franja de lectura: libre alrededor del ratón, velada arriba y abajo.
pub fn band(spec: Option<Band>) {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let fx = &mut *fx;
        let Some(spec) = spec else {
            fx.band.iter_mut().for_each(hide);
            return;
        };
        let c = cg::cursor();
        let all = screens(false);
        let Some(m) = all.iter().find(|m| contains(m, c.x, c.y)).or(all.first()).copied() else { return };
        let half = spec.height.max(20) as f64 / 2.0;
        let top_h = (c.y - half - m.1).max(0.0);
        let bottom_y = (c.y + half).min(m.1 + m.3);
        let parts = vec![(m.0, m.1, m.2, top_h), (m.0, bottom_y, m.2, m.1 + m.3 - bottom_y)];
        let holes = own_holes(fx, &cg::on_screen_windows());
        let rects = cut(parts, &holes);
        fill(&mut fx.band, &rects, spec.color, spec.alpha);
    });
}

/// Velo del fondo: cubre las pantallas menos la ventana activa.
pub fn veil(spec: Option<Veil>) {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let fx = &mut *fx;
        let Some(spec) = spec else {
            fx.veils.iter_mut().for_each(hide);
            return;
        };
        let list = cg::on_screen_windows();
        // La ventana de delante de la app activa (capa 0 = ventanas normales).
        let front = objc2_app_kit::NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|a| a.processIdentifier() as i64);
        let active = front
            .and_then(|pid| list.iter().find(|w| w.pid == pid && w.layer == 0))
            .map(|w| as_rect(w.bounds));
        let mut areas = screens(!spec.taskbar);
        if spec.current_monitor_only {
            let (px, py) = active.map(|a| (a.0 + a.2 / 2.0, a.1 + a.3 / 2.0)).unwrap_or_else(|| {
                let c = cg::cursor();
                (c.x, c.y)
            });
            areas.retain(|m| contains(m, px, py));
        }
        let mut holes = own_holes(fx, &list);
        holes.extend(active);
        let rects = cut(areas, &holes);
        fill(&mut fx.veils, &rects, spec.color, spec.alpha);
    });
}

pub fn destroy_veils() {
    FX.with(|fx| {
        let mut fx = fx.borrow_mut();
        let fx = &mut *fx;
        for p in fx.band.iter_mut().chain(fx.veils.iter_mut()) {
            hide(p);
            p.win.close();
        }
        fx.band.clear();
        fx.veils.clear();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hole_in_the_middle_leaves_four_parts() {
        let parts = cut(vec![(0.0, 0.0, 100.0, 100.0)], &[(40.0, 40.0, 20.0, 20.0)]);
        assert_eq!(parts.len(), 4);
        let area: f64 = parts.iter().map(|r| r.2 * r.3).sum();
        assert_eq!(area, 100.0 * 100.0 - 20.0 * 20.0);
    }
}
