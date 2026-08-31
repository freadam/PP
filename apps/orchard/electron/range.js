/**
 * Fetching a range from ActivityWatch.
 *
 * Split out of `main.js` so it can be tested without Electron. `main.js`
 * requires `electron` at its first line, which means anything living there is
 * reachable only from inside a running app — and this is the function most
 * worth holding to a real server's behaviour, because it decides which buckets
 * are read and over what window.
 */

'use strict';

const aw = require('./aw');
const engine = require('../src/engine');

/**
 * Fetch every bucket a range needs, in one round of requests.
 *
 * The window is widened by a day at each end before asking, then the engine
 * clips. An event that began at 23:50 yesterday and ran into today belongs to
 * today for the part that fell inside it, and a query starting exactly at
 * midnight would not return that event at all — the day would open with a
 * missing half hour that looks like a late start.
 */
async function readRange(settings, fromDate, toDate, client) {
  const io = client || aw;
  const s = engine.withDefaults(settings);
  const buckets = await io.getBuckets(s.serverUrl);
  const chosen = engine.chooseBuckets(buckets, s.hostname);

  if (!chosen.window) {
    throw new aw.AwError(
      'ActivityWatch is running but has no window-watcher bucket, so there is nothing to show. ' +
        'aw-watcher-window is what records which application is in front; it normally starts with ActivityWatch itself.',
      'no-bucket',
    );
  }

  const startIso = new Date(engine.dayStart(engine.shiftDate(fromDate, -1))).toISOString();
  const endIso = new Date(engine.dayEnd(engine.shiftDate(toDate, 1))).toISOString();
  const fetch = (bucket) => (bucket ? io.getEvents(s.serverUrl, bucket.id, startIso, endIso) : Promise.resolve([]));

  const [windowEvents, afkEvents, webEventLists] = await Promise.all([
    fetch(chosen.window),
    fetch(chosen.afk),
    // One unreadable browser bucket must not take the whole day down with it —
    // a stale bucket from an uninstalled browser is common and harmless.
    Promise.all(chosen.web.map((b) => fetch(b).catch(() => []))),
  ]);

  return {
    buckets: {
      window: chosen.window.id,
      afk: chosen.afk ? chosen.afk.id : null,
      web: chosen.web.map((b) => b.id),
      hosts: chosen.hosts,
    },
    window: windowEvents,
    afk: afkEvents,
    web: webEventLists.flat(),
  };
}

module.exports = { readRange };
