//! The hand-off between the native-messaging host and the running app.
//!
//! Chrome spawns the host process; the host cannot write to the database,
//! because two processes on one SQLite file is the corruption path §7.3 already
//! rules out (it is why the app is single-instance). So the host has to hand
//! samples to the app somehow, and the choice of *how* is the third of the three
//! decisions the connector spike had to make.
//!
//! # Why an append-only file
//!
//! The two obvious answers are both worse here.
//!
//! - **A localhost socket.** Rejected in `crate::connector` for the connector
//!   proper and rejected again here for the same reason: a listening port is
//!   reachable by every process on the machine, and an app badged OFFLINE
//!   cannot open one.
//! - **A named pipe.** Correct, and it needs `CreateNamedPipeW`, which means a
//!   Windows crate, `unsafe`, and a server thread — none of which can be
//!   compiled or tested on the machine this was written on. Buying that on a
//!   spike, to save 20 seconds of latency on a signal whose own sampling
//!   interval is 20 seconds, is a bad trade.
//!
//! A file has none of that. The host appends frames; the app drains them on the
//! activity tick it already runs. Access control is the user profile's own ACL —
//! the same protection the database gets. It survives the app being closed,
//! which matters because Chrome starts the host whenever the browser is open and
//! that is not the same as whenever Fruit is open.
//!
//! # What is on disk, and for how long
//!
//! Frames, in `crate::connector`'s own framing, and nothing else — a registrable
//! domain, a browser name, a timestamp and a focus flag. The file is truncated
//! on every drain, so its steady state is at most one tick's worth.
//!
//! Two guards, because an unbounded file written by one process and read by
//! another is a disk-filling bug waiting to happen:
//!
//! - `MAX_SPOOL_BYTES` — the host stops appending past it, and a spool larger
//!   than it is discarded rather than parsed. Fruit closed for a fortnight while
//!   Chrome ran is exactly the case, and days-old "what is frontmost right now"
//!   samples are worthless anyway.
//! - The [`ENABLED_SENTINEL`] file. The host has no way to read a setting, so
//!   the app publishes one bit — "domains may be recorded" — as a file's
//!   existence. No sentinel, no spooling: turning the switch off stops data
//!   being *written*, not merely stops it being read.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::connector::{decode_frame, encode_frame, Frame, Sample};
use crate::error::Result;

/// Where the host appends and the app drains.
pub const SPOOL_FILE: &str = "connector-spool.bin";

/// Present only while domains may be recorded. The host checks it before
/// writing anything at all.
pub const ENABLED_SENTINEL: &str = "connector-enabled";

/// ~26,000 samples. Far more than a drain interval, far less than a problem.
pub const MAX_SPOOL_BYTES: u64 = 1024 * 1024;

pub fn spool_path(dir: &Path) -> PathBuf {
    dir.join(SPOOL_FILE)
}

pub fn sentinel_path(dir: &Path) -> PathBuf {
    dir.join(ENABLED_SENTINEL)
}

/// True when the app has published the "domains may be recorded" bit.
pub fn spooling_permitted(dir: &Path) -> bool {
    sentinel_path(dir).exists()
}

/// Publishes (or withdraws) that bit. Called whenever the setting changes and
/// on every launch, so a crash mid-session cannot leave the host writing after
/// the user has switched it off.
pub fn set_spooling_permitted(dir: &Path, permitted: bool) -> Result<()> {
    fs::create_dir_all(dir)?;
    let path = sentinel_path(dir);
    if permitted {
        if !path.exists() {
            File::create(&path)?;
        }
    } else if path.exists() {
        fs::remove_file(&path)?;
        // The switch going off makes what is already queued undrainable, so it
        // must also be undeliverable. Leaving it would mean a re-enable
        // flushed observations from a period the user had turned off.
        let _ = fs::remove_file(spool_path(dir));
    }
    Ok(())
}

/// Appends one sample. Called by the host process, never by the app.
///
/// Returns `false` when the sample was refused — the switch is off, or the
/// spool is full — so the host can reply honestly rather than silently dropping.
pub fn append(dir: &Path, sample: &Sample) -> Result<bool> {
    if !spooling_permitted(dir) {
        return Ok(false);
    }
    let path = spool_path(dir);
    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) >= MAX_SPOOL_BYTES {
        return Ok(false);
    }
    let frame = encode_frame(sample)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    // One `write_all` of one frame. Two processes appending to the same file is
    // not this design — Chrome runs one host per extension — but a single write
    // is what keeps a second one from interleaving mid-frame if that ever
    // changes.
    file.write_all(&frame)?;
    Ok(true)
}

