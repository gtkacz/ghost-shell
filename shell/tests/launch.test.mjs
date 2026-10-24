import test from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { actionCommand, appCommand, searchDirs } from '../kit/GhostKit/lib/launch.mjs';

test('the user data directory is searched before the system ones, each once', () => {
  assert.deepEqual(searchDirs('/h/.local/share', '/usr/local/share:/usr/share:/h/.local/share'), [
    '/h/.local/share',
    '/h/.local/share/flatpak/exports/share',
    '/usr/local/share',
    '/usr/share',
  ]);
});

test('an unset XDG_DATA_DIRS falls back to the spec default', () => {
  assert.deepEqual(searchDirs('/h/.local/share', ''), [
    '/h/.local/share',
    '/h/.local/share/flatpak/exports/share',
    '/usr/local/share',
    '/usr/share',
  ]);
});

test('an app launches as an argv array, never a shell string built from the id', () => {
  const argv = appCommand('a b; rm x.desktop', ['/d1', '/d2']);
  assert.deepEqual(argv.slice(0, 2), ['sh', '-c']);
  assert.deepEqual(argv.slice(4), ['a b; rm x.desktop', '/d1', '/d2']);
  assert.ok(!argv[2].includes('rm x'));
});

test('an action runs its parsed command in a transient user service', () => {
  const argv = actionCommand({ name: 'New window', command: ['/bin/app', '--arg=two words'] }, '/home/u');
  assert.equal(argv[0], 'systemd-run');
  assert.ok(argv.includes('--user'));
  assert.deepEqual(argv.slice(argv.indexOf('--') + 1), ['/bin/app', '--arg=two words']);
  assert.ok(argv.includes('--working-directory=/home/u'));
});

test('the app script execs kioclient on the first desktop file that exists, and fails loudly when none does', () => {
  const root = mkdtempSync(join(tmpdir(), 'gs-launch-'));
  try {
    const first = join(root, 'a');
    const second = join(root, 'b');
    for (const dir of [first, second]) {
      mkdirSync(join(dir, 'applications'), { recursive: true });
    }
    writeFileSync(join(second, 'applications', 'x.desktop'), '');
    mkdirSync(join(root, 'bin'));
    const out = join(root, 'out');
    writeFileSync(join(root, 'bin', 'kioclient'), `#!/bin/sh\nprintf '%s\\n' "$@" > ${out}\n`);
    chmodSync(join(root, 'bin', 'kioclient'), 0o755);
    const env = { PATH: `${join(root, 'bin')}:/usr/bin:/bin` };

    const [cmd, ...args] = appCommand('x', [first, second]);
    assert.equal(spawnSync(cmd, args, { env }).status, 0);
    assert.equal(readFileSync(out, 'utf8'), `exec\n${join(second, 'applications', 'x.desktop')}\n`);

    const [cmd2, ...args2] = appCommand('missing', [first, second]);
    const miss = spawnSync(cmd2, args2, { env, encoding: 'utf8' });
    assert.notEqual(miss.status, 0);
    assert.match(miss.stderr, /missing/);
  } finally {
    rmSync(root, { recursive: true });
  }
});
