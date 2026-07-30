//! OS-wide input idle, for the idle challenge in §4.5.
//!
//! "Idle" has to mean *away from the machine*, not *away from Fruit's window* —
//! a developer reading a stack trace in their editor is working, and a tracker
//! that discards that time is worse than no tracker. So this asks the OS.
//!
//! Platform truth, stated rather than papered over (the same honesty §3.5
//! demands of Activity):
//!
//! | Platform | How | Works? |
//! |---|---|---|
//! | Windows | `GetLastInputInfo` (user32) | yes, no permission |
//! | macOS | `CGEventSourceSecondsSinceLastEventType` | yes, no permission |
//! | Linux/X11 | XScreenSaver extension | needs libXss; not wired up |
//! | Linux/Wayland | — | impossible by design |
//!
//! Where the OS can't answer, `os_idle_seconds` returns `None` and the caller
//! falls back to input reported by the window itself. That fallback is
//! narrower, and Settings says so instead of pretending.

/// Seconds since the last OS-wide keyboard or pointer input.
pub fn os_idle_seconds() -> Option<u64> {
    platform::idle_seconds()
}

#[cfg(target_os = "windows")]
mod platform {
    #[repr(C)]
    struct LastInputInfo {
        cb_size: u32,
        dw_time: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(plii: *mut LastInputInfo) -> i32;
        fn GetTickCount() -> u32;
    }

    pub fn idle_seconds() -> Option<u64> {
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        // SAFETY: `info` is a correctly sized, initialised LASTINPUTINFO and the
        // call only writes into it.
        let ok = unsafe { GetLastInputInfo(&mut info) } != 0;
        if !ok {
            return None;
        }
        let now = unsafe { GetTickCount() };
        Some((now.wrapping_sub(info.dw_time) / 1000) as u64)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    const HID_SYSTEM_STATE: u32 = 1;
    /// `kCGAnyInputEventType`
    const ANY_INPUT: u32 = 0xFFFF_FFFF;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
    }

    pub fn idle_seconds() -> Option<u64> {
        // SAFETY: both arguments are plain integers and the call has no
        // side effects beyond reading the HID event source.
        let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT) };
        if seconds.is_finite() && seconds >= 0.0 {
            Some(seconds as u64)
        } else {
            None
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    /// X11 could answer this through the XScreenSaver extension, and Wayland
    /// cannot answer it at all. Rather than link libXss for half of Linux, the
    /// timer falls back to input reported by the window — see the module docs.
    pub fn idle_seconds() -> Option<u64> {
        None
    }
}
