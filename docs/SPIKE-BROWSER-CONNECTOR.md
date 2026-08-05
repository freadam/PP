# Spike: the browser connector

**Question.** Can Fruit learn which website the frontmost tab is on, locally, on
Windows, without a full URL ever leaving the browser — and without opening a
listening port?

**Answer.** Yes. The design is settled and the risky parts are built and tested.
Two things remain unproven, both named below, and both need a Windows machine
with Chrome on it rather than more code.

This spike closes the last **Phase 1** item in `ROADMAP.md`, which the plan
scheduled for week 2 and which four wireframe screens have been waiting on.

---

## Why it exists at all

The product's primary outcome is *reduce unplanned PC entertainment*. Fruit's
existing observation is application-level, and at that level YouTube and a code
review are both `chrome.exe`. Every measure in the plan that mentions
entertainment — the 95% YouTube/Twitch figure, the dashboard's trend line, the
Day view's "Work + distraction" — is unreachable without something that sees the
domain. It has been the single largest technical risk in the plan since revision
1 and it was still unspiked.

## The three decisions

### 1. Transport — native messaging, not a localhost socket

The obvious shortcut is an extension `POST`ing to `127.0.0.1:PORT`. It was
rejected, and not on taste: **a listening socket is reachable by every other
process on the machine**, and with one careless CORS header by any web page. An
app whose top-bar badge says OFFLINE cannot open one.

Chrome's native messaging has no socket. The browser spawns a process and talks
to it over that process's own stdin/stdout, with the host declared in a registry
key only an installer can write. Nothing listens, nothing is routable, and the OS
process boundary is the access control.

The framing is Chrome's and it is the part most likely to be got wrong: a 4-byte
**native-endian** length prefix, then UTF-8 JSON, both directions, 1 MB maximum.

### 2. What crosses the boundary — a registrable domain, twice

`connector/background.js` reduces a URL to its registrable domain *before*
sending anything. `Sample::normalise` does it **again** on the way in.

That is not redundancy for its own sake. The extension is the part a user can
swap; the Rust side is not. The promise printed in Settings is enforced where the
write happens, exactly as the application exclusions already are.

`registrable_domain` is deliberately **not** a full Public Suffix List. The PSL
is ~10,000 rules that change monthly, and shipping a stale copy inside an offline
app means misgrouping domains and never finding out. The cost of the small list
is that `bbc.co.uk` and `itv.co.uk` might one day group oddly. The cost of the
PSL is a dependency that goes stale silently. For a classifier whose output the
user confirms anyway, that trade is easy.

### 3. The hand-off — an append-only file, not a pipe

The host process **cannot open the database**: two processes on one SQLite file
is the corruption path §7.3 already rules out, and the reason the app is
single-instance. So the host has to hand samples over somehow.

A named pipe is the textbook answer. It needs `CreateNamedPipeW`, a Windows
crate, `unsafe`, and a server thread — none of which can be compiled or tested on
the machine this was written on. Buying that on a spike, to save 20 seconds of
latency on a signal whose own sampling interval is 20 seconds, is a bad trade.

So: the host appends frames to `connector-spool.bin` in the app data directory,
and the app drains it on the activity tick it already runs. Access control is the
user profile's ACL — the same protection the database gets. It survives Fruit
being closed, which matters because Chrome runs the host whenever the *browser*
is open, and that is not the same thing.

Two guards, plus one bit of state:

| Mechanism | What it prevents |
|---|---|
| `MAX_SPOOL_BYTES` (1 MB) | A fortnight of Chrome against a closed Fruit filling the disk. Over it, the host stops appending and the app discards rather than parses. |
| `MAX_SAMPLE_AGE_MS` (6 h) | A stale "what is frontmost right now" landing in the middle of a day already reconciled. |
| `connector-enabled` sentinel | The host cannot read a setting. The app publishes one bit as a file's existence, so switching domains off stops data being **written**, not merely stops it being read. Switching off also deletes whatever was queued. |

---

## What is proven

Everything below runs under `cargo test`, on this machine, with no browser and no
webview. **168 tests pass**, of which 27 are new here.

| Claim | Where |
|---|---|
| A URL is reduced to a host; no path, query, fragment, credential or port survives | `connector::tests::a_url_is_reduced_to_a_host_and_never_stored_whole` |
| `chrome://`, `localhost`, bare IPs and non-websites are dropped, never stored under a fallback | `things_that_are_not_websites_are_dropped_rather_than_stored` |
| A frame split at **every single byte offset** still decodes exactly once | `frames_survive_being_split_at_every_byte` |
| Two frames in one read are both recovered | `two_frames_in_one_read_are_both_recovered` |
| An oversized length prefix is refused, not allocated | `an_oversized_length_prefix_is_refused_not_allocated` |
| A one-byte-at-a-time stream still yields whole messages, one reply each | `fruit-connector-host`: `a_stream_split_one_byte_at_a_time_still_yields_whole_messages` |
| A hostile length prefix ends the session rather than the machine | `a_hostile_length_prefix_ends_the_session_rather_than_the_machine` |
| Domains are not recorded until their **own** switch is on, and turning apps off turns them off too | `domains_are_not_recorded_until_their_own_switch_is_on`, `turning_apps_off_turns_domains_off_with_them` |
| An excluded domain is never written, subdomains included | `an_excluded_domain_is_never_written` |
| Two domains in one browser do not coalesce into one span | `two_domains_in_one_browser_do_not_merge_into_one_span` |
| Draining twice does not record the same twenty seconds twice | `draining_twice_does_not_deliver_the_same_sample_twice` |
| A host killed mid-write still delivers what it finished | `a_host_killed_mid_write_still_delivers_what_it_finished` |
| **A rule made while reconciling classifies forwards and never backwards** | `acceptance::a_rule_made_while_reconciling_classifies_forwards_and_never_backwards` |

