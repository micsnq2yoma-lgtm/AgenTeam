// The assistant template must not tell a desktop user's assistant to sync through git on its own:
// most workspaces are not repositories, and the memory files describe the user, so pushing them
// to a remote must be something the user set up and asked for.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..', 'bundled-workspaces/atmer');
const read = file => readFileSync(join(root, file), 'utf8');

test('no template rule pushes without permission or claims a GitHub "cloud existence"', () => {
  for (const file of ['CLAUDE.md', '.claude/rules/01-IDENTITY.md', '.claude/rules/02-SOUL.md', '.claude/commands/UPDATE_MEMORY.md', '.claude/commands/BOOTSTRAP.md']) {
    const text = read(file);
    assert.ok(!/No permission needed/i.test(text), `${file} must not authorise unprompted pushes`);
    assert.ok(!/cloud (existence|presence)/i.test(text), `${file} must not describe GitHub as the assistant's cloud existence`);
    assert.ok(!/your GitHub repo/i.test(text), `${file} must not assume a GitHub repository`);
  }
});

test('git pull, commit and push are conditional on a sync the user set up', () => {
  assert.match(read('CLAUDE.md'), /only if[^\n]*git repository with a remote[^\n]*set up for syncing/i);
  assert.match(read('.claude/rules/02-SOUL.md'), /Never set up a repository, add a remote, or push on your own/);
  assert.match(read('.claude/commands/UPDATE_MEMORY.md'), /only if this folder is a git repository your human set up for syncing/i);
});
