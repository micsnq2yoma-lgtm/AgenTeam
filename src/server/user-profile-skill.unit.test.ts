import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { handleHelp } from './admin-api';

const SKILL = readFileSync(
  resolve(process.cwd(), 'bundled-skills/agenteam-user-profile/SKILL.md'),
  'utf8',
).replace(/\r\n/g, '\n');

function help(path: string[]): string {
  return String((handleHelp({ path }).data as { text?: string })?.text ?? '');
}

/** Every concrete `myagents <group> <leaf> …` invocation written in the Skill. */
function invocations(markdown: string): string[] {
  return [...markdown.matchAll(/myagents [a-z][^`\n]*/g)].map(match => match[0].trim());
}

describe('agenteam-user-profile skill uses real CLI commands', () => {
  const lines = invocations(SKILL);

  it('invokes at least the AnyDoc conversion', () => {
    expect(lines.some(line => line.startsWith('myagents anydoc convert'))).toBe(true);
  });

  it.each(lines)('%s exists in its leaf help with every flag it uses', line => {
    const words = line.split(/\s+/).slice(1);
    const path: string[] = [];
    for (const word of words) {
      if (word.startsWith('-') || word.startsWith('<')) break;
      path.push(word);
    }
    expect(path.length, line).toBeGreaterThanOrEqual(2);
    const text = help(path);
    expect(text, `${line}: no help for ${path.join(' ')}`).toContain(`myagents ${path.join(' ')}`);
    for (const flag of line.matchAll(/--[a-z][a-z-]*/g)) {
      expect(text, `${line}: ${flag[0]}`).toContain(flag[0]);
    }
  });
});
