//! Atajos globales en macOS (Carbon, a través de global-hotkey).

use super::emit;
use crate::hotkeys::{ALT, CTRL, SHIFT, WIN};
use crate::platform::TrayEvent;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

thread_local! {
    static MANAGER: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Vec<HotKey>> = const { RefCell::new(Vec::new()) };
}

/// id interno de global-hotkey → número de atajo de la app.
static IDS: Mutex<Option<HashMap<u32, i32>>> = Mutex::new(None);

fn code(vk: u32) -> Option<Code> {
    use Code::*;
    const LETTERS: [Code; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR,
        KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [Code; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    const PAD: [Code; 10] =
        [Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    const FKEYS: [Code; 20] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
    ];
    Some(match vk {
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x60..=0x69 => PAD[(vk - 0x60) as usize],
        0x70..=0x83 => FKEYS[(vk - 0x70) as usize],
        0x25 => ArrowLeft,
        0x26 => ArrowUp,
        0x27 => ArrowRight,
        0x28 => ArrowDown,
        0x21 => PageUp,
        0x22 => PageDown,
        0x23 => End,
        0x24 => Home,
        0x2D => Insert,
        0x2E => Delete,
        0x20 => Space,
        0x09 => Tab,
        0x6A => NumpadMultiply,
        0x6B => NumpadAdd,
        0x6D => NumpadSubtract,
        0x6F => NumpadDivide,
        0xBB => Equal,
        0xBC => Comma,
        0xBD => Minus,
        0xBE => Period,
        _ => return None,
    })
}

fn modifiers(mods: u32) -> Modifiers {
    let mut m = Modifiers::empty();
    if mods & CTRL != 0 {
        m |= Modifiers::CONTROL;
    }
    if mods & ALT != 0 {
        m |= Modifiers::ALT;
    }
    if mods & SHIFT != 0 {
        m |= Modifiers::SHIFT;
    }
    if mods & WIN != 0 {
        m |= Modifiers::META;
    }
    m
}

fn manager_ready() -> bool {
    MANAGER.with(|m| {
        let mut m = m.borrow_mut();
        if m.is_none() {
            *m = GlobalHotKeyManager::new().ok();
            if m.is_some() {
                GlobalHotKeyEvent::set_event_handler(Some(|e: GlobalHotKeyEvent| {
                    if e.state() != HotKeyState::Pressed {
                        return;
                    }
                    let id = IDS.lock().ok().and_then(|g| g.as_ref().and_then(|map| map.get(&e.id()).copied()));
                    if let Some(id) = id {
                        emit(TrayEvent::Hotkey(id));
                    }
                }));
            }
        }
        m.is_some()
    })
}

/// Registra los atajos (número, modificadores, tecla virtual). Devuelve los que fallaron.
pub fn register_hotkeys(list: &[(i32, u32, u32)]) -> Vec<i32> {
    if !manager_ready() {
        return list.iter().map(|h| h.0).collect();
    }
    MANAGER.with(|m| {
        let m = m.borrow();
        let Some(manager) = m.as_ref() else { return list.iter().map(|h| h.0).collect() };
        ACTIVE.with(|a| {
            let mut active = a.borrow_mut();
            let _ = manager.unregister_all(&active);
            active.clear();
            let mut ids = HashMap::new();
            let mut failed = Vec::new();
            for &(id, mods, vk) in list {
                let Some(c) = code(vk) else {
                    failed.push(id);
                    continue;
                };
                let hk = HotKey::new(Some(modifiers(mods)), c);
                if manager.register(hk).is_ok() {
                    ids.insert(hk.id(), id);
                    active.push(hk);
                } else {
                    failed.push(id);
                }
            }
            if let Ok(mut g) = IDS.lock() {
                *g = Some(ids);
            }
            failed
        })
    })
}
