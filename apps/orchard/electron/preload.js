/**
 * The bridge, and the whole of the renderer's power.
 *
 * Four functions, each with a fixed shape. No `invoke(channel, …)` passthrough:
 * a generic channel argument would mean the renderer's reach grows every time
 * a handler is added in the main process, which is how a narrow bridge becomes
 * a wide one without anybody deciding to widen it.
 */

'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('orchard', {
  readSettings: () => ipcRenderer.invoke('settings:read'),
  writeSettings: (settings) => ipcRenderer.invoke('settings:write', settings),
  info: (serverUrl) => ipcRenderer.invoke('aw:info', serverUrl),
  range: (settings, from, to) => ipcRenderer.invoke('aw:range', { settings, from, to }),
});
