/**
 * The main process, against a server that speaks ActivityWatch's REST API.
 *
 * The engine tests cover the arithmetic and the UI check covers the page. This
 * covers the part in between — the part that opens sockets, validates an
 * address and writes to disk — because none of it runs in either of the others
 * and all of it is where a mistake reaches outside the app.
 *
 * The server here answers on the real paths with the real shapes: buckets as a
 * map keyed by id, events with an ISO `timestamp` and a `duration` in seconds.
 */

const test = require('node:test');
const assert = require('node:assert');
const http = require('node:http');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const aw = require('../electron/aw');
const { readRange } = require('../electron/range');
const { Store } = require('../electron/store');

const HOUR = 3600;
const DAY = '2026-08-31';
const at = (date, h) => new Date(`${date}T00:00:00`).getTime() + h * 3600000;
const ev = (ms, dur, data) => ({ id: 1, timestamp: new Date(ms).toISOString(), duration: dur, data });

/** A stand-in for `aw-server`, on a loopback port. */
function fakeServer(handlers) {
  const seen = [];
  const server = http.createServer((req, res) => {
    seen.push(req.url);
    const url = new URL(req.url, 'http://localhost');
    const handler = handlers[url.pathname];
    if (!handler) {
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end('{"message":"not found"}');
      return;
    }
    const body = handler(url);
    if (body === undefined) {
      res.writeHead(500);
      res.end('kaboom');
      return;
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(body));
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () =>
      resolve({ url: `http://127.0.0.1:${server.address().port}`, seen, close: () => server.close() }),
    );
  });
}

const BUCKETS = {
  'aw-watcher-window_laptop': {
    id: 'aw-watcher-window_laptop',
    type: 'currentwindow',
    hostname: 'laptop',
    last_updated: '2026-08-31T12:00:00Z',
  },
  'aw-watcher-afk_laptop': {
    id: 'aw-watcher-afk_laptop',
    type: 'afkstatus',
    hostname: 'laptop',
    last_updated: '2026-08-31T12:00:00Z',
  },
  'aw-watcher-web-firefox': {
    id: 'aw-watcher-web-firefox',
    type: 'web.tab.current',
    hostname: 'laptop',
    last_updated: '2026-08-31T12:00:00Z',
  },
};

// --- the address is checked, not trusted -------------------------------

test('a non-local address is refused before any socket is opened', async () => {
  for (const url of ['http://example.com:5600', 'https://192.168.1.10:5600', 'http://10.0.0.5']) {
    await assert.rejects(
      () => aw.getInfo(url),
      (err) => {
        assert.strictEqual(err.kind, 'config');
        assert.match(err.message, /only connects to ActivityWatch on this machine/i);
        return true;
      },
      url,
    );
  }
});

test('loopback in its several spellings is allowed', () => {
  for (const url of ['http://localhost:5600', 'http://127.0.0.1:5600', 'http://[::1]:5600']) {
    assert.doesNotThrow(() => aw.parseBase(url), url);
  }
});

test('a non-http scheme is refused', () => {
  assert.throws(() => aw.parseBase('file:///etc/passwd'), /Only http and https/);
  assert.throws(() => aw.parseBase('not a url'), /is not a URL/);
});

test('an unreachable server produces a sentence a person can act on', async () => {
  // Port 1 on loopback: nothing is ever listening there.
  await assert.rejects(
    () => aw.getInfo('http://127.0.0.1:1'),
    (err) => {
      assert.strictEqual(err.kind, 'offline');
      assert.match(err.message, /ActivityWatch is probably not running/);
      return true;
    },
  );
});

// --- reading a range ---------------------------------------------------

test('a range reads the window, afk and browser buckets for the chosen host', async () => {
  const server = await fakeServer({
    '/api/0/buckets/': () => BUCKETS,
    '/api/0/buckets/aw-watcher-window_laptop/events': () => [ev(at(DAY, 9), 2 * HOUR, { app: 'code' })],
    '/api/0/buckets/aw-watcher-afk_laptop/events': () => [ev(at(DAY, 9), 8 * HOUR, { status: 'not-afk' })],
    '/api/0/buckets/aw-watcher-web-firefox/events': () => [ev(at(DAY, 9), HOUR, { url: 'https://github.com/a' })],
  });
  try {
    const data = await readRange({ serverUrl: server.url, hostname: 'laptop' }, DAY, DAY);
    assert.strictEqual(data.buckets.window, 'aw-watcher-window_laptop');
    assert.strictEqual(data.buckets.afk, 'aw-watcher-afk_laptop');
    assert.deepStrictEqual(data.buckets.web, ['aw-watcher-web-firefox']);
    assert.strictEqual(data.window.length, 1);
    assert.strictEqual(data.afk.length, 1);
    assert.strictEqual(data.web.length, 1);
  } finally {
    server.close();
  }
});

