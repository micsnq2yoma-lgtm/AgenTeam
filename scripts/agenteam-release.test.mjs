import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { verifyInstaller } from './agenteam-release.mjs';
import { importProfile } from './import-agenteam-profile.mjs';

async function scratch(t) {
  const dir = await mkdtemp(join(tmpdir(), 'agenteam-release-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  return dir;
}

test('the release verifier accepts Tauri signatures and rejects altered files and foreign keys', async t => {
  const dir = await scratch(t);
  const cli = resolve(import.meta.dirname, '../node_modules/@tauri-apps/cli/tauri.js');
  const signer = (...args) => execFileSync(process.execPath, [cli, 'signer', ...args], { stdio: 'pipe' });
  const artifact = join(dir, 'candidate.exe');
  const key = join(dir, 'test.key');
  await writeFile(artifact, 'release verifier fixture');
  signer('generate', '--ci', '--password', 'fixture', '--write-keys', key);
  signer('sign', '--password', 'fixture', '--private-key-path', key, artifact);
  const signature = await readFile(artifact + '.sig', 'utf8');
  const publicKey = await readFile(key + '.pub', 'utf8');
  await verifyInstaller(artifact, signature, publicKey);
  const upstreamKey = await readFile(resolve(import.meta.dirname, '../src-tauri/upstream-resources.pub'), 'utf8');
  await assert.rejects(verifyInstaller(artifact, signature, upstreamKey), /signing key/);
  await writeFile(artifact, 'altered release verifier fixture');
  await assert.rejects(verifyInstaller(artifact, signature, publicKey), /Invalid installer signature/);
});

test('first-launch import copies portable content once and excludes pending updates and credentials', async t => {
  const home = await scratch(t);
  const legacy = join(home, '.myagents');
  await mkdir(join(legacy, 'skills'), { recursive: true });
  await writeFile(join(legacy, 'skills', 'hello.md'), 'hello');
  await writeFile(join(legacy, 'pending_update.bin'), 'old installer');
  await writeFile(join(legacy, 'config.json'), 'private');
  assert.deepEqual(await importProfile(home), ['skills']);
  const dest = join(home, '.agenteam');
  assert.equal(await readFile(join(dest, 'skills', 'hello.md'), 'utf8'), 'hello');
  await assert.rejects(readFile(join(dest, 'config.json')), { code: 'ENOENT' });
  await assert.rejects(readFile(join(dest, 'pending_update.bin')), { code: 'ENOENT' });
  await assert.rejects(importProfile(home), /already has a profile/);
  assert.equal(await readFile(join(legacy, 'config.json'), 'utf8'), 'private');
});
