//! Una sola copia de la app en Linux y macOS: la primera escucha en un socket
//! Unix; las siguientes le piden que se muestre y terminan.

use crate::platform::TrayEvent;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    // Sin XDG_RUNTIME_DIR la carpeta puede ser compartida: el nombre lleva el usuario.
    let user = std::env::var("USER").unwrap_or_default();
    dir.join(format!("careoureyes-{user}.sock"))
}

/// `false` si ya hay otra copia (a la que se le pide mostrarse).
pub fn single_instance(emit: fn(TrayEvent)) -> bool {
    let path = socket_path();
    if let Ok(mut s) = UnixStream::connect(&path) {
        let _ = s.write_all(b"show");
        return false;
    }
    // Un archivo de una sesión que terminó mal.
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return true };
    let _ = std::thread::Builder::new().name("single-instance".into()).spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 8];
            let _ = stream.read(&mut buf);
            emit(TrayEvent::ShowRequested);
        }
    });
    true
}

pub fn release() {
    let _ = std::fs::remove_file(socket_path());
}
