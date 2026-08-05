//! The Chrome native-messaging host (Plan Rev 3 §5.4, spike).
//!
//! Its own crate, and for the same reason `fruit-core` is one: nothing in here
//! needs Tauri, and `src-tauri` cannot be compiled without a system webview. The
//! read loop below is the single most breakable piece of the connector, so it
//! had to land somewhere `cargo test` can reach.
//!
//! Chrome launches `fruit.exe` a second time, and that second process is not
//! the app. It is a short-lived host Chrome talks to over *that process's own*
//! stdin and stdout. No window, no database, no port. It reads frames,
//! validates them, and appends them to the spool the running app drains — see
//! `fruit_core::spool` for why the hand-off is a file rather than a socket or a
//! pipe, and [`invoked_as_host`] for how the two launches are told apart.
//!
//! Three properties this file is responsible for, and one it is not.
//!
//! **It never opens the database.** Two processes on one SQLite file is the
//! corruption path §7.3 already rules out, and it is why the app is
//! single-instance. The host's only disk access is the spool and the sentinel.
//!
//! **It never trusts the extension.** `Sample::normalise` re-reduces the domain
//! and re-stamps the time on every frame, whatever the extension sent. The
//! extension is the part a user can swap; this is not.
//!
//! **It exits quietly.** Chrome starts a host per extension and kills it when
//! the browser closes. Every failure here is "stop reading and return" — a host
//! that writes to stderr on a closed pipe is a host that spins.
//!
//! What it is *not* responsible for: deciding whether a domain may be recorded.
//! It cannot read a setting, so it reads the sentinel the app publishes, and
//! `spool::append` is what enforces it. The final word belongs to
//! `Store::record_browser_sample`, below the IPC boundary, where every other
//! privacy rule in this app is enforced.

use std::io::{Read, Write};
use std::path::Path;

use fruit_core::connector::{decode_frame, encode_frame, Frame, Reply, Sample, MAX_FRAME_BYTES};
use fruit_core::spool;
use fruit_core::time::now_ms;

/// A manual override, for testing the host without a browser.
///
/// It is *not* how Chrome starts us. A native-host manifest has a `path` and no
/// argument list — there is nowhere to put a flag. Chrome invokes the binary
/// with the calling extension's origin as the first argument (and, on Windows,
/// `--parent-window=<handle>` as the second), so that origin is what the
/// detection below actually keys on.
pub const HOST_FLAG: &str = "--connector-host";

/// The prefix Chrome and Edge both pass as argv[1] when launching a native host.
const ORIGIN_PREFIX: &str = "chrome-extension://";

/// The name Chrome looks the host up by. It is the filename of the manifest
/// under `NativeMessagingHosts`, and the string the extension passes to
/// `connectNative`. The three must agree exactly.
pub const HOST_MANIFEST_NAME: &str = "app.fruit.connector";

/// Must match `tauri.conf.json`'s `identifier`, because that is what Tauri
/// derives the app data directory from and the host has no `AppHandle` to ask.
pub const BUNDLE_IDENTIFIER: &str = "app.fruit.planner";

/// True when this process was started by a browser rather than by a person.
///
/// Getting this wrong in either direction is bad in a different way: a false
/// positive means someone double-clicks Fruit and gets no window, a false
/// negative means Chrome launches a second copy of the *app* against a database
/// the first copy already holds. Hence the narrow test — the origin argument,
/// which nothing but a browser passes.
pub fn invoked_as_host() -> bool {
    std::env::args()
        .skip(1)
        .any(|a| a == HOST_FLAG || a.starts_with(ORIGIN_PREFIX))
}

/// The same directory `data_dir(&AppHandle)` resolves to, computed without
/// Tauri because the host process never builds one.
///
/// Duplicating this resolution is a real risk — the two could drift and the
/// host would spool into a directory nothing drains, with no symptom except a
/// connector that records nothing. So `run()`'s setup compares this against the
/// `AppHandle`'s answer on every launch and logs `connector.datadir.mismatch`,
/// rather than leaving it to be discovered in the field.
pub fn host_data_dir() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join(BUNDLE_IDENTIFIER))
}