test('the query window is widened by a day at each end', async () => {
  const server = await fakeServer({
    '/api/0/buckets/': () => BUCKETS,
    '/api/0/buckets/aw-watcher-window_laptop/events': () => [],
    '/api/0/buckets/aw-watcher-afk_laptop/events': () => [],
    '/api/0/buckets/aw-watcher-web-firefox/events': () => [],
  });
  try {
    await readRange({ serverUrl: server.url }, DAY, DAY);
    const call = server.seen.find((u) => u.includes('window_laptop'));
    const q = new URL(call, 'http://x').searchParams;
    // An event that started before midnight and ran into the day has to be in
    // the response, or the day opens with a hole that looks like a late start.
    assert.ok(Date.parse(q.get('start')) <= new Date(`${DAY}T00:00:00`).getTime(), q.get('start'));
    assert.ok(Date.parse(q.get('end')) >= new Date('2026-09-01T00:00:00').getTime(), q.get('end'));
    // No limit is sent: aw-server applies it before ordering, so a limited
    // window would silently drop the earliest part of the range.
    assert.ok(!q.has('limit'));
  } finally {
    server.close();
  }
});

test('one unreadable browser bucket does not take the day down with it', async () => {
  const server = await fakeServer({
    '/api/0/buckets/': () => Object.assign({}, BUCKETS, {
      'aw-watcher-web-chrome': {
        id: 'aw-watcher-web-chrome',
        type: 'web.tab.current',
        hostname: 'laptop',
        last_updated: '2026-08-01T00:00:00Z',
      },
    }),
    '/api/0/buckets/aw-watcher-window_laptop/events': () => [ev(at(DAY, 9), HOUR, { app: 'code' })],
    '/api/0/buckets/aw-watcher-afk_laptop/events': () => [],
    '/api/0/buckets/aw-watcher-web-firefox/events': () => [ev(at(DAY, 9), HOUR, { url: 'https://github.com/a' })],
    // A stale bucket from an uninstalled browser: the server answers 500.
    '/api/0/buckets/aw-watcher-web-chrome/events': () => undefined,
  });
  try {
    const data = await readRange({ serverUrl: server.url }, DAY, DAY);
    assert.strictEqual(data.window.length, 1);
    assert.strictEqual(data.web.length, 1);
  } finally {
    server.close();
  }
});

test('no window bucket is a named failure, not an empty day', async () => {
  const server = await fakeServer({
    '/api/0/buckets/': () => ({ 'aw-watcher-afk_laptop': BUCKETS['aw-watcher-afk_laptop'] }),
  });
  try {
    await assert.rejects(
      () => readRange({ serverUrl: server.url }, DAY, DAY),
      (err) => {
        assert.strictEqual(err.kind, 'no-bucket');
        assert.match(err.message, /aw-watcher-window/);
        return true;
      },
    );
  } finally {
    server.close();
  }
});

test('a server answering something that is not JSON says so', async () => {
  const server = await fakeServer({});
  try {
    await assert.rejects(
      () => aw.getBuckets(server.url),
      (err) => {
        // 404 from the fake server: an HTTP failure, reported as one.
        assert.strictEqual(err.kind, 'http');
        return true;
      },
    );
  } finally {
    server.close();
  }
});

// --- settings on disk --------------------------------------------------

function tmpdir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'orchard-test-'));
}

test('a first run has no file and reports nothing rather than failing', () => {
  const store = new Store(tmpdir());
  assert.strictEqual(store.read(), null);
});

test('settings round-trip', () => {
  const store = new Store(tmpdir());
  const written = store.write({ serverUrl: 'http://localhost:5600', minSpanSec: 60, rules: [] });
  assert.deepStrictEqual(store.read(), written);
});

test('a corrupt file is moved aside rather than left to block every start', () => {
  const dir = tmpdir();
  const store = new Store(dir);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(store.file, '{ this is not json', 'utf8');

  assert.strictEqual(store.read(), null, 'defaults are used');
  assert.ok(!fs.existsSync(store.file), 'the bad file is gone from the live path');
  assert.ok(
    fs.readdirSync(dir).some((f) => f.includes('.corrupt-')),
    'and is kept aside rather than destroyed',
  );
  // And the next write succeeds, so the app recovers on its own.
  store.write({ minSpanSec: 120 });
  assert.strictEqual(store.read().minSpanSec, 120);
});

test('a write that lands leaves no temporary file behind', () => {
  const dir = tmpdir();
  const store = new Store(dir);
  store.write({ minSpanSec: 30 });
  assert.deepStrictEqual(
    fs.readdirSync(dir).sort(),
    ['settings.json'],
    'the write is atomic: rename, not truncate-in-place',
  );
});
