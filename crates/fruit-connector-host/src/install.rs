//! Registering the native-messaging host, on request.
//!
//! Fruit does not do this on first run. Registering a host means pointing a
//! browser at an executable, and an app that arranges that silently has
//! installed a browser hook nobody asked for — the behaviour the rest of the
//! privacy contract exists to rule out.
//!
//! But *refusing to automate it at all* was the wrong conclusion. Doing it by
//! hand means writing a JSON file and an `HKCU` registry key, and someone who
//! cannot face that simply never gets domain-level tracking — so the feature
//! they asked for does not exist for them. A button they press, having read
//! what it does, is consent. Silence is what was objectionable, not automation.
//!
//! So: this module builds the manifest and says exactly where it goes. The
//! caller presses the button.
//!
//! # The one thing that cannot be automated
//!
//! `allowed_origins` must name the extension's id, and Chrome only assigns that
//! when the extension is loaded. So the order is fixed: load the extension,
//! read the id off `chrome://extensions`, paste it here. Nothing can shortcut
//! it, and a UI that pretends otherwise would just fail later with a message
//! about a host that "cannot be reached".

use std::path::{Path, PathBuf};

use crate::HOST_MANIFEST_NAME;

/// Chrome extension ids are 32 characters drawn from `a`–`p`: a hex digest with
/// `0`–`f` mapped up into the alphabet. Checking the shape here turns "the
/// connector silently never works" into "that does not look like an extension
/// id", which is a difference the user can act on.
pub fn validate_extension_id(id: &str) -> Result<String, String> {
    let id = id.trim().to_ascii_lowercase();
    if id.len() != 32 || !id.bytes().all(|b| (b'a'..=b'p').contains(&b)) {
        return Err(
            "An extension id is 32 letters between a and p. Copy it from chrome://extensions."
                .into(),
        );
    }
    Ok(id)
}

/// The manifest Chrome reads to find the host.
///
/// `type: stdio` is the only transport native messaging has, which is the whole
/// reason this design needs no port.
pub fn manifest_json(exe: &Path, extension_id: &str) -> String {
    // `serde_json` would be a dependency for four fields, one of which is a
    // path needing backslash escaping — which `to_string` on a `String` does
    // correctly anyway.
    let exe = serde_escape(&exe.to_string_lossy());
    format!(
        "{{\n  \"name\": \"{HOST_MANIFEST_NAME}\",\n  \"description\": \"Fruit browser connector host\",\n  \"path\": \"{exe}\",\n  \"type\": \"stdio\",\n  \"allowed_origins\": [\n    \"chrome-extension://{extension_id}/\"\n  ]\n}}\n"
    )
}

fn serde_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            other => vec![other],
        })
        .collect()
}

pub fn manifest_path(data_dir: &Path) -> PathBuf {
    data_dir.join(format!("{HOST_MANIFEST_NAME}.json"))
}

/// Writes the manifest and returns where it landed.
pub fn write_manifest(data_dir: &Path, exe: &Path, extension_id: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(data_dir)?;
    let path = manifest_path(data_dir);
    std::fs::write(&path, manifest_json(exe, extension_id))?;
    Ok(path)
}

/// The registry keys Chrome and Edge look the host up under, on Windows.
///
/// `HKCU`, never `HKLM`: this is a per-user choice and must not need
/// administrator rights, which would turn a checkbox into a UAC prompt.
pub fn registry_keys() -> [String; 2] {
    [
        format!("HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\{HOST_MANIFEST_NAME}"),
        format!("HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\{HOST_MANIFEST_NAME}"),
    ]
}

/// `reg.exe` arguments that point one key at the manifest.
///
/// Shelling out to `reg.exe` rather than taking a registry crate: it ships with
/// Windows, it needs no `unsafe`, and it adds no dependency to a build that
/// cannot be compiled on the machine this was written on. The command is also
/// something a user can read, copy and run themselves, which matters for a step
/// that touches their registry.
pub fn registry_args(key: &str, manifest: &Path) -> Vec<String> {
    vec![
        "add".into(),
        key.into(),
        "/ve".into(),
        "/t".into(),
        "REG_SZ".into(),
        "/d".into(),
        manifest.to_string_lossy().into_owned(),
        "/f".into(),
    ]
}

/// Where Chrome and Edge look for host manifests on macOS and Linux — no
/// registry involved, the file's location *is* the registration.
pub fn manifest_install_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    if cfg!(target_os = "macos") {
        let base = home.join("Library/Application Support");
        vec![
            base.join("Google/Chrome/NativeMessagingHosts"),
            base.join("Microsoft Edge/NativeMessagingHosts"),
            base.join("Chromium/NativeMessagingHosts"),
        ]
    } else {
        vec![
            home.join(".config/google-chrome/NativeMessagingHosts"),
            home.join(".config/microsoft-edge/NativeMessagingHosts"),
            home.join(".config/chromium/NativeMessagingHosts"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_id_is_checked_for_shape_not_merely_accepted() {
        assert_eq!(
            validate_extension_id("  ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP  ").unwrap(),
            "abcdefghijklmnopabcdefghijklmnop",
            "pasted ids arrive with whitespace and sometimes capitalised"
        );
        // Too short, too long, and — the common paste error — a full URL.
        assert!(validate_extension_id("abc").is_err());
        assert!(validate_extension_id(&"a".repeat(33)).is_err());
        assert!(validate_extension_id("chrome-extension://abcdefghijklmnop/").is_err());
        // `q`–`z` never appear in a real id.
        assert!(validate_extension_id(&"z".repeat(32)).is_err());
    }

    /// A Windows path is full of backslashes, and an unescaped one makes the
    /// manifest invalid JSON — which Chrome reports as a host that cannot be
    /// found, with no hint about why.
    #[test]
    fn a_windows_path_survives_into_valid_json() {
        let json = manifest_json(
            Path::new(r"C:\Program Files\Fruit\fruit.exe"),
            "abcdefghijklmnopabcdefghijklmnop",
        );
        assert!(
            json.contains(r#""path": "C:\\Program Files\\Fruit\\fruit.exe""#),
            "backslashes were not escaped: {json}"
        );
        assert!(json.contains(&format!("\"name\": \"{HOST_MANIFEST_NAME}\"")));
        assert!(json.contains("chrome-extension://abcdefghijklmnopabcdefghijklmnop/"));
        assert!(json.contains("\"type\": \"stdio\""));
    }

    #[test]
    fn the_manifest_is_written_where_the_registry_key_will_point() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(
            dir.path(),
            Path::new("/opt/fruit"),
            "abcdefghijklmnopabcdefghijklmnop",
        )
        .unwrap();
        assert_eq!(path, manifest_path(dir.path()));
        assert!(std::fs::read_to_string(&path).unwrap().contains("stdio"));

        let args = registry_args(&registry_keys()[0], &path);
        assert!(args.contains(&path.to_string_lossy().into_owned()));
        // `/f` matters: without it `reg add` prompts, and a prompt in a process
        // with no console hangs forever.
        assert!(args.contains(&"/f".to_string()));
    }

    /// Per-user, never machine-wide: HKLM would need administrator rights and
    /// turn a checkbox into a UAC prompt.
    #[test]
    fn registration_is_per_user() {
        for key in registry_keys() {
            assert!(key.starts_with("HKCU\\"), "{key}");
            assert!(key.ends_with(HOST_MANIFEST_NAME));
        }
    }
}
