#!/usr/bin/env python3
"""Genera docs/index.html (español) y docs/en/index.html (inglés).

El sitio se publica con GitHub Pages desde la carpeta docs/ de la rama main.
Para cambiar un texto, edítalo aquí y vuelve a ejecutar:  python3 packaging/site/build.py
"""
import html
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REPO = "https://github.com/DSGM-GMSD/CareOurEyes"
RELEASES = REPO + "/releases"

src = open(os.path.join(ROOT, "ui", "icons.slint"), encoding="utf-8").read()
ICONS = dict(re.findall(r'out property <string> ([a-z-]+): "([^"]+)"', src))
ICONS["download"] = "M5 20h14v-2H5v2zM19 9h-4V3H9v6H5l7 7 7-7z"
ICONS["shield"] = "M12 1 3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z"
ICONS["keyboard"] = "M20 5H4c-1.1 0-1.99.9-1.99 2L2 17c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm-9 3h2v2h-2V8zm0 3h2v2h-2v-2zM8 8h2v2H8V8zm0 3h2v2H8v-2zm-1 2H5v-2h2v2zm0-3H5V8h2v2zm9 7H8v-2h8v2zm0-4h-2v-2h2v2zm0-3h-2V8h2v2zm3 3h-2v-2h2v2zm0-3h-2V8h2v2z"


def icon(name):
    return f'<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="{ICONS[name]}"/></svg>'


