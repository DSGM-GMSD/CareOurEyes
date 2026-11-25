//! Icono en la barra de menús con su menú (Mostrar, Pausar/Reanudar, Salir).

use super::emit;
use crate::platform::TrayEvent;
use std::cell::RefCell;
use std::sync::OnceLock;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub use crate::platform::TrayLabels as Labels;

struct Tray {
    _icon: TrayIcon,
    show: MenuItem,
    pause: MenuItem,
    quit: MenuItem,
    labels: Labels,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

static IDS: OnceLock<(MenuId, MenuId, MenuId)> = OnceLock::new();

fn icon_from_png(png_bytes: &[u8]) -> Option<Icon> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

/// Crea el icono. Hay que llamarla con el bucle de eventos ya en marcha.
pub fn create(png: &[u8], labels: Labels) -> bool {
    let show = MenuItem::new(&labels.show, true, None);
    let pause = MenuItem::new(&labels.pause, true, None);
    let quit = MenuItem::new(&labels.quit, true, None);
    let menu = Menu::new();
    if menu.append_items(&[&show, &pause, &PredefinedMenuItem::separator(), &quit]).is_err() {
        return false;
    }
    let _ = IDS.set((show.id().clone(), pause.id().clone(), quit.id().clone()));
    MenuEvent::set_event_handler(Some(std::sync::Arc::new(|e: MenuEvent| {
        let Some((show, pause, quit)) = IDS.get() else { return };
        if &e.id == show {
            emit(TrayEvent::Open);
        } else if &e.id == pause {
            emit(TrayEvent::TogglePause);
        } else if &e.id == quit {
            emit(TrayEvent::Quit);
        }
    })));
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("CareOurEyes");
    if let Some(icon) = icon_from_png(png) {
        builder = builder.with_icon(icon);
    }
    match builder.build() {
        Ok(icon) => {
            TRAY.with(|t| *t.borrow_mut() = Some(Tray { _icon: icon, show, pause, quit, labels }));
            true
        }
        Err(e) => {
            crate::app::debug_log(&format!("barra de menús: {e}"));
            false
        }
    }
}

/// Cambia los textos del menú (solo si cambiaron).
pub fn update(labels: Labels) {
    TRAY.with(|t| {
        if let Some(t) = t.borrow_mut().as_mut() {
            if t.labels != labels {
                t.show.set_text(&labels.show);
                t.pause.set_text(&labels.pause);
                t.quit.set_text(&labels.quit);
                t.labels = labels;
            }
        }
    });
}

pub fn remove() {
    TRAY.with(|t| t.borrow_mut().take());
}
