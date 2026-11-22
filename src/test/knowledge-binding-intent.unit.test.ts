import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it, vi } from 'vitest';

const gate = vi.hoisted(() => ({
  entered: () => {},
  resume: Promise.resolve(),
}));

vi.mock('../server/utils/admin-config', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../server/utils/admin-config')>();
  return {
    ...actual,
    withAgentConfigIntentLock: <T>(fn: () => Promise<T>) =>
      actual.withAgentConfigIntentLock(async () => {
        gate.entered();
        await gate.resume;
        return fn();
      }),
  };
});

vi.mock('../server/utils/management-api-client', () => ({
  managementApi: vi.fn(async () => ({ ok: true })),
}));

// Use the shared on-disk config transaction behind the Renderer's IPC adapter.
vi.mock('../renderer/config/services/appConfigService', async () => {
  const config = await import('../server/utils/admin-config');
  return {
    loadAppConfig: config.loadConfig,
    atomicModifyConfig: config.atomicModifyConfig,
    notifyConfigChanged: vi.fn(),
  };
});

it('preserves a Renderer Agent edit concurrent with an Admin library binding intent', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'kb-binding-intent-'));
  const userDir = join(profile, '.agenteam');
  mkdirSync(userDir);
  const configPath = join(userDir, 'config.json');
  writeFileSync(configPath, JSON.stringify({
    agents: [{ id: 'owner', name: 'Before', enabled: true, channels: [] }],
  }));
  vi.stubEnv(process.platform === 'win32' ? 'USERPROFILE' : 'HOME', profile);
  vi.stubEnv('MYAGENTS_SIDECAR_ID', 'kb-binding-intent-sidecar');
  vi.resetModules();
  const { promise: entered, resolve: enter } = Promise.withResolvers<void>();
  const { promise: resume, resolve: release } = Promise.withResolvers<void>();
  gate.entered = enter;
  gate.resume = resume;
  const kb = 'kb_0123456789ab';

  try {
    const { handleAgentSet } = await import('../server/admin-api');
    const { patchAgentConfig } = await import('../renderer/config/services/agentConfigService');
    const binding = handleAgentSet({ id: 'owner', key: 'knowledgeBaseIds', value: [kb] });
    try {
      await Promise.race([entered, binding]);
      expect(existsSync(join(userDir, 'agent-config-intent.lock'))).toBe(true);
      await patchAgentConfig('owner', { name: 'Renderer edit' });
    } finally {
      release();
      await binding;
    }
    expect(await binding).toMatchObject({ success: true });
    expect(JSON.parse(readFileSync(configPath, 'utf8'))).toMatchObject({
      agents: [{ id: 'owner', name: 'Renderer edit', knowledgeBaseIds: [kb] }],
    });
  } finally {
    vi.unstubAllEnvs();
    rmSync(profile, { recursive: true });
  }
});