T = {
    "es": {
        "lang": "es", "other": "en", "other_label": "English", "other_href": "en/", "base": "",
        "title": "CareOurEyes — brillo, temperatura de color y descansos para tus ojos",
        "desc": "App gratuita y de código abierto para cuidar la vista: brillo por debajo del mínimo del monitor, temperatura de color, perfiles por pantalla, Auto Día/Noche y recordatorios de pausa. Windows, Linux y macOS.",
        "nav": [("#funciones", "Funciones"), ("#capturas", "Capturas"), ("#sistemas", "Sistemas"), ("#preguntas", "Preguntas")],
        "badge": "Gratis · Código abierto · GPL-3.0",
        "h1": 'Cuida tu vista frente a la <span>pantalla</span>',
        "lead": "Baja el brillo más allá del mínimo de tu monitor, calienta el color de noche y recuerda descansar la vista. Sin cuentas, sin anuncios y sin versión de pago.",
        "dl": "Descargar", "dl_win": "Descargar para Windows", "dl_linux": "Descargar para Linux (.deb)", "dl_mac": "Descargar para macOS",
        "other_dl": "Otros sistemas y archivos", "latest": "Última versión:", "github": "Código en GitHub",
        "shot_alt": "Página Pantalla de CareOurEyes con perfiles de color, brillo y temperatura",
        "feat_h": "Todo lo que hace", "feat_sub": "Una sola app para el brillo, el color y los descansos.",
        "features": [
            ("wb-sunny", "Brillo por debajo del mínimo", "Escala cada píxel, así que baja más que el control del monitor. Ideal para trabajar de noche."),
            ("thermostat", "Temperatura de color", "Del blanco neutro a un tono cálido que cansa menos la vista, con transiciones suaves."),
            ("nights-stay", "Auto Día/Noche", "Valores de día y de noche que cambian solos por horario o según el amanecer y el atardecer de tu ciudad."),
            ("desktop-windows", "Perfiles por pantalla", "Lectura, Edición, Juego, Película, Salud visual, Oficina… Igual en todas las pantallas o cada una por su lado."),
            ("timer", "Recordatorio de pausa", "Regla 20-20-20, Pomodoro, pausas suaves, a pantalla completa o estrictas, y recordatorio de parpadeo."),
            ("filter-center-focus", "Enfoque", "Una franja clara para leer, o un velo que oscurece todo menos la ventana que estás usando."),
            ("visibility", "Prisma", "Oscurece o pone en gris una sola ventana que no tiene modo oscuro (Windows)."),
            ("keyboard", "Atajos y reglas", "Atajos de teclado globales y perfiles automáticos según el programa que tengas delante."),
        ],
        "gal_h": "Capturas", "gal_sub": "Tema claro, oscuro o el del sistema, con siete colores de acento.",
        "gallery": [("screen-dark", "Pantalla, tema oscuro"), ("screen-light", "Pantalla, tema claro"), ("breaks", "Recordatorio de pausa"), ("focus", "Enfoque: franja y velo")],
        "from_h": "¿Vienes de CareUEyes, f.lux o Iris?",
        "from_p": "CareOurEyes nació como una alternativa libre a programas de este tipo. Es un proyecto independiente, escrito desde cero en Rust y Slint, y siempre será gratis.",
        "from_list": ["Gratis de verdad: sin licencias, sin versión Pro y sin cuenta.", "Código abierto (GPL-3.0): cualquiera puede revisarlo, mejorarlo o compilarlo.", "Funciona en Windows, Linux y macOS con la misma interfaz.", "Respeta la calibración de tu monitor y la devuelve tal cual al salir.", "En 10 idiomas, con el español como idioma base."],
        "from_note": "CareUEyes, f.lux e Iris son marcas de sus respectivos dueños. CareOurEyes no tiene relación con ellos.",
        "sys_h": "Sistemas", "sys_sub": "Lo que funciona en cada uno.",
        "sys_cols": ["Función", "Windows", "Linux", "macOS"],
        "sys_rows": [
            ("Brillo, temperatura, perfiles y Auto Día/Noche", "yes", "yes¹", "exp"),
            ("Icono en la bandeja y arrancar al iniciar sesión", "yes", "yes", "exp"),
            ("Recordatorio de pausa", "yes", "yes", "exp"),
            ("Atajos de teclado y reglas por programa", "yes", "x11", "exp"),
            ("Enfoque (franja y velo)", "yes", "x11", "exp"),
            ("Prisma", "yes", "no", "no"),
        ],
        "sys_words": {"yes": "Sí", "exp": "Experimental", "x11": "Sí (X11)", "no": "—", "yes¹": "Sí ¹"},
        "sys_notes": "¹ En X11, y en Wayland con Sway, Hyprland y otros compositores wlroots. En GNOME y KDE Plasma sobre Wayland solo se puede cambiar el color (con su luz nocturna). macOS: escrito pero todavía sin probar en un Mac; agradecemos reportes.",
        "faq_h": "Preguntas frecuentes",
        "faq": [
            ("¿Es gratis de verdad?", "Sí. CareOurEyes es software libre bajo la licencia GPL-3.0. No hay versión de pago, anuncios ni cuentas."),
            ("Windows avisa al instalarla. ¿Es segura?", "Los instaladores no están firmados con un certificado comercial, por eso SmartScreen avisa: elige «Más información» → «Ejecutar de todas formas». Cada descarga incluye una atestación de GitHub que prueba que se compiló desde este repositorio (ver abajo)."),
            ("¿Cómo compruebo que la descarga es auténtica?", "Con GitHub CLI: <code>gh attestation verify ARCHIVO -R DSGM-GMSD/CareOurEyes</code>. También puedes comparar el SHA-256 con el archivo <code>SHA256SUMS</code> del release."),
            ("¿Funciona en Wayland?", "Sí, con límites. En Sway, Hyprland y similares, el brillo y el color funcionan completos. En GNOME y KDE Plasma solo el color, a través de su luz nocturna. Wayland no permite atajos globales, reglas por programa ni Enfoque."),
            ("¿Y en macOS?", "La versión de Mac existe pero todavía nadie la ha probado en un equipo real. No está firmada por Apple: la primera vez, ve a Ajustes del Sistema › Privacidad y seguridad y elige «Abrir igualmente». Desactiva Night Shift mientras la uses."),
            ("¿Cambia la calibración de mi monitor?", "No. Al abrir guarda la tabla de color de cada pantalla, aplica los ajustes encima y al salir la deja exactamente como estaba."),
        ],
        "foot_license": "Software libre bajo la licencia GPL-3.0.",
        "foot_marks": "CareUEyes, f.lux e Iris son marcas de sus respectivos dueños; CareOurEyes es un proyecto independiente sin relación con ellos.",
    },
    "en": {
        "lang": "en", "other": "es", "other_label": "Español", "other_href": "../", "base": "../",
        "title": "CareOurEyes — brightness, color temperature and breaks for your eyes",
        "desc": "Free, open-source eye-care app: dimming below your monitor's minimum, color temperature, per-display profiles, automatic day/night and break reminders. Windows, Linux and macOS.",
        "nav": [("#features", "Features"), ("#screenshots", "Screenshots"), ("#systems", "Systems"), ("#faq", "FAQ")],
        "badge": "Free · Open source · GPL-3.0",
        "h1": 'Look after your eyes at the <span>screen</span>',
        "lead": "Dim below your monitor's minimum, warm up the colors at night and get reminded to rest your eyes. No accounts, no ads and no paid version.",
        "dl": "Download", "dl_win": "Download for Windows", "dl_linux": "Download for Linux (.deb)", "dl_mac": "Download for macOS",
        "other_dl": "Other systems and files", "latest": "Latest version:", "github": "Code on GitHub",
        "shot_alt": "CareOurEyes Display page with color profiles, brightness and temperature",
        "feat_h": "What it does", "feat_sub": "One app for brightness, color and breaks.",
        "features": [
            ("wb-sunny", "Dimming below the minimum", "It scales every pixel, so it goes lower than your monitor's own control. Great for working at night."),
            ("thermostat", "Color temperature", "From neutral white to a warm tone that is easier on the eyes, with smooth transitions."),
            ("nights-stay", "Auto day/night", "Day and night values that switch on their own by schedule or by sunrise and sunset where you live."),
            ("desktop-windows", "Per-display profiles", "Reading, Editing, Gaming, Movie, Eye health, Office… The same on every display or each one on its own."),
            ("timer", "Break reminder", "20-20-20 rule, Pomodoro, gentle, full-screen or strict breaks, and a blink reminder."),
            ("filter-center-focus", "Focus", "A clear band for reading, or a veil that dims everything except the window you are using."),
            ("visibility", "Prisma", "Darkens or grays out a single window that has no dark mode (Windows)."),
            ("keyboard", "Shortcuts and rules", "Global keyboard shortcuts and automatic profiles for the program in front of you."),
        ],
        "gal_h": "Screenshots", "gal_sub": "Light, dark or system theme, with seven accent colors.",
        "gallery": [("screen-dark", "Display, dark theme"), ("screen-light", "Display, light theme"), ("breaks", "Break reminder"), ("focus", "Focus: band and veil")],
        "from_h": "Coming from CareUEyes, f.lux or Iris?",
        "from_p": "CareOurEyes started as a free alternative to programs like these. It is an independent project, written from scratch in Rust and Slint, and it will always be free.",
        "from_list": ["Truly free: no licenses, no Pro version and no account.", "Open source (GPL-3.0): anyone can review it, improve it or build it.", "Runs on Windows, Linux and macOS with the same interface.", "Respects your monitor calibration and restores it exactly on exit.", "In 10 languages."],
        "from_note": "CareUEyes, f.lux and Iris are trademarks of their respective owners. CareOurEyes is not affiliated with them.",
        "sys_h": "Systems", "sys_sub": "What works on each one.",
        "sys_cols": ["Feature", "Windows", "Linux", "macOS"],
        "sys_rows": [
            ("Brightness, temperature, profiles and auto day/night", "yes", "yes¹", "exp"),
            ("Tray icon and start at login", "yes", "yes", "exp"),
            ("Break reminder", "yes", "yes", "exp"),
            ("Keyboard shortcuts and per-program rules", "yes", "x11", "exp"),
            ("Focus (band and veil)", "yes", "x11", "exp"),
            ("Prisma", "yes", "no", "no"),
        ],
        "sys_words": {"yes": "Yes", "exp": "Experimental", "x11": "Yes (X11)", "no": "—", "yes¹": "Yes ¹"},
        "sys_notes": "¹ On X11, and on Wayland with Sway, Hyprland and other wlroots compositors. On GNOME and KDE Plasma under Wayland only the color can change (through their night light). macOS: written but not yet tested on a Mac; reports are welcome.",
        "faq_h": "Frequently asked questions",
        "faq": [
            ("Is it really free?", "Yes. CareOurEyes is free software under the GPL-3.0 license. There is no paid version, no ads and no accounts."),
            ("Windows warns me when installing. Is it safe?", "The installers are not signed with a commercial certificate, so SmartScreen warns: choose “More info” → “Run anyway”. Every download comes with a GitHub attestation proving it was built from this repository (see below)."),
            ("How do I check that a download is genuine?", "With GitHub CLI: <code>gh attestation verify FILE -R DSGM-GMSD/CareOurEyes</code>. You can also compare the SHA-256 with the release's <code>SHA256SUMS</code> file."),
            ("Does it work on Wayland?", "Yes, with limits. On Sway, Hyprland and similar, brightness and color work fully. On GNOME and KDE Plasma only the color, through their night light. Wayland does not allow global shortcuts, per-program rules or Focus."),
            ("And on macOS?", "The Mac version exists but nobody has tested it on a real Mac yet. It is not signed by Apple: the first time, go to System Settings › Privacy & Security and choose “Open Anyway”. Turn off Night Shift while you use it."),
            ("Does it change my monitor calibration?", "No. On start it saves each display's color table, applies its settings on top and restores it exactly on exit."),
        ],
        "foot_license": "Free software under the GPL-3.0 license.",
        "foot_marks": "CareUEyes, f.lux and Iris are trademarks of their respective owners; CareOurEyes is an independent project not affiliated with them.",
    },
}

