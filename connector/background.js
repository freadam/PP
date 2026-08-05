/**
 * Fruit browser connector — MV3 service worker.
 *
 * The whole extension is this file, and it is short on purpose: everything it
 * could plausibly do beyond "which site is in the active tab of the focused
 * window" is something the privacy note in Settings would then have to explain.
 *
 * ## What leaves the browser
 *
 * One object, every SAMPLE_MS, and only while a window is focused:
 *
 *     { domain: "youtube.com", browser: "Chrome", at: 1700000000000, focused: true }
 *
 * `domain` is reduced here, before anything is sent — see `registrableDomain`.
 * The path, the query, the fragment and the page title never enter the message.
 * The Rust side reduces it *again* on the way in (`Sample::normalise`), because
 * an extension is a thing a user can swap and the promise has to be kept
 * somewhere they cannot.
 *
 * ## Permissions, and the one that is deliberately absent
 *
 * `tabs` gives `tab.url` for the active tab. `idle` is how a locked screen stops
 * counting. `nativeMessaging` is the transport.
 *
 * There is **no `host_permissions`**, which is the important one: without it
 * this extension cannot inject a content script, read a page's DOM, or see a
 * request. It can learn that a tab is on `youtube.com` and it has no mechanism
 * to learn anything about what is on the page. The manifest is the argument.
 *
 * ## Failure is quiet
 *
 * Fruit not running, the host not installed, the switch off — all the same
 * response: stop, and try again on the next alarm. An extension that retries
 * hard against a missing host is one that shows up in someone's battery report.
 */

/**
 * The heartbeat, in minutes.
 *
 * It is not the app's 20-second sampling interval, and it cannot be: MV3 evicts
 * an idle service worker, `setInterval` dies with it, and `chrome.alarms` — the
 * only thing that survives — clamps its period to a floor. One minute is the
 * value that is honoured everywhere rather than the value that reads best.
 *
 * The cost is the tail of a visit: the app treats a sample as twenty seconds of
 * evidence, so a stretch of watching is measured to within about forty seconds
 * at its end. That is well inside what a half-hour grid can express, and the
 * *change* events below are what keep the boundaries themselves sharp — a heavy
 * heartbeat would only make the middle of a visit more granular, which is the
 * part nobody is asking about.
 */
const HEARTBEAT_MINUTES = 1;
const HOST = "app.fruit.connector";

/** Back off after this many consecutive refusals, so a browser left open
 *  against a closed app is not spawning a host process every 20 seconds. */
const MAX_REFUSALS = 5;

let port = null;
let refusals = 0;

/**
 * The same reduction the Rust side performs, kept deliberately identical in
 * shape: strip to the host, drop anything that is not a website, keep the last
 * two labels (three under a known two-part suffix).
 *
 * Being the second of two implementations is the point. This one exists so a
 * full URL never crosses the process boundary at all; the Rust one exists so it
 * would not matter if this were replaced.
 */
const MULTI_PART = new Set([
  "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "net.uk", "sch.uk",
  "com.au", "net.au", "org.au", "edu.au", "gov.au", "co.nz", "org.nz",
  "co.za", "co.jp", "or.jp", "ne.jp", "com.br", "com.mx", "com.tr",
  "com.cn", "com.sg", "com.hk", "co.in", "co.kr", "co.il",
]);

function registrableDomain(url) {
  let host;
  try {
    const parsed = new URL(url);
    // chrome://, about:, file:, devtools:, extension pages — not websites, and
    // `chrome://settings` is a more sensitive thing to log than most websites.
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
    host = parsed.hostname.toLowerCase();
  } catch {
    return null;
  }
  if (!host || host === "localhost") return null;
  // A bare address is a device on the network, not a site worth classifying.
  if (/^[\d.]+$/.test(host) || host.includes(":")) return null;

  const labels = host.split(".").filter(Boolean);
  if (labels.length < 2) return null;
  const lastTwo = labels.slice(-2).join(".");
  const keep = MULTI_PART.has(lastTwo) && labels.length >= 3 ? 3 : 2;
  return labels.slice(-keep).join(".");
}

function browserName() {
  // No user-agent parsing: the field exists to fill one line in the evidence
  // panel, and a guess is better than a fingerprint.
  return navigator.userAgent.includes("Edg/") ? "Edge" : "Chrome";
}

function connect() {
  if (port) return port;
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    // Host not installed. Nothing to do, and nothing to say about it.
    port = null;
    return null;
  }
  port.onDisconnect.addListener(() => {
    port = null;
  });
  port.onMessage.addListener((reply) => {
    // `recorded: false` is a legitimate answer — the switch is off, or the
    // domain is excluded. Back off rather than treating it as a retry.
    if (reply && reply.kind === "ok" && reply.recorded) refusals = 0;
    else refusals += 1;
  });
  return port;
}

async function sample() {
  if (refusals >= MAX_REFUSALS) {
    // One probe per backoff window instead of one per tick.
    refusals = MAX_REFUSALS - 1;
    if (port) {
      port.disconnect();
      port = null;
    }
  }

  // A locked or idle machine is not time spent on a website; the app has its
  // own idle detection, and two sources disagreeing about it is worse than one.
  const state = await chrome.idle.queryState(60);
  if (state !== "active") return;

  // `lastFocusedWindow` plus `focused` is what makes this "the frontmost tab"
  // rather than "a tab that exists". A background window is not time you spent.
  const win = await chrome.windows.getLastFocused({ populate: false });
  if (!win || !win.focused) return;

  const [tab] = await chrome.tabs.query({ active: true, windowId: win.id });
  if (!tab || !tab.url) return;

  const domain = registrableDomain(tab.url);
  if (!domain) return;

  const p = connect();
  if (!p) return;
  try {
    p.postMessage({
      domain,
      browser: browserName(),
      at: Date.now(),
      focused: true,
    });
  } catch {
    port = null;
  }
}

/* Two schedules, doing two different jobs.
 *
 * The **events** are what make a boundary accurate: switching tabs, following a
 * link, or alt-tabbing to the browser samples immediately, so the moment you
 * moved from the code review to YouTube is recorded as that moment rather than
 * rounded to the next tick. This is the half that matters for the product's
 * primary outcome, because "how long was I on it" is a question about where the
 * edges fall.
 *
 * The **alarm** is what covers the case with no events at all: a video playing
 * for forty minutes while nothing is clicked. It also revives an evicted service
 * worker, which a `setInterval` cannot.
 */
chrome.alarms.create("fruit-heartbeat", { periodInMinutes: HEARTBEAT_MINUTES });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "fruit-heartbeat") void sample();
});

chrome.tabs.onActivated.addListener(() => void sample());
chrome.windows.onFocusChanged.addListener((id) => {
  // Leaving the browser entirely is not something to record — the app's own
  // foreground sampler owns whatever you switched to.
  if (id !== chrome.windows.WINDOW_ID_NONE) void sample();
});
chrome.tabs.onUpdated.addListener((_id, change, tab) => {
  // Only a completed navigation in the active tab. A background tab finishing a
  // redirect is not time you spent.
  if (change.status === "complete" && tab.active) void sample();
});
chrome.idle.onStateChanged.addListener((state) => {
  if (state === "active") void sample();
});
