//! Simulador para sistemas sin motor todavía: dos pantallas de ejemplo que
//! aceptan cualquier valor. Sirve para probar la interfaz.

use super::{Backend, DisplayInfo};

#[derive(Default)]
pub struct Simulated;

impl Backend for Simulated {
    fn name(&self) -> String {
        // COE_SIM_ENGINE permite poner otro nombre en las capturas del README.
        std::env::var("COE_SIM_ENGINE").unwrap_or_else(|_| "Simulador".into())
    }

    fn displays(&mut self) -> Vec<DisplayInfo> {
        vec![
            DisplayInfo {
                id: "SIM\\MONITOR1".into(),
                name: "AOC AG276QZD2".into(),
                width: 2560,
                height: 1440,
                primary: true,
                gamma: true,
            },
            DisplayInfo {
                id: "SIM\\MONITOR2".into(),
                name: "Dell P2419H".into(),
                width: 1920,
                height: 1080,
                primary: false,
                gamma: true,
            },
        ]
    }

    fn apply(&mut self, _id: &str, _b: f64, _k: f64, _standard: bool) -> bool {
        true
    }

    fn restore(&mut self, _id: &str) {}

    fn restore_all(&mut self) {}
}
