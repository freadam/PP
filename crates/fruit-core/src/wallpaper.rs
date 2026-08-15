//! Focus-mode wallpapers — a folder of your own images behind the clock.
//!
//! Focus mode ships four gradients bound to the Pomodoro phase. They are good
//! at the one job a phase colour has (a break is recognisable across the room)
//! and they are not what anyone means by a wallpaper. This module is the other
//! option: point Focus at a folder, and it draws what is in it.
//!
//! **Fruit does not bundle photographs.** It ships no images because it has no
//! licence to redistribute anyone's, and inventing a folder of "beautiful
//! wallpapers" would mean shipping other people's work inside a paid binary.
//! What it does instead is create the folder, explain what to put in it, and
//! read whatever you drop there. Your own photos, on your own disk, with no
//! network call — which is the only version of this feature consistent with the
//! rest of the app.
//!
//! ## The security shape, which is the important part
//!
//! `read` takes a **directory and a file name**, never a path. It joins them,
//! canonicalises both sides, and refuses anything that does not land inside the
//! directory. Without that, a command that returns file bytes over IPC is an
//! arbitrary-file-read primitive in a webview — precisely what §7.3's narrow
//! capability file exists to prevent. A `..` in the name, a symlink pointing
//! out of the folder, and an absolute path are all rejected by the same check.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, Result};

/// What the renderer can actually draw. Deliberately short: these are the
/// formats every WebView2 and WKWebView build decodes without a plugin. TIFF
/// and HEIC are common camera outputs and are *not* here, because a wallpaper
/// that silently fails to paint is worse than one that was never offered.
const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "gif"];

/// A full-screen wallpaper crosses IPC as base64, which costs about a third
/// more than the file. 16 MB of source is a very large photograph and still
/// only ~21 MB of string; past that the honest answer is "resize it" rather
/// than a frozen window while the bridge marshals a RAW export.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Enough to fill a picker, few enough that a listing never walks a photo
/// library. The folder is meant to be curated.
const MAX_FILES: usize = 200;

/// What the picker gets: the images, and the folder they came from.
///
/// The path is here so Settings can show the user where to put files — it is
/// display text, and no command accepts one back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperFolder {
    pub dir: String,
    pub items: Vec<Wallpaper>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallpaper {
    /// The file name, which is also the handle `read` takes. Never a path: the
    /// renderer is not given one and cannot construct one.
    pub name: String,
    /// For the picker — "beach-2019.jpg" is not a caption.
    pub label: String,
    pub bytes: u64,
}

/// The text dropped into a newly created wallpaper folder, so the folder
/// explains itself when someone opens it from the file manager rather than
/// from the app.
pub const README: &str = "\
Focus-mode wallpapers
=====================

Drop image files in this folder and Fruit will offer them behind the Focus
clock. Nothing is downloaded and nothing leaves this machine — Fruit reads
what is here and nothing else.

Formats: jpg, jpeg, png, webp, avif, gif
Largest file: 16 MB
Most files listed: 200

Fruit ships no images of its own. It has no licence to redistribute
photographs, so the folder starts empty and what goes in it is yours.

A note on what reads well behind a clock: the timer sits dead centre in large
white type over a dark scrim, so pictures with a busy or bright middle fight
it. Landscapes, skies, long exposures and anything with a calm centre work
best. Fruit does not crop or resize — an image is drawn to cover the screen,
so something near your monitor's aspect ratio will lose the least.
";

/// Create the wallpaper folder if it is missing, and drop the README in beside
/// it the first time. Returns the folder either way.
///
/// Idempotent, and it does not rewrite an existing README — someone may have
/// put their own notes there.
pub fn ensure_dir(parent: &Path) -> Result<PathBuf> {
    let dir = parent.join("wallpapers");
    std::fs::create_dir_all(&dir)?;
    let readme = dir.join("README.txt");
    if !readme.exists() {
        std::fs::write(&readme, README)?;
    }
    Ok(dir)
}

/// Every drawable image in `dir`, sorted by name.
///
/// A missing folder is an empty list, not an error: the folder is a setting a
/// user can point anywhere, including at a drive that is not plugged in, and
/// Focus mode opening on a red banner because a USB stick is out is worse than
/// Focus mode opening on its gradients.
pub fn list(dir: &Path) -> Result<Vec<Wallpaper>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // Files only. A folder named `photo.jpg` is not an image, and
        // descending into subfolders would turn "a curated folder" into "a
        // recursive walk of wherever you pointed it".
        if !entry.file_type()?.is_file() || !has_drawable_extension(&path) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let bytes = entry.metadata()?.len();
        if bytes == 0 || bytes > MAX_BYTES {
            continue;
        }
        out.push(Wallpaper {
            name: name.to_string(),
            label: label_for(name),
            bytes,
        });
    }
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out.truncate(MAX_FILES);
    Ok(out)
}

/// One wallpaper, as a `data:` URI ready to drop into a CSS background.
///
/// `name` is a file name inside `dir` and is treated as untrusted: see the
/// module docs. The renderer never holds a path and cannot ask for one.
pub fn read(dir: &Path, name: &str) -> Result<String> {
    let path = resolve(dir, name)?;
    let bytes = std::fs::metadata(&path)?.len();
    if bytes > MAX_BYTES {
        return Err(AppError::invalid(format!(
            "{name} is {:.1}MB, past the {}MB limit for a wallpaper. Resize it first.",
            bytes as f64 / 1024.0 / 1024.0,
            MAX_BYTES / 1024 / 1024
        )));
    }
    let data = std::fs::read(&path)?;
    Ok(format!("data:{};base64,{}", mime_for(&path), base64(&data)))
}

