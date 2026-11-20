import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readModelEgressPolicy, sealRestrictedSdkRoute, supportsVerifiedModelProvider, verifyNativeModelAdmission } from './egress-model-policy';

const managementApi = vi.hoisted(() => vi.fn());
vi.mock('./utils/management-api-client', () => ({ managementApi }));

describe('model egress admission', () => {
  let directory: string;
  let path: string;
  const ip = '203.0.113.7';

  beforeEach(() => {
    directory = mkdtempSync(join(tmpdir(), 'agenteam-model-egress-'));
    path = join(directory, 'config.json');
    managementApi.mockReset();
  });
  afterEach(() => rmSync(directory, { recursive: true, force: true }));

  it('keeps first launch and legacy configs with no restriction usable', () => {
    expect(readModelEgressPolicy(path, '')).toBe(null);
    writeFileSync(path, '{"defaultIntegratedRuntime":"builtin"}');
    expect(readModelEgressPolicy(path, '')).toBe(null);
    writeFileSync(path, '{"egressRestriction":{"enabled":false}}');
    expect(readModelEgressPolicy(path, '')).toBe(null);
  });

  it('refuses all old sidecars when enabling, changing or disabling the restriction', () => {
    writeFileSync(path, JSON.stringify({ egressRestriction: { enabled: true, expectedIp: ip } }));
    expect(() => readModelEgressPolicy(path, '')).toThrow();
    expect(() => readModelEgressPolicy(path, '203.0.113.8')).toThrow();
    expect(readModelEgressPolicy(path, ip)).toBe(ip);
    writeFileSync(path, JSON.stringify({ egressRestriction: { enabled: false, expectedIp: ip } }));
    expect(() => readModelEgressPolicy(path, ip)).toThrow();
  });

  it('matches equivalent IPv6 notation against the native address', () => {
    writeFileSync(path, JSON.stringify({ egressRestriction: { enabled: true, expectedIp: '2606:4700:0000:0000:0000:0000:0:1111' } }));
    expect(readModelEgressPolicy(path, '2606:4700::1111')).toBe('2606:4700::1111');
  });

  it('fails closed on malformed or unreadable policy evidence', () => {
    for (const config of ['bad', 'null', '[]', '{"egressRestriction":null}', '{"egressRestriction":{"enabled":true}}',
      '{"egressRestriction":{"enabled":true,"expectedIp":"example.com"}}']) {
      writeFileSync(path, config);
      expect(() => readModelEgressPolicy(path, '')).toThrow();
    }
    expect(() => readModelEgressPolicy(directory, '')).toThrow();
  });

  it('admits only official OpenAI and Anthropic destinations with explicit API keys', () => {
    const openai = { apiProtocol: 'openai' as const, baseUrl: 'https://api.openai.com/v1', apiKey: 'fixture' };
    expect(supportsVerifiedModelProvider(openai)).toBe(true);
    for (const baseUrl of ['https://proxy.example/v1', 'http://api.openai.com/v1', 'https://api.openai.com.example/v1',
      'https://user@api.openai.com/v1', 'https://api.openai.com:8443/v1']) {
      expect(supportsVerifiedModelProvider({ ...openai, baseUrl })).toBe(false);
    }
    expect(supportsVerifiedModelProvider(undefined)).toBe(false);
    expect(supportsVerifiedModelProvider({ ...openai, apiProtocol: 'anthropic' })).toBe(false);
    const native = { apiProtocol: 'anthropic' as const, apiKey: 'fixture', baseUrl: 'https://api.anthropic.com', authType: 'both' as const };
    expect(supportsVerifiedModelProvider(native)).toBe(true);
    expect(supportsVerifiedModelProvider({ ...native, providerId: 'anthropic-sub' })).toBe(false);
    expect(supportsVerifiedModelProvider({ ...native, apiKey: '' })).toBe(false);
    expect(supportsVerifiedModelProvider({ ...native, authType: 'auth_token' })).toBe(false);
    expect(supportsVerifiedModelProvider({ ...native, baseUrl: 'https://api.anthropic.com.relay.invalid' })).toBe(false);
  });

  it('requires matching fresh native admission and honors abort before requesting it', async () => {
    const controller = new AbortController();
    for (const response of [{ ok: false }, { ok: true }, { ok: true, expectedIp: '203.0.113.8' }]) {
      managementApi.mockResolvedValueOnce(response);
      await expect(verifyNativeModelAdmission(ip, controller.signal)).rejects.toThrow();
    }
    managementApi.mockResolvedValueOnce({ ok: true, expectedIp: ip });
    await expect(verifyNativeModelAdmission(ip, controller.signal)).resolves.toBeUndefined();
    controller.abort();
    await expect(verifyNativeModelAdmission(ip, controller.signal)).rejects.toThrow();
    expect(managementApi).toHaveBeenCalledTimes(4);
  });

  it('overrides cloud backends, mixed-case inherited routes and upstream credentials in SDK children', () => {
    const env: NodeJS.ProcessEnv = {
      ANTHROPIC_API_KEY: 'fixture-api-key', anthropic_base_url: 'https://relay.invalid',
      CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD: '1', Claude_Code_Use_Gateway: 'true',
      CLAUDE_CODE_OAUTH_TOKEN: 'fixture-oauth-token', HOME: '/fixture/home',
    };
    const loopback = 'http://127.0.0.1:19123/bridge/example';
    sealRestrictedSdkRoute(env, loopback);
    expect(env).toMatchObject({
      ANTHROPIC_BASE_URL: loopback, CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST: '1', HOME: '/fixture/home',
      CLAUDE_CODE_USE_GATEWAY: '', CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD: '', CLAUDE_CODE_OAUTH_TOKEN: '',
    });
    expect(env).not.toHaveProperty('anthropic_base_url');
    expect(env).not.toHaveProperty('Claude_Code_Use_Gateway');
    expect(Object.values(env)).not.toContain('fixture-api-key');
  });
});
