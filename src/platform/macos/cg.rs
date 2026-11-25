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