/// Reads frames until stdin closes. Returns the number of samples spooled.
///
/// The read loop keeps a buffer across reads rather than assuming one `read`
/// yields one message. A pipe splits wherever it likes, and the framing tests in
/// `fruit_core::connector` exist because that assumption is the classic way to
/// get native messaging subtly wrong.
pub fn run_host(data_dir: &Path) -> std::io::Result<usize> {
    pump(
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        data_dir,
    )
}

/// `run_host` with the pipes as parameters, so the read loop can be tested
/// against a buffer rather than against a browser.
///
/// The loop is the part of this file most likely to be wrong, and it is also
/// the part that cannot be exercised on the machine it was written on: this
/// crate needs a system webview to link. Splitting the I/O out is what lets it
/// be covered anyway.
pub fn pump(
    input: &mut impl Read,
    output: &mut impl Write,
    data_dir: &Path,
) -> std::io::Result<usize> {
    let stdin = input;
    let stdout = output;

    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    let mut spooled = 0usize;

    loop {
        let read = match stdin.read(&mut chunk) {
            Ok(0) => break, // Browser closed the pipe. Normal shutdown.
            Ok(n) => n,
            Err(_) => break,
        };
        buf.extend_from_slice(&chunk[..read]);

        // A buffer past the frame limit with no complete frame in it means the
        // sender is not speaking this protocol. Stop rather than grow.
        if buf.len() > MAX_FRAME_BYTES + 4 {
            break;
        }

        loop {
            match decode_frame::<Sample>(&buf) {
                Ok(Frame::Message(sample, used)) => {
                    buf.drain(..used);
                    let reply = match sample.normalise(now_ms()) {
                        Some(sample) => match spool::append(data_dir, &sample) {
                            Ok(true) => {
                                spooled += 1;
                                Reply::Ok { recorded: true }
                            }
                            // Not an error. The switch is off, or the app has
                            // been closed long enough for the spool to fill.
                            // The extension backs off; it does not retry.
                            Ok(false) => Reply::Ok { recorded: false },
                            Err(err) => Reply::Error {
                                message: err.to_string(),
                            },
                        },
                        // Unfocused, or not a website. Dropped on purpose, and
                        // reported as such so the extension can stop sending it.
                        None => Reply::Ok { recorded: false },
                    };
                    if write_reply(stdout, &reply).is_err() {
                        return Ok(spooled);
                    }
                }
                Ok(Frame::Incomplete) => break,
                // A frame we cannot parse leaves every later offset a guess.
                Err(_) => return Ok(spooled),
            }
        }
    }
    Ok(spooled)
}

