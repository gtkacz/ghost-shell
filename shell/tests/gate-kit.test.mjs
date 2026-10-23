import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const KIT = join(ROOT, 'shell/kit/GhostKit');
const GATE = join(ROOT, 'shell/gate/GhostGate');
const QSB_SUFFIX = '.qsb';
const QT_TYPES = new Set(['Item', 'Text', 'Rectangle', 'Shape', 'ShapePath', 'ShaderEffect', 'QtObject', 'Binding', 'Connections', 'Timer', 'Image', 'PathSvg', 'PathLine', 'PathMove', 'PathArc', 'PathQuad', 'PathCubic', 'Repeater', 'Row', 'Column', 'MouseArea', 'Component', 'Loader']);

const script = readFileSync(join(ROOT, 'scripts/assemble-gate.sh'), 'utf8');
const sharedBlock = script.match(/SHARED_FILES=\(([^)]*)\)/);
const SHARED_FILES = sharedBlock ? sharedBlock[1].split(/\s+/).filter(Boolean) : [];
const sharedSources = SHARED_FILES.filter((name) => !name.endsWith(QSB_SUFFIX));

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

const gateFiles = walk(join(ROOT, 'shell/gate')).filter((path) => /\.(qml|mjs)$/.test(path));
const sharedQml = sharedSources.filter((name) => name.endsWith('.qml')).map((name) => join(KIT, name));
const gateQml = gateFiles.filter((path) => path.endsWith('.qml'));
const read = (path) => readFileSync(path, 'utf8');

test('SHARED_FILES is parsed out of the assemble script and every entry exists', () => {
  assert.ok(SHARED_FILES.length > 0);
  for (const name of SHARED_FILES) {
    // Shaders are built by `make shaders`; their source stands in for the build output.
    const source = name.endsWith(QSB_SUFFIX) ? name.slice(0, -QSB_SUFFIX.length) : name;
    assert.ok(existsSync(join(KIT, source)), name);
  }
});

test('every Theme member the shared files read is declared by the gate Theme', () => {
  const theme = read(join(GATE, 'Theme.qml'));
  const used = new Set();
  for (const path of sharedQml) {
    for (const match of read(path).matchAll(/\bTheme\.([A-Za-z]\w*)/g)) {
      used.add(match[1]);
    }
  }
  assert.ok(used.size > 0);
  for (const name of used) {
    const declared = new RegExp(`(property\\s+\\S+\\s+${name}\\b|function\\s+${name}\\s*\\()`);
    assert.match(theme, declared, `Theme.${name}`);
  }
});

test('every QML type the shared files instantiate is a Qt type or in the gate qmldir', () => {
  const qmldir = read(join(GATE, 'qmldir'));
  const known = new Set([...QT_TYPES, ...[...qmldir.matchAll(/^(?:singleton\s+)?(\w+)\s+\d/gm)].map((m) => m[1])]);
  for (const path of [...sharedQml, ...gateQml]) {
    for (const match of read(path).matchAll(/^\s*([A-Z]\w*)\s*\{/gm)) {
      assert.ok(known.has(match[1]), `${match[1]} in ${path}`);
    }
  }
});

test('nothing in the gate or the shared files imports Quickshell or GhostKit', () => {
  const files = [...gateFiles, ...sharedSources.map((name) => join(KIT, name))];
  for (const path of files) {
    assert.doesNotMatch(read(path), /^\s*import\s+(Quickshell|GhostKit)\b/m, path);
  }
});