That last one is the property worth the most. `activity_span.category` is stamped
at write time from the rules in force then, rather than joined on read, so a rule
added in September cannot rewrite what August said you were doing. Nothing in the
UI can demonstrate that; only a test can.

The host crate has **no Tauri dependency**, for the same reason `fruit-core` has
none: `src-tauri` needs a system webview to link, and the read loop is the single
most breakable piece of this feature. It had to live somewhere `cargo test` can
reach.

## What is assumed, not proven

Three things, and all three need a Windows box with Chrome, not more code.

1. **Chrome actually launches the app binary as a host, and stdio survives the
   `windows_subsystem = "windows"` attribute.** Native-host manifests have a
   `path` and no argument list, so the app detects a browser launch by the
   *origin* argument Chrome passes (`chrome-extension://…`), which is covered by
   a test. Whether a GUI-subsystem binary reliably gets Chrome's stdin/stdout
   handles is the piece that has to be observed. **If it does not, the fix is a
   second tiny binary** — `fruit-connector-host.exe`, which this crate already
   is — and the manifest points at that instead. The design does not change.
2. **The extension survives MV3 service-worker eviction in practice.** The
   schedule is built for it — `chrome.alarms` rather than `setInterval` — but
   "built for it" and "observed over a working day" are different claims.
3. **Edge behaves as Chrome does.** The manifest key differs
   (`Software\Microsoft\Edge\NativeMessagingHosts`) and the origin prefix is
   still `chrome-extension://`, but this is repeated from documentation, not
   seen.

## What was found along the way

Three things the design did not survive first contact with:

- **`chrome.alarms` clamps its period.** A 20-second alarm to match the app's
  sampling interval is not achievable in MV3. The extension is now event-driven —
  tab switch, window focus, completed navigation — with a one-minute alarm as the
  heartbeat. This is arguably better: events are what make the *boundary* of a
  visit accurate, which is the part "how long was I on it" actually depends on.
  The cost is that the tail of a visit is measured to within about forty seconds.
- **Re-stamping a queued sample collapses a tick's worth onto one instant.** The
  first version had `normalise` overwrite `at` with the host clock on both sides
  of the spool. Splitting it into `normalise` (host side, stamps — the extension
  is untrusted) and `accept` (app side, keeps the stamp, bounds its age) is the
  honest version: the host is Fruit's own code on the same machine.
- **An excluded domain must cost a row, not gain one.** In `record_activity` an
  excluded domain is *stripped* and the app record survives, exactly as a window
  title is. Through the connector the domain is the whole observation, so
  stripping it would leave a bare "Chrome was frontmost" span the connector had
  no business writing. Both checks exist and they do different jobs.

## What this unblocks

Four wireframe screens were waiting on this and are now live rather than dark:

- **Day** — "Work + distraction" and "Observed Entertainment" fire. The fixture
  demonstrates both, computed by real Rust rather than asserted.
- **Reconcile** — the evidence panel names the site rather than the browser, and
  the wireframe's *"apply my choice to future activity in this context"* is a
  working checkbox rather than a sentence explaining its absence.
- **Month dashboard** — entertainment is measurable, so the trend line and the
  YouTube/Twitch split in the findings have a source.
- **Settings** — the switch, the domain exclusion list, and an honest install
  panel.

## Recommendation

**Adopt the design and schedule one day on the client's Windows PC** to close the
three assumptions. That day is the whole remaining risk; everything reachable
without a browser is done.

Do **not** automate the install. Registering a native host means writing an
`HKCU` key that points a browser at an executable. An app that does that silently
on first run has installed a browser hook without asking, which is precisely the
behaviour the rest of the privacy contract exists to rule out. Settings shows the
two paths and the person puts the files there. If the client wants it automated
later, it belongs in the installer, with a checkbox, not in first-run.

### Install, for the record

1. Load `connector/` in `chrome://extensions` with Developer mode on. Note the
   extension id.
2. Copy `connector/app.fruit.connector.json`, set `path` to the installed
   `fruit.exe` and `allowed_origins` to `chrome-extension://<id>/`.
3. Point Chrome at it:
   `HKCU\Software\Google\Chrome\NativeMessagingHosts\app.fruit.connector`,
   default value = the manifest's full path. Edge uses
   `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\…`.
4. Settings → Activity → **Track websites**.

Nothing is recorded until step 4, whatever steps 1–3 say.
