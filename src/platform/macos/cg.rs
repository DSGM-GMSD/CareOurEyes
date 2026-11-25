//! Lo mínimo de CoreGraphics que hace falta, declarado a mano.

#![allow(non_snake_case)]

pub type CGDirectDisplayID = u32;
pub type CGError = i32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

/// kCGEventSourceStateCombinedSessionState
pub const COMBINED_SESSION: i32 = 0;
/// kCGAnyInputEventType
pub const ANY_INPUT: u32 = !0;

pub type ReconfigurationCallback = extern "C" fn(display: CGDirectDisplayID, flags: u32, user: *mut core::ffi::c_void);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGMainDisplayID() -> CGDirectDisplayID;
    pub fn CGGetActiveDisplayList(max: u32, displays: *mut CGDirectDisplayID, count: *mut u32) -> CGError;
    pub fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> usize;
    pub fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> usize;
    pub fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
    pub fn CGDisplayVendorNumber(display: CGDirectDisplayID) -> u32;
    pub fn CGDisplayModelNumber(display: CGDirectDisplayID) -> u32;
    pub fn CGDisplaySerialNumber(display: CGDirectDisplayID) -> u32;
    pub fn CGDisplayIsBuiltin(display: CGDirectDisplayID) -> i32;
    pub fn CGDisplayGammaTableCapacity(display: CGDirectDisplayID) -> u32;
    pub fn CGGetDisplayTransferByTable(
        display: CGDirectDisplayID,
        capacity: u32,
        red: *mut f32,
        green: *mut f32,
        blue: *mut f32,
        sample_count: *mut u32,
    ) -> CGError;
    pub fn CGSetDisplayTransferByTable(
        display: CGDirectDisplayID,
        size: u32,
        red: *const f32,
        green: *const f32,
        blue: *const f32,
    ) -> CGError;
    pub fn CGDisplayRestoreColorSyncSettings();
    pub fn CGDisplayRegisterReconfigurationCallback(cb: ReconfigurationCallback, user: *mut core::ffi::c_void) -> CGError;
    pub fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    pub fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    pub fn CGEventSourceFlagsState(state: i32) -> u64;
}

// ---------------------------------------------------------------- ventanas y ratón

pub type CFTypeRef = *const core::ffi::c_void;

/// kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements
pub const ON_SCREEN_NO_DESKTOP: u32 = 1 | 16;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    pub fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut CGRect) -> bool;
    pub fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    pub fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
    pub static kCGWindowOwnerPID: CFTypeRef;
    pub static kCGWindowLayer: CFTypeRef;
    pub static kCGWindowBounds: CFTypeRef;
    pub static kCGWindowNumber: CFTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFArrayGetCount(array: CFTypeRef) -> isize;
    pub fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
    pub fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    pub fn CFNumberGetValue(number: CFTypeRef, kind: isize, value: *mut core::ffi::c_void) -> bool;
    pub fn CFRelease(cf: CFTypeRef);
}

/// kCFNumberSInt64Type
pub const CF_SINT64: isize = 4;

/// Una ventana en pantalla según el servidor de ventanas.
pub struct WindowInfo {
    pub pid: i64,
    pub layer: i64,
    pub number: i64,
    /// Arriba a la izquierda, en puntos.
    pub bounds: CGRect,
}

fn number(dict: CFTypeRef, key: CFTypeRef) -> Option<i64> {
    let v = unsafe { CFDictionaryGetValue(dict, key) };
    if v.is_null() {
        return None;
    }
    let mut out = 0i64;
    unsafe { CFNumberGetValue(v, CF_SINT64, &mut out as *mut i64 as *mut _) }.then_some(out)
}

/// Ventanas en pantalla, de delante hacia atrás. No hace falta ningún permiso
/// para estos datos (sí para los títulos, que no se piden).
pub fn on_screen_windows() -> Vec<WindowInfo> {
    let mut out = Vec::new();
    unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_NO_DESKTOP, 0);
        if list.is_null() {
            return out;
        }
        for i in 0..CFArrayGetCount(list) {
            let d = CFArrayGetValueAtIndex(list, i);
            let b = CFDictionaryGetValue(d, kCGWindowBounds);
            let mut bounds = CGRect::default();
            if b.is_null() || !CGRectMakeWithDictionaryRepresentation(b, &mut bounds) {
                continue;
            }
            out.push(WindowInfo {
                pid: number(d, kCGWindowOwnerPID).unwrap_or(0),
                layer: number(d, kCGWindowLayer).unwrap_or(0),
                number: number(d, kCGWindowNumber).unwrap_or(0),
                bounds,
            });
        }
        CFRelease(list);
    }
    out
}

/// Posición del ratón, arriba a la izquierda, en puntos.
pub fn cursor() -> CGPoint {
    unsafe {
        let e = CGEventCreate(core::ptr::null());
        if e.is_null() {
            return CGPoint::default();
        }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        p
    }
}
