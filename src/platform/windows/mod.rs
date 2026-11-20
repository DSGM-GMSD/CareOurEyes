//! Windows: rampa gamma de GDI, bandeja, autoarranque y detalles de la ventana.

pub mod gamma;
pub mod shell;
pub mod tray;

/// Texto en UTF-16 terminado en 0, para las API de Windows.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
