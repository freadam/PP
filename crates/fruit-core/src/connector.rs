//! The browser connector protocol (Plan Rev 3 §5.4, spike).
//!
//! The product's primary outcome is *reduce unplanned PC entertainment*, and
//! application-level observation cannot serve it: YouTube and a code review are
//! both `chrome.exe`. Something has to see the domain.
//!
//! Three things had to be settled before writing any of it, and this module is
//! where two of them live.
//!
//! # 1. Transport: native messaging, not a localhost socket
//!
//! The obvious shortcut is an extension POSTing to `127.0.0.1:PORT`. It is
//! wrong here for a reason bigger than taste: a listening socket is reachable by
//! every other process on the machine and, with a careless CORS header, by any
//! web page. An app whose top-bar badge says OFFLINE cannot open one.
//!
//! Chrome's **native messaging** has no socket at all. The browser spawns a
//! process and talks to it over that process's own stdin/stdout, with the host
//! declared in a registry key only an installer can write. Nothing is listening,
//! nothing is routable, and the OS process boundary is the access control.
//!
//! The framing is Chrome's, and it is the part most likely to be got wrong:
//! **4-byte native-endian length prefix, then UTF-8 JSON**, both directions,
//! 1 MB maximum. `decode_frame` / `encode_frame` below are that, with the
//! partial-read and oversize cases handled rather than assumed away.
//!
//! # 2. What crosses the boundary: a registrable domain, and nothing else
//!
//! The extension reduces a URL to its registrable domain *before* it sends
//! anything — see `connector/background.js`. This module does it **again** on
//! the way in. That is not redundant: the extension is the part a user could
//! swap, and the promise printed in Settings is enforced where the write
//! happens, exactly as the app-level exclusions are.
//!
//! A full URL, a query string or a page title reaching `Sample::normalise` is
//! reduced to a hostname or dropped. There is no code path that stores one.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Chrome refuses anything larger, so accepting more is a way to be handed a
/// gigabyte by a compromised extension.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// What the extension sends. Deliberately tiny: every field here is one the
/// privacy note in Settings has to be able to describe in a sentence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Registrable domain — `youtube.com`, never a path or a query.
    pub domain: String,
    /// The browser that saw it, for the evidence panel's Source line.
    pub browser: String,
    /// Milliseconds, UTC. The extension's clock; the host re-stamps it, because
    /// a browser's idea of "now" is not a thing to trust with the record.
    pub at: i64,
    /// False when the browser window is not the frontmost window. Sent so the
    /// host can drop it — a background tab is not time you spent.
    #[serde(default)]
    pub focused: bool,
}

/// What the host sends back. Only ever a receipt or a refusal — the app never
/// asks the browser for anything, which is what keeps the connector one-way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Reply {
    Ok { recorded: bool },
    Error { message: String },
}

/// How old a queued sample may be and still be worth recording.
///
/// The spool exists because Chrome runs the host whenever the browser is open,
/// which is not the same as whenever Fruit is open. A sample that waited six
/// hours is not evidence of anything — "what is frontmost" is a claim about a
/// moment, and a stale one would land in the middle of a day whose account the
/// user has already reconciled.
pub const MAX_SAMPLE_AGE_MS: i64 = 6 * 3600 * 1000;

/// A little slack for a host and an app disagreeing about the millisecond.
const CLOCK_SKEW_MS: i64 = 60_000;

impl Sample {
    /// Validates the parts that are claims about the world, leaving `at` alone.
    fn validated(mut self) -> Option<Self> {
        if !self.focused {
            return None;
        }
        self.domain = registrable_domain(&self.domain)?;
        self.browser = self
            .browser
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
            .take(32)
            .collect();
        if self.browser.is_empty() {
            self.browser = "Browser".into();
        }
        Some(self)
    }

    /// **Host side.** Enforces the contract on the way in from the extension,
    /// and stamps the arrival time.
    ///
    /// Returns `None` for anything that should not become a record at all: an
    /// unfocused tab, a browser-internal page, a bare IP, or a host that does
    /// not parse. Dropping is always preferable to storing a fallback — a row
    /// nobody can explain is worse than a missing row.
    ///
    /// The timestamp is overwritten deliberately. An extension is a thing a user
    /// can swap, and one that lies about the time would otherwise write samples
    /// into yesterday, past the reconciler, into a day already closed.
    pub fn normalise(self, now: i64) -> Option<Self> {
        let mut sample = self.validated()?;
        sample.at = now;
        Some(sample)
    }

    /// **App side.** Re-validates a sample that has been sitting in the spool,
    /// and decides whether its age still permits recording it.
    ///
    /// Unlike `normalise` this *keeps* `at`. The stamp came from the host — the
    /// app's own code, on the same machine, on the same clock — so re-stamping
    /// here would take a queue of samples spread across twenty seconds and
    /// collapse them all onto the instant the app happened to drain them.
    /// Trusting the host and bounding the age is the honest version.
    pub fn accept(self, now: i64) -> Option<Self> {
        let sample = self.validated()?;
        // Future stamps are a clock change or a corrupt frame, not an
        // observation. Neither is something to write into the record.
        if sample.at > now + CLOCK_SKEW_MS {
            return None;
        }
        if now - sample.at > MAX_SAMPLE_AGE_MS {
            return None;
        }
        Some(sample)
    }
}

