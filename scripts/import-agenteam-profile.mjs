import { cp, lstat, mkdtemp, readdir, rename, rm } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function importProfile(home) {
  const source = join(home, '.myagents');
  const target = join(home, '.agenteam');
  const existing = await lstat(target).catch(error => {
    if (error.code !== 'ENOENT') throw error;
  });
  if (existing) throw new Error('AgenTeam already has a profile. Import before first launch.');
  if (!(await lstat(source)).isDirectory()) throw new Error('Expected a MyAgents profile directory.');
  const allowed = new Set(['commands', 'skills', 'templates']);
  const entries = (await readdir(source, { withFileTypes: true })).filter(entry => allowed.has(entry.name) && entry.isDirectory());
  if (!entries.length) throw new Error('No portable commands, skills or templates found.');
  const staging = await mkdtemp(join(home, '.agenteam-import-'));
  try {
    for (const { name } of entries) {
      await cp(join(source, name), join(staging, name), {
        recursive: true, force: false, errorOnExist: true,
        filter: async path => {
          const entry = await lstat(path);
          return entry.isDirectory() || entry.isFile();
        },
      });
    }
    await rename(staging, target);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
  return entries.map(entry => entry.name);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log('Imported into ~/.agenteam:', (await importProfile(homedir())).join(', '));
}