fn write_reply(out: &mut impl Write, reply: &Reply) -> std::io::Result<()> {
    let frame = encode_frame(reply)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    out.write_all(&frame)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks_like_host(args: &[&str]) -> bool {
        args.iter()
            .any(|a| *a == HOST_FLAG || a.starts_with(ORIGIN_PREFIX))
    }

    /// The detection has to be exactly wide enough. Too wide and a normal
    /// double-click gets no window; too narrow and Chrome starts a second copy
    /// of the app against a database the first one holds open.
    #[test]
    fn only_a_browser_launch_looks_like_a_browser_launch() {
        assert!(looks_like_host(&[
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            "--parent-window=12345",
        ]));
        assert!(looks_like_host(&[HOST_FLAG]));

        assert!(!looks_like_host(&[]));
        assert!(!looks_like_host(&["--minimized"]));
        assert!(!looks_like_host(&["C:\\Users\\me\\a file.ics"]));
    }

    fn sample(domain: &str) -> Sample {
        Sample {
            domain: domain.into(),
            browser: "Chrome".into(),
            at: 0,
            focused: true,
        }
    }

    fn enabled_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        spool::set_spooling_permitted(dir.path(), true).unwrap();
        dir
    }

    /// A reader that hands back one byte at a time — the worst case a pipe can
    /// produce, and the one a naive "one read is one message" loop fails on.
    struct Dribble(Vec<u8>, usize);
    impl Read for Dribble {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.1 >= self.0.len() || out.is_empty() {
                return Ok(0);
            }
            out[0] = self.0[self.1];
            self.1 += 1;
            Ok(1)
        }
    }

    #[test]
    fn a_stream_split_one_byte_at_a_time_still_yields_whole_messages() {
        let dir = enabled_dir();
        let mut bytes = Vec::new();
        for d in ["youtube.com", "github.com", "twitch.tv"] {
            bytes.extend(encode_frame(&sample(d)).unwrap());
        }

        let mut out = Vec::new();
        let spooled = pump(&mut Dribble(bytes, 0), &mut out, dir.path()).unwrap();
        assert_eq!(spooled, 3);

        let drained = spool::drain(dir.path()).unwrap();
        assert_eq!(
            drained.iter().map(|s| s.domain.as_str()).collect::<Vec<_>>(),
            ["youtube.com", "github.com", "twitch.tv"]
        );
        // One reply per message, so the extension's back-off counter tracks
        // reality rather than drifting.
        let mut replies = 0;
        let mut at = 0;
        while let Ok(Frame::Message(reply, used)) = decode_frame::<Reply>(&out[at..]) {
            assert_eq!(reply, Reply::Ok { recorded: true });
            replies += 1;
            at += used;
            if at >= out.len() {
                break;
            }
        }
        assert_eq!(replies, 3);
    }

    /// Without the sentinel nothing may be written, and the host says so rather
    /// than accepting a message it silently drops.
    #[test]
    fn with_the_switch_off_the_host_accepts_nothing_and_admits_it() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = encode_frame(&sample("youtube.com")).unwrap();

        let mut out = Vec::new();
        assert_eq!(pump(&mut &bytes[..], &mut out, dir.path()).unwrap(), 0);
        assert!(!spool::spool_path(dir.path()).exists());

        let Ok(Frame::Message(reply, _)) = decode_frame::<Reply>(&out) else {
            panic!("the host said nothing at all");
        };
        assert_eq!(reply, Reply::Ok { recorded: false });
    }

    /// An unfocused tab and a `chrome://` page are both refused at the host, so
    /// they never reach the spool — the app is not the first line of defence.
    #[test]
    fn the_host_refuses_what_is_not_time_spent_on_a_website() {
        let dir = enabled_dir();
        let mut bytes = Vec::new();
        bytes.extend(
            encode_frame(&Sample {
                focused: false,
                ..sample("youtube.com")
            })
            .unwrap(),
        );
        bytes.extend(encode_frame(&sample("chrome://settings")).unwrap());
        bytes.extend(encode_frame(&sample("youtube.com")).unwrap());

        let mut out = Vec::new();
        assert_eq!(pump(&mut &bytes[..], &mut out, dir.path()).unwrap(), 1);
        let drained = spool::drain(dir.path()).unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].domain, "youtube.com");
    }

    /// A length prefix claiming more than the protocol allows must not become an
    /// allocation, and must not become an infinite loop either.
    #[test]
    fn a_hostile_length_prefix_ends_the_session_rather_than_the_machine() {
        let dir = enabled_dir();
        let mut bytes = encode_frame(&sample("youtube.com")).unwrap();
        bytes.extend_from_slice(&u32::MAX.to_ne_bytes());
        bytes.extend_from_slice(b"and then some");

        let mut out = Vec::new();
        // The good frame before it still counts; everything after is refused.
        assert_eq!(pump(&mut &bytes[..], &mut out, dir.path()).unwrap(), 1);
    }

    #[test]
    fn the_host_name_matches_what_the_extension_asks_for() {
        // `HOST_MANIFEST_NAME` is the manifest's filename, its `name` field, and
        // the string `connectNative` is called with. All three, or nothing works
        // and the failure is silent.
        let extension = include_str!("../../../connector/background.js");
        assert!(
            extension.contains(&format!("\"{HOST_MANIFEST_NAME}\"")),
            "connector/background.js does not call connectNative(\"{HOST_MANIFEST_NAME}\")"
        );
        let manifest = include_str!("../../../connector/app.fruit.connector.json");
        assert!(manifest.contains(&format!("\"name\": \"{HOST_MANIFEST_NAME}\"")));
    }
}