/// Reduces anything URL-shaped to its registrable domain.
///
/// **Deliberately not a full Public Suffix List.** The PSL is ~10,000 rules that
/// change monthly; shipping a stale copy in an offline app means quietly
/// misgrouping domains and never finding out. The list below covers the
/// two-label suffixes an English-speaking user actually meets, and everything
/// else falls back to the last two labels — which is right for `youtube.com`,
/// `twitch.tv` and the overwhelming majority of what matters here.
///
/// The cost of being wrong is a rule that groups `bbc.co.uk` and `itv.co.uk`
/// together. The cost of the PSL is a dependency that goes stale silently. For
/// a classifier whose output the user confirms anyway, that trade is easy.
pub fn registrable_domain(input: &str) -> Option<String> {
    const MULTI_PART: &[&str] = &[
        "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "net.uk", "sch.uk", "com.au", "net.au",
        "org.au", "edu.au", "gov.au", "co.nz", "org.nz", "co.za", "co.jp", "or.jp", "ne.jp",
        "com.br", "com.mx", "com.tr", "com.cn", "com.sg", "com.hk", "co.in", "co.kr", "co.il",
    ];

    let mut host = input.trim().to_ascii_lowercase();

    // Tolerate a full URL even though the extension should never send one.
    if let Some(rest) = host.split("://").nth(1) {
        host = rest.to_string();
    }
    host = host
        .split('/')
        .next()?
        .split('?')
        .next()?
        .split('#')
        .next()?
        .to_string();
    // Strip credentials and a port.
    if let Some(at) = host.rfind('@') {
        host = host[at + 1..].to_string();
    }
    if let Some(colon) = host.rfind(':') {
        // Not an IPv6 literal, which we reject below anyway.
        if !host.contains('[') {
            host = host[..colon].to_string();
        }
    }
    host = host.trim_matches('.').to_string();

    if host.is_empty() || host.len() > 253 {
        return None;
    }
    // Browser-internal pages, extensions, local files and bare addresses are not
    // websites and must never become a record.
    if host.contains(' ')
        || host.contains('[')
        || host == "localhost"
        || host.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return None;
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }

    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() < 2 {
        return None;
    }
    let last_two = labels[labels.len() - 2..].join(".");
    let keep = if MULTI_PART.contains(&last_two.as_str()) && labels.len() >= 3 {
        3
    } else {
        2
    };
    if labels.len() < keep {
        return None;
    }
    Some(labels[labels.len() - keep..].join("."))
}

/// Chrome's native-messaging framing: a 4-byte native-endian length, then JSON.
pub fn encode_frame(value: &impl Serialize) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(AppError::invalid("That message is too large to send."));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_ne_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// What a frame read produced.
#[derive(Debug, PartialEq)]
pub enum Frame<T> {
    /// A complete message, and how many bytes it consumed.
    Message(T, usize),
    /// Not enough bytes yet. The caller reads more and tries again — the case
    /// a naive `read()` loop gets wrong, because a pipe splits wherever it likes.
    Incomplete,
}

