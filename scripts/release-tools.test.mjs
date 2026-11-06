import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { makeLatestJson } from './make-latest-json.mjs';
import { bumpVersion } from './bump-version.mjs';

let dir;
afterEach(async () => { if (dir) await rm(dir, { recursive: true, force: true }); dir = undefined; });

test('latest manifest has the Tauri updater fields', () => {
  const json = makeLatestJson({ version: '0.2.0', notes: 'Fixes', signature: 'sig', url: 'https://example.test/app.tar.gz', pubDate: '2026-11-05T00:00:00Z' });
  assert.deepEqual(json, {
    version: '0.2.0',
    notes: 'Fixes',
    pub_date: '2026-11-05T00:00:00.000Z',
    platforms: { 'darwin-aarch64': { signature: 'sig', url: 'https://example.test/app.tar.gz' } },
  });
});

test('version bump updates Cargo, Tauri config and package consistently', async () => {
  dir = await mkdtemp(join(tmpdir(), 'scribular-version-'));
  await mkdir(join(dir, 'src-tauri'));
  await writeFile(join(dir, 'src-tauri/Cargo.toml'), '[package]\nversion = "1.2.3"\n');
  await writeFile(join(dir, 'src-tauri/tauri.conf.json'), '{"version":"1.2.3"}');
  await writeFile(join(dir, 'package.json'), '{"version":"1.2.3"}');
  assert.equal(await bumpVersion('minor', dir), '1.3.0');
  for (const name of ['src-tauri/Cargo.toml', 'src-tauri/tauri.conf.json', 'package.json']) assert.match(await readFile(join(dir, name), 'utf8'), /1\.3\.0/);
  assert.equal(await bumpVersion('patch', dir), '1.3.1');
  assert.equal(await bumpVersion('major', dir), '2.0.0');
});
