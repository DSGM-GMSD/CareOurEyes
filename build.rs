fn main() {
    slint_build::compile("ui/app.slint").expect("Slint: no se pudo compilar ui/app.slint");
    // Las traducciones se incluyen con include_str!; recompila si cambian.
    println!("cargo:rerun-if-changed=i18n");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Icono del ejecutable (Explorador, accesos directos).
        // Si falta rc.exe (Windows SDK) el programa compila igual, sin icono en el .exe.
        if embed_resource::compile("assets/careoureyes.rc", embed_resource::NONE).manifest_optional().is_err() {
            println!("cargo:warning=No se pudo incrustar el icono del .exe (falta rc.exe del Windows SDK).");
        }
    }
}
