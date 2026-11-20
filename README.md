# CareOurEyes (Rust + Slint)

Brillo, temperatura de color y perfiles de cuidado visual, en Rust con
[Slint](https://slint.dev). La interfaz sigue la de la versión Kotlin y usa el mismo
`config.json` (`%APPDATA%\CareOurEyes\config.json`).

> **Estado: 2.0.0-alpha.1, solo Windows.** Funciona: brillo y temperatura por pantalla,
> los 8 perfiles, Auto Día/Noche (horas fijas o amanecer/atardecer), transiciones suaves,
> bandeja con menú propio (Mostrar ventana · Pausar/Reanudar filtro · Salir), arrancar con
> Windows, buscar actualizaciones, 10 idiomas y temas claro/oscuro con 7 colores de acento.

## Pendiente

- **Linux y macOS.** En otros sistemas solo hay un simulador de dos pantallas para
  probar la interfaz. No hay rampa gamma, bandeja ni arranque al iniciar sesión.
- **Disponible en una próxima versión:** Pausa, Enfoque, Prisma, y en Opciones las
  pestañas Temporizador, Atajos y Reglas. Esas secciones no se aplican todavía; si
  ya están en `config.json`, se conservan intactas.

## Compilar en Windows

1. Instala [Rust](https://rustup.rs) (elige las *Build Tools* de Visual Studio cuando lo pida).
2. En esta carpeta:

```powershell
cargo run --release
```

El ejecutable queda en `target\release\CareOurEyes.exe`. Para arrancar oculto en la bandeja:
`CareOurEyes.exe --autostart`.

## Estructura

| Carpeta | Contenido |
| --- | --- |
| `src/config.rs` | `config.json` (compatible con la versión Kotlin) |
| `src/color.rs` | Kelvin → RGB y rampa gamma |
| `src/schedule.rs` | Horario día/noche y cálculo solar |
| `src/i18n.rs`, `i18n/` | Traducciones |
| `src/app.rs` | Estado de la app y conexión con la interfaz |
| `src/platform/windows/` | Rampa gamma GDI, bandeja, autoarranque |
| `ui/` | Interfaz Slint |

## Licencia

GNU GPL v3 (ver `LICENSE`). Slint se usa bajo su licencia GPLv3.
Iconos: [Material Icons](https://github.com/google/material-design-icons) (Apache 2.0).
Fuentes: Inter y JetBrains Mono (SIL OFL 1.1, ver `ui/fonts/`).
Conversión de temperatura: ajuste público de Tanner Helland.