/// Reads everything complete and truncates the file.
///
/// A trailing partial frame is dropped rather than kept: the alternative is
/// carrying a fragment across drains, and a fragment whose writer has since
/// exited never completes. One lost 20-second sample is not worth the state.
pub fn drain(dir: &Path) -> Result<Vec<Sample>> {
    let path = spool_path(dir);
    let Ok(meta) = fs::metadata(&path) else {
        return Ok(Vec::new());
    };
    if meta.len() == 0 {
        return Ok(Vec::new());
    }
    if meta.len() > MAX_SPOOL_BYTES {
        // Bigger than the host will ever write means the file is not one the
        // host wrote. Discard it; do not parse a megabyte of someone's guess.
        let _ = fs::remove_file(&path);
        return Ok(Vec::new());
    }

    let mut buf = Vec::with_capacity(meta.len() as usize);
    File::open(&path)?.read_to_end(&mut buf)?;
    // Truncate before returning, not after processing: a panic downstream
    // would otherwise replay the same samples on the next tick forever.
    let _ = fs::remove_file(&path);

    let mut out = Vec::new();
    let mut at = 0usize;
    while at < buf.len() {
        match decode_frame::<Sample>(&buf[at..]) {
            Ok(Frame::Message(sample, used)) => {
                out.push(sample);
                at += used;
            }
            // Incomplete: the tail is a partial write. Nothing after it can be
            // read either, so stop.
            Ok(Frame::Incomplete) => break,
            // A corrupt frame makes every offset after it a guess. Keeping what
            // parsed and stopping is the only honest option.
            Err(_) => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        set_spooling_permitted(dir.path(), true).unwrap();
        dir
    }

    #[test]
    fn a_round_trip_preserves_order() {
        let dir = enabled_dir();
        for d in ["youtube.com", "github.com", "twitch.tv"] {
            assert!(append(dir.path(), &sample(d)).unwrap());
        }
        let drained = drain(dir.path()).unwrap();
        assert_eq!(
            drained.iter().map(|s| s.domain.as_str()).collect::<Vec<_>>(),
            ["youtube.com", "github.com", "twitch.tv"]
        );
    }

    /// The property that makes the app's tick safe to run on a timer: draining
    /// twice must not record the same twenty seconds twice.
    #[test]
    fn draining_twice_does_not_deliver_the_same_sample_twice() {
        let dir = enabled_dir();
        append(dir.path(), &sample("youtube.com")).unwrap();
        assert_eq!(drain(dir.path()).unwrap().len(), 1);
        assert_eq!(drain(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn draining_nothing_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(drain(dir.path()).unwrap().is_empty());
    }

    /// The host writes whole frames, but it can be killed mid-write — Chrome
    /// terminates the host when the browser closes. Whatever landed before the
    /// cut must still be readable.
    #[test]
    fn a_host_killed_mid_write_still_delivers_what_it_finished() {
        let dir = enabled_dir();
        append(dir.path(), &sample("youtube.com")).unwrap();
        // A torn frame at the tail.
        let mut f = OpenOptions::new()
            .append(true)
            .open(spool_path(dir.path()))
            .unwrap();
        f.write_all(&encode_frame(&sample("twitch.tv")).unwrap()[..6])
            .unwrap();
        drop(f);

        let drained = drain(dir.path()).unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].domain, "youtube.com");
        // And the fragment is gone rather than replayed forever.
        assert!(drain(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn nothing_is_written_without_the_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!append(dir.path(), &sample("youtube.com")).unwrap());
        assert!(!spool_path(dir.path()).exists());
    }

    /// Switching domains off must not leave a queue that a later re-enable
    /// flushes — that would record a period the user had explicitly closed.
    #[test]
    fn switching_off_discards_what_was_already_queued() {
        let dir = enabled_dir();
        append(dir.path(), &sample("youtube.com")).unwrap();
        set_spooling_permitted(dir.path(), false).unwrap();
        assert!(!spool_path(dir.path()).exists());

        set_spooling_permitted(dir.path(), true).unwrap();
        assert!(drain(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn an_oversized_spool_is_discarded_rather_than_parsed() {
        let dir = enabled_dir();
        fs::write(
            spool_path(dir.path()),
            vec![0u8; MAX_SPOOL_BYTES as usize + 1],
        )
        .unwrap();
        assert!(drain(dir.path()).unwrap().is_empty());
        assert!(!spool_path(dir.path()).exists());
        // And the host refuses to make it worse in the meantime.
        fs::write(spool_path(dir.path()), vec![0u8; MAX_SPOOL_BYTES as usize]).unwrap();
        assert!(!append(dir.path(), &sample("youtube.com")).unwrap());
    }

    #[test]
    fn garbage_after_a_good_frame_costs_only_what_follows_it() {
        let dir = enabled_dir();
        append(dir.path(), &sample("youtube.com")).unwrap();
        let mut f = OpenOptions::new()
            .append(true)
            .open(spool_path(dir.path()))
            .unwrap();
        // A plausible length prefix over bytes that are not JSON.
        f.write_all(&4u32.to_ne_bytes()).unwrap();
        f.write_all(b"\xff\xff\xff\xff").unwrap();
        drop(f);

        let drained = drain(dir.path()).unwrap();
        assert_eq!(drained.len(), 1, "the good frame before it still arrives");
    }

    #[test]
    fn the_sentinel_is_idempotent_in_both_directions() {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            set_spooling_permitted(dir.path(), true).unwrap();
            assert!(spooling_permitted(dir.path()));
        }
        for _ in 0..2 {
            set_spooling_permitted(dir.path(), false).unwrap();
            assert!(!spooling_permitted(dir.path()));
        }
    }
}
