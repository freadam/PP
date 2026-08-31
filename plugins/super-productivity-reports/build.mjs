/**
 * Inline everything into one `index.html`, then zip what Super Productivity
 * installs.
 *
 * The app serves plugin UI through an iframe `srcdoc`, so no second file from
 * the ZIP is reachable from the page. Rather than write the whole plugin as one
 * unreadable HTML file, the sources live under `src/` — where the engine can be
 * unit-tested on its own — and this step assembles them.
 */

import { readFileSync, writeFileSync, mkdirSync, rmSync, copyFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const src = (f) => readFileSync(join(here, 'src', f), 'utf8');
const dist = join(here, 'dist');

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

/**
 * `</script>` inside a string literal ends the enclosing script element, even
 * in the middle of a JS string — the HTML tokenizer does not know it is looking
 * at JavaScript. None of these sources contain one today; the guard is here so
 * that adding one later fails loudly at build time instead of producing a page
 * that silently stops running halfway down.
 */
function inline(code, name) {
  if (/<\/script/i.test(code)) {
    throw new Error(`${name} contains "</script", which would terminate the inlined block. Split it as "<\\/script".`);
  }
  return code;
}

const html = src('index.html')
  .replace('/* __STYLES__ */', () => src('styles.css'))
  .replace('/* __ENGINE__ */', () => inline(src('engine.js'), 'engine.js'))
  .replace('/* __RENDER__ */', () => inline(src('render.js'), 'render.js'))
  .replace('/* __APP__ */', () => inline(src('app.js'), 'app.js'));

writeFileSync(join(dist, 'index.html'), html);
for (const f of ['manifest.json', 'plugin.js', 'icon.svg']) copyFileSync(join(here, f), join(dist, f));

const { id, version } = JSON.parse(readFileSync(join(here, 'manifest.json'), 'utf8'));
const zip = `${id}-${version}.zip`;
try {
  execFileSync('zip', ['-q', '-j', '-r', zip, 'index.html', 'manifest.json', 'plugin.js', 'icon.svg'], {
    cwd: dist,
    stdio: 'inherit',
  });
  console.log(`dist/${zip}  ·  index.html ${(html.length / 1024).toFixed(1)} kB`);
} catch (err) {
  // A missing `zip` binary is not a failed build — the four files in dist/ are
  // the plugin, and Super Productivity can install them from a folder too.
  console.log(`dist/ built (index.html ${(html.length / 1024).toFixed(1)} kB). No "zip" binary — archive not created.`);
}
