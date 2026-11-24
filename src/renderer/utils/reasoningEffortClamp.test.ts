import { describe, expect, it } from 'vitest';

import { shouldResetEffortForProtocol } from './reasoningEffortClamp';

const base = {
  reasoningEffort: 'high',
  inputUsesExternalRuntimeControls: false,
  managedProviderRuntimeActive: false,
  runtime: 'builtin',
  apiProtocol: 'anthropic' as const,
  providerId: 'anthropic-sub',
  model: 'claude-x',
  baseUrl: undefined,
};

describe('shouldResetEffortForProtocol', () => {
  it('keeps a level the protocol supports and never resets "default"', () => {
    expect(shouldResetEffortForProtocol(base)).toBe(false);
    expect(shouldResetEffortForProtocol({ ...base, reasoningEffort: 'default' })).toBe(false);
  });

  it('resets an OpenAI-only level left over on an Anthropic-protocol provider (the original reason for the clamp)', () => {
    expect(shouldResetEffortForProtocol({ ...base, reasoningEffort: 'minimal' })).toBe(true);
  });

  it('does not judge Managed Codex against the OpenAI vocabulary: catalog-only levels like ultra survive', () => {
    const managed = { ...base, runtime: 'codex', managedProviderRuntimeActive: true, apiProtocol: 'openai' as const, providerId: 'codex-sub' };
    for (const level of ['ultra', 'max', 'xhigh', 'future-tier']) {
      expect(shouldResetEffortForProtocol({ ...managed, reasoningEffort: level })).toBe(false);
    }
  });

  it('would have reset ultra for the same OpenAI-protocol provider when it is not Managed Codex', () => {
    expect(shouldResetEffortForProtocol({ ...base, apiProtocol: 'openai', providerId: 'some-openai', reasoningEffort: 'ultra' })).toBe(true);
  });

  it('leaves external CLI runtimes to their own controls', () => {
    expect(shouldResetEffortForProtocol({ ...base, runtime: 'claude-code', inputUsesExternalRuntimeControls: true, reasoningEffort: 'ultra' })).toBe(false);
  });
});
