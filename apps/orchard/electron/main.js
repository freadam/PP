/**
 * The Electron shell: one window, and the four things the renderer is allowed
 * to ask for.
 *
 * # The security posture, stated once
 *
 * The renderer runs with `contextIsolation`, `sandbox`, and no Node integration
 * — it is a web page that can draw and nothing else. It has no network access
 * of its own: a Content-Security-Policy header denies it, and every outbound
 * request is made here, against an address this process validates. The bridge
 * in `preload.js` exposes four functions and no general-purpose escape hatch.
 *
 * That matters more than usual for this app. It renders window *titles* — the
 * documents, tickets and videos someone had open — which is among the most
 * sensitive data a desktop produces. A renderer that could both read that and
 * open a socket would be one bug away from exfiltrating it.
 */

'use strict';

const { app, BrowserWindow, ipcMain, shell, session } = require('electron');
const path = require('node:path');

const aw = require('./aw');
const { readRange } = require('./range');
const { Store } = require('./store');
const engine = require('../src/engine');

/** Kept module-level so the IPC handlers can reach it after `whenReady`. */
let store;

// --- IPC ---------------------------------------------------------------

/**
 * Every handler returns `{ok: true, …}` or `{ok: false, error, kind}` rather
 * than rejecting. A rejected IPC call reaches the renderer as an opaque
 * "Error invoking remote method", which would throw away the one thing the
 * user needs — the sentence saying what went wrong and what to do about it.
 */
function registerIpc() {
  ipcMain.handle('settings:read', () => {
    try {
      return { ok: true, settings: engine.withDefaults(store.read()) };
    } catch (err) {
      return { ok: false, error: String((err && err.message) || err), kind: 'disk' };
    }
  });

  ipcMain.handle('settings:write', (_e, settings) => {
    try {
      // Normalised before it is written, so a bad value from the renderer
      // cannot end up persisted as the thing every later read trusts.
      return { ok: true, settings: store.write(engine.withDefaults(settings)) };
    } catch (err) {
      return { ok: false, error: String((err && err.message) || err), kind: 'disk' };
    }
  });

  ipcMain.handle('aw:info', async (_e, serverUrl) => {
    try {
      return { ok: true, info: await aw.getInfo(serverUrl) };
    } catch (err) {
      return { ok: false, error: String((err && err.message) || err), kind: (err && err.kind) || 'unknown' };
    }
  });

  ipcMain.handle('aw:range', async (_e, { settings, from, to }) => {
    try {
      return { ok: true, data: await readRange(settings, from, to) };
    } catch (err) {
      return { ok: false, error: String((err && err.message) || err), kind: (err && err.kind) || 'unknown' };
    }
  });
}

// --- the window --------------------------------------------------------

function createWindow() {
  const win = new BrowserWindow({
    width: 1180,
    height: 860,
    minWidth: 720,
    minHeight: 520,
    title: 'Orchard',
    backgroundColor: '#ffffff',
    // The page paints its own chrome-coloured background before first paint,
    // so starting hidden avoids the white flash on a dark-themed desktop.
    show: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webviewTag: false,
    },
  });

  // `show: false` avoids a white flash on a dark desktop, but it means the
  // window exists and is invisible until the renderer paints. If anything stops
  // that first paint, the result is a running process with nothing on screen
  // and a terminal that looks hung — so the wait is bounded rather than
  // indefinite, and a window that is merely slow still appears.
  let shown = false;
  const reveal = () => {
    if (shown || win.isDestroyed()) return;
    shown = true;
    win.show();
  };
  win.once('ready-to-show', reveal);
  const fallback = setTimeout(reveal, 4000);
  win.on('closed', () => clearTimeout(fallback));

  win.loadFile(path.join(__dirname, '..', 'src', 'index.html'));

  // A failed load would otherwise be silent: the window shows on the fallback
  // timer with a blank page and nothing says why.
  win.webContents.on('did-fail-load', (_e, code, description, url) => {
    console.error(`Orchard could not load its interface (${code} ${description}) from ${url}`);
    reveal();
  });

  // `npm start` holds the terminal until the app quits, which reads as a hang
  // if the window is on another workspace or behind the editor. One line says
  // the process is alive and doing what it should.
  console.log('Orchard is running. Close the window to quit.');

  // Nothing in this app should ever navigate or open a window. Both are denied
  // rather than handled: a renderer that cannot navigate cannot be steered
  // somewhere else by a window title that happens to look like a link.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) void shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (event) => event.preventDefault());

  return win;
}

app.whenReady().then(() => {
  store = new Store(app.getPath('userData'));
  registerIpc();

  // The renderer is local files and a preload bridge; it needs no network of
  // its own, and saying so in a header means a mistake in the page cannot
  // quietly become an outbound request.
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: Object.assign({}, details.responseHeaders, {
        'Content-Security-Policy': [
          "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
        ],
      }),
    });
  });

  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  // macOS keeps the app alive without windows; everywhere else this is a quit.
  if (process.platform !== 'darwin') app.quit();
});
