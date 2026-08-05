// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Chrome spawns this same executable as its native-messaging host. That path
    // must return *before* Tauri builds anything: the host has no window, and —
    // more importantly — it must never open the database, because the app is
    // single-instance precisely so two processes never share that file.
    if fruit_lib::connector::invoked_as_host() {
        if let Some(dir) = fruit_lib::connector::host_data_dir() {
            let _ = fruit_lib::connector::run_host(&dir);
        }
        return;
    }
    fruit_lib::run()
}
