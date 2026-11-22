import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { afterEach, describe, expect, it } from 'vitest';

import { isProactiveEnabled, PROACTIVE_RULES_FILE, readProactiveHeartbeatNotice } from './proactive-heartbeat';

const dirs: string[] = [];
function workspace(rules?: string): string {
  const dir = mkdtempSync(join(tmpdir(), 'proactive-'));
  dirs.push(dir);
  if (rules !== undefined) {
    mkdirSync(join(dir, 'memory'));
    writeFileSync(join(dir, PROACTIVE_RULES_FILE), rules);
  }
  return dir;
}
afterEach(() => { for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true }); });

describe('proactive heartbeat opt-in', () => {
  it('is off without the file, by default, and when disabled', () => {
    expect(readProactiveHeartbeatNotice(workspace())).toBeNull();
    expect(readProactiveHeartbeatNotice(workspace('---\nenabled: false\n---\n# x'))).toBeNull();
    expect(readProactiveHeartbeatNotice(workspace('# no front matter\nenabled: true'))).toBeNull();
  });
  it('is on only for enabled: true in the front matter', () => {
    expect(isProactiveEnabled('---\r\nenabled: true\r\n---\r\nbody')).toBe(true);
    expect(isProactiveEnabled('---\nenabled: "true"\n---\n')).toBe(true);
    expect(isProactiveEnabled('---\nenabled: truely\n---\n')).toBe(false);
    expect(isProactiveEnabled('---\ndailyCap: 3\n---\nenabled: true')).toBe(false);
    expect(readProactiveHeartbeatNotice(workspace('---\nenabled: true\n---\n'))).toContain('agenteam-proactive');
  });
});
