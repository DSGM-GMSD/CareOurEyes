//! Icono de bandeja por StatusNotifierItem (D-Bus). Lo muestran Cinnamon (xapp),
//! KDE, XFCE, MATE y GNOME con la extensión AppIndicator. El menú es el nativo
//! del escritorio.

use super::emit;
use crate::platform::TrayEvent;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip, Tray};
use std::sync::Mutex;

pub use crate::platform::TrayLabels as Labels;

struct CoeTray {
    icon: Vec<Icon>,
    labels: Labels,
}

impl Tray for CoeTray {
    fn id(&self) -> String {
        "careoureyes".into()
    }
    fn title(&self) -> String {
        "CareOurEyes".into()
    }
    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip { title: "CareOurEyes".into(), ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        emit(TrayEvent::Open);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: self.labels.show.clone(),
                activate: Box::new(|_: &mut Self| emit(TrayEvent::Open)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.labels.pause.clone(),
                activate: Box::new(|_: &mut Self| emit(TrayEvent::TogglePause)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: self.labels.quit.clone(),
                activate: Box::new(|_: &mut Self| emit(TrayEvent::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

static HANDLE: Mutex<Option<Handle<CoeTray>>> = Mutex::new(None);
static LAST: Mutex<Option<Labels>> = Mutex::new(None);

/// PNG → ARGB32 en orden de red, como pide la especificación.
fn icon_from_png(png_bytes: &[u8]) -> Option<Icon> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let data: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|p| [255, p[0], p[1], p[2]]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|p| [p[1], p[0], p[0], p[0]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [255, g, g, g]).collect(),
        _ => return None,
    };
    Some(Icon { width: info.width as i32, height: info.height as i32, data })
}

/// Crea el icono. `false` si el escritorio no tiene bandeja compatible.
/// `at_login`: al iniciar sesión la bandeja del escritorio (p. ej. xapp-sn-watcher
/// en Cinnamon) puede arrancar después que la app; entonces se espera a que
/// aparezca en lugar de darla por ausente.
pub fn create(png: &[u8], labels: Labels, at_login: bool) -> bool {
    let tray = CoeTray { icon: icon_from_png(png).into_iter().collect(), labels };
    match tray.assume_sni_available(at_login).spawn() {
        Ok(h) => {
            if let Ok(mut g) = HANDLE.lock() {
                *g = Some(h);
            }
            true
        }
        Err(e) => {
            crate::app::debug_log(&format!("bandeja: {e}"));
            false
        }
    }
}

/// Cambia los textos del menú (solo si cambiaron).
pub fn update(labels: Labels) {
    if let Ok(mut last) = LAST.lock() {
        if last.as_ref() == Some(&labels) {
            return;
        }
        *last = Some(labels.clone());
    }
    if let Ok(g) = HANDLE.lock() {
        if let Some(h) = g.as_ref() {
            h.update(move |t| t.labels = labels);
        }
    }
}

pub fn remove() {
    if let Ok(mut g) = HANDLE.lock() {
        if let Some(h) = g.take() {
            let _ = h.shutdown();
        }
    }
}