IDS = {"es": ["funciones", "capturas", "sistemas", "preguntas"], "en": ["features", "screenshots", "systems", "faq"]}


def page(L):
    t = T[L]
    b = t["base"]
    ids = IDS[L]
    e = html.escape
    nav = "".join(f'<a class="opt" href="{h}">{e(n)}</a>' for h, n in t["nav"])
    feats = "".join(
        f'<div class="card"><div class="icon">{icon(i)}</div><h3>{e(h)}</h3><p>{e(p)}</p></div>' for i, h, p in t["features"]
    )
    gal = "".join(
        f'<figure><img src="{b}assets/screens/{k}-{L}.png" width="980" height="700" loading="lazy" alt="{e(c)}">'
        f"<figcaption>{e(c)}</figcaption></figure>"
        for k, c in t["gallery"]
    )
    cls = {"yes": "yes", "yes¹": "yes", "x11": "part", "exp": "part", "no": "no"}
    rows = "".join(
        "<tr><td>" + e(r[0]) + "</td>" + "".join(f'<td class="c {cls[v]}">{e(t["sys_words"][v])}</td>' for v in r[1:]) + "</tr>"
        for r in t["sys_rows"]
    )
    cols = "".join(f'<th{" class=c" if i else ""}>{e(c)}</th>' for i, c in enumerate(t["sys_cols"]))
    faq = "".join(f"<details><summary>{e(q)}</summary><p>{a}</p></details>" for q, a in t["faq"])
    checks = "".join(f"<li>{e(x)}</li>" for x in t["from_list"])
    alt_lang = "en" if L == "es" else "es"
    return f"""<!doctype html>
<html lang="{t['lang']}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{e(t['title'])}</title>
<meta name="description" content="{e(t['desc'])}">
<meta property="og:title" content="{e(t['title'])}">
<meta property="og:description" content="{e(t['desc'])}">
<meta property="og:image" content="https://dsgm-gmsd.github.io/CareOurEyes/assets/screens/screen-dark-{L}.png">
<meta property="og:type" content="website">
<link rel="alternate" hreflang="{alt_lang}" href="{t['other_href']}">
<link rel="icon" href="{b}assets/favicon.png">
<link rel="stylesheet" href="{b}assets/style.css">
</head>
<body>
<header class="top"><div class="wrap">
  <a class="brand" href="./"><img src="{b}assets/icon-192.png" alt="" width="28" height="28">CareOurEyes</a>
  <nav>{nav}<a href="{REPO}">GitHub</a><a class="lang" href="{t['other_href']}" hreflang="{alt_lang}">{e(t['other_label'])}</a></nav>
</div></header>

<main>
<section class="hero"><div class="wrap">
  <div class="badge">{e(t['badge'])}</div>
  <h1>{t['h1']}</h1>
  <p class="lead">{e(t['lead'])}</p>
  <div class="cta">
    <a id="dl-main" class="btn primary" href="{RELEASES}" data-windows="{e(t['dl_win'])}" data-linux="{e(t['dl_linux'])}" data-macos="{e(t['dl_mac'])}">{icon('download')}<span>{e(t['dl'])}</span></a>
    <a class="btn" href="{REPO}">{e(t['github'])}</a>
  </div>
  <p class="small">{e(t['latest'])} <span id="dl-version">—</span> ·
    <a id="dl-windows" href="{RELEASES}">Windows</a> ·
    <a id="dl-linux" href="{RELEASES}">Linux</a> ·
    <a id="dl-macos" href="{RELEASES}">macOS</a> ·
    <a href="{RELEASES}">{e(t['other_dl'])}</a></p>
  <div class="shot"><picture>
    <source srcset="{b}assets/screens/screen-light-{L}.png" media="(prefers-color-scheme: light)">
    <img src="{b}assets/screens/screen-dark-{L}.png" width="980" height="700" alt="{e(t['shot_alt'])}">
  </picture></div>
</div></section>

<section id="{ids[0]}"><div class="wrap">
  <h2>{e(t['feat_h'])}</h2><p class="sub">{e(t['feat_sub'])}</p>
  <div class="grid">{feats}</div>
</div></section>

<section id="{ids[1]}"><div class="wrap">
  <h2>{e(t['gal_h'])}</h2><p class="sub">{e(t['gal_sub'])}</p>
  <div class="gallery">{gal}</div>
</div></section>

<section><div class="wrap from">
  <div><h2>{e(t['from_h'])}</h2><p class="sub">{e(t['from_p'])}</p><p class="notes">{e(t['from_note'])}</p></div>
  <ul class="checks">{checks}</ul>
</div></section>

<section id="{ids[2]}"><div class="wrap">
  <h2>{e(t['sys_h'])}</h2><p class="sub">{e(t['sys_sub'])}</p>
  <div class="table-wrap"><table><thead><tr>{cols}</tr></thead><tbody>{rows}</tbody></table></div>
  <p class="notes">{e(t['sys_notes'])}</p>
</div></section>

<section id="{ids[3]}"><div class="wrap">
  <h2>{e(t['faq_h'])}</h2>
  {faq}
</div></section>
</main>

<footer><div class="wrap">
  <div><p>© DSGM-GMSD · {e(t['foot_license'])} <a href="{REPO}/blob/main/LICENSE">LICENSE</a></p>
  <p>{e(t['foot_marks'])}</p></div>
  <p><a href="{REPO}">GitHub</a> · <a href="{RELEASES}">Releases</a> · <a href="{REPO}/issues">Issues</a></p>
</div></footer>
<script src="{b}assets/site.js"></script>
</body>
</html>
"""


for L, out in (("es", "docs/index.html"), ("en", "docs/en/index.html")):
    path = os.path.join(ROOT, out)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(page(L))
    print("escrito", out)
