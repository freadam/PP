/**
 * The ActivityWatch client. Lives in the main process, which is the whole
 * point: the renderer never opens a socket, so a page that only ever draws
 * cannot be talked into fetching something else.
 *
 * Nothing here interprets the data. It fetches buckets and events and hands
 * them on; `src/engine.js` decides what they mean. Keeping the two apart is
 * what lets the arithmetic be tested without a server running.
 */

'use strict';

const http = require('node:http');
const https = require('node:https');
const { URL } = require('node:url');

/** Long enough for a cold `aw-server` on a slow disk, short enough to fail. */
const TIMEOUT_MS = 15000;

/**
 * A local ActivityWatch is the only supported target.
 *
 * `aw-server` has no authentication whatsoever — it assumes it is reachable
 * only from the machine it runs on. Pointing this app at someone else's host
 * would be reading a stranger's screen history over an unauthenticated
 * connection, so the address is checked rather than trusted, and a stray
 * setting cannot turn the app into a scanner.
 */
const LOCAL_HOSTS = new Set(['localhost', '127.0.0.1', '::1', '[::1]', '0.0.0.0']);

class AwError extends Error {
  constructor(message, kind) {
    super(message);
    this.name = 'AwError';
    this.kind = kind;
  }
}

function parseBase(serverUrl) {
  let url;
  try {
    url = new URL(String(serverUrl));
  } catch (_) {
    throw new AwError(`"${serverUrl}" is not a URL. It should look like http://localhost:5600.`, 'config');
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new AwError(`Only http and https are supported, not "${url.protocol}".`, 'config');
  }
  if (!LOCAL_HOSTS.has(url.hostname)) {
    throw new AwError(
      `Orchard only connects to ActivityWatch on this machine, and "${url.hostname}" is not local. ` +
        'aw-server has no authentication, so a remote address would read another machine\'s history over an open connection.',
      'config',
    );
  }
  return url;
}

/**
 * `async` so that a rejected address rejects rather than throwing where the
 * caller is holding a promise. A function that sometimes throws synchronously
 * and sometimes rejects is one a `.catch()` silently fails to cover — and
 * `range.js` guards its browser-bucket reads with exactly that.
 */
async function request(serverUrl, path) {
  const base = parseBase(serverUrl);
  const target = new URL(path, base);
  const lib = target.protocol === 'https:' ? https : http;

  return new Promise((resolve, reject) => {
    const req = lib.request(
      target,
      { method: 'GET', headers: { Accept: 'application/json' }, timeout: TIMEOUT_MS },
      (res) => {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => {
          const body = Buffer.concat(chunks).toString('utf8');
          if (res.statusCode < 200 || res.statusCode >= 300) {
            reject(new AwError(`ActivityWatch answered ${res.statusCode} for ${target.pathname}.`, 'http'));
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (_) {
            reject(new AwError(`ActivityWatch sent something that is not JSON for ${target.pathname}.`, 'parse'));
          }
        });
      },
    );
    req.on('timeout', () => {
      req.destroy();
      reject(new AwError(`ActivityWatch did not answer within ${TIMEOUT_MS / 1000}s at ${base.origin}.`, 'timeout'));
    });
    req.on('error', (err) => {
      // The message a user can act on, rather than the one Node produces. This
      // is the single most likely failure and "ECONNREFUSED" is not a sentence.
      if (err && (err.code === 'ECONNREFUSED' || err.code === 'ECONNRESET')) {
        reject(
          new AwError(
            `Nothing is listening at ${base.origin}. ActivityWatch is probably not running — start it, or change the address in Settings.`,
            'offline',
          ),
        );
        return;
      }
      reject(new AwError(`Could not reach ${base.origin}: ${(err && err.message) || err}`, 'network'));
    });
    req.end();
  });
}

/** `GET /api/0/info` — also the reachability check. */
const getInfo = (serverUrl) => request(serverUrl, '/api/0/info');

/** `GET /api/0/buckets/` — a map of bucket id to its metadata. */
const getBuckets = (serverUrl) => request(serverUrl, '/api/0/buckets/');

/**
 * `GET /api/0/buckets/{id}/events`.
 *
 * `start` and `end` are ISO-8601 instants. ActivityWatch returns events newest
 * first and applies `limit` before ordering, so no limit is sent: a truncated
 * window would silently drop the *earliest* part of the day, which looks like a
 * morning you did not work rather than like missing data.
 */
function getEvents(serverUrl, bucketId, startIso, endIso) {
  const q = new URLSearchParams({ start: startIso, end: endIso });
  return request(serverUrl, `/api/0/buckets/${encodeURIComponent(bucketId)}/events?${q}`);
}

module.exports = { AwError, getInfo, getBuckets, getEvents, parseBase };
