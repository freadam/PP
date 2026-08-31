/**
 * Settings, on disk.
 *
 * One JSON file in the OS's application-data directory. Not a database:
 * everything here is a handful of rules and a server address, and the whole
 * file is rewritten on every save because it is measured in kilobytes.
 *
 * A corrupt file is not a reason to refuse to start. It is renamed aside and
 * the defaults are used, so a bad write can never leave someone with an app
 * that will not open and no way to fix it.
 */

'use strict';

const fs = require('node:fs');
const path = require('node:path');

class Store {
  constructor(dir, name) {
    this.file = path.join(dir, name || 'settings.json');
  }

  read() {
    let raw;
    try {
      raw = fs.readFileSync(this.file, 'utf8');
    } catch (err) {
      // No file yet is the normal first run, not a problem to report.
      if (err.code === 'ENOENT') return null;
      throw err;
    }
    try {
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === 'object' ? parsed : null;
    } catch (_) {
      try {
        fs.renameSync(this.file, `${this.file}.corrupt-${Date.now()}`);
      } catch (_e) {
        // If even the rename fails there is nothing further to try, and
        // starting with defaults still beats refusing to open.
      }
      return null;
    }
  }

  write(settings) {
    fs.mkdirSync(path.dirname(this.file), { recursive: true });
    // Written to a sibling and renamed, so an interrupted save cannot leave a
    // half-written file where the settings used to be.
    const tmp = `${this.file}.tmp`;
    fs.writeFileSync(tmp, JSON.stringify(settings, null, 2), 'utf8');
    fs.renameSync(tmp, this.file);
    return settings;
  }
}

module.exports = { Store };
