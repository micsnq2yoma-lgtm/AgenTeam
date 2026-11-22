import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { handleHelp } from './admin-api';

const read = (relative: string) =>
  readFileSync(resolve(process.cwd(), relative), 'utf8').replace(/\r\n/g, '\n');

const BOOTSTRAP = read('bundled-workspaces/atmer/.claude/commands/BOOTSTRAP.md');
const CLAUDE = read('bundled-workspaces/atmer/CLAUDE.md');

function help(path: string[]): string {
  return String((handleHelp({ path }).data as { text?: string })?.text ?? '');
}

/** Every concrete `myagents <group> <leaf> …` invocation inside a fenced code block. */
function fencedCommands(markdown: string): string[] {
  return [...markdown.matchAll(/```[a-z]*\n([\s\S]*?)```/g)]
    .flatMap(block => block[1].split('\n'))
    .map(line => line.trim())
    .filter(line => line.startsWith('myagents '));
}

describe('assistant profile flow uses real CLI commands', () => {
  const commands = fencedCommands(CLAUDE);

  it('creates the profile Task as a one-shot and starts it', () => {
    expect(commands.some(line => line.startsWith('myagents task create-direct') && line.includes('--executionMode once'))).toBe(true);
    expect(commands.some(line => line.startsWith('myagents task run'))).toBe(true);
  });

  it.each(commands)('%s exists in its leaf help with every flag it uses', line => {
    const path: string[] = [];
    for (const word of line.split(/\s+/).slice(1)) {
      if (word.startsWith('-') || word.startsWith('<') || word.startsWith('"')) break;
      path.push(word);
    }
    const text = help(path);
    expect(text, `${line}: no help for ${path.join(' ')}`).toContain(`myagents ${path.join(' ')}`);
    for (const flag of line.matchAll(/--[A-Za-z][A-Za-z-]*/g)) {
      expect(text, `${line}: ${flag[0]}`).toContain(flag[0]);
    }
  });
});

describe('assistant profile flow rules', () => {
  it('BOOTSTRAP invites materials but creates nothing when skipped, and only reads what is given', () => {
    expect(BOOTSTRAP).toContain('## Get To Know Them (optional)');
    expect(BOOTSTRAP).toMatch(/They skip or say no:[^\n]*Create nothing/);
    expect(BOOTSTRAP).toMatch(/Only use what they hand you/);
    expect(BOOTSTRAP).toContain('Know Your Human');
  });

  it('BOOTSTRAP only fills the basic-information chapter of USER.md', () => {
    expect(BOOTSTRAP).toMatch(/only the 基本信息 chapter/);
  });

  it('CLAUDE.md keeps the lasting rules so they survive BOOTSTRAP being deleted', () => {
    const section = CLAUDE.split('## Know Your Human')[1].split('\n## ')[0];
    for (const mode of ['first', 'update', 'correction']) expect(section).toContain(mode);
    expect(section).toContain('agenteam-user-profile');
    expect(section).toMatch(/No materials and no correction: don't start anything/);
    expect(section).toMatch(/Read only what they gave you/);
    expect(section).toMatch(/Don't paste the report, and don't quote their materials/);
    expect(section).toMatch(/A correction from them beats anything in the profile/);
  });

  it('day-to-day learning goes under 近期观察 instead of rewriting chapters', () => {
    expect(CLAUDE).toMatch(/add a dated line under 近期观察/);
    expect(CLAUDE).not.toMatch(/New understanding of your human → update `03-USER.md`\n/);
  });
});
