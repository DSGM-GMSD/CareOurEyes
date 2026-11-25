//! Brillo y temperatura en Wayland.
//!
//! - Compositores wlroots (Sway, Hyprland, river, Wayfire, labwc…) y otros con
//!   `wlr-gamma-control`: rampa gamma completa por pantalla, como en X11.
//! - GNOME y KDE Plasma no dejan cambiar la gamma desde otra app: se usa su luz
//!   nocturna, que solo cambia la temperatura (el brillo no se puede).

use crate::color;
use crate::platform::{Backend, DisplayInfo};
use std::io::{Seek, Write};
use std::os::fd::AsFd;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

/// ¿La app corre como cliente Wayland? (winit elige Wayland si hay WAYLAND_DISPLAY).
pub fn session_is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        && !std::env::var("WINIT_UNIX_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("x11"))
}

#[derive(Default)]
struct Output {
    output: Option<wl_output::WlOutput>,
    global: u32,
    name: String,
    description: String,
    make: String,
    model: String,
    width: i32,
    height: i32,
    control: Option<ZwlrGammaControlV1>,
    size: u32,
    failed: bool,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    manager: Option<ZwlrGammaControlManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version } => match interface.as_str() {
                "wl_output" => {
                    let output = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state.outputs.push(Output { output: Some(output), global: name, ..Default::default() });
                }
                "zwlr_gamma_control_manager_v1" => {
                    state.manager = Some(registry.bind::<ZwlrGammaControlManagerV1, _, _>(name, 1, qh, ()));
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(i) = state.outputs.iter().position(|o| o.global == name) {
                    let o = state.outputs.remove(i);
                    if let Some(c) = o.control {
                        c.destroy();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(o) = state.outputs.iter_mut().find(|o| o.global == *global) else { return };
        match event {
            wl_output::Event::Geometry { make, model, .. } => {
                o.make = make;
                o.model = model;
            }
            wl_output::Event::Mode { flags, width, height, .. } => {
                if flags.into_result().is_ok_and(|f| f.contains(wl_output::Mode::Current)) {
                    o.width = width;
                    o.height = height;
                }
            }
            wl_output::Event::Name { name } => o.name = name,
            wl_output::Event::Description { description } => o.description = description,
            _ => {}
        }
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrGammaControlManagerV1,
        _: <ZwlrGammaControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, u32> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(o) = state.outputs.iter_mut().find(|o| o.global == *global) else { return };
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => o.size = size,
            // Otra app (p. ej. wlsunset o gammastep) ya controla esta pantalla.
            zwlr_gamma_control_v1::Event::Failed => {
                o.failed = true;
                if let Some(c) = o.control.take() {
                    c.destroy();
                }
            }
            _ => {}
        }
    }
}

pub struct WlrGamma {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
}

impl WlrGamma {
    /// `None` si el compositor no ofrece wlr-gamma-control.
    pub fn new() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).ok()?;
        // Segunda vuelta: nombres y modos de las pantallas.
        queue.roundtrip(&mut state).ok()?;
        state.manager.as_ref()?;
        Some(Self { conn, queue, state })
    }

    fn id_of(o: &Output) -> String {
        let key = if !o.make.is_empty() || !o.model.is_empty() {
            format!("{} {}", o.make, o.model).trim().to_string()
        } else {
            o.name.clone()
        };
        format!("WL\\{key}")
    }

    fn ensure_controls(&mut self) {
        let Some(manager) = self.state.manager.clone() else { return };
        let qh = self.queue.handle();
        let mut created = false;
        for o in &mut self.state.outputs {
            if o.control.is_none() && !o.failed {
                if let Some(out) = &o.output {
                    o.control = Some(manager.get_gamma_control(out, &qh, o.global));
                    created = true;
                }
            }
        }
        if created {
            let _ = self.queue.roundtrip(&mut self.state);
        }
    }

    fn write(&mut self, id: &str, brightness: f64, kelvin: f64) -> bool {
        let _ = self.queue.dispatch_pending(&mut self.state);
        self.ensure_controls();
        let Some(o) = self.state.outputs.iter().find(|o| Self::id_of(o) == id) else { return false };
        let (Some(control), size) = (o.control.as_ref(), o.size as usize) else { return false };
        if size < 2 {
            return false;
        }
        let m = color::multipliers(kelvin.round() as i32);
        let scale = (brightness / 100.0).clamp(0.0, 1.0);
        let mut bytes = Vec::with_capacity(size * 6);
        for f in m {
            for i in 0..size {
                let v = (i as f64 / (size - 1) as f64 * 65535.0 * f * scale).round().clamp(0.0, 65535.0) as u16;
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        // El compositor lee la rampa de un archivo: uno temporal, ya borrado.
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let path = dir.join(format!("careoureyes-gamma-{}", std::process::id()));
        let Ok(mut file) = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path)
        else {
            return false;
        };
        let _ = std::fs::remove_file(&path);
        if file.write_all(&bytes).is_err() || file.rewind().is_err() {
            return false;
        }
        control.set_gamma(file.as_fd());
        self.conn.flush().is_ok()
    }
}

impl Backend for WlrGamma {
    fn name(&self) -> String {
        "Wayland (wlr-gamma-control)".into()
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        let _ = self.queue.roundtrip(&mut self.state);
        self.ensure_controls();
        let mut list: Vec<DisplayInfo> = self
            .state
            .outputs
            .iter()
            .map(|o| {
                let name = if !o.description.is_empty() {
                    o.description.clone()
                } else if !o.model.is_empty() {
                    format!("{} {}", o.make, o.model).trim().to_string()
                } else {
                    o.name.clone()
                };
                DisplayInfo {
                    id: Self::id_of(o),
                    name,
                    width: o.width,
                    height: o.height,
                    primary: false,
                    gamma: o.control.is_some() && o.size > 1,
                }
            })
            .collect();
        // Wayland no tiene pantalla principal: la primera hace de principal.
        if let Some(first) = list.first_mut() {
            first.primary = true;
        }
        list
    }

    fn apply(&mut self, id: &str, brightness: f64, kelvin: f64, _standard: bool) -> bool {
        self.write(id, brightness, kelvin)
    }

    fn restore(&mut self, id: &str) {
        // Soltar el control devuelve la gamma que la pantalla tenía.
        if let Some(o) = self.state.outputs.iter_mut().find(|o| Self::id_of(o) == id) {
            if let Some(c) = o.control.take() {
                c.destroy();
            }
        }
        let _ = self.conn.flush();
    }

    fn restore_all(&mut self) {
        for o in &mut self.state.outputs {
            if let Some(c) = o.control.take() {
                c.destroy();
            }
        }
        let _ = self.conn.flush();
    }
}