/// Join, canonicalise, and refuse anything that escapes `dir`.
///
/// Canonicalisation is what makes this hold: it resolves `..`, `.` and symlinks
/// on both sides, so containment is checked on the real location rather than on
/// the spelling of the path. A symlink inside the folder pointing at
/// `/etc/passwd` fails here, and so does a name of `../../secrets.png`.
fn resolve(dir: &Path, name: &str) -> Result<PathBuf> {
    // Reject the obvious before touching the filesystem, so an attempt never
    // becomes a stat call on a path outside the folder.
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || Path::new(name).is_absolute()
    {
        return Err(AppError::invalid("That isn't a wallpaper in this folder."));
    }
    let root = dir
        .canonicalize()
        .map_err(|_| AppError::invalid("That wallpaper folder isn't there any more."))?;
    let path = root
        .join(name)
        .canonicalize()
        .map_err(|_| AppError::invalid(format!("{name} isn't in the wallpaper folder.")))?;
    if !path.starts_with(&root) || !path.is_file() || !has_drawable_extension(&path) {
        return Err(AppError::invalid("That isn't a wallpaper in this folder."));
    }
    Ok(path)
}

fn has_drawable_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

/// "long-exposure_bridge-01.jpg" → "long exposure bridge 01".
fn label_for(name: &str) -> String {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let cleaned = stem.replace(['_', '-'], " ");
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        name.to_string()
    } else {
        trimmed
    }
}

/// Standard base64, written out rather than pulled in.
///
/// A dependency for forty lines that will never change is a dependency to
/// audit, update and ship for the life of the product; this is the one place
/// in the codebase that needs it.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ensure_dir(tmp.path()).unwrap();
        (tmp, dir)
    }

    #[test]
    fn ensure_dir_is_idempotent_and_keeps_your_readme() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ensure_dir(tmp.path()).unwrap();
        assert!(dir.join("README.txt").exists());

        std::fs::write(dir.join("README.txt"), "my own notes").unwrap();
        let again = ensure_dir(tmp.path()).unwrap();
        assert_eq!(again, dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("README.txt")).unwrap(),
            "my own notes",
            "a second call must not overwrite what the user wrote there"
        );
    }

    #[test]
    fn lists_only_drawable_files_and_names_them_readably() {
        let (_tmp, dir) = fixture();
        std::fs::write(dir.join("long-exposure_bridge-01.jpg"), b"x").unwrap();
        std::fs::write(dir.join("Sky.PNG"), b"x").unwrap();
        // Not drawable, and not listed: a wallpaper that silently fails to
        // paint is worse than one that was never offered.
        std::fs::write(dir.join("holiday.heic"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.join("empty.png"), b"").unwrap();
        std::fs::create_dir(dir.join("album.jpg")).unwrap();

        let found = list(&dir).unwrap();
        let names: Vec<&str> = found.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["long-exposure_bridge-01.jpg", "Sky.PNG"]);
        assert_eq!(found[0].label, "long exposure bridge 01");
    }

    #[test]
    fn a_missing_folder_is_empty_rather_than_an_error() {
        // The folder is a path the user can point at a drive that is not
        // plugged in. Focus mode should open on its gradients, not on a banner.
        assert!(list(Path::new("/definitely/not/here")).unwrap().is_empty());
    }

    #[test]
    fn reads_a_wallpaper_as_a_data_uri() {
        let (_tmp, dir) = fixture();
        // A one-pixel PNG, so the encoding is checked against real bytes.
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        std::fs::write(dir.join("one.png"), png).unwrap();
        let uri = read(&dir, "one.png").unwrap();
        // 12 bytes is exactly four groups of three, so there is no padding.
        assert_eq!(uri, "data:image/png;base64,iVBORw0KGgoAAAAN");
    }

    #[test]
    fn base64_matches_the_rfc_including_both_padding_cases() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // The high bytes exercise the last two alphabet entries.
        assert_eq!(base64(&[0xFB, 0xFF, 0xFE]), "+//+");
    }

    /// The one that matters. `read` must not be an arbitrary-file-read
    /// primitive wearing a wallpaper costume.
    #[test]
    fn read_refuses_anything_outside_the_folder() {
        let (tmp, dir) = fixture();
        let secret = tmp.path().join("secret.png");
        std::fs::write(&secret, b"not yours").unwrap();
        std::fs::write(dir.join("ok.png"), b"fine").unwrap();

        for attempt in [
            "../secret.png",
            "..\\secret.png",
            "sub/../../secret.png",
            "/etc/passwd",
            "",
            "notes.txt",
        ] {
            assert!(
                read(&dir, attempt).is_err(),
                "{attempt:?} should have been refused"
            );
        }

        // …and a symlink inside the folder pointing out of it is refused too,
        // which is the case a string check alone would miss.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, dir.join("escape.png")).unwrap();
            assert!(read(&dir, "escape.png").is_err());
        }

        // The legitimate case still works, or the check is just a wall.
        assert!(read(&dir, "ok.png").is_ok());
    }
}
