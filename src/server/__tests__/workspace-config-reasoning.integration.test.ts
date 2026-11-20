import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { SessionMetadata } from '../types/session';

let scratch: string;
let prevHome: string | undefined;
let prevUserProfile: string | undefined;

function writeJson(name: string, value: unknown): void {
  writeFileSync(join(scratch, '.agenteam', name), JSON.stringify(value, null, 2), 'utf-8');
}

beforeEach(() => {
  scratch = mkdtempSync(join(tmpdir(), 'myagents-workspace-effort-'));
  mkdirSync(join(scratch, '.agenteam'), { recursive: true });
  prevHome = process.env.HOME;
  prevUserProfile = process.env.USERPROFILE;
  process.env.HOME = scratch;
  process.env.USERPROFILE = scratch;
});

afterEach(() => {
  process.env.HOME = prevHome;
  process.env.USERPROFILE = prevUserProfile;
  rmSync(scratch, { recursive: true, force: true });
});

describe('resolveWorkspaceConfig reasoning effort fallback (provider 自定义默认设置)', () => {
  async function resolveEffort(
    agent: Record<string, unknown>,
    metadata?: Partial<SessionMetadata>,
  ): Promise<string | undefined> {
    const workspacePath = join(scratch, 'workspace');
    writeJson('config.json', {
      providerDefaultReasoningEfforts: { deepseek: 'high' },
      agents: [{ id: 'agent', name: 'Agent', enabled: true, workspacePath, providerId: 'deepseek', ...agent }],
    });
    writeJson('projects.json', []);
    const { resolveWorkspaceConfig } = await import('../utils/admin-config');
    return resolveWorkspaceConfig(workspacePath, metadata as SessionMetadata | undefined, { includeMcp: false })
      .reasoningEffort;
  }

  it('uses the provider default when neither the Session nor the Agent chose an effort', async () => {
    // IM /new and Cron births reach here without an effort of their own.
    expect(await resolveEffort({})).toBe('high');
    expect(await resolveEffort({}, { configSnapshotAt: '2026-11-19T00:00:00.000Z', providerId: 'deepseek' })).toBe('high');
  });

  it('keeps an explicit Agent or Session choice, including an explicit default', async () => {
    expect(await resolveEffort({ reasoningEffort: 'low' })).toBe('low');
    expect(await resolveEffort({}, {
      configSnapshotAt: '2026-11-19T00:00:00.000Z',
      providerId: 'deepseek',
      reasoningEffort: 'default',
    })).toBe('default');
  });

  it('follows the provider when it has no default of its own', async () => {
    expect(await resolveEffort({ providerId: 'zhipu' })).toBeUndefined();
  });
});
