//! Which application is in front (§3.5, P2).
//!
//! Platform truth, stated rather than papered over — the spec is explicit that
//! the UI must not pretend otherwise:
//!
//! | Platform | Frontmost app | Window title | Status |
//! |---|---|---|---|
//! | Windows | `GetForegroundWindow` + `QueryFullProcessImageName` | `GetWindowText` | implemented, no permission needed |
//! | macOS | `NSWorkspace.frontmostApplication` | needs Accessibility | **not implemented** |
//! | Linux/X11 | `_NET_ACTIVE_WINDOW` | `_NET_WM_NAME` | **not implemented** |
//! | Linux/Wayland | — | — | **impossible by design** |
//!
//! `Support::describe()` is what Settings shows, so the reason a platform can't
//! do this is always on screen next to the switch that would enable it.
//!
//! No new crates: the Windows path is raw FFI against `user32` and `kernel32`,
//! which is a stable, documented ABI and cheaper than pulling in a binding
//! crate for four calls.

/// What this platform can actually do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Support {
    /// Apps and window titles are both available.
    Full,
    /// Nothing is available, and it never will be here.
    Impossible,
    /// Nothing is available yet, but the platform could do it.
    NotImplemented,
}

impl Support {
    pub fn available(self) -> bool {
        self == Support::Full
    }

    /// The sentence Settings shows. Never "unavailable" with no reason.
    pub fn describe(self) -> &'static str {
        match self {
            Support::Full => {
                "Sampling the frontmost application locally. Nothing leaves this machine."
            }
            Support::Impossible => {
                "Wayland does not let an application see which window is in front, \
                 so Activity cannot work here. It works on X11 sessions."
            }
            Support::NotImplemented => {
                "Activity isn't wired up on this platform yet — the tracking itself is \
                 built, only the per-platform hook is missing. It works on Windows today."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmost {
    /// Executable name or bundle id — never a full path, which would leak the
    /// user's directory layout into the database for no benefit.
    pub app_id: String,
    pub window_title: Option<String>,
}

pub fn support() -> Support {
    platform::support()
}

/// `None` when nothing is focused, or when the platform can't answer.
pub fn current() -> Option<Frontmost> {
    platform::current()
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Frontmost, Support};

    type Hwnd = *mut core::ffi::c_void;
    type Handle = *mut core::ffi::c_void;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetWindowTextW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            buf: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub fn support() -> Support {
        Support::Full
    }

    pub fn current() -> Option<Frontmost> {
        // SAFETY: every call below is a documented Win32 entry point given a
        // correctly sized buffer, and each handle opened is closed on all paths.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None; // nothing focused, e.g. the lock screen
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }

            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None; // an elevated process we're not allowed to inspect
            }
            let mut buf = [0u16; 520];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size) != 0;
            CloseHandle(process);
            if !ok {
                return None;
            }

            let path = String::from_utf16_lossy(&buf[..size as usize]);
            // The executable name, not the path: `code.exe`, not
            // `C:\Users\someone\AppData\...\code.exe`.
            let app_id = path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&path)
                .to_string();
            if app_id.is_empty() {
                return None;
            }

            let mut title_buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
            let window_title = (len > 0)
                .then(|| String::from_utf16_lossy(&title_buf[..len as usize]))
                .filter(|t| !t.trim().is_empty());

            Some(Frontmost {
                app_id,
                window_title,
            })
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{Frontmost, Support};

    pub fn support() -> Support {
        if cfg!(target_os = "linux") && std::env::var("WAYLAND_DISPLAY").is_ok() {
            Support::Impossible
        } else {
            Support::NotImplemented
        }
    }

    pub fn current() -> Option<Frontmost> {
        None
    }
}
