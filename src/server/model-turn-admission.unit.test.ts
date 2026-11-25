import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  expectedIp: '203.0.113.10' as string | null,
  verifyNative: vi.fn(async (_ip: string, _signal: AbortSignal, _maxAge?: number) => {}),
  probe: vi.fn(),
}));

vi.mock('./egress-model-policy', () => ({
  getRestrictedModelExpectedIp: () => mocks.expectedIp,
  supportsVerifiedModelProvider: (p?: { baseUrl?: string; apiKey?: string }) => p?.baseUrl === 'https://api.anthropic.com' && !!p.apiKey,
  verifyNativeModelAdmission: mocks.verifyNative,
}));
vi.mock('./openai-bridge/verified-egress', () => ({
  ModelEgressError: class ModelEgressError extends Error {},
  probeEdgeEgress: mocks.probe,
}));
vi.mock('./proxy-state', () => ({ getProxyForProviderUrl: () => undefined }));

import {
  admitModelAccess,
  admitModelTurn,
  builtinTurnNeedsAdmission,
  modelEgressHostsForProvider,
  modelEgressHostsForRuntime,
} from './model-turn-admission';

describe('restricted-mode model admission', () => {
  beforeEach(() => {
    mocks.expectedIp = '203.0.113.10';
    mocks.verifyNative.mockReset().mockResolvedValue(undefined);
    mocks.probe.mockReset();
  });

  it('does nothing when no restriction is configured', async () => {
    mocks.expectedIp = null;
    await admitModelTurn({ hosts: ['api.anthropic.com'] });
    await admitModelAccess();
    expect(mocks.verifyNative).not.toHaveBeenCalled();
    expect(mocks.probe).not.toHaveBeenCalled();
  });

  it('layer 1 asks the host for fresh evidence before every turn', async () => {
    await admitModelTurn({ hosts: [] });
    expect(mocks.verifyNative).toHaveBeenCalledWith('203.0.113.10', expect.any(AbortSignal), 3000);
  });

  it('refuses the turn when the host does not confirm the egress', async () => {
    mocks.verifyNative.mockRejectedValue(new Error('客户端尚未确认当前出口 IP，已暂停模型请求。'));
    await expect(admitModelTurn({ hosts: ['api.anthropic.com'] })).rejects.toThrow('出口 IP');
    expect(mocks.probe).not.toHaveBeenCalled();
  });

  it('layer 2 fails closed for official hosts on mismatch and on an unavailable proof', async () => {
    mocks.probe.mockResolvedValueOnce({ status: 'mismatch', observed: '198.51.100.7' });
    await expect(admitModelTurn({ hosts: ['api.anthropic.com'] })).rejects.toThrow('198.51.100.7');
    mocks.probe.mockResolvedValueOnce({ status: 'unavailable' });
    await expect(admitModelTurn({ hosts: ['chatgpt.com'] })).rejects.toThrow('无法核验');
    mocks.probe.mockResolvedValueOnce({ status: 'match', observed: '203.0.113.10' });
    await expect(admitModelTurn({ hosts: ['api.anthropic.com'] })).resolves.toBeUndefined();
  });

  it('layer 2 rejects other hosts only on a positive mismatch', async () => {
    mocks.probe.mockResolvedValueOnce({ status: 'unavailable' });
    await expect(admitModelTurn({ hosts: ['relay.example.com'] })).resolves.toBeUndefined();
    mocks.probe.mockResolvedValueOnce({ status: 'mismatch', observed: '198.51.100.7' });
    await expect(admitModelTurn({ hosts: ['relay.example.com'] })).rejects.toThrow('relay.example.com');
  });

  it('checks each host once', async () => {
    mocks.probe.mockResolvedValue({ status: 'match', observed: '203.0.113.10' });
    await admitModelTurn({ hosts: ['api.anthropic.com', 'api.anthropic.com'] });
    expect(mocks.probe).toHaveBeenCalledTimes(1);
  });

  it('maps providers and runtimes to the hosts that carry their model traffic', () => {
    expect(modelEgressHostsForProvider({ providerId: 'anthropic-sub' })).toEqual(['api.anthropic.com']);
    expect(modelEgressHostsForProvider({ providerId: 'custom', baseUrl: 'https://Relay.Example.com/v1' })).toEqual(['relay.example.com']);
    expect(modelEgressHostsForProvider(undefined)).toEqual([]);
    expect(modelEgressHostsForRuntime('claude-code')).toEqual(['api.anthropic.com']);
    expect(modelEgressHostsForRuntime('codex')).toEqual(['chatgpt.com']);
    expect(modelEgressHostsForRuntime('antigravity')).toEqual([]);
    expect(modelEgressHostsForRuntime('dsh')).toEqual([]);
  });

  it('leaves official API-key providers to their per-request proof', () => {
    expect(builtinTurnNeedsAdmission({ providerId: 'anthropic-api', baseUrl: 'https://api.anthropic.com', apiKey: 'k' })).toBe(false);
    expect(builtinTurnNeedsAdmission({ providerId: 'anthropic-sub' })).toBe(true);
    mocks.expectedIp = null;
    expect(builtinTurnNeedsAdmission({ providerId: 'anthropic-sub' })).toBe(false);
  });
});
