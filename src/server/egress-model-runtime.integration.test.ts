import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { buildClaudeSessionEnv, getSidecarPort, setSidecarPort, startOneShotBridge } from './agent-session';
import { lookupBridge } from './openai-bridge/bridge-registry';
import { modelSdkSettingSources } from './egress-model-policy';
import { handleVerifiedAnthropicRoute } from './openai-bridge/verified-anthropic-route';
import * as verifiedEgress from './openai-bridge/verified-egress';
import { resolveCodexCommandContext } from './runtimes/codex-command-context';
import { validateGeminiImage } from './tools/gemini-image-tool';

describe('model execution under an egress restriction', () => {
  let home: string;
  let previousPort: number;
  beforeEach(() => {
    home = mkdtempSync(join(tmpdir(), 'agenteam-model-runtime-'));
    mkdirSync(join(home, '.agenteam'));
    writeFileSync(join(home, '.agenteam', 'config.json'), JSON.stringify({
      egressRestriction: { enabled: true, expectedIp: '203.0.113.10' },
    }));
    vi.stubEnv('USERPROFILE', home);
    vi.stubEnv('HOME', home);
    previousPort = getSidecarPort();
    setSidecarPort(17007);
  });
  afterEach(() => {
    setSidecarPort(previousPort);
    vi.restoreAllMocks();
    vi.unstubAllEnvs();
    rmSync(home, { recursive: true, force: true });
  });

  it('only allows the SDK child to contact the token-bound local bridge for the official provider', () => {
    vi.stubEnv('CLAUDE_CODE_USE_GATEWAY', '1');
    const provider = { apiProtocol: 'openai' as const, baseUrl: 'https://api.openai.com/v1',
      apiKey: 'upstream-test-key', providerId: 'custom-openai' };
    const native = buildClaudeSessionEnv(provider, 'gpt-test', { bridgeToken: 'fixture', providerId: provider.providerId });
    expect(native.ANTHROPIC_BASE_URL).toBe('http://127.0.0.1:17007/bridge/fixture');
    expect(native.ANTHROPIC_API_KEY).not.toBe(provider.apiKey);
    expect(native.CLAUDE_CODE_USE_GATEWAY).toBe('');
    expect(modelSdkSettingSources()).toEqual([]);
    // A verified provider never runs without its bridge route.
    expect(() => buildClaudeSessionEnv(provider)).toThrow('限制模式');
    // Providers without the per-request proof are no longer refused; they take their normal route and
    // are admitted per turn (model-turn-admission.unit.test.ts). Project settings stay locked out.
    expect(() => buildClaudeSessionEnv({})).not.toThrow();
    expect(() => buildClaudeSessionEnv({ ...provider, apiProtocol: 'anthropic', baseUrl: 'https://relay.invalid' }, 'gpt-test'))
      .not.toThrow();
    expect(modelSdkSettingSources()).toEqual([]);
  });

  it('holds Codex start and image probes until the host confirms the egress', async () => {
    // No native host verdict exists in this test process, so admission is refused before anything leaves.
    await expect(resolveCodexCommandContext({ source: 'managed-provider' })).rejects.toThrow('出口 IP');
    await expect(validateGeminiImage({ GEMINI_API_KEY: 'fixture' }))
      .resolves.toMatchObject({ type: 'runtime_error', message: expect.stringContaining('出口 IP') });
  });

  it('sends an official Anthropic API key only from the native forwarding owner', async () => {
    const provider = { providerId: 'anthropic-api', baseUrl: 'https://api.anthropic.com',
      authType: 'both' as const, apiKey: 'anthropic-fixture-key' };
    const bridge = startOneShotBridge(provider, 'claude-model', 'restriction-test');
    const request = new Request('http://127.0.0.1/fixture', { method: 'POST', body: '{}' });
    const forward = vi.spyOn(verifiedEgress, 'forwardVerifiedAnthropic').mockResolvedValue(new Response('verified'));
    try {
      const env = buildClaudeSessionEnv(provider, 'claude-model', { bridgeToken: bridge.token });
      expect(env.ANTHROPIC_BASE_URL).toBe(`http://127.0.0.1:17007/bridge/${bridge.token}/anthropic`);
      expect(env.ANTHROPIC_API_KEY).not.toBe(provider.apiKey);
      expect(env.ANTHROPIC_AUTH_TOKEN).toBe('');
      expect(await lookupBridge(bridge.token)).toMatchObject({ restrictedAnthropic: true, apiKey: provider.apiKey });
      expect((await handleVerifiedAnthropicRoute(request, 'unknown-token', 'messages')).status).toBe(403);
      expect(forward).not.toHaveBeenCalled();
      expect(await (await handleVerifiedAnthropicRoute(request, bridge.token, 'messages/count_tokens')).text()).toBe('verified');
      expect(forward).toHaveBeenCalledWith(request, 'messages/count_tokens', provider.providerId, provider.apiKey, '203.0.113.10');
      // A subscription is admitted per turn and keeps Claude Code's own OAuth route (not sealed).
      expect(buildClaudeSessionEnv({ providerId: 'anthropic-sub' }).ANTHROPIC_BASE_URL).not.toContain('/bridge/');
    } finally {
      bridge.release();
    }
    expect(await lookupBridge(bridge.token)).toBeUndefined();
  });

  it('keeps normal project settings and native Anthropic routing when disabled', () => {
    writeFileSync(join(home, '.agenteam', 'config.json'), '{}');
    expect(modelSdkSettingSources()).toEqual(['project']);
    const provider = { providerId: 'anthropic-api', baseUrl: 'https://api.anthropic.com',
      authType: 'api_key' as const, apiKey: 'anthropic-fixture-key' };
    const env = buildClaudeSessionEnv(provider);
    expect(env.ANTHROPIC_BASE_URL).toBe(provider.baseUrl);
    expect(env.ANTHROPIC_API_KEY).toBe(provider.apiKey);
  });
});