pub fn decode_frame<T: for<'de> Deserialize<'de>>(buf: &[u8]) -> Result<Frame<T>> {
    if buf.len() < 4 {
        return Ok(Frame::Incomplete);
    }
    let len = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_FRAME_BYTES {
        // Refusing beats reading: a bad length prefix is how a 4-byte header
        // turns into an allocation the size of the machine.
        return Err(AppError::invalid(
            "The browser connector sent an oversized message. Ignoring it.",
        ));
    }
    if buf.len() < 4 + len {
        return Ok(Frame::Incomplete);
    }
    let value = serde_json::from_slice(&buf[4..4 + len])?;
    Ok(Frame::Message(value, 4 + len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domains_survive_the_shapes_a_browser_produces() {
        for (input, expected) in [
            ("youtube.com", "youtube.com"),
            ("www.youtube.com", "youtube.com"),
            ("m.youtube.com", "youtube.com"),
            ("music.youtube.com", "youtube.com"),
            ("YouTube.COM", "youtube.com"),
            ("youtu.be", "youtu.be"),
            ("twitch.tv", "twitch.tv"),
            ("www.twitch.tv", "twitch.tv"),
            ("news.bbc.co.uk", "bbc.co.uk"),
            ("bbc.co.uk", "bbc.co.uk"),
            ("docs.google.com", "google.com"),
            ("a.b.c.example.org", "example.org"),
            ("example.com.", "example.com"),
            ("example.com:8443", "example.com"),
        ] {
            assert_eq!(
                registrable_domain(input).as_deref(),
                Some(expected),
                "for {input}"
            );
        }
    }

    /// A full URL should never arrive — but if one does, nothing beyond the host
    /// may survive. This is the assertion the privacy note in Settings rests on.
    #[test]
    fn a_url_is_reduced_to_a_host_and_never_stored_whole() {
        let url = "https://user:pw@www.youtube.com:443/watch?v=SECRET&t=90#comments";
        assert_eq!(registrable_domain(url).as_deref(), Some("youtube.com"));

        let reduced = registrable_domain(url).unwrap();
        for leak in ["watch", "SECRET", "v=", "comments", "user", "pw", "443"] {
            assert!(
                !reduced.contains(leak),
                "'{leak}' survived into {reduced}"
            );
        }
    }

    #[test]
    fn things_that_are_not_websites_are_dropped_rather_than_stored() {
        for input in [
            "",
            "   ",
            "localhost",
            "127.0.0.1",
            "192.168.1.10",
            "chrome",
            "newtab",
            "[::1]",
            "file",
            "a b.com",
            "exam ple.com",
        ] {
            assert_eq!(registrable_domain(input), None, "for {input:?}");
        }
    }

    /// The host stamps; the app keeps that stamp and bounds its age. Re-stamping
    /// on the way out of the spool would collapse a tick's worth of samples onto
    /// one instant.
    #[test]
    fn a_queued_sample_keeps_its_stamp_until_it_is_too_old_to_mean_anything() {
        let now = 1_700_000_000_000;
        let queued = |at: i64| Sample {
            domain: "youtube.com".into(),
            browser: "Chrome".into(),
            at,
            focused: true,
        };

        let recent = queued(now - 20_000).accept(now).unwrap();
        assert_eq!(recent.at, now - 20_000, "the host's stamp was overwritten");

        assert_eq!(queued(now - MAX_SAMPLE_AGE_MS - 1).accept(now), None);
        // A sample from the future is a clock change, not an observation.
        assert_eq!(queued(now + 10 * 60_000).accept(now), None);
        // But a minute of skew between two processes is not a scandal.
        assert!(queued(now + 10_000).accept(now).is_some());
        // An unfocused tab is dropped on this path too, not only the host's.
        assert_eq!(
            Sample {
                focused: false,
                ..queued(now)
            }
            .accept(now),
            None
        );
    }

    #[test]
    fn an_unfocused_tab_is_not_time_you_spent() {
        let sample = Sample {
            domain: "youtube.com".into(),
            browser: "Chrome".into(),
            at: 1,
            focused: false,
        };
        assert_eq!(sample.normalise(1_000), None);
    }

    #[test]
    fn the_host_clock_wins_over_the_browsers() {
        let sample = Sample {
            domain: "www.twitch.tv".into(),
            // An extension claiming it is 1970 must not write into 1970.
            at: 0,
            browser: "Chrome/128 <script>".into(),
            focused: true,
        };
        let normalised = sample.normalise(1_700_000_000_000).unwrap();
        assert_eq!(normalised.at, 1_700_000_000_000);
        assert_eq!(normalised.domain, "twitch.tv");
        assert_eq!(
            normalised.browser, "Chrome128 script",
            "the browser name is sanitised, not trusted"
        );
    }

    /// The framing is the part most likely to be got wrong, because a pipe
    /// splits wherever it likes and a naive read loop assumes it doesn't.
    #[test]
    fn frames_survive_being_split_at_every_byte() {
        let sample = Sample {
            domain: "youtube.com".into(),
            browser: "Chrome".into(),
            at: 42,
            focused: true,
        };
        let frame = encode_frame(&sample).unwrap();

        for split in 0..frame.len() {
            let (head, tail) = frame.split_at(split);
            // Everything short of the whole frame must say "not yet".
            assert_eq!(
                decode_frame::<Sample>(head).unwrap(),
                Frame::Incomplete,
                "split at {split} decoded early"
            );
            let mut joined = head.to_vec();
            joined.extend_from_slice(tail);
            assert_eq!(
                decode_frame::<Sample>(&joined).unwrap(),
                Frame::Message(sample.clone(), frame.len())
            );
        }
    }

    #[test]
    fn two_frames_in_one_read_are_both_recovered() {
        let a = Sample {
            domain: "youtube.com".into(),
            browser: "Chrome".into(),
            at: 1,
            focused: true,
        };
        let b = Sample {
            domain: "twitch.tv".into(),
            browser: "Chrome".into(),
            at: 2,
            focused: true,
        };
        let mut buf = encode_frame(&a).unwrap();
        buf.extend(encode_frame(&b).unwrap());

        let Frame::Message(first, used) = decode_frame::<Sample>(&buf).unwrap() else {
            panic!("expected a message");
        };
        assert_eq!(first, a);
        let Frame::Message(second, _) = decode_frame::<Sample>(&buf[used..]).unwrap() else {
            panic!("expected a second message");
        };
        assert_eq!(second, b);
    }

    #[test]
    fn an_oversized_length_prefix_is_refused_not_allocated() {
        let mut buf = (MAX_FRAME_BYTES as u32 + 1).to_ne_bytes().to_vec();
        buf.extend_from_slice(b"{}");
        assert!(decode_frame::<Sample>(&buf).is_err());
    }
}
